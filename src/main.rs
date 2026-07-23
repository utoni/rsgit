mod git;
mod highlight;
mod util;
mod views;

use axum::{
    extract::{Path, RawQuery, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::get,
    routing::post,
    Router,
};
use git::{
    clone as GitClone, commit,
    diff::commit_diff,
    error::Error as GitError,
    references::{refs as git_refs, Refs},
    repository::{discover as GitDiscover, open as GitOpen},
    tree::{find_readme, path_content, PathContent, TreeItem},
};
use highlight::Highlighter;
use std::path::PathBuf;
use std::sync::Arc;
use views::RepoCtx;

#[derive(Clone)]
struct AppState {
    root: Arc<PathBuf>,
    hl: Arc<Highlighter>,
    clone_limit: Arc<tokio::sync::Semaphore>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(all(not(feature = "jemalloc"), feature = "heap-analyse"))]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
#[cfg(all(feature = "jemalloc", not(feature = "heap-analyse")))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static malloc_conf: &[u8] =
    b"background_thread:true,dirty_decay_ms:1000,muzzy_decay_ms:1000,narenas:2\0";

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
}

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .thread_stack_size(1024 * 1024)
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime");

    rt.block_on(async_main());
}

async fn async_main() {
    #[cfg(all(not(feature = "jemalloc"), feature = "heap-analyse"))]
    let profiler = dhat::Profiler::new_heap();
    #[cfg(all(not(feature = "jemalloc"), feature = "heap-analyse"))]
    println!(
        "Heap profiler active: view results on https://nnethercote.github.io/dh_view/dh_view.html"
    );

    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let bind = args
        .next()
        .or_else(|| std::env::var("FERRO_BIND").ok())
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    if !root.is_dir() {
        eprintln!("Error: scan directory '{}' does not exist.", root.display());
        std::process::exit(1);
    }

    let state = AppState {
        root: Arc::new(root.clone()),
        hl: Arc::new(Highlighter::new()),
        clone_limit: Arc::new(tokio::sync::Semaphore::new(8)),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/{repo}", get(summary))
        .route("/{repo}/log", get(log))
        .route("/{repo}/refs", get(refs))
        .route("/{repo}/tree", get(tree_root))
        .route("/{repo}/tree/", get(tree_root))
        .route("/{repo}/tree/{*path}", get(tree_path))
        .route("/{repo}/blob/{*path}", get(blob))
        .route("/{repo}/commit/{id}", get(commit))
        .route("/{repo}/info/refs", get(GitClone::info_refs))
        .route("/{repo}/git-upload-pack", post(GitClone::upload_pack))
        .fallback(not_found)
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Cannot bind to {bind}: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "Listening on http://{bind} (git repository root: {})",
        root.display()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    #[cfg(all(not(feature = "jemalloc"), feature = "heap-analyse"))]
    drop(profiler); // erzwingt das Schreiben vor exit
}

fn page(m: maud::Markup) -> Response {
    Html(m.into_string()).into_response()
}

fn fail(code: StatusCode, msg: &str) -> Response {
    (
        code,
        Html(views::error_page(code.as_u16(), msg).into_string()),
    )
        .into_response()
}

async fn not_found() -> Response {
    fail(
        StatusCode::NOT_FOUND,
        "The page you requested does not exist.",
    )
}

fn map_err(e: GitError) -> Response {
    match e {
        GitError::NotFound => fail(StatusCode::NOT_FOUND, "Not found."),
        GitError::Empty => fail(StatusCode::OK, "This repository has no commits yet."),
        GitError::Git(s) => fail(StatusCode::INTERNAL_SERVER_ERROR, &s),
    }
}

fn render_readme(repo: &gix::Repository, items: &[TreeItem]) -> Option<String> {
    let name = find_readme(items)?;
    let bytes = match path_content(repo, None, &name).ok()? {
        PathContent::File { data, .. } => data,
        _ => return None,
    };
    let text = String::from_utf8_lossy(&bytes);
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        use pulldown_cmark::{html, Options, Parser};
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        opts.insert(Options::ENABLE_FOOTNOTES);
        let parser = Parser::new_ext(&text, opts);
        let mut out = String::new();
        html::push_html(&mut out, parser);
        Some(out)
    } else {
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        Some(format!("<pre>{escaped}</pre>"))
    }
}

