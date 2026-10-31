use std::env;
use std::path::{Path, PathBuf};

#[cfg(unix)]
const MARKER: &str = "__JOBDESK_PATH__";

/// An app opened from the Finder has none of the PATH set in shell profiles, so `claude`,
/// `bun` and `lualatex` would all look missing. Ask the login shell.
pub fn resolved_path() -> String {
    let inherited = env::var("PATH").unwrap_or_default();
    match login_shell_path() {
        Some(login) if !login.is_empty() => format!("{login}{}{inherited}", if cfg!(windows) { ';' } else { ':' }),
        _ => inherited,
    }
}

#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    const SHELL_TIMEOUT: Duration = Duration::from_secs(5);

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_owned());
    let script = format!("printf '{MARKER}%s{MARKER}' \"$PATH\"");
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let output = Command::new(shell).args(["-ilc", &script]).stdin(Stdio::null()).stderr(Stdio::null()).output();
        let _ = sent.send(output);
    });
    let output = received.recv_timeout(SHELL_TIMEOUT).ok()?.ok()?;
    between_markers(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(unix))]
fn login_shell_path() -> Option<String> {
    None
}

#[cfg(unix)]
fn between_markers(output: &str) -> Option<String> {
    let start = output.find(MARKER)? + MARKER.len();
    let end = output[start..].find(MARKER)? + start;
    Some(output[start..end].to_owned())
}

fn candidates(dir: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        ["claude.exe", "claude.cmd", "claude.bat"].iter().map(|name| dir.join(name)).collect()
    } else {
        vec![dir.join("claude")]
    }
}

pub fn find_claude(path: &str) -> Option<PathBuf> {
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE")).map(PathBuf::from);
    let usual = home.iter().flat_map(|home| [home.join(".local/bin"), home.join(".claude/local")]);
    env::split_paths(path).chain(usual).flat_map(|dir| candidates(&dir)).find(|candidate| candidate.is_file())
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn ignores_what_profiles_print_around_the_path() {
        let output = format!("welcome!\n{MARKER}/a:/b{MARKER}\nbye");
        assert_eq!(between_markers(&output).as_deref(), Some("/a:/b"));
        assert_eq!(between_markers("nothing"), None);
    }

    #[test]
    fn finds_claude_on_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude"), "").unwrap();
        assert_eq!(find_claude(dir.path().to_str().unwrap()), Some(dir.path().join("claude")));
    }
}
