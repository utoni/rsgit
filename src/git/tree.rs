use super::{
    commit::resolve_commit,
    error::{Error, Result},
};
use gix::bstr::ByteSlice;

pub struct TreeItem {
    pub name: String,
    pub is_dir: bool,
    pub is_exec: bool,
    pub is_link: bool,
    pub is_submodule: bool,
    pub size: usize,
}

const MAX_BLOB_BYTES: usize = 128 * 1024;

pub enum PathContent {
    Dir(Vec<TreeItem>),
    File {
        data: Vec<u8>,
        size: usize,
        too_large: bool,
    },
}

fn commit_tree<'r>(repo: &'r gix::Repository, spec: Option<&str>) -> Result<gix::Tree<'r>> {
    let id = resolve_commit(repo, spec)?;
    let commit = repo.find_object(id)?.try_into_commit()?;
    Ok(commit.tree()?)
}

pub fn path_content(repo: &gix::Repository, spec: Option<&str>, path: &str) -> Result<PathContent> {
    let tree = commit_tree(repo, spec)?;
    let path = path.trim_matches('/');

    let subtree = if path.is_empty() {
        tree
    } else {
        let entry = tree.lookup_entry_by_path(path)?.ok_or(Error::NotFound)?;
        if entry.mode().is_commit() {
            return Err(Error::NotFound);
        }
        let header = repo.find_header(entry.oid())?;
        match header.kind() {
            gix::object::Kind::Blob => {
                let size = header.size() as usize;
                if size > MAX_BLOB_BYTES {
                    return Ok(PathContent::File {
                        data: Vec::new(),
                        size,
                        too_large: true,
                    });
                }
                let blob = entry.object()?.into_blob();
                return Ok(PathContent::File {
                    size: blob.data.len(),
                    data: blob.detach().data,
                    too_large: false,
                });
            }
            gix::object::Kind::Tree => entry.object()?.into_tree(),
            _ => return Err(Error::NotFound),
        }
    };

    let mut items = Vec::new();
    for e in subtree.iter() {
        let e = e?;
        let mode = e.mode();
        let kind = mode.kind();
        let is_dir = matches!(kind, gix::objs::tree::EntryKind::Tree);
        let is_exec = matches!(kind, gix::objs::tree::EntryKind::BlobExecutable);
        let is_link = matches!(kind, gix::objs::tree::EntryKind::Link);
        let is_submodule = matches!(kind, gix::objs::tree::EntryKind::Commit);
        let size = if is_dir || is_submodule {
            0
        } else {
            repo.find_header(e.oid())
                .map(|h| h.size() as usize)
                .unwrap_or(0)
        };
        items.push(TreeItem {
            name: e.filename().to_str_lossy().to_string(),
            is_dir,
            is_exec,
            is_link,
            is_submodule,
            size,
        });
    }
    items.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(PathContent::Dir(items))
}

pub fn find_readme(items: &[TreeItem]) -> Option<String> {
    let mut candidates: Vec<&TreeItem> = items
        .iter()
        .filter(|i| !i.is_dir && i.name.to_ascii_lowercase().starts_with("readme"))
        .collect();
    candidates.sort_by_key(|i| i.name.len());
    candidates.first().map(|i| i.name.clone())
}

pub fn raw_blob(repo: &gix::Repository, spec: Option<&str>, path: &str) -> Result<Vec<u8>> {
    let tree = commit_tree(repo, spec)?;
    let entry = tree
        .lookup_entry_by_path(path.trim_matches('/'))?
        .ok_or(Error::NotFound)?;
    if entry.mode().is_tree() || entry.mode().is_commit() {
        return Err(Error::NotFound);
    }
    Ok(entry.object()?.into_blob().detach().data)
}
