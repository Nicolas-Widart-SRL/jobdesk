use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(unix)]
const MARKER: &str = "__JOBDESK_PATH__";

/// What a program's file name may end in on Windows, in the order Jobdesk tries them.
const WINDOWS_EXTENSIONS: [&str; 3] = ["exe", "cmd", "bat"];

/// An app opened from the Finder has none of the PATH set in shell profiles, so `claude`,
/// `bun` and `lualatex` would all look missing. Ask the login shell.
pub fn resolved_path() -> &'static str {
    static RESOLVED: OnceLock<String> = OnceLock::new();
    RESOLVED.get_or_init(|| {
        let inherited = env::var("PATH").unwrap_or_default();
        match login_shell_path() {
            Some(login) if !login.is_empty() => format!("{login}{}{inherited}", if cfg!(windows) { ';' } else { ':' }),
            _ => inherited,
        }
    })
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

/// The files `name` could be in `dir`: the bare name when there are no `extensions`, else one per extension.
fn candidates(dir: &Path, name: &str, extensions: &[&str]) -> Vec<PathBuf> {
    if extensions.is_empty() { vec![dir.join(name)] } else { extensions.iter().map(|extension| dir.join(format!("{name}.{extension}"))).collect() }
}

/// The folders the Claude installer and npm put `claude` in. On Windows the profile is `USERPROFILE`, `HOME` only stands
/// in for it, and npm's global installs go in `%APPDATA%\npm`. An empty variable counts as unset.
fn usual_folders(home: Option<&OsStr>, userprofile: Option<&OsStr>, appdata: Option<&OsStr>, windows: bool) -> Vec<PathBuf> {
    let set = |value: Option<&OsStr>| value.filter(|value| !value.is_empty()).map(PathBuf::from);
    let (home, userprofile) = (set(home), set(userprofile));
    let profile = if windows { userprofile.or(home) } else { home.or(userprofile) };
    let mut folders: Vec<PathBuf> = profile.iter().flat_map(|profile| [profile.join(".local").join("bin"), profile.join(".claude").join("local")]).collect();
    if windows {
        folders.extend(set(appdata).map(|appdata| appdata.join("npm")));
    }
    folders
}

/// Looks through `path`, then the folders the Claude installer and npm use.
pub fn find_executable(path: &str, name: &str) -> Option<PathBuf> {
    let windows = cfg!(windows);
    let usual = usual_folders(env::var_os("HOME").as_deref(), env::var_os("USERPROFILE").as_deref(), env::var_os("APPDATA").as_deref(), windows);
    let extensions: &[&str] = if windows { &WINDOWS_EXTENSIONS } else { &[] };
    env::split_paths(path).chain(usual).flat_map(|dir| candidates(&dir, name, extensions)).find(|candidate| candidate.is_file())
}

/// A program that ships with Windows, by its full path under `system_root`, which is `%SystemRoot%`.
/// Windows looks for a bare name in the app's own folder first, and a per-user install folder is writable by the user.
pub fn system_program(system_root: Option<&OsStr>, parts: &[&str]) -> PathBuf {
    let root = system_root.filter(|root| !root.is_empty()).map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    parts.iter().fold(root, |path, part| path.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(text: &str) -> Option<&OsStr> {
        Some(OsStr::new(text))
    }

    /// Compared as text, so a path joined with `/` on Windows does not pass for one joined by part.
    fn texts(paths: &[PathBuf]) -> Vec<&OsStr> {
        paths.iter().map(|path| path.as_os_str()).collect()
    }

    #[test]
    fn candidates_try_each_windows_extension_in_order() {
        let dir = Path::new("bin");
        assert_eq!(candidates(dir, "claude", &[]), [dir.join("claude")]);
        assert_eq!(candidates(dir, "claude", &WINDOWS_EXTENSIONS), [dir.join("claude.exe"), dir.join("claude.cmd"), dir.join("claude.bat")]);
    }

    #[test]
    fn on_windows_the_profile_comes_first_and_npm_is_searched() {
        let profile = Path::new(r"C:\Users\ada");
        let npm = Path::new(r"C:\Users\ada\AppData\Roaming").join("npm");
        let expected = [profile.join(".local").join("bin"), profile.join(".claude").join("local"), npm];
        let folders = usual_folders(os(r"C:\Users\other"), os(r"C:\Users\ada"), os(r"C:\Users\ada\AppData\Roaming"), true);
        assert_eq!(texts(&folders), texts(&expected));
        let folders = usual_folders(os(r"C:\Users\ada"), None, None, true);
        assert_eq!(texts(&folders), texts(&expected[..2]), "HOME stands in for a missing USERPROFILE");
        let folders = usual_folders(None, os(""), os(r"C:\Roaming"), true);
        assert_eq!(texts(&folders), [Path::new(r"C:\Roaming").join("npm").as_os_str()], "an empty USERPROFILE is no folder");
    }

    #[test]
    fn elsewhere_home_comes_first_and_appdata_is_ignored() {
        let home = Path::new("/home/ada");
        let expected = [home.join(".local").join("bin"), home.join(".claude").join("local")];
        assert_eq!(texts(&usual_folders(os("/home/ada"), os("/profile"), os("/appdata"), false)), texts(&expected));
        assert!(usual_folders(os(""), None, os("/appdata"), false).is_empty(), "an empty HOME is no folder");
    }

    #[test]
    fn system_programs_have_a_full_path_even_without_system_root() {
        let expected = Path::new(r"D:\Win").join("System32").join("taskkill.exe");
        assert_eq!(system_program(os(r"D:\Win"), &["System32", "taskkill.exe"]).as_os_str(), expected.as_os_str());
        let fallback = Path::new(r"C:\Windows").join("explorer.exe");
        assert_eq!(system_program(None, &["explorer.exe"]).as_os_str(), fallback.as_os_str());
        assert_eq!(system_program(os(""), &["explorer.exe"]).as_os_str(), fallback.as_os_str());
    }

    #[cfg(unix)]
    #[test]
    fn ignores_what_profiles_print_around_the_path() {
        let output = format!("welcome!\n{MARKER}/a:/b{MARKER}\nbye");
        assert_eq!(between_markers(&output).as_deref(), Some("/a:/b"));
        assert_eq!(between_markers("nothing"), None);
    }

    #[cfg(unix)]
    #[test]
    fn finds_claude_on_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude"), "").unwrap();
        assert_eq!(find_executable(dir.path().to_str().unwrap(), "claude"), Some(dir.path().join("claude")));
    }

    #[cfg(windows)]
    #[test]
    fn finds_a_cmd_stub_on_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("claude.cmd"), "").unwrap();
        assert_eq!(find_executable(dir.path().to_str().unwrap(), "claude"), Some(dir.path().join("claude.cmd")));
    }
}
