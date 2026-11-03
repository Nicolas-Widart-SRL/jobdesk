use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Local};

use crate::profile::domain::{CvFiles, FileStamp};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::host_path::system_program;
use crate::shared_kernel::process_tree::no_window;
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::workspace_path::within_workspace;

const FOLDER: &str = "cv";

/// The `cv/` folder of one checkout. Its files come from an untrusted checkout, so a name is checked again, with every link
/// resolved, before anything touches the file.
pub struct CvFolder {
    root: PathBuf,
}

impl CvFolder {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    fn folder(&self) -> PathBuf {
        self.root.join(FOLDER)
    }

    /// The `.tex` and `.pdf` files of `cv/`, grouped by stem. Subfolders are not entered. A missing `cv/` is an empty list.
    /// Names that are not text, start with a dot, hold a control or hidden character, or are refused by `checked` are skipped,
    /// so no row shows a name that looks like another file's.
    pub fn list(&self) -> Result<Vec<CvFiles>, LoadError> {
        let entries = match fs::read_dir(self.folder()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LoadError::new("cv/", error)),
        };
        // A `cv/` that leads out of the checkout has nothing safe to show, and that is no failure to read it.
        if self.resolved_folder().is_none() {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).filter(|name| listed(name)).collect();
        names.sort();
        // Two stems that differ only in case are one document: on a case-insensitive disk they could not both exist.
        let mut groups: BTreeMap<String, CvFiles> = BTreeMap::new();
        for name in names {
            let Some(stamp) = self.stamp(&name) else { continue };
            let Some(stem) = Path::new(&name).file_stem().and_then(|stem| stem.to_str()).map(str::to_owned) else { continue };
            let group = groups.entry(stem.to_ascii_lowercase()).or_insert_with(|| CvFiles { stem: stem.clone(), tex: None, pdf: None });
            // The first name in order wins when two differ only in case.
            if is_extension(&name, "tex") {
                if group.tex.is_none() {
                    group.stem = stem;
                    group.tex = Some(stamp);
                }
            } else if group.pdf.is_none() {
                group.pdf = Some(stamp);
            }
        }
        Ok(groups.into_values().collect())
    }

    /// The file's stamp. Its path is the one under the checkout's root, links unresolved, as the dossier names the same
    /// PDF, so both screens key its pages alike. What it weighs and when it changed come from the resolved file.
    fn stamp(&self, name: &str) -> Option<FileStamp> {
        let (path, resolved) = self.named(name)?;
        let metadata = fs::metadata(&resolved).ok()?;
        let moment = metadata.modified().ok()?;
        let modified = moment.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        Some(FileStamp { name: name.to_owned(), path, modified, at: DateTime::<Local>::from(moment).naive_local(), bytes: metadata.len() })
    }

    /// `cv/` with every link resolved, only when it sits inside the resolved checkout: a `cv` committed as a link to a folder
    /// elsewhere shows nothing.
    fn resolved_folder(&self) -> Option<PathBuf> {
        let root = fs::canonicalize(&self.root).ok()?;
        let folder = fs::canonicalize(self.folder()).ok()?;
        folder.starts_with(&root).then_some(folder)
    }

    /// The file `name` names in `cv/`, with every link resolved: a regular file that sits inside the resolved `cv/`,
    /// which sits inside the resolved checkout.
    /// A link to a device, a folder or anything outside `cv/` gives `None`, and so does a name that is not one plain part.
    fn checked(&self, name: &str) -> Option<PathBuf> {
        self.named(name).map(|(_, resolved)| resolved)
    }

    /// The file `name` names in `cv/` as the checkout's root reaches it, links unresolved, then as `checked` gives it.
    /// `None` wherever `checked` gives `None`.
    fn named(&self, name: &str) -> Option<(PathBuf, PathBuf)> {
        let folder = self.folder();
        let path = within_workspace(&self.root, &format!("{FOLDER}/{name}"))?;
        // `within_workspace` trims and splits: only a path that is exactly the name, one part down, is the name asked for.
        if path.parent() != Some(folder.as_path()) || path.file_name().and_then(|part| part.to_str()) != Some(name) {
            return None;
        }
        let inside = self.resolved_folder()?;
        let resolved = fs::canonicalize(&path).ok()?;
        (resolved.starts_with(&inside) && fs::metadata(&resolved).ok()?.is_file()).then_some((path, resolved))
    }

    /// The PDF to open, only when the file the name leads to is a PDF itself and not a link to something else.
    pub fn pdf_to_open(&self, name: &str) -> Option<PathBuf> {
        self.checked(name).filter(|path| path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")))
    }

    pub fn file_to_reveal(&self, name: &str) -> Option<PathBuf> {
        self.checked(name)
    }

    /// Opens the PDF in the system's viewer. The check runs again now, and a name that fails it does nothing.
    /// The path is one argument and no shell reads it. When the program is missing there is nothing to show, and nothing to report.
    ///
    /// The check runs once, before the spawn. A running task can replace the file with a link between the two, so this narrows
    /// the window and does not close it: the same limit as the check before a profile save.
    pub fn open_pdf(&self, name: &str) {
        let Some(path) = self.pdf_to_open(name) else { return };
        run(open_command(Host::current(), std::env::var_os("SystemRoot").as_deref(), &path));
    }

    /// Shows the file in Finder or Explorer, selected. The desktop's file manager gets its folder. The check runs again now,
    /// with the same limit as `open_pdf`: a file replaced by a link after the check is not caught.
    pub fn reveal(&self, name: &str) {
        let Some(path) = self.file_to_reveal(name) else { return };
        if let Some(spawn) = reveal_command(Host::current(), std::env::var_os("SystemRoot").as_deref(), &path) {
            run(spawn);
        }
    }
}

