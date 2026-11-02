//! Running slash commands through the local `claude` CLI and showing what they do.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::runs::application::{RunDesk, UsageLogs};
use crate::runs::infrastructure::{ClaudeGateway, RunLogUsage, RunLogs, prune};
use crate::shared_kernel::KeyReader;

/// `logs` is the app's own data folder. Nothing here writes to the workspace itself.
/// `keys` reads the saved API key, and is called only when a process starts under an API key.
pub fn open_desk(workspace: &Path, logs: PathBuf, repaint: Arc<dyn Fn() + Send + Sync>, keys: KeyReader) -> RunDesk {
    let history = RunLogs::new(logs.clone(), workspace);
    let mut desk = RunDesk::new(Box::new(ClaudeGateway::new(workspace.to_owned(), logs, keys)), repaint);
    desk.restore(&history);
    desk
}

/// What the tasks of one workspace cost, read from the run logs in `logs`.
pub fn usage_logs(logs: PathBuf, workspace: &Path) -> Box<dyn UsageLogs> {
    Box::new(RunLogUsage::new(logs, workspace))
}

/// Deletes the run logs in `logs` last written before `keep_after`, and returns how many went.
/// It touches only regular `<run id>.jsonl` files directly in that folder.
pub fn prune_logs(logs: &Path, keep_after: SystemTime) -> usize {
    prune(logs, keep_after)
}
