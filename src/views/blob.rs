use super::{RepoCtx, breadcrumbs, human_size, render_html, rev_query};
use maud::{Markup, PreEscaped, html};

pub fn blob_page(
    ctx: &RepoCtx,
    path: &str,
    rev: Option<&str>,
    lines: &[String],
    size: usize,
    is_binary: bool,
    too_large: bool,
) -> Markup {
    let body = html! {
        (breadcrumbs(ctx, "blob", path, rev))
        "\n"
        .blobhead {
            span { (path.rsplit('/').next().unwrap_or(path)) }
            span .mut {
                @if let Some(r) = rev { "@" (&r[..r.len().min(10)]) " · " }
                @if !too_large { (lines.len()) " lines · " }
                (human_size(size)) " · "
                a href={ "/" (ctx.name) "/raw/" (path) (rev_query(rev)) } { "raw" } }
        }
        @if is_binary {
            "\n"
            .code { p .note {
                @if too_large { "File too large for preview — use the raw link." }
                @else { "Binary file — no preview available." }
            } }
        } @else {
            "\n"
            .code {
                table .src {
                    tbody {
                        @for (i, line) in lines.iter().enumerate() {
                            "\n\t"
                            @let n = i + 1;
                            tr id={ "L" (n) } {
                                td .ln { a href={ "#L" (n) } { (n) } }
                                td .cl { (PreEscaped(line.clone())) }
                            }
                        }
                        "\n"
                    }
                }
            }
        }
    };
    render_html(&format!("{} — {}", ctx.name, path), Some(ctx), "tree", body)
}
