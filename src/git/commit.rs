use crate::git::error::{Error, Result};
use gix::bstr::ByteSlice;

pub struct CommitInfo {
    pub id: String,
    pub short: String,
    pub title: String,
    pub author_name: String,
    pub author_email: String,
    pub time_unix: i64,
    pub time_offset: i32,
}

pub fn commit_info(repo: &gix::Repository, id: gix::ObjectId) -> Result<CommitInfo> {
    let commit = repo.find_object(id)?.try_into_commit()?;
    let msg = commit.message()?;
    let author = commit.author()?;
    let time = commit.time()?;
    Ok(CommitInfo {
        id: id.to_string(),
        short: id.to_string()[..id.to_string().len().min(10)].to_string(),
        title: msg.title.to_str_lossy().trim().to_string(),
        author_name: author.name.to_str_lossy().to_string(),
        author_email: author.email.to_str_lossy().to_string(),
        time_unix: time.seconds,
        time_offset: time.offset,
    })
}

pub fn resolve_commit(repo: &gix::Repository, spec: Option<&str>) -> Result<gix::ObjectId> {
    match spec {
        None => {
            let c = repo.head_commit().map_err(|_| Error::Empty)?;
            Ok(c.id().detach())
        }
        Some(s) => {
            if let Ok(id) = gix::ObjectId::from_hex(s.as_bytes()) {
                return Ok(id);
            }
            for name in [
                s.to_string(),
                format!("refs/heads/{s}"),
                format!("refs/tags/{s}"),
            ] {
                if let Ok(mut r) = repo.find_reference(name.as_str()) {
                    if let Ok(id) = r.peel_to_id() {
                        return Ok(id.detach());
                    }
                }
            }
            Err(Error::NotFound)
        }
    }
}

pub fn log(
    repo: &gix::Repository,
    start: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<CommitInfo>> {
    let start_id = resolve_commit(repo, start)?;
    let mut out = Vec::with_capacity(limit);
    let walk = repo
        .find_object(start_id)?
        .try_into_commit()?
        .id()
        .ancestors()
        .all()?;
    for (i, info) in walk.enumerate() {
        if i < offset {
            continue;
        }
        if out.len() >= limit {
            break;
        }
        let info = info?;
        out.push(commit_info(repo, info.id)?);
    }
    Ok(out)
}
