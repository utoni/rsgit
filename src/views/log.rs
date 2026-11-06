use super::{RepoCtx, render_html};
use crate::git::commit::CommitInfo;
use crate::util::{format_date, format_time};
use maud::{Markup, html};

pub fn log_page(
    ctx: &RepoCtx,
    commits: &[CommitInfo],
    offset: usize,
    page: usize,
    has_more: bool,
    now: i64,
) -> Markup {
    let body = html! {
        "\n\t"
        .section-title { "Commit history" }
        "\n\t"
        table .grid {
            thead { tr { th { "Commit" } th { "Message" } th { "Author" } th { "Date" } }}
            "\n\t"
            tbody {
                @for c in commits {
                    "\n\t\t"
                    tr {
                        td { a .mono-id href={ "/" (ctx.name) "/commit/" (c.id) } { (c.short) } }
                        td { a href={ "/" (ctx.name) "/commit/" (c.id) } { (c.title) } }
                        td .mut { (c.author_name) }
                        td .mut title=(format_time(c.time_unix, c.time_offset)) {
                            (format_date(c.time_unix, c.time_offset))
                        }
                    }
                }
                "\n\t"
            }
            "\n"
        }
        div .pager {
            @if offset > 0 {
                a href={ "/" (ctx.name) "/log?offset=" (offset.saturating_sub(page)) } { "← newer" }
            }
            @if has_more {
                a href={ "/" (ctx.name) "/log?offset=" (offset + page) } { "older →" }
            }
        }
        @let _ = now;
    };
    render_html(&format!("{} — log", ctx.name), Some(ctx), "log", body)
}
