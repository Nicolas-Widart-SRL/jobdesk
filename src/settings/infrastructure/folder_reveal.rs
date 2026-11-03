use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::shared_kernel::host_path::system_program;

/// Opens the folder in Finder, Explorer or the desktop's file manager. The path is one argument and no shell reads it.
/// When the program is missing there is nothing to show, and nothing to report.
/// Explorer is a window program, so it opens no console and needs no `no_window`.
pub fn reveal_folder(path: &Path) {
    let program = if cfg!(target_os = "macos") {
        PathBuf::from("open")
    } else if cfg!(windows) {
        system_program(std::env::var_os("SystemRoot").as_deref(), &["explorer.exe"])
    } else {
        PathBuf::from("xdg-open")
    };
    let _ = Command::new(program).arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
