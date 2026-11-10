use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(unix)]
const MARKER: &str = "__JOBDESK_PATH__";

/// What a program's file name may end in on Windows, in the order Jobdesk tries them.
const WINDOWS_EXTENSIONS: [&str; 3] = ["exe", "cmd", "bat"];

/// The folders, under the home folder, that hold the programs of the helpers first-run setup installs on macOS, all in
/// Jobdesk's own `~/.jobdesk`: a standalone Python, Bun and TinyTeX, in that order.
pub const HELPER_BINS: [&[&str]; 3] = [&[".jobdesk", "python", "bin"], &[".jobdesk", "bun", "bin"], &[".jobdesk", "TinyTeX", "bin", "universal-darwin"]];

/// On macOS these programs in `/usr/bin` only hand over to the real one in Xcode or its Command Line Tools. With
/// neither installed, running one opens a dialog that offers to install them.
const DEVELOPER_STUBS: [&str; 2] = ["python3", "git"];

/// An app opened from the Finder has none of the PATH set in shell profiles, so `claude`, `bun` and `lualatex` would
/// all look missing. Ask the login shell. The helper folders come first, ahead of every other copy, newer ones and the
/// shell's own included, because setup installs a helper only when no copy works with ai-job-search, and then only
/// its pinned one is known to. Runs, the tool check and Runtime status all find it. They are listed even before an
/// install, because the lookup runs once.
pub fn resolved_path() -> &'static str {
    static RESOLVED: OnceLock<String> = OnceLock::new();
    RESOLVED.get_or_init(|| {
        let helpers = helper_bins(env::var_os("HOME").as_deref(), cfg!(target_os = "macos"));
        let separator = if cfg!(windows) { ';' } else { ':' };
        joined(&helpers, login_shell_path(), &env::var("PATH").unwrap_or_default(), separator)
    })
}

/// The helper folders under `home`, on macOS only, whether or not they exist yet. An empty `home` is no folder.
pub fn helper_bins(home: Option<&OsStr>, macos: bool) -> Vec<PathBuf> {
    let Some(home) = home.filter(|home| macos && !home.is_empty()) else { return Vec::new() };
    HELPER_BINS.iter().map(|parts| parts.iter().fold(PathBuf::from(home), |path, part| path.join(part))).collect()
}

/// `first`, then the login shell's `PATH`, then the inherited one. A folder that a `PATH` cannot hold, because it is
/// not text or holds the separator, is left out, and so is an empty part, which would name the current folder. With
/// Windows' `;`, a part made only of quotes is empty too. Elsewhere such a part is a folder's name and stays.
fn joined(first: &[PathBuf], login: Option<String>, inherited: &str, separator: char) -> String {
    let first = first.iter().filter_map(|folder| folder.to_str()).filter(|folder| !folder.contains(separator));
    let rest = login.iter().flat_map(|login| parts(login, separator)).chain(parts(inherited, separator));
    let empty = |part: &&str| if separator == ';' { part.replace('"', "").is_empty() } else { part.is_empty() };
    first.chain(rest.filter(|part| !empty(part))).collect::<Vec<_>>().join(&separator.to_string())
}

