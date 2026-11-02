use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use chrono::{Local, NaiveDate, NaiveDateTime};

use crate::runs::application::{UsageLogs, UsageReport};
use crate::shell::Snapshot;
use crate::workspace::ChangeWatcher;
use crate::workspace::application::WorkspaceRoot;

const DEBOUNCE: Duration = Duration::from_millis(300);
/// Dates age even when no file changes, so counts refresh on a slow clock too.
const REFRESH: Duration = Duration::from_secs(3600);

/// How long to wait for a change: the slow clock, cut short at local midnight so today rolls over on time.
fn wait(now: NaiveDateTime) -> Duration {
    let midnight = now.date().succ_opt().and_then(|tomorrow| tomorrow.and_hms_opt(0, 0, 0));
    let to_midnight = midnight.and_then(|midnight| (midnight - now).to_std().ok());
    to_midnight.map_or(REFRESH, |left| left.min(REFRESH))
}

/// How long to wait before the next load, or none when the day moved on while `today`'s snapshot loaded.
fn next_wait(today: NaiveDate, now: NaiveDateTime) -> Option<Duration> {
    (now.date() == today).then(|| wait(now))
}

/// Rebuilds the snapshot on a worker when the workspace changes. Stops when the receiver drops.
/// `usage` is read on each load. The first read parses every log, so the checkout's snapshot goes out before it, with
/// its usage unread, then again with it. Later reads parse only the logs that changed.
/// Run logs sit outside the checkout, so a send on the returned sender reloads after a run ends.
pub fn spawn(root: WorkspaceRoot, mut usage: Option<Box<dyn UsageLogs>>, repaint: impl Fn() + Send + 'static) -> (Receiver<Snapshot>, Sender<()>) {
    let (snapshots, received) = mpsc::channel();
    let (changed, changes) = mpsc::channel();
    let nudge = changed.clone();
    thread::spawn(move || {
        let _watcher = ChangeWatcher::start(root.path(), changed);
        let mut logs_read = false;
        loop {
            let today = Local::now().date_naive();
            let mut snapshot = Snapshot::load(root.path(), today);
            if let Some(logs) = usage.as_mut() {
                if !logs_read {
                    if snapshots.send(snapshot.clone()).is_err() {
                        return;
                    }
                    repaint();
                    logs_read = true;
                }
                snapshot.usage = UsageReport::new(logs.tasks());
            }
            // With no logs to read, there is no usage to wait for.
            snapshot.usage_read = true;
            if snapshots.send(snapshot).is_err() {
                return;
            }
            repaint();
            let Some(wait) = next_wait(today, Local::now().naive_local()) else {
                continue;
            };
            match changes.recv_timeout(wait) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => thread::sleep(wait),
            }
            thread::sleep(DEBOUNCE);
            while changes.try_recv().is_ok() {}
        }
    });
    (received, nudge)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::workspace;

    #[test]
    fn the_wait_ends_at_midnight() {
        let at = |h, m| chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(h, m, 0).unwrap();
        assert_eq!(wait(at(23, 50)), Duration::from_secs(600));
        assert_eq!(wait(at(12, 0)), REFRESH);
        assert_eq!(wait(at(0, 0)), REFRESH);
    }

    #[test]
    fn a_load_that_crosses_midnight_reloads_at_once() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        let at = |date: chrono::NaiveDate, h, m| date.and_hms_opt(h, m, 0).unwrap();
        assert_eq!(next_wait(day, at(day, 23, 50)), Some(Duration::from_secs(600)));
        assert_eq!(next_wait(day, at(day.succ_opt().unwrap(), 0, 0)), None);
    }

    #[test]
    fn a_changed_tracker_reaches_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/commands")).unwrap();
        fs::write(dir.path().join(".claude/commands/apply.md"), "").unwrap();
        let (updates, _) = spawn(workspace::open(dir.path()).unwrap(), None, || {});
        let timeout = Duration::from_secs(10);
        assert!(updates.recv_timeout(timeout).unwrap().pipeline.is_empty());

        fs::write(dir.path().join("job_search_tracker.csv"), "date,company,role,status\n2026-10-01,Acme,Lead,applied\n").unwrap();

        assert_eq!(updates.recv_timeout(timeout).unwrap().pipeline.open_count(), 1);
    }

    #[test]
    fn a_finished_run_reaches_the_usage_after_a_nudge() {
        let checkout = tempfile::tempdir().unwrap();
        fs::create_dir_all(checkout.path().join(".claude/commands")).unwrap();
        fs::write(checkout.path().join(".claude/commands/apply.md"), "").unwrap();
        let logs = tempfile::tempdir().unwrap();
        let root = workspace::open(checkout.path()).unwrap();
        let reader = crate::runs::usage_logs(logs.path().to_owned(), root.path());
        let (updates, nudge) = spawn(root.clone(), Some(reader), || {});
        let timeout = Duration::from_secs(10);
        let first = updates.recv_timeout(timeout).unwrap();
        assert!(!first.usage_read, "the checkout shows before the logs are read");
        let read = updates.recv_timeout(timeout).unwrap();
        assert!(read.usage_read && read.usage.is_empty());
        assert_eq!(read.pipeline, first.pipeline, "the same checkout, with its usage");

        let meta = serde_json::json!({
            "type": "jobdesk_meta", "workspace": root.path().display().to_string(), "id": "a", "line": "/rank",
            "model": null, "budget_usd": 2.0, "started_at": "2026-10-31 10:00",
        });
        let result = r#"{"type":"result","subtype":"success","total_cost_usd":0.5,"duration_ms":3}"#;
        fs::write(logs.path().join("a.jsonl"), format!("{meta}\n{result}\n")).unwrap();
        nudge.send(()).unwrap();

        // The runs folder is not watched, so only the nudge brings the log in. A load already under way may come first.
        let shown = std::iter::from_fn(|| updates.recv_timeout(timeout).ok())
            .take(3)
            .inspect(|snapshot| assert!(snapshot.usage_read, "only the first load goes out unread"))
            .any(|snapshot| !snapshot.usage.is_empty());
        assert!(shown);
    }

    #[test]
    fn without_logs_to_read_the_first_snapshot_has_its_usage_read() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".claude/commands")).unwrap();
        fs::write(dir.path().join(".claude/commands/apply.md"), "").unwrap();
        let (updates, _) = spawn(workspace::open(dir.path()).unwrap(), None, || {});
        assert!(updates.recv_timeout(Duration::from_secs(10)).unwrap().usage_read);
    }
}
