use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::shared_kernel::host_path::{find_executable, resolved_path};
use crate::workspace::domain::{ToolProber, ToolSpec};

const TIMEOUT: Duration = Duration::from_secs(10);

pub struct ProcessProber;

impl ToolProber for ProcessProber {
    fn probe(&self, tool: &ToolSpec) -> Option<String> {
        let path = resolved_path();
        // A python.org install on Windows ships no `python3`.
        let program = find_executable(path, tool.name).or_else(|| (tool.name == "python3").then(|| find_executable(path, "python")).flatten())?;
        let args = tool.args;
        let (sent, received) = mpsc::channel();
        thread::spawn(move || {
            let mut command = Command::new(program);
            command.args(args).env("PATH", path).stdin(Stdio::null());
            #[cfg(windows)]
            std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
            let _ = sent.send(command.output());
        });
        let output = received.recv_timeout(TIMEOUT).ok()?.ok()?;
        let text = if output.stdout.is_empty() { output.stderr } else { output.stdout };
        let first = String::from_utf8_lossy(&text).lines().next().unwrap_or_default().trim().to_owned();
        output.status.success().then_some(first)
    }
}
