use super::{breadcrumbs, human_size, render_html, RepoCtx};
use maud::{html, Markup, PreEscaped};

pub fn blob_page(
    ctx: &RepoCtx,
    path: &str,
    lines: &[String],
    size: usize,
    is_binary: bool,
) -> Markup {
    let body = html! {
        (breadcrumbs(ctx, "blob", path))
        .blobhead {
            span { (path.rsplit('/').next().unwrap_or(path)) }
            span .mut { (lines.len()) " lines · " (human_size(size)) }
        }
        @if is_binary {
            .code { p .note { "Binary file — no preview available." } }
        } @else {
            .code {
                table .src {
                    tbody {
                        @for (i, line) in lines.iter().enumerate() {
                            @let n = i + 1;
                            tr id={ "L" (n) } {
                                td .ln { a href={ "#L" (n) } { (n) } }
                                td .cl { (PreEscaped(line.clone())) }
                            }
                        }
                    }
                }
            }
        }
    };
    render_html(&format!("{} — {}", ctx.name, path), Some(ctx), "tree", body)
}