/// The systems that open a file differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Host {
    Mac,
    Windows,
    Other,
}

impl Host {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Host::Mac
        } else if cfg!(windows) {
            Host::Windows
        } else {
            Host::Other
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Arg {
    /// One argument, quoted by the standard library as needed.
    Plain(OsString),
    /// A command line piece passed as it is. Explorer reads a comma as its own separator unless the path is quoted.
    Raw(String),
}

/// A program and its arguments, before anything starts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Spawn {
    program: PathBuf,
    args: Vec<Arg>,
}

fn explorer(system_root: Option<&OsStr>) -> PathBuf {
    system_program(system_root, &["explorer.exe"])
}

/// The path in quotes for Explorer. A file name cannot hold a quote on Windows, so the quoted path cannot be cut short.
fn quoted(path: &Path) -> String {
    format!("\"{}\"", shell_path(path).display())
}

fn open_command(host: Host, system_root: Option<&OsStr>, path: &Path) -> Spawn {
    match host {
        Host::Mac => Spawn { program: PathBuf::from("open"), args: vec![Arg::Plain(path.into())] },
        Host::Windows => Spawn { program: explorer(system_root), args: vec![Arg::Raw(quoted(path))] },
        Host::Other => Spawn { program: PathBuf::from("xdg-open"), args: vec![Arg::Plain(path.into())] },
    }
}

/// `None` only when there is no folder to give the desktop's file manager.
fn reveal_command(host: Host, system_root: Option<&OsStr>, path: &Path) -> Option<Spawn> {
    match host {
        Host::Mac => Some(Spawn { program: PathBuf::from("open"), args: vec![Arg::Plain("-R".into()), Arg::Plain(path.into())] }),
        Host::Windows => Some(Spawn { program: explorer(system_root), args: vec![Arg::Raw(format!("/select,{}", quoted(path)))] }),
        Host::Other => Some(Spawn { program: PathBuf::from("xdg-open"), args: vec![Arg::Plain(path.parent()?.into())] }),
    }
}

