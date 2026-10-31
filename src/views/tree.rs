use super::{RepoCtx, TreeItem, breadcrumbs, human_size, render_html, rev_query};
use maud::{Markup, PreEscaped, html};

fn mode_str(it: &TreeItem) -> &'static str {
    if it.is_dir {
        "drwxr-xr-x"
    } else if it.is_link {
        "lrwxrwxrwx"
    } else if it.is_exec {
        "-rwxr-xr-x"
    } else {
        "-rw-r--r--"
    }
}

fn parent_path(path: &str) -> String {
    match path.trim_end_matches('/').rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn join_path(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }
}

pub fn tree_page(
    ctx: &RepoCtx,
    path: &str,
    rev: Option<&str>,
    items: &[TreeItem],
    readme_html: Option<String>,
) -> Markup {
    let body = html! {
        "\n\t"
        .section-title { "Files" }
        "\n\t"
        (breadcrumbs(ctx, "tree", path, rev))
        "\n\t"
        table .grid {
            thead { tr { th { "Mode" } th { "Name" } th .num { "Size" } }}
            "\n\t"
            tbody {
                @if !path.is_empty() {
                    @let parent = parent_path(path);
                    "\n\t\t"
                    tr {
                        td .faint { "d---------" }
                        td { a href={ "/" (ctx.name) "/tree/" (parent) (rev_query(rev)) } { ".." } }
                        td {}
                    }
                }
                @for it in items {
                    "\n\t\t"
                    tr {
                        td .faint .nowrap { (mode_str(it)) }
                        td {
                            span .icon .dir[it.is_dir] {
                                @if it.is_dir { "▸" } @else if it.is_link { "↪" } @else { "·" }
                            }
                            " "
                            @let child = join_path(path, &it.name);
                            @if it.is_dir {
                                a href={ "/" (ctx.name) "/tree/" (child) (rev_query(rev)) } { (it.name) "/" }
                            } @else {
                                a href={ "/" (ctx.name) "/blob/" (child) (rev_query(rev)) } { (it.name) }
                            }
                        }
                        td .num .nowrap { @if it.is_dir { "" } @else { (human_size(it.size)) } }
                    }
                }
                "\n\t"
            }
            "\n"
        }
        @if let Some(html) = readme_html {
            .section-title { "README" }
            .readme { (PreEscaped(html)) }
        }
    };
    render_html(
        &format!(
            "{} — {}",
            ctx.name,
            if path.is_empty() { "tree" } else { path }
        ),
        Some(ctx),
        "tree",
        body,
    )
}
