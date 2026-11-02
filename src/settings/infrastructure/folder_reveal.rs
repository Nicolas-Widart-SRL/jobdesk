use std::path::Path;
use std::process::{Command, Stdio};

/// Opens the folder in Finder, Explorer or the desktop's file manager. The path is one argument and no shell reads it.
/// When the program is missing there is nothing to show, and nothing to report.
pub fn reveal_folder(path: &Path) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = Command::new(program).arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
