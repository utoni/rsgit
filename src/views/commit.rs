use super::{render_html, RepoCtx};
use crate::git::{
    commit::CommitInfo, diff::ChangeKind, diff::DiffLine, diff::FileDiff, diff::LineKind,
};
use crate::util::format_time;
use maud::{html, Markup};

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
            dl {
                dt { "commit" } dd .mono-id { (info.id) }
                dt { "author" } dd { (info.author_name) " <" (info.author_email) ">" }
                dt { "date" } dd { (format_time(info.time_unix, info.time_offset)) }
                @for p in parents {
                    dt { "parent" }
                    dd { a .mono-id href={ "/" (ctx.name) "/commit/" (p) } { (p) } }
                }
            }
        }
        .section-title {
            (files.len()) " files changed · "
            span .stat-add { "+" (total_add) } " "
            span .stat-del { "−" (total_del) }
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
                        span .mut {
                            span .stat-add { "+" (f.added) } " "
                            span .stat-del { "−" (f.removed) }
                        }
                    }
                }
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
                                        tr .hsep { td colspan="3" { "⋮" } }
                                    }
                                    @for line in hunk {
                                        (diff_row(line))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    render_html(
        &format!("{} — commit {}", ctx.name, info.short),
        Some(ctx),
        "log",
        body,
    )
}
