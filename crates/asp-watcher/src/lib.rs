use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tracing::{info, warn};

const DEBOUNCE_MS: u64 = 300;

static IGNORE_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next",
];

static IGNORE_EXTS: &[&str] = &["pyc"];

pub struct FileWatcher {
    pub project_root: PathBuf,
}

impl FileWatcher {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    pub fn watch<F>(&self, mut callback: F) -> Result<()>
    where
        F: FnMut(HashSet<PathBuf>) -> Result<()>,
    {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = RecommendedWatcher::new(
            move |event| {
                let _ = tx.send(event);
            },
            notify::Config::default(),
        )?;

        watcher.watch(&self.project_root, RecursiveMode::Recursive)?;
        info!("Watching {:?}", self.project_root);

        let mut pending: HashSet<PathBuf> = HashSet::new();
        let debounce = Duration::from_millis(DEBOUNCE_MS);

        loop {
            match rx.recv_timeout(debounce) {
                Ok(Ok(event)) => {
                    for path in event.paths {
                        if should_watch(&path) {
                            pending.insert(path);
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("Watch error: {}", e);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !pending.is_empty() {
                        let batch = std::mem::take(&mut pending);
                        if let Err(e) = callback(batch) {
                            warn!("Callback error: {}", e);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        Ok(())
    }
}

fn should_watch(path: &PathBuf) -> bool {
    for component in path.components() {
        let s = component.as_os_str().to_string_lossy();
        if IGNORE_DIRS.contains(&s.as_ref()) {
            return false;
        }
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if IGNORE_EXTS.contains(&ext) {
            return false;
        }
    }

    true
}