/// Starts the program with no input and no output, and does not wait for it.
/// Explorer is a window program and ignores `no_window`, which only keeps a console program from opening a console.
fn run(spawn: Spawn) {
    let mut command = Command::new(spawn.program);
    for arg in spawn.args {
        match arg {
            Arg::Plain(arg) => {
                command.arg(arg);
            }
            #[cfg(windows)]
            Arg::Raw(text) => {
                std::os::windows::process::CommandExt::raw_arg(&mut command, text);
            }
            #[cfg(not(windows))]
            Arg::Raw(text) => {
                command.arg(text);
            }
        }
    }
    no_window(&mut command);
    let _ = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Windows resolves a link to the `\\?\C:\...` form, or `\\?\UNC\server\share\...` for a network share. Explorer and the viewers
/// do not all read those: they go without the prefix, as `C:\...` and `\\server\share\...`.
fn shell_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix(r"\\?\") else { return path.to_owned() };
    if let Some(share) = rest.strip_prefix(r"UNC\") {
        return PathBuf::from(format!(r"\\{share}"));
    }
    if rest.as_bytes().get(1) == Some(&b':') { PathBuf::from(rest) } else { path.to_owned() }
}

fn is_extension(name: &str, wanted: &str) -> bool {
    Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
}

/// Whether a name is one the list may show: a `.tex` or `.pdf` that is not hidden and has nothing a screen would drop or act on.
fn listed(name: &str) -> bool {
    !name.starts_with('.')
        && (is_extension(name, "tex") || is_extension(name, "pdf"))
        && !name.chars().any(char::is_control)
        && visible_chars(name).eq(name.chars())
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::Duration;

    use super::*;
    use crate::profile::application::{CvShelf, cv_shelf};
    use crate::profile::domain::Freshness;

    fn folder() -> (tempfile::TempDir, CvFolder) {
        let dir = tempfile::tempdir().unwrap();
        let folder = CvFolder::in_workspace(dir.path());
        (dir, folder)
    }

    fn put(dir: &Path, name: &str) {
        fs::create_dir_all(dir.join("cv")).unwrap();
        fs::write(dir.join("cv").join(name), b"x").unwrap();
    }

    fn at(dir: &Path, name: &str, seconds: u64) {
        let file = File::options().write(true).open(dir.join("cv").join(name)).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds)).unwrap();
    }

    fn names(files: &CvFiles) -> (Option<&str>, Option<&str>) {
        (files.tex.as_ref().map(|file| file.name.as_str()), files.pdf.as_ref().map(|file| file.name.as_str()))
    }

    #[test]
    fn a_missing_folder_is_an_empty_list() {
        let (_dir, cv) = folder();
        assert_eq!(cv.list(), Ok(Vec::new()));
    }

    #[test]
    fn a_cv_that_is_a_file_is_an_error() {
        let (dir, cv) = folder();
        fs::write(dir.path().join("cv"), b"x").unwrap();
        assert_eq!(cv.list().unwrap_err().source, "cv/");
    }

    #[test]
    fn files_group_by_stem_and_the_rest_is_ignored() {
        let (dir, cv) = folder();
        for name in ["main_a.tex", "main_a.pdf", "main_b.PDF", "c.TeX", "notes.md", "x.aux", ".DS_Store", "._a.pdf", ".hidden.tex"] {
            put(dir.path(), name);
        }
        fs::create_dir_all(dir.path().join("cv/d.pdf")).unwrap();
        fs::create_dir_all(dir.path().join("cv/sub")).unwrap();
        fs::write(dir.path().join("cv/sub/inner.pdf"), b"x").unwrap();
        let files = cv.list().unwrap();
        let stems: Vec<&str> = files.iter().map(|files| files.stem.as_str()).collect();
        assert_eq!(stems, ["c", "main_a", "main_b"]);
        assert_eq!(names(&files[0]), (Some("c.TeX"), None));
        assert_eq!(names(&files[1]), (Some("main_a.tex"), Some("main_a.pdf")));
        assert_eq!(names(&files[2]), (None, Some("main_b.PDF")));
        let pdf = files[1].pdf.as_ref().unwrap();
        assert_eq!(pdf.bytes, 1);
        assert!(pdf.modified > 0);
    }

    #[test]
    fn a_newer_tex_reads_source_newer() {
        let (dir, cv) = folder();
        put(dir.path(), "main_a.tex");
        put(dir.path(), "main_a.pdf");
        at(dir.path(), "main_a.pdf", 1_000);
        at(dir.path(), "main_a.tex", 2_000);
        let CvShelf::Read(documents) = cv_shelf(cv.list().unwrap()) else { panic!("read") };
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].freshness, Freshness::SourceNewer);
        assert_eq!((documents[0].pdf.as_ref().unwrap().modified, documents[0].tex.as_ref().unwrap().modified), (1_000, 2_000));
    }

    #[test]
    fn the_modification_time_and_size_come_from_the_file() {
        let (dir, cv) = folder();
        put(dir.path(), "a.pdf");
        at(dir.path(), "a.pdf", 86_400 * 365);
        let stamp = cv.list().unwrap().remove(0).pdf.unwrap();
        assert_eq!((stamp.modified, stamp.bytes), (86_400 * 365, 1));
    }

    #[test]
    fn a_name_that_differs_only_in_case_takes_the_first_in_name_order() {
        let (dir, cv) = folder();
        put(dir.path(), "a.pdf");
        put(dir.path(), "a.PDF");
        let files = cv.list().unwrap();
        assert_eq!(files.len(), 1);
        // A disk that ignores case holds one file for the two names, and the name it kept is the one that shows.
        let kept = fs::read_dir(dir.path().join("cv")).unwrap().count();
        let name = files[0].pdf.as_ref().unwrap().name.as_str();
        assert!(kept == 1 || name == "a.PDF", "{name}");
    }

    #[test]
    fn a_name_with_a_hidden_or_control_character_is_skipped() {
        let (dir, cv) = folder();
        put(dir.path(), "main_example.tex");
        let twins = ["main\u{200B}_example.tex", "main_a\u{202e}b.pdf", "tab\there.pdf", "bell\u{7}.pdf", "esc\u{1b}[31m.tex", "c1\u{85}.pdf"];
        for name in twins {
            assert!(!listed(name), "{name:?}");
            // Some disks refuse a name like these, and the others must not list it.
            let _ = fs::write(dir.path().join("cv").join(name), b"x");
        }
        let files = cv.list().unwrap();
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(files[0].stem, "main_example");
        assert_eq!(names(&files[0]), (Some("main_example.tex"), None));
    }

    #[test]
    fn a_name_that_is_not_one_plain_part_is_refused() {
        let (dir, cv) = folder();
        put(dir.path(), "x.pdf");
        fs::create_dir_all(dir.path().join("cv/sub")).unwrap();
        fs::write(dir.path().join("cv/sub/inner.pdf"), b"x").unwrap();
        fs::write(dir.path().join("secret.pdf"), b"x").unwrap();
        assert!(cv.checked("x.pdf").is_some());
        for bad in ["../x.pdf", "../secret.pdf", "sub/inner.pdf", "sub\\inner.pdf", "x.pdf ", " x.pdf", "", ".", "..", "/x.pdf", "missing.pdf", "sub"] {
            assert_eq!(cv.checked(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_folder_is_not_a_file_to_open() {
        let (dir, cv) = folder();
        fs::create_dir_all(dir.path().join("cv/d.pdf")).unwrap();
        assert_eq!(cv.checked("d.pdf"), None);
        assert_eq!(cv.pdf_to_open("d.pdf"), None);
        assert_eq!(cv.file_to_reveal("d.pdf"), None);
    }

    #[test]
    fn a_pdf_is_opened_by_its_extension_in_any_case() {
        let (dir, cv) = folder();
        put(dir.path(), "a.PDF");
        put(dir.path(), "b.tex");
        assert!(cv.pdf_to_open("a.PDF").is_some());
        assert_eq!(cv.pdf_to_open("b.tex"), None);
        assert!(cv.file_to_reveal("b.tex").is_some());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_outside_the_workspace_is_skipped_and_one_inside_cv_is_kept() {
        use std::os::unix::fs::symlink;
        let (dir, cv) = folder();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.pdf"), b"secret").unwrap();
        put(dir.path(), "real.pdf");
        symlink(outside.path().join("secret.pdf"), dir.path().join("cv/out.pdf")).unwrap();
        symlink(dir.path().join("cv/real.pdf"), dir.path().join("cv/in.pdf")).unwrap();
        symlink(dir.path().join("cv/missing.pdf"), dir.path().join("cv/dangling.pdf")).unwrap();
        symlink("/dev/null", dir.path().join("cv/device.pdf")).unwrap();
        assert_eq!(cv.checked("out.pdf"), None);
        assert_eq!(cv.pdf_to_open("out.pdf"), None);
        assert_eq!(cv.file_to_reveal("out.pdf"), None);
        assert_eq!(cv.checked("dangling.pdf"), None);
        assert_eq!(cv.checked("device.pdf"), None);
        assert!(cv.checked("in.pdf").is_some());
        let stems: Vec<String> = cv.list().unwrap().into_iter().map(|files| files.stem).collect();
        assert_eq!(stems, ["in", "real"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_pdf_link_that_leads_to_a_non_pdf_is_not_opened() {
        use std::os::unix::fs::symlink;
        let (dir, cv) = folder();
        put(dir.path(), "main_a.tex");
        symlink(dir.path().join("cv/main_a.tex"), dir.path().join("cv/x.pdf")).unwrap();
        assert!(cv.checked("x.pdf").is_some());
        assert_eq!(cv.pdf_to_open("x.pdf"), None);
        assert!(cv.file_to_reveal("x.pdf").is_some());
    }

    #[test]
    fn the_extended_prefix_is_dropped_from_a_drive_or_share_path() {
        assert_eq!(shell_path(Path::new(r"\\?\C:\w\cv\a.pdf")), PathBuf::from(r"C:\w\cv\a.pdf"));
        assert_eq!(shell_path(Path::new(r"\\?\UNC\host\share\cv\a.pdf")), PathBuf::from(r"\\host\share\cv\a.pdf"));
        assert_eq!(shell_path(Path::new(r"\\?\Volume{1}\a.pdf")), PathBuf::from(r"\\?\Volume{1}\a.pdf"));
        assert_eq!(shell_path(Path::new("/w/cv/a.pdf")), PathBuf::from("/w/cv/a.pdf"));
    }

    #[cfg(unix)]
    #[test]
    fn a_cv_that_is_a_link_out_of_the_checkout_shows_nothing() {
        use std::os::unix::fs::symlink;
        let (dir, cv) = folder();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.pdf"), b"secret").unwrap();
        symlink(outside.path(), dir.path().join("cv")).unwrap();
        assert_eq!(cv.list(), Ok(Vec::new()));
        assert_eq!(cv.checked("secret.pdf"), None);
        assert_eq!(cv.pdf_to_open("secret.pdf"), None);
        assert_eq!(cv.file_to_reveal("secret.pdf"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_cv_that_is_a_link_to_a_folder_inside_the_checkout_is_read() {
        use std::os::unix::fs::symlink;
        let (dir, cv) = folder();
        fs::create_dir_all(dir.path().join("documents")).unwrap();
        fs::write(dir.path().join("documents/main_a.pdf"), b"x").unwrap();
        symlink(dir.path().join("documents"), dir.path().join("cv")).unwrap();
        assert_eq!(cv.list().unwrap().len(), 1);
        assert!(cv.checked("main_a.pdf").is_some());
    }

    #[test]
    fn mac_and_the_desktop_open_a_pdf_with_the_path_as_one_argument() {
        let path = Path::new("/w/cv/CV, final.pdf");
        let plain = |text: &str| Arg::Plain(text.into());
        let mac = open_command(Host::Mac, None, path);
        assert_eq!((mac.program, mac.args), (PathBuf::from("open"), vec![plain("/w/cv/CV, final.pdf")]));
        let other = open_command(Host::Other, None, path);
        assert_eq!((other.program, other.args), (PathBuf::from("xdg-open"), vec![plain("/w/cv/CV, final.pdf")]));
    }

    #[test]
    fn explorer_gets_a_path_with_a_comma_in_quotes() {
        let path = Path::new(r"\\?\C:\w\cv\CV, final.pdf");
        let root = OsStr::new(r"D:\Win");
        let open = open_command(Host::Windows, Some(root), path);
        assert_eq!(open.program, explorer(Some(root)));
        assert_eq!(open.args, [Arg::Raw(r#""C:\w\cv\CV, final.pdf""#.to_owned())]);
        let reveal = reveal_command(Host::Windows, Some(root), path).unwrap();
        assert_eq!(reveal.program, explorer(Some(root)));
        assert_eq!(reveal.args, [Arg::Raw(r#"/select,"C:\w\cv\CV, final.pdf""#.to_owned())]);
    }

    #[test]
    fn each_system_shows_a_file_its_own_way() {
        let path = Path::new("/w/cv/a.pdf");
        let plain = |text: &str| Arg::Plain(text.into());
        let mac = reveal_command(Host::Mac, None, path).unwrap();
        assert_eq!((mac.program, mac.args), (PathBuf::from("open"), vec![plain("-R"), plain("/w/cv/a.pdf")]));
        let other = reveal_command(Host::Other, None, path).unwrap();
        assert_eq!((other.program, other.args), (PathBuf::from("xdg-open"), vec![plain("/w/cv")]));
        assert_eq!(reveal_command(Host::Other, None, Path::new("/")), None, "a path with no folder gives nothing to show");
    }
}
