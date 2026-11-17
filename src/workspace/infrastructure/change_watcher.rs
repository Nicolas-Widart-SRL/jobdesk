use std::path::{Path, PathBuf};
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
    watcher: RecommendedWatcher,
    /// The watched paths the checkout did not have when last looked at.
    missing: Vec<(PathBuf, RecursiveMode)>,
}

impl ChangeWatcher {
    /// Folders a fresh checkout does not have yet are skipped. Their creation shows up in the root watch, and
    /// `watch_new` watches them from then on.
    pub fn start(root: &Path, changed: Sender<()>) -> notify::Result<Self> {
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| !event.kind.is_access()) {
                let _ = changed.send(());
            }
        })?;
        let mut this = Self { watcher, missing: Vec::new() };
        for (relative, mode) in WATCHED {
            let path = root.join(relative);
            if path.exists() {
                this.watcher.watch(&path, mode)?;
            } else {
                this.missing.push((path, mode));
            }
        }
        Ok(this)
    }

    /// Watches each skipped folder that is there now, such as `.agents/skills` once the first portal is installed in a
    /// checkout that had none. Called before each read, so a change made after the read is seen. A folder that cannot be
    /// watched yet is tried again next time.
    pub fn watch_new(&mut self) {
        let watcher = &mut self.watcher;
        self.missing.retain(|(path, mode)| !(path.exists() && watcher.watch(path, *mode).is_ok()));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// A folder made after the start is watched once `watch_new` runs: a file written in it then is a change.
    #[test]
    fn a_folder_made_later_is_watched_from_then() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (changed, changes) = mpsc::channel();
        let mut watcher = ChangeWatcher::start(&root, changed).unwrap();
        assert!(watcher.missing.iter().any(|(path, _)| path.ends_with(".agents/skills")));

        fs::create_dir_all(root.join(".agents/skills/alder-search")).unwrap();
        watcher.watch_new();
        assert!(!watcher.missing.iter().any(|(path, _)| path.ends_with(".agents/skills")), "watched now");
        // Settle the events of the folders made, so only the write below is left to hear.
        std::thread::sleep(Duration::from_millis(500));
        while changes.try_recv().is_ok() {}

        fs::write(root.join(".agents/skills/alder-search/SKILL.md"), "# Alder Search Skill\n").unwrap();
        assert!(changes.recv_timeout(Duration::from_secs(10)).is_ok(), "the write in the new folder is a change");
    }
}
