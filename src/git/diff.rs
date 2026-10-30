use super::{
    commit::{CommitInfo, commit_info, resolve_commit},
    error::Result,
};
use gix::bstr::ByteSlice;

#[derive(PartialEq, Clone, Copy)]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
    Renamed,
}

pub enum LineKind {
    Ctx,
    Add,
    Del,
}

pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
}

pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
    pub binary: bool,
    pub too_large: bool,
    pub added: usize,
    pub removed: usize,
    pub hunks: Vec<Vec<DiffLine>>,
}

const MAX_DIFF_BYTES: usize = 128 * 1024;
const MAX_TOTAL_DIFF_BYTES: u64 = 256 * 1024;
const MAX_DIFF_FILES: usize = 64;

fn blob_size(repo: &gix::Repository, id: Option<gix::ObjectId>) -> u64 {
    id.and_then(|i| repo.find_header(i).ok())
        .map(|h| h.size())
        .unwrap_or(0)
}
fn blob_bytes(repo: &gix::Repository, id: Option<gix::ObjectId>) -> Vec<u8> {
    id.and_then(|i| repo.find_object(i).ok())
        .map(|o| o.detach().data)
        .unwrap_or_default()
}

pub fn commit_diff(
    repo: &gix::Repository,
    spec: &str,
) -> Result<(CommitInfo, Vec<String>, Vec<FileDiff>)> {
    let id = resolve_commit(repo, Some(spec))?;
    let commit = repo.find_object(id)?.try_into_commit()?;
    let info = commit_info(repo, id)?;

    let parents: Vec<gix::ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
    let parent_hex: Vec<String> = parents.iter().map(|p| p.to_string()).collect();

    let new_tree = commit.tree()?;
    let old_tree = match parents.first() {
        Some(pid) => repo.find_object(*pid)?.try_into_commit()?.tree()?,
        None => repo.empty_tree(),
    };

    struct Raw {
        path: String,
        old_path: Option<String>,
        kind: ChangeKind,
        old_id: Option<gix::ObjectId>,
        new_id: Option<gix::ObjectId>,
    }
    let mut raws: Vec<Raw> = Vec::new();

    let mut changes = old_tree.changes()?;
    changes.for_each_to_obtain_tree(
        &new_tree,
        |change| -> std::result::Result<gix::object::tree::diff::Action, std::convert::Infallible> {
            use gix::object::tree::diff::Change::*;
            let loc = change.location().to_str_lossy().to_string();
            match change {
                Addition {
                    location: loc,
                    entry_mode,
                    id,
                    ..
                } if entry_mode.is_blob() => raws.push(Raw {
                    path: loc.to_str_lossy().to_string(),
                    old_path: None,
                    kind: ChangeKind::Added,
                    old_id: None,
                    new_id: Some(id.detach()),
                }),
                Deletion {
                    location: loc,
                    entry_mode,
                    id,
                    ..
                } if entry_mode.is_blob() => raws.push(Raw {
                    path: loc.to_str_lossy().to_string(),
                    old_path: None,
                    kind: ChangeKind::Deleted,
                    old_id: Some(id.detach()),
                    new_id: None,
                }),
                Modification {
                    previous_id,
                    id,
                    entry_mode,
                    ..
                } if entry_mode.is_blob() => raws.push(Raw {
                    path: loc,
                    old_path: None,
                    kind: ChangeKind::Modified,
                    old_id: Some(previous_id.detach()),
                    new_id: Some(id.detach()),
                }),
                Rewrite {
                    source_location,
                    source_id,
                    id,
                    ..
                } => raws.push(Raw {
                    path: loc,
                    old_path: Some(source_location.to_str_lossy().to_string()),
                    kind: ChangeKind::Renamed,
                    old_id: Some(source_id.detach()),
                    new_id: Some(id.detach()),
                }),
                _ => {}
            }
            Ok(gix::object::tree::diff::Action::Continue(()))
        },
    )?;

    let mut files = Vec::new();
    let mut total: u64 = 0;
    if raws.len() > MAX_DIFF_FILES {
        return Ok((info, parent_hex, files));
    }
    for (n, raw) in raws.into_iter().enumerate() {
        let old_size = blob_size(repo, raw.old_id);
        let new_size = blob_size(repo, raw.new_id);
        let too_large = old_size > MAX_DIFF_BYTES as u64
            || new_size > MAX_DIFF_BYTES as u64
            || n >= MAX_DIFF_FILES
            || total + old_size + new_size > MAX_TOTAL_DIFF_BYTES;

        let (binary, hunks, added, removed) = if too_large {
            (false, Vec::new(), 0, 0)
        } else {
            total += old_size + new_size;
            let old = blob_bytes(repo, raw.old_id);
            let new = blob_bytes(repo, raw.new_id);
            let binary = (old.len() > 0 && crate::util::looks_binary(&old))
                || (new.len() > 0 && crate::util::looks_binary(&new));
            if binary {
                (true, Vec::new(), 0, 0)
            } else {
                let (h, a, r) = build_hunks(&old, &new);
                (false, h, a, r)
            }
        };

        files.push(FileDiff {
            path: raw.path,
            old_path: raw.old_path,
            kind: raw.kind,
            binary,
            too_large,
            added,
            removed,
            hunks,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok((info, parent_hex, files))
}

fn build_hunks(old: &[u8], new: &[u8]) -> (Vec<Vec<DiffLine>>, usize, usize) {
    let old_s = String::from_utf8_lossy(old);
    let new_s = String::from_utf8_lossy(new);
    let diff = similar::TextDiff::from_lines(old_s.as_ref(), new_s.as_ref());

    let mut hunks = Vec::new();
    let mut added = 0;
    let mut removed = 0;

    for group in diff.grouped_ops(3) {
        let mut lines = Vec::new();
        for op in group {
            for change in diff.iter_changes(&op) {
                let text = change.value().trim_end_matches('\n').to_string();
                let line = match change.tag() {
                    similar::ChangeTag::Equal => DiffLine {
                        kind: LineKind::Ctx,
                        old_no: change.old_index().map(|i| i + 1),
                        new_no: change.new_index().map(|i| i + 1),
                        text,
                    },
                    similar::ChangeTag::Delete => {
                        removed += 1;
                        DiffLine {
                            kind: LineKind::Del,
                            old_no: change.old_index().map(|i| i + 1),
                            new_no: None,
                            text,
                        }
                    }
                    similar::ChangeTag::Insert => {
                        added += 1;
                        DiffLine {
                            kind: LineKind::Add,
                            old_no: None,
                            new_no: change.new_index().map(|i| i + 1),
                            text,
                        }
                    }
                };
                lines.push(line);
            }
        }
        hunks.push(lines);
    }
    (hunks, added, removed)
}