/// The parts of a `PATH` split on `separator`. With Windows' `;`, a part may be quoted to hold one, as in
/// `"C:\a;b"`, so a `;` inside quotes does not split. Elsewhere a quote is part of a folder's name.
fn parts(path: &str, separator: char) -> Vec<&str> {
    let mut quoted = false;
    path.split(|character| {
        if separator == ';' && character == '"' {
            quoted = !quoted;
        }
        character == separator && !quoted
    })
    .collect()
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

/// Looks through `path`, then the folders the Claude installer and npm use. On macOS a `/usr/bin` stub with no
/// developer tools behind it is not a program: running it would open a dialog, so the search goes on past it.
pub fn find_executable(path: &str, name: &str) -> Option<PathBuf> {
    let windows = cfg!(windows);
    let usual = usual_folders(env::var_os("HOME").as_deref(), env::var_os("USERPROFILE").as_deref(), env::var_os("APPDATA").as_deref(), windows);
    let extensions: &[&str] = if windows { &WINDOWS_EXTENSIONS } else { &[] };
    let dead = |candidate: &Path| cfg!(target_os = "macos") && is_dead_stub(candidate, Path::new("/usr/bin"), &developer_dirs);
    first_found(env::split_paths(path).chain(usual), name, extensions, &dead)
}

/// The first file `name` could be in `folders`, in order, that is not `dead`.
fn first_found(folders: impl Iterator<Item = PathBuf>, name: &str, extensions: &[&str], dead: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    folders.flat_map(|dir| candidates(&dir, name, extensions)).find(|candidate| candidate.is_file() && !dead(candidate))
}

/// Whether `candidate` is one of the developer stubs in `usr_bin` with no program behind it in any of the folders
/// `developer_dirs` gives, which it asks only for a stub.
fn is_dead_stub(candidate: &Path, usr_bin: &Path, developer_dirs: &dyn Fn() -> Vec<PathBuf>) -> bool {
    let Some(name) = candidate.file_name().and_then(OsStr::to_str) else { return false };
    if candidate.parent() != Some(usr_bin) || !DEVELOPER_STUBS.contains(&name) {
        return false;
    }
    // A folder may be named down to `Contents/Developer`, or as the Xcode app itself.
    !developer_dirs()
        .iter()
        .any(|dir| [dir.to_path_buf(), dir.join("Contents").join("Developer")].iter().any(|dir| dir.join("usr").join("bin").join(name).is_file()))
}

/// Where the stubs look for the real programs: `DEVELOPER_DIR`, else the folder `xcode-select` chose, else Xcode's and
/// the Command Line Tools' usual places.
fn developer_dirs() -> Vec<PathBuf> {
    if let Some(dir) = env::var_os("DEVELOPER_DIR").filter(|dir| !dir.is_empty()) {
        return vec![PathBuf::from(dir)];
    }
    if let Ok(chosen) = fs::read_link("/var/db/xcode_select_link") {
        return vec![Path::new("/var/db").join(chosen)];
    }
    vec![PathBuf::from("/Applications/Xcode.app/Contents/Developer"), PathBuf::from("/Library/Developer/CommandLineTools")]
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
    fn helper_folders_are_listed_on_macos_only_and_in_order() {
        let own = Path::new("/Users/ada").join(".jobdesk");
        let expected = [own.join("python").join("bin"), own.join("bun").join("bin"), own.join("TinyTeX").join("bin").join("universal-darwin")];
        assert_eq!(texts(&helper_bins(os("/Users/ada"), true)), texts(&expected), "listed whether or not they exist");
        assert!(helper_bins(os("/Users/ada"), false).is_empty());
        assert!(helper_bins(os(""), true).is_empty(), "an empty HOME is no folder");
        assert!(helper_bins(None, true).is_empty());
    }

    #[test]
    fn the_helper_folders_come_before_the_login_shell_and_the_inherited_path() {
        let helpers = [PathBuf::from("/h/python/bin"), PathBuf::from("/h/bun/bin")];
        assert_eq!(
            joined(&helpers, Some("/opt/homebrew/bin:/usr/bin".into()), "/usr/bin:/bin", ':'),
            "/h/python/bin:/h/bun/bin:/opt/homebrew/bin:/usr/bin:/usr/bin:/bin"
        );
        assert_eq!(joined(&helpers, None, "/usr/bin", ':'), "/h/python/bin:/h/bun/bin:/usr/bin");
        assert_eq!(joined(&[], Some(String::new()), "/usr/bin", ':'), "/usr/bin", "an empty login path adds nothing");
        assert_eq!(joined(&helpers, Some("/a".into()), "", ':'), "/h/python/bin:/h/bun/bin:/a", "no empty part, which would be the current folder");
        let odd = [PathBuf::from("/h/a:b/bin"), PathBuf::from("/h/ok")];
        assert_eq!(joined(&odd, None, "/usr/bin", ':'), "/h/ok:/usr/bin", "a folder holding the separator is left out");
    }

    #[test]
    fn empty_parts_inside_a_path_are_left_out() {
        assert_eq!(joined(&[], Some("/a::/b:".into()), "", ':'), "/a:/b", "an empty part would be the current folder");
        assert_eq!(joined(&[], Some(":/a".into()), ":/b", ':'), "/a:/b", "a leading separator is an empty part too");
        assert_eq!(joined(&[], None, "/usr/bin::/bin:", ':'), "/usr/bin:/bin", "the inherited path is split as well");
        let helpers = [PathBuf::from("/h/python/bin")];
        assert_eq!(joined(&helpers, Some("::/a".into()), "/b::", ':'), "/h/python/bin:/a:/b", "the helpers stay first");
        assert_eq!(joined(&[], None, "/a\"b:/c", ':'), "/a\"b:/c", "outside Windows a quote is part of a name");
        assert_eq!(joined(&[], None, "/a\"b::/c\"d", ':'), "/a\"b:/c\"d", "outside Windows a quote does not join parts");
        assert_eq!(joined(&[], None, "/a:\"\":/b", ':'), "/a:\"\":/b", "outside Windows a part made only of quotes is a name");
    }

    #[test]
    fn a_quoted_windows_part_keeps_its_separator() {
        assert_eq!(
            joined(&[], Some(String::new()), r#";C:\a;;"C:\b;c";"";C:\d;"#, ';'),
            r#"C:\a;"C:\b;c";C:\d"#,
            "a ; inside quotes does not split, and an empty or quoted empty part is left out"
        );
    }

    #[test]
    fn a_developer_stub_with_no_tools_behind_it_counts_as_missing() {
        let root = tempfile::tempdir().unwrap();
        let usr_bin = root.path().join("usr").join("bin");
        let tools = root.path().join("CommandLineTools");
        let xcode = root.path().join("Xcode.app");
        fs::create_dir_all(&usr_bin).unwrap();
        for name in ["python3", "git", "ls"] {
            fs::write(usr_bin.join(name), "").unwrap();
        }
        let none = || vec![tools.clone()];
        assert!(is_dead_stub(&usr_bin.join("python3"), &usr_bin, &none), "no Command Line Tools: the stub would open a dialog");
        assert!(is_dead_stub(&usr_bin.join("git"), &usr_bin, &none));
        assert!(!is_dead_stub(&usr_bin.join("ls"), &usr_bin, &none), "only the developer stubs are checked");
        assert!(!is_dead_stub(&tools.join("python3"), &usr_bin, &none), "a python3 elsewhere is a program");
        fs::create_dir_all(tools.join("usr").join("bin")).unwrap();
        fs::write(tools.join("usr").join("bin").join("python3"), "").unwrap();
        assert!(!is_dead_stub(&usr_bin.join("python3"), &usr_bin, &none), "the tools are there");
        assert!(is_dead_stub(&usr_bin.join("git"), &usr_bin, &none), "each stub needs its own program");
        let app = xcode.join("Contents").join("Developer").join("usr").join("bin");
        fs::create_dir_all(&app).unwrap();
        fs::write(app.join("git"), "").unwrap();
        assert!(!is_dead_stub(&usr_bin.join("git"), &usr_bin, &|| vec![xcode.clone()]), "a folder named as the Xcode app");
    }

    #[test]
    fn the_search_goes_on_past_a_dead_stub() {
        let root = tempfile::tempdir().unwrap();
        let usr_bin = root.path().join("usr").join("bin");
        let later = root.path().join("opt").join("bin");
        let tools = root.path().join("CommandLineTools");
        for folder in [&usr_bin, &later] {
            fs::create_dir_all(folder).unwrap();
            fs::write(folder.join("python3"), "").unwrap();
        }
        let folders = || [usr_bin.clone(), later.clone()].into_iter();
        let dead = |candidate: &Path| is_dead_stub(candidate, &usr_bin, &|| vec![tools.clone()]);
        assert_eq!(first_found(folders(), "python3", &[], &dead), Some(later.join("python3")), "the stub would open a dialog");
        assert_eq!(first_found([usr_bin.clone()].into_iter(), "python3", &[], &dead), None, "a dead stub alone is no program");
        fs::create_dir_all(tools.join("usr").join("bin")).unwrap();
        fs::write(tools.join("usr").join("bin").join("python3"), "").unwrap();
        assert_eq!(first_found(folders(), "python3", &[], &dead), Some(usr_bin.join("python3")), "a backed stub is found first");
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
