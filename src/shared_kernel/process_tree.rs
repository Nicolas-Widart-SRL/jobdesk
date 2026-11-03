use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;

/// Starts the process in its own process group, so one kill reaches the tools it runs too.
pub fn own_group(command: &mut Command) {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(not(unix))]
    let _ = command;
}

/// Starts a console program on Windows without the console window it would otherwise open.
pub fn no_window(command: &mut Command) {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(command, 0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(not(windows))]
    let _ = command;
}

/// Kills a process started with `own_group` and every process it started.
#[cfg(unix)]
pub fn kill_tree(pid: u32) {
    let Ok(group) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: kill only sends a signal. A negative id names the process group the child leads.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}

/// Kills a process and every process it started. A `SystemRoot` that names no `taskkill` falls back to the usual one.
#[cfg(windows)]
pub fn kill_tree(pid: u32) {
    use crate::shared_kernel::host_path::system_program;

    const TASKKILL: [&str; 2] = ["System32", "taskkill.exe"];
    let kill = |taskkill: &std::path::Path| {
        let mut command = Command::new(taskkill);
        command.args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        no_window(&mut command);
        command.status()
    };
    let inherited = system_program(std::env::var_os("SystemRoot").as_deref(), &TASKKILL);
    let usual = system_program(None, &TASKKILL);
    if kill(&inherited).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound) && inherited != usual {
        let _ = kill(&usual);
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;

    fn alive(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// A tool Claude started, here a background `sleep`, dies with Claude.
    #[test]
    fn killing_the_tree_reaches_grandchildren() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60 & echo $!; wait"]).stdout(Stdio::piped());
        own_group(&mut command);
        let mut child = command.spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let grandchild: libc::pid_t = line.trim().parse().unwrap();

        kill_tree(child.id());
        child.wait().unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(grandchild) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(grandchild));
    }
}

#[cfg(test)]
#[cfg(windows)]
mod windows_tests {
    use std::io::{BufRead, BufReader, Read};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::shared_kernel::host_path::system_program;

    /// A tool Claude started, here a `ping` that would run for a minute, dies with Claude. It shares the stdout pipe,
    /// so the pipe closes only once the `ping` is gone too.
    #[test]
    fn killing_the_tree_reaches_grandchildren() {
        let cmd = system_program(std::env::var_os("SystemRoot").as_deref(), &["System32", "cmd.exe"]);
        let mut command = Command::new(cmd);
        command.args(["/c", "ping", "-n", "60", "127.0.0.1"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        own_group(&mut command);
        no_window(&mut command);
        let mut child = command.spawn().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        // `ping` names its target once it runs, so the grandchild exists before the kill.
        let mut line = Vec::new();
        while !line.windows(9).any(|part| part == b"127.0.0.1") {
            line.clear();
            assert_ne!(stdout.read_until(b'\n', &mut line).unwrap(), 0, "ping ended before it started");
        }

        kill_tree(child.id());
        // Waiting without a deadline would pass a kill that did nothing, once `ping` ended by itself.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut ended = child.try_wait().unwrap().is_some();
        while !ended && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
            ended = child.try_wait().unwrap().is_some();
        }
        if !ended {
            let _ = child.kill();
        }
        assert!(ended, "cmd still runs after the kill");

        let (sent, closed) = mpsc::channel();
        thread::spawn(move || {
            let _ = stdout.read_to_end(&mut Vec::new());
            let _ = sent.send(());
        });
        assert!(closed.recv_timeout(Duration::from_secs(10)).is_ok(), "the ping still holds the pipe, so it still runs");
    }
}
