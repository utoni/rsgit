pub mod blob;
pub mod commit;
pub mod css;
pub mod log;
pub mod refs;
pub mod summary;
pub mod tree;

use super::git::{repository::RepoSummary, tree::TreeItem};
use crate::util::{format_time, human_size, relative};
use crate::views::css::CSS;
use maud::{DOCTYPE, Markup, PreEscaped, html};

pub struct RepoCtx<'a> {
    pub name: &'a str,
    pub description: &'a str,
}

fn render_html(title: &str, repo: Option<&RepoCtx>, active: &str, body: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="theme-color" content="#121316";
                title { (title) }
                style { (PreEscaped(CSS)) }
            }
            "\n"
            body {
                header .topbar {
                    .inner {
                        "\n\t"
                        span .wordmark {
                            a href="/" { "rsgit" }
                        }
                        "\n\t"
                        @if let Some(r) = repo {
                            span .crumbs {
                                span .sep { "/" }
                                a href={ "/" (r.name) } { (r.name) }
                            }
                        }
                        span .tagline { "A hyper-hyperfast web frontend for git repositories written in Rust." }
                        "\n"
                    }
                }
                "\n"
                @if let Some(r) = repo {
                    nav .tabs {
                        "\n\t"
                        a .active[active=="summary"] href={ "/" (r.name) } { "summary" }
                        "\n\t"
                        a .active[active=="log"] href={ "/" (r.name) "/log" } { "log" }
                        "\n\t"
                        a .active[active=="tree"] href={ "/" (r.name) "/tree" } { "tree" }
                        "\n\t"
                        a .active[active=="refs"] href={ "/" (r.name) "/refs" } { "refs" }
                        "\n"
                    }
                }
                "\n"
                main { (body) }
                "\n"
                footer {
                    "\n\t"
                    a href="https://github.com/utoni/rsgit" { "rsgit" }
                    ". Powered by "
                    "\n\t"
                    a href="https://github.com/GitoxideLabs/gitoxide" { "gitoxide" }
                    " and "
                    "\n\t"
                    a href="https://github.com/tree-sitter/tree-sitter" { "tree-sitter" }
                    "."
                    "\n"
                }
            }
        }
    }
}

pub fn index(repos: &[RepoSummary], now: i64) -> Markup {
    let body = html! {
        "\n\t"
        .section-title { "Repositories" }
        "\n\t"
        @if repos.is_empty() {
            p .desc { "No repositories found in the scan directory." }
        } @else {
            .table-scroll { table .grid {
                thead { tr {
                    th { "Name" } th { "Description" } th { "Last activity" }
                }}
                "\n\t"
                tbody {
                    @for r in repos {
                        "\n\t\t"
                        tr {
                            td .repo-name { a href={ "/" (r.name) } { (r.name) } }
                            td .desc { (r.description) }
                            td .nowrap .mut {
                                @if let Some(t) = r.last_commit_unix {
                                    span title=(format_time(t, r.last_commit_offset)) {
                                        (relative(t, now))
                                    }
                                } @else { "—" }
                            }
                        }
                    }
                    "\n\t"
                }
                "\n"
            } }
        }
    };
    render_html("rsgit — Repositories", None, "", body)
}

pub fn rev_query(rev: Option<&str>) -> String {
    match rev {
        Some(r) => format!(
            "?rev={}",
            percent_encoding::utf8_percent_encode(r, percent_encoding::NON_ALPHANUMERIC)
        ),
        None => String::new(),
    }
}

fn breadcrumbs(ctx: &RepoCtx, kind: &str, path: &str, rev: Option<&str>) -> Markup {
    let q = rev_query(rev);
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut crumbs: Vec<(String, Option<String>)> = Vec::new();
    let mut acc = String::new();
    for (i, p) in parts.iter().enumerate() {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(p);
        let is_last = i + 1 == parts.len();
        if is_last && kind == "blob" {
            crumbs.push((p.to_string(), None));
        } else {
            crumbs.push((p.to_string(), Some(acc.clone())));
        }
    }
    html! {
        span .crumbs {
            a href={ "/" (ctx.name) "/tree" (q) } { "tree" }
            @for (label, link) in &crumbs {
                span .sep { "/" }
                @match link {
                    Some(href) => a href={ "/" (ctx.name) "/tree/" (href) (q) } { (label) },
                    None => (label),
                }
            }
        }
    }
}

pub fn error_page(code: u16, msg: &str) -> Markup {
    let body = html! {
        .section-title { (code) }
        p .desc { (msg) }
        p { a href="/" { "← back to index" } }
    };
    render_html(&format!("rsgit — {code}"), None, "", body)
}
