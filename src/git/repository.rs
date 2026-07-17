use crate::git::error::{Error as GitError, Result as GitResult};
use gix::bstr::ByteSlice;
use std::path::Path;

pub struct RepoSummary {
    pub name: String,
    pub description: String,
    pub last_commit_unix: Option<i64>,
    pub last_commit_offset: i32,
}

pub fn valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < 256
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && !name.contains("..")
}

pub fn open(root: &Path, name: &str) -> GitResult<gix::Repository> {
    if !valid_repo_name(name) {
        return Err(GitError::NotFound);
    }
    for candidate in [root.join(name), root.join(format!("{name}.git"))] {
        if candidate.exists() {
            if let Ok(repo) = gix::open(&candidate) {
                return Ok(repo);
            }
        }
    }
    Err(GitError::NotFound)
}

fn read_description(repo: &gix::Repository) -> String {
    let path = repo.git_dir().join("description");
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let s = s.trim();
            if s.is_empty() || s.starts_with("Unnamed repository") {
                String::new()
            } else {
                s.to_string()
            }
        }
        Err(_) => String::new(),
    }
}

pub fn discover(root: &Path) -> Vec<RepoSummary> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(repo) = gix::open(&path) else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().to_string();
        let name = file_name
            .strip_suffix(".git")
            .unwrap_or(&file_name)
            .to_string();

        let (unix, offset, _) = match repo.head_commit() {
            Ok(c) => {
                let t = c.time().unwrap_or_default();
                let title = c
                    .message()
                    .map(|m| m.title.to_str_lossy().trim().to_string())
                    .unwrap_or_default();
                (Some(t.seconds), t.offset, title)
            }
            Err(_) => (None, 0, String::new()),
        };

        out.push(RepoSummary {
            name,
            description: read_description(&repo),
            last_commit_unix: unix,
            last_commit_offset: offset,
        });
    }
    out.sort_by(|a, b| b.last_commit_unix.cmp(&a.last_commit_unix));
    out
}
