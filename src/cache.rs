use crate::git::repository::{RepoSummary, discover};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct RepoListCache {
    root: PathBuf,
    dirty: Arc<AtomicBool>,
    watcher: async_lock::Mutex<RecommendedWatcher>,
    watches: async_lock::Mutex<HashMap<PathBuf, RecursiveMode>>,
    repos: async_lock::RwLock<Option<Arc<Vec<RepoSummary>>>>,
}

impl RepoListCache {
    pub fn new(root: PathBuf) -> notify::Result<Self> {
        let dirty = Arc::new(AtomicBool::new(true));
        let flag = dirty.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let is_access = matches!(&res, Ok(ev) if matches!(ev.kind, EventKind::Access(_)));
            if !is_access {
                flag.store(true, Ordering::Release);
            }
        })?;
        Ok(Self {
            root,
            dirty,
            watcher: async_lock::Mutex::new(watcher),
            watches: async_lock::Mutex::new(HashMap::new()),
            repos: async_lock::RwLock::new(None),
        })
    }

    pub async fn repos(&self) -> Arc<Vec<RepoSummary>> {
        {
            let cached = self.repos.read().await;
            if let Some(repos) = cached.as_ref() {
                if !self.dirty.load(Ordering::Acquire) {
                    return repos.clone();
                }
            }
        }
        self.dirty.store(false, Ordering::Release);
        let repos = Arc::new(discover(&self.root));
        self.sync_watches(&repos).await;
        {
            let mut cached = self.repos.write().await;
            *cached = Some(repos.clone());
        }
        repos
    }

    async fn sync_watches(&self, repos: &[RepoSummary]) {
        let mut desired: HashMap<PathBuf, RecursiveMode> = HashMap::new();
        desired.insert(self.root.clone(), RecursiveMode::NonRecursive);
        for r in repos {
            desired.insert(r.git_dir.clone(), RecursiveMode::NonRecursive);
            desired.insert(r.git_dir.join("refs"), RecursiveMode::Recursive);
        }

        let mut watches = self.watches.lock().await;
        let mut watcher = self.watcher.lock().await;
        watches.retain(|path, mode| {
            if desired.get(path) == Some(mode) {
                true
            } else {
                let _ = watcher.unwatch(path);
                false
            }
        });
        for (path, mode) in desired {
            if watches.contains_key(&path) {
                continue;
            }
            match watcher.watch(&path, mode) {
                Ok(()) => {
                    watches.insert(path, mode);
                }
                Err(e) => eprintln!("Cannot watch '{}': {e}", path.display()),
            }
        }
    }
}
