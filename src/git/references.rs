use super::{
    commit::commit_info,
    error::{Error, Result},
};
use gix::bstr::ByteSlice;

pub struct RefInfo {
    pub name: String,
    pub target: String,
    pub short: String,
    pub time_unix: i64,
    pub time_offset: i32,
    pub subject: String,
}

pub struct Refs {
    pub branches: Vec<RefInfo>,
    pub tags: Vec<RefInfo>,
}

pub fn refs(repo: &gix::Repository, offset: usize, limit: usize) -> Result<Refs> {
    let mut branches = Vec::new();
    let mut tags = Vec::new();
    let platform = repo.references()?;
    for r in platform.all()? {
        let mut r = r.map_err(|e| Error::Git(e.to_string()))?;
        let full = r.name().as_bstr().to_str_lossy().to_string();
        let target_id = match r.peel_to_id() {
            Ok(id) => id.detach(),
            Err(_) => continue,
        };
        let (unix, offset, subject) = match commit_info(repo, target_id) {
            Ok(ci) => (ci.time_unix, ci.time_offset, ci.title),
            Err(_) => (0, 0, String::new()),
        };
        let target = target_id.to_string();
        let short = target[..target.len().min(10)].to_string();

        if let Some(name) = full.strip_prefix("refs/heads/") {
            branches.push(RefInfo {
                name: name.to_string(),
                target,
                short,
                time_unix: unix,
                time_offset: offset,
                subject,
            });
        } else if let Some(name) = full.strip_prefix("refs/tags/") {
            tags.push(RefInfo {
                name: name.to_string(),
                target,
                short,
                time_unix: unix,
                time_offset: offset,
                subject,
            });
        }
    }
    branches.sort_by(|a, b| b.time_unix.cmp(&a.time_unix));
    tags.sort_by(|a, b| b.time_unix.cmp(&a.time_unix));
    let branches: Vec<RefInfo> = branches.into_iter().skip(offset).take(limit).collect();
    let tags = tags.into_iter().skip(offset).take(limit).collect();
    Ok(Refs { branches, tags })
}
