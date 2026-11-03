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

/// Kills a process started with `own_group` and every process it started.
#[cfg(unix)]
pub fn kill_tree(pid: u32) {
    let Ok(group) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: kill only sends a signal. A negative id names the process group the child leads.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}

/// Kills a process and every process it started.
#[cfg(windows)]
pub fn kill_tree(pid: u32) {
    let mut command = Command::new("taskkill");
    command.args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
    let _ = command.status();
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
