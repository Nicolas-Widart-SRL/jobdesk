use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use chrono::Local;

use crate::shell::Snapshot;
use crate::workspace::ChangeWatcher;
use crate::workspace::application::WorkspaceRoot;

const DEBOUNCE: Duration = Duration::from_millis(300);
/// Dates age even when no file changes, so counts refresh on a slow clock too.
const REFRESH: Duration = Duration::from_secs(3600);

/// Rebuilds the snapshot on a worker when the workspace changes. Stops when the receiver drops.
pub fn spawn(root: WorkspaceRoot, repaint: impl Fn() + Send + 'static) -> Receiver<Snapshot> {
    let (snapshots, received) = mpsc::channel();
    thread::spawn(move || {
        let (changed, changes) = mpsc::channel();
        let _watcher = ChangeWatcher::start(root.path(), changed);
        loop {
            let snapshot = Snapshot::load(root.path(), Local::now().date_naive());
            if snapshots.send(snapshot).is_err() {
                return;
            }
            repaint();
            match changes.recv_timeout(REFRESH) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => thread::sleep(REFRESH),
            }
            thread::sleep(DEBOUNCE);
            while changes.try_recv().is_ok() {}
        }
    });
    received
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::workspace;

    #[test]
    fn a_changed_tracker_reaches_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/commands")).unwrap();
        fs::write(dir.path().join(".claude/commands/apply.md"), "").unwrap();
        let updates = spawn(workspace::open(dir.path()).unwrap(), || {});
        let timeout = Duration::from_secs(10);
        assert!(updates.recv_timeout(timeout).unwrap().pipeline.is_empty());

        fs::write(dir.path().join("job_search_tracker.csv"), "date,company,role,status\n2026-10-01,Acme,Lead,applied\n").unwrap();

        assert_eq!(updates.recv_timeout(timeout).unwrap().pipeline.open_count(), 1);
    }
}
