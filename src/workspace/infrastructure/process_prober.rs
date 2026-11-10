use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::shared_kernel::host_path::{find_executable, resolved_path};
use crate::shared_kernel::process_tree::no_window;
use crate::workspace::domain::{ToolProber, ToolSpec};

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct ProcessProber;

impl ToolProber for ProcessProber {
    fn probe(&self, tool: &ToolSpec) -> Option<String> {
        let path = resolved_path();
        // A python.org install on Windows ships no `python3`.
        let program = find_executable(path, tool.name).or_else(|| (tool.name == "python3").then(|| find_executable(path, "python")).flatten())?;
        let output = run(program, tool.args, None)?;
        let text = if output.stdout.is_empty() { output.stderr } else { output.stdout };
        Some(String::from_utf8_lossy(&text).lines().next().unwrap_or_default().trim().to_owned())
    }
}

impl ProcessProber {
    /// All `program` prints when run with `args` from the folder where the tool check finds the tool `beside`, so both
    /// belong to one install. It runs in the root folder, so no file in Jobdesk's own current folder is found. `None`
    /// when either is missing, or it fails or takes longer than `TIMEOUT`.
    pub fn beside(&self, beside: &str, program: &str, args: &[&str]) -> Option<String> {
        let program = find_executable(resolved_path(), beside)?.parent()?.join(program);
        if !program.is_file() {
            return None;
        }
        let output = run(program, args, Some(Path::new("/")))?;
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Runs `program` with `args` on the resolved `PATH`, in `folder` when one is given, with nothing to read. Gives what it
/// printed when it succeeds within `TIMEOUT`.
fn run(program: PathBuf, args: &[&str], folder: Option<&'static Path>) -> Option<Output> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let mut command = Command::new(program);
        command.args(args).env("PATH", resolved_path()).stdin(Stdio::null());
        if let Some(folder) = folder {
            command.current_dir(folder);
        }
        no_window(&mut command);
        let _ = sent.send(command.output());
    });
    let output = received.recv_timeout(TIMEOUT).ok()?.ok()?;
    output.status.success().then_some(output)
}