async fn index(State(st): State<AppState>) -> Response {
    let repos = GitDiscover(&st.root);
    page(views::index(&repos, now_unix()))
}

async fn summary(State(st): State<AppState>, Path(repo): Path<String>) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    let commits = commit::log(&r, None, 0, 5).unwrap_or_default();
    let refs = git_refs(&r, 0, 5).unwrap_or(Refs {
        branches: vec![],
        tags: vec![],
    });

    let readme = match path_content(&r, None, "") {
        Ok(PathContent::Dir(items)) => render_readme(&r, &items),
        _ => None,
    };

    page(views::summary::summary(
        &ctx,
        &commits,
        &refs,
        readme,
        now_unix(),
    ))
}

async fn log(
    State(st): State<AppState>,
    Path(repo): Path<String>,
    RawQuery(q): RawQuery,
) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    const PAGE: usize = 50;
    let offset = parse_offset(q.as_deref());
    let mut commits = match commit::log(&r, None, offset, PAGE + 1) {
        Ok(c) => c,
        Err(e) => return map_err(e),
    };
    let has_more = commits.len() > PAGE;
    commits.truncate(PAGE);

    page(views::log::log_page(
        &ctx,
        &commits,
        offset,
        PAGE,
        has_more,
        now_unix(),
    ))
}

async fn refs(
    State(st): State<AppState>,
    Path(repo): Path<String>,
    RawQuery(q): RawQuery,
) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    const PAGE: usize = 15;
    let offset = parse_offset(q.as_deref());
    let mut refs = match git_refs(&r, offset, PAGE + 1) {
        Ok(x) => x,
        Err(e) => return map_err(e),
    };
    let has_more_branches = refs.branches.len() > PAGE;
    refs.branches.truncate(PAGE);
    let has_more_tags = refs.tags.len() > PAGE;
    refs.tags.truncate(PAGE);

    page(views::refs::refs_page(
        &ctx,
        &refs,
        offset,
        PAGE,
        has_more_branches,
        has_more_tags,
        now_unix(),
    ))
}

async fn tree_root(State(st): State<AppState>, Path(repo): Path<String>) -> Response {
    tree_inner(st, repo, String::new()).await
}

async fn tree_path(
    State(st): State<AppState>,
    Path((repo, path)): Path<(String, String)>,
) -> Response {
    tree_inner(st, repo, path).await
}

async fn tree_inner(st: AppState, repo: String, path: String) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    match path_content(&r, None, &path) {
        Ok(PathContent::Dir(items)) => {
            let readme = render_readme(&r, &items);
            page(views::tree::tree_page(&ctx, &path, &items, readme))
        }
        Ok(PathContent::File { .. }) => {
            Redirect::to(&format!("/{repo}/blob/{path}")).into_response()
        }
        Err(e) => map_err(e),
    }
}

async fn blob(State(st): State<AppState>, Path((repo, path)): Path<(String, String)>) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    match path_content(&r, None, &path) {
        Ok(PathContent::File { data, .. }) => {
            let size = data.len();
            if util::looks_binary(&data) {
                page(views::blob::blob_page(&ctx, &path, &[], size, true))
            } else {
                let text = String::from_utf8_lossy(&data);
                let lines = st.hl.highlight(&text, &path);
                page(views::blob::blob_page(&ctx, &path, &lines, size, false))
            }
        }
        Ok(PathContent::Dir(_)) => Redirect::to(&format!("/{repo}/tree/{path}")).into_response(),
        Err(e) => map_err(e),
    }
}

async fn commit(State(st): State<AppState>, Path((repo, id)): Path<(String, String)>) -> Response {
    let r = match GitOpen(&st.root, &repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: &repo,
        description: &desc,
    };

    match commit_diff(&r, &id) {
        Ok((info, parents, files)) => {
            page(views::commit::commit_page(&ctx, &info, &parents, &files))
        }
        Err(e) => map_err(e),
    }
}

fn read_desc(repo: &gix::Repository) -> String {
    let path = repo.git_dir().join("description");
    std::fs::read_to_string(path)
        .map(|s| {
            let s = s.trim();
            if s.starts_with("Unnamed repository") {
                String::new()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_default()
}

fn parse_offset(query: Option<&str>) -> usize {
    let Some(q) = query else { return 0 };
    for pair in q.split('&') {
        if let Some(v) = pair.strip_prefix("offset=") {
            return v.parse().unwrap_or(0);
        }
    }
    0
}
