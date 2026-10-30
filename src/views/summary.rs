use super::{RepoCtx, render_html};
use crate::git::commit::CommitInfo;
use crate::git::references::Refs;
use crate::util::{format_time, relative};
use crate::views::refs::ref_table;
use maud::{Markup, PreEscaped, html};

pub fn summary(
    ctx: &RepoCtx,
    commits: &[CommitInfo],
    refs: &Refs,
    readme_html: Option<String>,
    now: i64,
) -> Markup {
    let body = html! {
        @if !ctx.description.is_empty() {
            p .desc { (ctx.description) }
        }

        "\n\t"
        .section-title { "Latest commits" }
        "\n\t"
        table .grid {
            thead { tr { th { "Commit" } th { "Message" } th { "Author" } th { "Date" } }}
            "\n\t"
            tbody {
                @for c in commits {
                    "\n\t\t"
                    tr {
                        td .nowrap { a .mono-id href={ "/" (ctx.name) "/commit/" (c.id) } { (c.short) } }
                        td { a href={ "/" (ctx.name) "/commit/" (c.id) } { (c.title) } }
                        td .nowrap .mut { (c.author_name) }
                        td .nowrap .mut title=(format_time(c.time_unix, c.time_offset)) {
                            (relative(c.time_unix, now))
                        }
                    }
                }
                "\n\t"
            }
            "\n"
        }
        p { a href={ "/" (ctx.name) "/log" } { "→ full history" } }

        @if !refs.branches.is_empty() {
            "\n"
            .section-title { "Branches" }
            (ref_table(ctx, &refs.branches, 0, 0, false, now))
        }
        @if !refs.tags.is_empty() {
            "\n"
            .section-title { "Tags" }
            (ref_table(ctx, &refs.tags, 0, 0, false, now))
        }

        @if let Some(html) = readme_html {
            "\n"
            .section-title { "README" }
            .readme { (PreEscaped(html)) }
        }
    };
    render_html(&format!("{} — rsgit", ctx.name), Some(ctx), "summary", body)
}
