use super::{RepoCtx, render_html};
use crate::git::{
    commit::CommitInfo, diff::ChangeKind, diff::DiffLine, diff::FileDiff, diff::LineKind,
};
use crate::util::format_time;
use maud::{Markup, html};

fn opt_no(n: Option<usize>) -> String {
    n.map(|v| v.to_string()).unwrap_or_default()
}

fn diff_row(line: &DiffLine) -> Markup {
    let (cls, sign) = match line.kind {
        LineKind::Add => ("add", "+"),
        LineKind::Del => ("del", "−"),
        LineKind::Ctx => ("", " "),
    };
    html! {
        tr class=(cls) {
            td .no { (opt_no(line.old_no)) }
            td .no { (opt_no(line.new_no)) }
            td .cl { (sign) (line.text) }
        }
    }
}

fn change_badge(kind: ChangeKind) -> Markup {
    let (c, t) = match kind {
        ChangeKind::Added => ("A", "added"),
        ChangeKind::Deleted => ("D", "deleted"),
        ChangeKind::Modified => ("M", "modified"),
        ChangeKind::Renamed => ("R", "renamed"),
    };
    html! { span class={ "badge " (c) } title=(t) { (c) } }
}

pub fn commit_page(
    ctx: &RepoCtx,
    info: &CommitInfo,
    parents: &[String],
    files: &[FileDiff],
) -> Markup {
    let total_add: usize = files.iter().map(|f| f.added).sum();
    let total_del: usize = files.iter().map(|f| f.removed).sum();
    let body = html! {
        .meta {
            .subject { (info.title) }
            dl { "\n"
                "\t" dt { "commit" } dd .mono-id { (info.id) } "\n"
                "\t" dt { "author" } dd { (info.author_name) " <" (info.author_email) ">" } "\n"
                "\t" dt { "date" } dd { (format_time(info.time_unix, info.time_offset)) } "\n"
                @for p in parents {
                    "\t" dt { "parent" } "\n"
                    "\t" dd { a .mono-id href={ "/" (ctx.name) "/commit/" (p) } { (p) } } "\n"
                }
            }
        }
        @if files.len() > 0 {
            "\n"
            .section-title {
                (files.len()) " files changed · "
                @if total_add > 0 {
                    span .stat-add { "+" (total_add) } " "
                }
                @if total_del > 0 {
                    span .stat-del { "−" (total_del) }
                }
            } "\n"
        } @else {
            .section-title {
                "Too many files changed"
            }
        }
        @for f in files {
            .filediff {
                .fh {
                    (change_badge(f.kind))
                    span {
                        @if let Some(old) = &f.old_path {
                            (old) " → " (f.path)
                        } @else { (f.path) }
                    }
                    @if !f.too_large && !f.binary {
                        "\n"
                        span .mut {
                            "\t" span .stat-add { "+" (f.added) } " "
                            "\t" span .stat-del { "−" (f.removed) }
                        }
                    }
                } "\n"
                @if f.binary {
                    p .note { "Binary file" }
                } @else if f.too_large {
                    p .note { "File too large for inline diff" }
                } @else if f.hunks.is_empty() {
                    p .note { "No text changes" }
                } @else {
                    .diffwrap {
                        table .diff {
                            tbody {
                                @for (hi, hunk) in f.hunks.iter().enumerate() {
                                    @if hi > 0 {
                                        "\t" tr .hsep { td colspan="3" { "⋮" } } "\n"
                                    }
                                    @for line in hunk {
                                        "\t" (diff_row(line)) "\n"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "\n";
        }
    };
    render_html(
        &format!("{} — commit {}", ctx.name, info.short),
        Some(ctx),
        "log",
        body,
    )
}
