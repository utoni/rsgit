use super::{RepoCtx, render_html};
use crate::git::references::{RefInfo, Refs};
use crate::util::{format_time, relative};
use maud::{Markup, html};

pub fn ref_table(
    ctx: &RepoCtx,
    refs: &[RefInfo],
    offset: usize,
    page: usize,
    has_more: bool,
    now: i64,
) -> Markup {
    html! {
        "\n\t"
        table .grid {
            thead { tr { th { "Name" } th { "Target" } th { "Subject" } th { "Date" } }}
            "\n\t"
            tbody {
                @for r in refs {
                    "\n\t\t"
                    tr {
                        td .nowrap { (r.name) }
                        td .nowrap { a .mono-id href={ "/" (ctx.name) "/commit/" (r.target) } { (r.short) } }
                        td .mut { (r.subject) }
                        td .nowrap .mut title=(format_time(r.time_unix, r.time_offset)) {
                            (relative(r.time_unix, now))
                        }
                    }
                }
                "\n\t"
            }
            "\n"
        }
        div .pager {
            @if offset > 0 {
                "\n"
                a href={ "/" (ctx.name) "/refs?offset=" (offset.saturating_sub(page)) } { "← newer" }
            }
            @if has_more {
                "\n"
                a href={ "/" (ctx.name) "/refs?offset=" (offset + page) } { "older →" }
            }
        }
        @let _ = now;
    }
}

pub fn refs_page(
    ctx: &RepoCtx,
    refs: &Refs,
    offset: usize,
    page: usize,
    has_more_branches: bool,
    has_more_tags: bool,
    now: i64,
) -> Markup {
    let body = html! {
        .section-title { "Branches" }
        @if refs.branches.is_empty() { p .desc { "none" } }
        @else { (ref_table(ctx, &refs.branches, offset, page, has_more_branches, now)) }
        .section-title { "Tags" }
        @if refs.tags.is_empty() { p .desc { "none" } }
        @else { (ref_table(ctx, &refs.tags, offset, page, has_more_tags, now)) }
    };
    render_html(&format!("{} — refs", ctx.name), Some(ctx), "refs", body)
}
