use super::{
    commit::{commit_info, resolve_commit, CommitInfo},
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

const MAX_DIFF_BYTES: usize = 512 * 1024;

fn blob_bytes(repo: &gix::Repository, id: gix::ObjectId) -> Vec<u8> {
    repo.find_object(id)
        .ok()
        .map(|o| o.into_blob().data.clone())
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
    for raw in raws {
        let old = raw.old_id.map(|i| blob_bytes(repo, i)).unwrap_or_default();
        let new = raw.new_id.map(|i| blob_bytes(repo, i)).unwrap_or_default();

        let binary = crate::util::looks_binary(&old) || crate::util::looks_binary(&new);
        let too_large = old.len() > MAX_DIFF_BYTES || new.len() > MAX_DIFF_BYTES;

        let (hunks, added, removed) = if binary || too_large {
            (Vec::new(), 0, 0)
        } else {
            build_hunks(&old, &new)
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
