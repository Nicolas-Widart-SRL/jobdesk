//! One queue of decisions: Claude prepares, the user decides. Cards derive from each snapshot.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::{Path, PathBuf};

use crate::intray::application::InTray;
use crate::intray::infrastructure::DecisionFile;

/// `folder` is the app's own data folder. Decisions never reach the workspace.
pub fn open_tray(folder: PathBuf, workspace: &Path) -> InTray {
    InTray::new(Box::new(DecisionFile::new(folder, workspace)))
}
