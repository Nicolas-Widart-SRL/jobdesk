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
    root: PathBuf,
    /// The watched paths the checkout did not have when last looked at.
    missing: Vec<(PathBuf, RecursiveMode)>,
    /// The nearest folder above each missing path, watched alone, so the next folder down shows up as a change.
    parents: Vec<PathBuf>,
}

impl ChangeWatcher {
    /// Folders a fresh checkout does not have yet are skipped. The nearest folder above each one that is there is
    /// watched alone, or the root watch covers it, so its creation shows up and `watch_new` watches it from then on.
    pub fn start(root: &Path, changed: Sender<()>) -> notify::Result<Self> {
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok_and(|event| !event.kind.is_access()) {
                let _ = changed.send(());
            }
        })?;
        let mut this = Self { watcher, root: root.to_path_buf(), missing: Vec::new(), parents: Vec::new() };
        for (relative, mode) in WATCHED {
            let path = root.join(relative);
            if path.exists() {
                this.watcher.watch(&path, mode)?;
            } else {
                this.missing.push((path, mode));
            }
        }
        this.watch_parents();
        Ok(this)
    }

    /// Watches each skipped folder that is there now, such as `.agents/skills` once the first portal is installed in a
    /// checkout that had none. Called before each read, so a change made after the read is seen. A folder that cannot be
    /// watched yet is tried again next time.
    pub fn watch_new(&mut self) {
        let watcher = &mut self.watcher;
        self.missing.retain(|(path, mode)| !(path.exists() && watcher.watch(path, *mode).is_ok()));
        self.watch_parents();
    }

    /// Watches the nearest folder above each missing path, and stops watching one no missing path needs any more,
    /// such as `.agents` once `.agents/skills` is watched. A folder that cannot be watched is tried again next time.
    fn watch_parents(&mut self) {
        let mut needed: Vec<PathBuf> = self.missing.iter().filter_map(|(path, _)| nearest_existing_parent(&self.root, path)).collect();
        needed.sort();
        needed.dedup();
        for gone in self.parents.iter().filter(|parent| !needed.contains(parent)) {
            let _ = self.watcher.unwatch(gone);
        }
        let watcher = &mut self.watcher;
        let watched = &self.parents;
        needed.retain(|parent| watched.contains(parent) || watcher.watch(parent, RecursiveMode::NonRecursive).is_ok());
        self.parents = needed;
    }
}

/// The nearest folder above `path` that is there, when it is below `root`, whose own watch already shows its entries.
/// None when that folder or one above it is a link or not a folder, so no watch reaches outside the checkout.
fn nearest_existing_parent(root: &Path, path: &Path) -> Option<PathBuf> {
    let mut parents = path.ancestors().skip(1).take_while(|parent| *parent != root);
    let nearest = parents.by_ref().find(|parent| parent.symlink_metadata().is_ok())?;
    let real_folder = |folder: &Path| folder.symlink_metadata().is_ok_and(|meta| meta.is_dir());
    (real_folder(nearest) && parents.all(real_folder)).then(|| nearest.to_path_buf())
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

    /// With `.agents` there but not `.agents/skills`, `.agents` is watched until `.agents/skills` is.
    #[test]
    fn the_folder_above_a_missing_one_is_watched_until_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join(".agents")).unwrap();
        let (changed, changes) = mpsc::channel();
        let mut watcher = ChangeWatcher::start(&root, changed).unwrap();
        assert_eq!(watcher.parents, [root.join(".agents")], "the root watch covers the other missing folders");
        std::thread::sleep(Duration::from_millis(500));
        while changes.try_recv().is_ok() {}

        fs::create_dir(root.join(".agents/skills")).unwrap();
        assert!(changes.recv_timeout(Duration::from_secs(10)).is_ok(), "the new folder under .agents is a change");
        watcher.watch_new();
        assert!(watcher.parents.is_empty(), ".agents is not needed once .agents/skills is watched");
    }

    #[test]
    fn the_nearest_folder_below_the_root_is_watched() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        assert_eq!(nearest_existing_parent(&root, &root.join(".agents/skills")), None, "the root is watched already");
        fs::create_dir(root.join("documents")).unwrap();
        let applications = root.join("documents/applications");
        assert_eq!(nearest_existing_parent(&root, &applications), Some(root.join("documents")));
    }

    /// `.agents` links outside the checkout, where `skills` is a folder: neither is watched.
    #[cfg(unix)]
    #[test]
    fn a_link_above_a_missing_folder_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(outside.path().join("skills")).unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join(".agents")).unwrap();
        assert_eq!(nearest_existing_parent(&root, &root.join(".agents/skills/alder")), None, "a link is not followed");
    }
}
