mod git;
mod highlight;
mod util;
mod views;

use async_lock::Semaphore;
use futures_lite::{FutureExt, StreamExt};
use http_types::{Method, Request, Response, StatusCode, mime};
use std::path::PathBuf;
use std::sync::Arc;

use git::{
    clone as GitClone, commit,
    diff::commit_diff,
    error::Error as GitError,
    references::{Refs, refs as git_refs},
    repository::{discover as GitDiscover, open as GitOpen},
    tree::{PathContent, TreeItem, find_readme, path_content},
};
use highlight::Highlighter;
use views::RepoCtx;

#[derive(Clone)]
pub struct AppState {
    pub root: Arc<PathBuf>,
    pub hl: Arc<Highlighter>,
    pub clone_limit: Arc<Semaphore>,
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

#[cfg(all(feature = "jemalloc", not(feature = "heap-analyse")))]
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
pub static malloc_conf: &[u8] =
    b"background_thread:true,dirty_decay_ms:1000,muzzy_decay_ms:1000,narenas:2\0";

fn main() -> std::io::Result<()> {
    smol::block_on(async_main())
}

async fn async_main() -> std::io::Result<()> {
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
        .or_else(|| std::env::var("RSGIT_BIND").ok())
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    if !root.is_dir() {
        eprintln!("Error: scan directory '{}' does not exist.", root.display());
        std::process::exit(1);
    }

    let state = AppState {
        root: Arc::new(root.clone()),
        hl: Arc::new(Highlighter::new()),
        clone_limit: Arc::new(Semaphore::new(8)),
    };

    let listener = match smol::net::TcpListener::bind(&bind).await {
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

    let serve = async {
        loop {
            let (stream, _) = listener.accept().await?;
            let state = state.clone();
            smol::spawn(handle_conn(stream, state)).detach();
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };

    let shutdown = async {
        let mut signals = async_signal::Signals::new([async_signal::Signal::Int])?;
        signals.next().await;
        eprintln!("shutting down");
        Ok::<(), std::io::Error>(())
    };

    let result = serve.or(shutdown).await;

    #[cfg(all(not(feature = "jemalloc"), feature = "heap-analyse"))]
    drop(profiler);

    result
}

async fn handle_conn(stream: smol::net::TcpStream, state: AppState) {
    let _ = async_h1::accept(stream, move |req| {
        let state = state.clone();
        async move { Ok(route(state, req).await) }
    })
    .await;
}

async fn route(st: AppState, req: Request) -> Response {
    let method = req.method();
    let query = req.url().query().map(str::to_string);

    let segs: Vec<String> = req
        .url()
        .path()
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .into_owned()
        })
        .collect();
    let s: Vec<&str> = segs.iter().map(String::as_str).collect();

    match s.as_slice() {
        [] => index(&st).await,
        [repo, "info", "refs"] if method == Method::Get => {
            GitClone::info_refs(&st, repo, query.as_deref()).await
        }
        [repo, "git-upload-pack"] if method == Method::Post => {
            GitClone::upload_pack(&st, repo, req).await
        }
        [repo] => summary(&st, repo).await,
        [repo, "log"] => log(&st, repo, query.as_deref()).await,
        [repo, "refs"] => refs(&st, repo, query.as_deref()).await,
        [repo, "tree"] => tree(&st, repo, "").await,
        [repo, "tree", rest @ ..] => tree(&st, repo, &rest.join("/")).await,
        [repo, "blob", rest @ ..] => blob(&st, repo, &rest.join("/")).await,
        [repo, "commit", id] => commit_view(&st, repo, id).await,
        _ => not_found(),
    }
}

fn page(m: maud::Markup) -> Response {
    let mut res = Response::new(StatusCode::Ok);
    res.set_content_type(mime::HTML);
    res.set_body(m.into_string());
    res
}

fn fail(code: StatusCode, msg: &str) -> Response {
    let mut res = Response::new(code);
    res.set_content_type(mime::HTML);
    res.set_body(views::error_page(u16::from(code), msg).into_string());
    res
}

fn redirect(to: &str) -> Response {
    let mut res = Response::new(StatusCode::SeeOther);
    res.insert_header("location", to);
    res
}

fn not_found() -> Response {
    fail(
        StatusCode::NotFound,
        "The page you requested does not exist.",
    )
}

fn map_err(e: GitError) -> Response {
    match e {
        GitError::NotFound => fail(StatusCode::NotFound, "Not found."),
        GitError::Empty => fail(StatusCode::Ok, "This repository has no commits yet."),
        GitError::Git(s) => fail(StatusCode::InternalServerError, &s),
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
        use pulldown_cmark::{Options, Parser, html};
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

async fn index(st: &AppState) -> Response {
    let repos = GitDiscover(&st.root);
    page(views::index(&repos, now_unix()))
}

async fn summary(st: &AppState, repo: &str) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
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

async fn log(st: &AppState, repo: &str, query: Option<&str>) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
        description: &desc,
    };

    const PAGE: usize = 50;
    let offset = parse_offset(query);
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

async fn refs(st: &AppState, repo: &str, query: Option<&str>) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
        description: &desc,
    };

    const PAGE: usize = 15;
    let offset = parse_offset(query);
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

async fn tree(st: &AppState, repo: &str, path: &str) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
        description: &desc,
    };

    match path_content(&r, None, path) {
        Ok(PathContent::Dir(items)) => {
            let readme = render_readme(&r, &items);
            page(views::tree::tree_page(&ctx, path, &items, readme))
        }
        Ok(PathContent::File { .. }) => redirect(&format!("/{repo}/blob/{path}")),
        Err(e) => map_err(e),
    }
}

async fn blob(st: &AppState, repo: &str, path: &str) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
        description: &desc,
    };

    match path_content(&r, None, path) {
        Ok(PathContent::File {
            data,
            size,
            too_large,
        }) => {
            if too_large || util::looks_binary(&data) {
                page(views::blob::blob_page(&ctx, path, &[], size, true))
            } else {
                let text = String::from_utf8_lossy(&data);
                let lines = st.hl.highlight(&text, path);
                page(views::blob::blob_page(&ctx, path, &lines, size, false))
            }
        }
        Ok(PathContent::Dir(_)) => redirect(&format!("/{repo}/tree/{path}")),
        Err(e) => map_err(e),
    }
}

async fn commit_view(st: &AppState, repo: &str, id: &str) -> Response {
    let r = match GitOpen(&st.root, repo) {
        Ok(r) => r,
        Err(e) => return map_err(e),
    };
    let desc = read_desc(&r);
    let ctx = RepoCtx {
        name: repo,
        description: &desc,
    };

    match commit_diff(&r, id) {
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
