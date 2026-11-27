use crate::git::error::{Error as GitError, Result as GitResult};
use gix::sec::Trust;
use std::path::{Path, PathBuf};

pub struct RepoSummary {
    pub name: String,
    pub git_dir: PathBuf,
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
            let trust = Trust::from_path_ownership(&candidate)?;
            if let Ok(repo) = gix::open_opts(&candidate, gix::open::Options::default().with(trust))
            {
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
        let Ok(trust) = Trust::from_path_ownership(path.as_path()) else {
            continue;
        };
        let Ok(repo) = gix::open_opts(&path, gix::open::Options::default().with(trust)) else {
            continue;
        };
        let file_name = entry.file_name().to_string_lossy().to_string();
        let name = file_name
            .strip_suffix(".git")
            .unwrap_or(&file_name)
            .to_string();

        let mut last_commit_unix: Option<i64> = None;
        let mut last_commit_offset = 0;
        match repo.head_commit() {
            Ok(head_commit) => {
                if let Ok(head_commit_ts) = head_commit.time() {
                    last_commit_unix = Some(head_commit_ts.seconds);
                    last_commit_offset = head_commit_ts.offset;
                }
            }
            Err(_) => (),
        }
        let Ok(refs) = repo.references() else {
            continue;
        };
        let Ok(branches) = refs.local_branches() else {
            continue;
        };
        for reference in branches.take(10) {
            let Ok(mut head) = reference else {
                continue;
            };
            let Ok(commit) = head.peel_to_commit() else {
                continue;
            };
            let Ok(time) = commit.time() else {
                continue;
            };
            last_commit_unix = match last_commit_unix {
                Some(old_time) => {
                    if old_time < time.seconds {
                        last_commit_offset = time.offset;
                        Some(time.seconds)
                    } else {
                        Some(old_time)
                    }
                }
                None => {
                    last_commit_offset = time.offset;
                    Some(time.seconds)
                }
            };
        }

        out.push(RepoSummary {
            name,
            git_dir: repo.git_dir().to_path_buf(),
            description: read_description(&repo),
            last_commit_unix,
            last_commit_offset,
        });
    }
    out.sort_by(|a, b| b.last_commit_unix.cmp(&a.last_commit_unix));
    out
}
