use std::path::Path;
use std::sync::mpsc::Sender;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Paths whose changes can alter what the screens show.
const WATCHED: [(&str, RecursiveMode); 13] = [
    (".", RecursiveMode::NonRecursive),
    ("job_scraper", RecursiveMode::Recursive),
    ("documents/applications", RecursiveMode::Recursive),
    ("upskill", RecursiveMode::Recursive),
    ("reports", RecursiveMode::Recursive),
    ("cv", RecursiveMode::NonRecursive),
    ("cover_letters", RecursiveMode::NonRecursive),
    ("company_research", RecursiveMode::Recursive),
    (".agents/skills", RecursiveMode::Recursive),
    (".claude/skills", RecursiveMode::Recursive),
    (".claude/commands", RecursiveMode::NonRecursive),
    ("templates", RecursiveMode::Recursive),
    ("gmail_sync", RecursiveMode::Recursive),
];

/// Keeps the operating system watch alive for as long as it is held.
pub struct ChangeWatcher {
    _watcher: RecommendedWatcher,
}

impl ChangeWatcher {
    /// Folders a fresh checkout does not have yet are skipped. Their creation shows up in the root watch.
    pub fn start(root: &Path, changed: Sender<()>) -> notify::Result<Self> {
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| !event.kind.is_access()) {
                let _ = changed.send(());
            }
        })?;
        for (relative, mode) in WATCHED {
            let path = root.join(relative);
            if path.exists() {
                watcher.watch(&path, mode)?;
            }
        }
        Ok(Self { _watcher: watcher })
    }
}
