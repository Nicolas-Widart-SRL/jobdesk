//! The `ai-job-search` checkout this app views, and the changes made to it.

pub mod application;
mod domain;
mod infrastructure;

use std::path::Path;

use crate::workspace::application::{WorkspaceRoot, open_workspace};
pub use crate::workspace::infrastructure::ChangeWatcher;

pub fn open(path: &Path) -> Option<WorkspaceRoot> {
    open_workspace(path, infrastructure::path_exists)
}
