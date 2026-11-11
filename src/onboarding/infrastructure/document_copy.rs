use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use hayro::hayro_syntax::Pdf;

use crate::shared_kernel::own_folder;

use crate::onboarding::domain::{
    CopyFailure, DOCUMENTS, DocumentCopier, DocumentKind, MAX_DOCUMENT, MAX_NUMBER, Refusal, by_name, copy_name, looks_like_pdf, numbered, shown_name,
};

/// Copies the user's documents into the job search folder. It reads each file once through a handle it checked is the
/// regular file it looked at, and writes only new files directly in the subfolder of `documents/` that the document's
/// kind names.
pub struct FileCopier {
    /// The data folder's `onboarding/`, where each running Jobdesk holds its `<pid>` folder locked, which tells a part
    /// file of a live copy from one a stopped Jobdesk left.
    claims: PathBuf,
    /// How a copy takes its name: a hard link, which a test can make fail as a disk without hard links does.
    link: Naming,
    /// How a copy takes its name where the link fails: a rename that refuses a taken name, which a test can make fail.
    rename: Naming,
}

/// Gives the part file at the first path the second path as its name, in one step, failing with `AlreadyExists` when
/// the name is taken.
type Naming = fn(&Path, &Path) -> io::Result<()>;

impl FileCopier {
    /// `claims` is the data folder's `onboarding/`.
    pub fn new(claims: PathBuf) -> Self {
        Self { claims, link: |part, target| fs::hard_link(part, target), rename: rename_exclusive }
    }
}

impl DocumentCopier for FileCopier {
    fn look(&self, source: &Path) -> Result<Option<usize>, Refusal> {
        by_name(&shown_name(source))?;
        let mut file = open_regular(source)?;
        let mut bytes = Vec::new();
        (&mut file).take(MAX_DOCUMENT + 1).read_to_end(&mut bytes).map_err(|_| Refusal::Unreadable)?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err(Refusal::TooLarge);
        }
        if !looks_like_pdf(&bytes[..bytes.len().min(1029)]) {
            return Err(Refusal::NotPdf);
        }
        Ok(page_count(bytes))
    }

    /// The bytes go to a hidden part file in the kind's subfolder first, on disk before the copy takes its name, so a
    /// crash never leaves a cut PDF under a real name. The name is then linked to the part file, which fails on any name
    /// already there, a link included, so nothing is written over. On a disk without hard links the part file is
    /// renamed onto the name instead, on macOS with a rename that refuses a taken name. A disk that can do neither
    /// refuses the copy, since no other way gives the name in one step without writing over a file. Part files a
    /// stopped copy left are removed first, as `clear_stray_parts` says. Nothing is written in the `documents/` root.
    fn copy(&self, source: &Path, folder: &Path, kind: DocumentKind) -> Result<String, CopyFailure> {
        let name = shown_name(source);
        by_name(&name).map_err(CopyFailure::Refused)?;
        let mut file = open_regular(source).map_err(CopyFailure::Refused)?;
        let documents = kind_folder(folder, kind)?;
        let canonical = fs::canonicalize(&documents).map_err(|_| CopyFailure::NoDocumentsFolder)?;
        if let Some(kept) = already_inside(source, &canonical) {
            return Ok(kept);
        }
        // Held from now until the process exits, so another Jobdesk leaves this one's part files alone. Best effort: a
        // copy goes on without it.
        let _ = own_folder::claim(&self.claims);
        for each in DocumentKind::ALL {
            clear_stray_parts(&folder.join(DOCUMENTS).join(each.folder()), &self.claims);
        }
        let part = documents.join(part_name());
        let out = OpenOptions::new().write(true).create_new(true).open(&part).map_err(|_| CopyFailure::Failed)?;
        if !stays_in(&part, &canonical) {
            let _ = fs::remove_file(&part);
            return Err(CopyFailure::NoDocumentsFolder);
        }
        write_copy(&mut file, out, &part)?;
        let placed = place(&part, &documents, &canonical, &copy_name(&name), self.link, self.rename);
        let _ = fs::remove_file(&part);
        placed
    }
}

/// What a part file's name starts and ends with.
const PART_PREFIX: &str = ".jobdesk-copy-";
const PART_SUFFIX: &str = ".part";

/// A part file of a Jobdesk that still runs, other than this one, older than this belongs to a copy that never
/// finished: a copy takes seconds.
const STRAY_PART_AGE: Duration = Duration::from_secs(10 * 60);

/// A part file name no other copy uses: this process's id and a count.
fn part_name() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    format!("{PART_PREFIX}{}-{}{PART_SUFFIX}", process::id(), COUNT.fetch_add(1, Ordering::Relaxed))
}

/// The process id a part file's name carries.
fn part_pid(name: &str) -> Option<u32> {
    name.strip_prefix(PART_PREFIX)?.strip_suffix(PART_SUFFIX)?.split_once('-')?.0.parse().ok()
}

/// Removes the part files that copies which never finished left in a subfolder of `documents/`, since `/setup` reads
/// every file there. One of this process goes at once, as its copies run one after another and none runs now. So does
/// one of a Jobdesk that no longer runs: one whose `<pid>` folder in `claims` no running Jobdesk holds locked. One of
/// another running Jobdesk goes only once it is `STRAY_PART_AGE` old. Only regular files of that name go, never a link.
fn clear_stray_parts(folder: &Path, claims: &Path) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(PART_PREFIX) || !name.ends_with(PART_SUFFIX) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
        if !meta.is_file() {
            continue;
        }
        let stray = match part_pid(&name) {
            Some(pid) if pid != process::id() && own_folder::in_use(&claims.join(pid.to_string())) => {
                meta.modified().ok().and_then(|at| SystemTime::now().duration_since(at).ok()).is_some_and(|age| age > STRAY_PART_AGE)
            }
            _ => true,
        };
        if stray {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Gives the copy in `part` its name, `name` or the first free numbered one, in one step and never over a file. `link`
/// makes the name a hard link to the part file. Once it fails for any reason but a taken name, as on a disk without
/// hard links, every name is tried with `rename` instead. When that fails too for any reason but a taken name, the
/// disk is refused: a file the bytes were copied into would sit under its real name while it fills.
fn place(part: &Path, documents: &Path, canonical: &Path, name: &str, link: Naming, rename: Naming) -> Result<String, CopyFailure> {
    let mut linking = true;
    for number in 1..=MAX_NUMBER {
        let candidate = if number == 1 { name.to_owned() } else { numbered(name, number) };
        let target = documents.join(&candidate);
        let placed = if linking {
            match link(part, &target) {
                Err(error) if error.kind() != ErrorKind::AlreadyExists => {
                    linking = false;
                    rename(part, &target)
                }
                linked => linked,
            }
        } else {
            rename(part, &target)
        };
        match placed {
            Ok(()) if stays_in(&target, canonical) => return Ok(candidate),
            Ok(()) => {
                let _ = fs::remove_file(&target);
                return Err(CopyFailure::NoDocumentsFolder);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) if !linking => return Err(CopyFailure::UnsafeDisk),
            Err(_) => return Err(CopyFailure::Failed),
        }
    }
    Err(CopyFailure::NoFreeName)
}

/// Renames `from` to `to` in one step, with `renamex_np` and `RENAME_EXCL`, which fails when `to` exists, a link
/// included.
#[cfg(target_os = "macos")]
fn rename_exclusive(from: &Path, to: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both are valid NUL-terminated paths that live until the call returns.
    let done = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if done == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// Elsewhere no rename that refuses a taken name is at hand, so a disk without hard links is refused.
#[cfg(not(target_os = "macos"))]
fn rename_exclusive(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::from(ErrorKind::Unsupported))
}

/// Whether the file at `path` sits directly in the folder that `canonical` names, links resolved: a best-effort check
/// that `documents/` was not swapped for a link while the copy ran.
fn stays_in(path: &Path, canonical: &Path) -> bool {
    fs::canonicalize(path).is_ok_and(|resolved| resolved.parent() == Some(canonical))
}

/// Opens `path` for reading only when it is a regular file and not a link, and checks that the handle reads the very
/// file looked at, so a link put in its place meanwhile is not followed.
fn open_regular(path: &Path) -> Result<File, Refusal> {
    let looked = fs::symlink_metadata(path).map_err(|_| Refusal::Unreadable)?;
    if looked.file_type().is_symlink() {
        return Err(Refusal::Link);
    }
    if !looked.is_file() {
        return Err(Refusal::NotAFile);
    }
    if looked.len() > MAX_DOCUMENT {
        return Err(Refusal::TooLarge);
    }
    let file = open_read_only(path).map_err(|_| Refusal::Unreadable)?;
    let opened = file.metadata().map_err(|_| Refusal::Unreadable)?;
    if !opened.is_file() || !same_file(&looked, &opened) {
        return Err(Refusal::NotAFile);
    }
    Ok(file)
}

#[cfg(unix)]
fn open_read_only(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // No link is followed, and a special file swapped in cannot hold the open.
    OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)
}

#[cfg(not(unix))]
fn open_read_only(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
fn same_file(looked: &Metadata, opened: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    looked.dev() == opened.dev() && looked.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_file(looked: &Metadata, opened: &Metadata) -> bool {
    looked.len() == opened.len()
}

/// `documents/<kind folder>/` of the job search folder, each folder made when it is missing. The job search folder,
/// `documents/` and the subfolder must all be real folders, never links, so nothing is written outside them.
fn kind_folder(folder: &Path, kind: DocumentKind) -> Result<PathBuf, CopyFailure> {
    if !fs::symlink_metadata(folder).is_ok_and(|meta| meta.is_dir()) {
        return Err(CopyFailure::NoDocumentsFolder);
    }
    let documents = real_folder(folder.join(DOCUMENTS))?;
    real_folder(documents.join(kind.folder()))
}

/// `path` when it is a real folder, never a link, made when it is missing.
fn real_folder(path: PathBuf) -> Result<PathBuf, CopyFailure> {
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.is_dir() => Ok(path),
        Ok(_) => Err(CopyFailure::NoDocumentsFolder),
        Err(error) if error.kind() == ErrorKind::NotFound => fs::create_dir(&path).map(|()| path).map_err(|_| CopyFailure::NoDocumentsFolder),
        Err(_) => Err(CopyFailure::NoDocumentsFolder),
    }
}

/// The name of a file the user added from the kind's subfolder itself, whose links-resolved path is `canonical`, which
/// needs no copy.
fn already_inside(source: &Path, canonical: &Path) -> Option<String> {
    let source = fs::canonicalize(source).ok()?;
    (source.parent() == Some(canonical)).then(|| source.file_name().map(|name| name.to_string_lossy().into_owned())).flatten()
}

/// Copies what `file` reads into the new part file `out`, on disk before it returns. A part file that fails or grows past
/// the limit is removed.
fn write_copy(file: &mut File, mut out: File, target: &Path) -> Result<(), CopyFailure> {
    let written = io::copy(&mut file.take(MAX_DOCUMENT + 1), &mut out).and_then(|copied| out.flush().and_then(|()| out.sync_all()).map(|()| copied));
    drop(out);
    match written {
        Ok(copied) if copied <= MAX_DOCUMENT => Ok(()),
        Ok(_) => {
            let _ = fs::remove_file(target);
            Err(CopyFailure::Refused(Refusal::TooLarge))
        }
        Err(_) => {
            let _ = fs::remove_file(target);
            Err(CopyFailure::Failed)
        }
    }
}

/// How many pages the PDF holds, or `None` when hayro cannot read it, as for an encrypted one. A PDF it cannot count
/// is still taken: Claude may read what the renderer cannot.
fn page_count(bytes: Vec<u8>) -> Option<usize> {
    panic::catch_unwind(AssertUnwindSafe(|| Pdf::new(Arc::new(bytes)).ok().map(|pdf| pdf.pages().len()))).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF of `pages` empty A4 pages, with its cross-reference offsets counted as it is written.
    fn pdf(pages: usize) -> Vec<u8> {
        let kids: Vec<String> = (0..pages).map(|page| format!("{} 0 R", 3 + page)).collect();
        let mut objects = vec!["<</Type/Catalog/Pages 2 0 R>>".to_owned(), format!("<</Type/Pages/Kids[{}]/Count {pages}>>", kids.join(" "))];
        objects.extend((0..pages).map(|_| "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]>>".to_owned()));
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
        out
    }

    /// A job search folder with its `documents/`, and a Desktop holding the user's files.
    fn places() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let (folder, desktop) = (root.path().join("Job search"), root.path().join("Desktop"));
        fs::create_dir_all(folder.join(DOCUMENTS)).unwrap();
        fs::create_dir_all(&desktop).unwrap();
        (root, folder, desktop)
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// Fails when the `documents/` root of `folder` holds anything but real subfolders.
    fn root_holds_only_subfolders(folder: &Path) {
        for entry in fs::read_dir(folder.join(DOCUMENTS)).unwrap() {
            let entry = entry.unwrap();
            assert!(fs::symlink_metadata(entry.path()).unwrap().is_dir(), "{:?} in the documents root", entry.file_name());
        }
    }

    fn cv_folder(folder: &Path) -> PathBuf {
        folder.join(DOCUMENTS).join("cv")
    }

    /// The data folder's `onboarding/` of a test, beside its job search folder or Desktop.
    fn claims(beside: &Path) -> PathBuf {
        beside.parent().unwrap().join("data").join("onboarding")
    }

    fn copier(beside: &Path) -> FileCopier {
        FileCopier::new(claims(beside))
    }

    /// A copier on a disk without hard links.
    fn linkless(beside: &Path) -> FileCopier {
        FileCopier { link: |_, _| Err(io::Error::from(ErrorKind::Unsupported)), ..copier(beside) }
    }

    use DocumentKind::{Cv, Reference};

    #[test]
    fn a_pdf_shows_its_page_count() {
        let (_root, _, desktop) = places();
        fs::write(desktop.join("cv.pdf"), pdf(2)).unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("cv.pdf")), Ok(Some(2)));
        fs::write(desktop.join("odd.pdf"), b"%PDF-1.4\nnothing else").unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("odd.pdf")), Ok(None), "a PDF the renderer cannot count is still taken");
    }

    #[test]
    fn a_word_file_or_a_file_that_is_no_pdf_is_refused() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("letter.docx"), b"PK\x03\x04").unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("letter.docx")), Err(Refusal::Word));
        assert_eq!(copier(&folder).copy(&desktop.join("letter.docx"), &folder, Reference), Err(CopyFailure::Refused(Refusal::Word)));
        fs::write(desktop.join("renamed.pdf"), b"PK\x03\x04word/document.xml").unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("renamed.pdf")), Err(Refusal::NotPdf));
        fs::create_dir(desktop.join("folder.pdf")).unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("folder.pdf")), Err(Refusal::NotAFile));
        assert_eq!(copier(&desktop).look(&desktop.join("gone.pdf")), Err(Refusal::Unreadable));
        assert_eq!(names(&folder.join(DOCUMENTS)), Vec::<String>::new(), "nothing was copied");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_refused_and_never_followed() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        std::os::unix::fs::symlink(desktop.join("cv.pdf"), desktop.join("link.pdf")).unwrap();
        assert_eq!(copier(&desktop).look(&desktop.join("link.pdf")), Err(Refusal::Link));
        assert_eq!(copier(&folder).copy(&desktop.join("link.pdf"), &folder, Cv), Err(CopyFailure::Refused(Refusal::Link)));
    }

    #[test]
    fn a_cv_lands_in_documents_cv_and_a_missing_subfolder_is_made() {
        let (_root, folder, desktop) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        fs::write(desktop.join("Sam Rivera CV.pdf"), pdf(2)).unwrap();
        fs::write(desktop.join("Tessel.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("Sam Rivera CV.pdf"), &folder, Cv).as_deref(), Ok("Sam Rivera CV.pdf"));
        assert_eq!(fs::read(cv_folder(&folder).join("Sam Rivera CV.pdf")).unwrap(), pdf(2));
        assert_eq!(fs::read(desktop.join("Sam Rivera CV.pdf")).unwrap(), pdf(2), "copied, never moved");
        assert_eq!(copier(&folder).copy(&desktop.join("Tessel.pdf"), &folder, Reference).as_deref(), Ok("Tessel.pdf"));
        assert_eq!(names(&folder.join(DOCUMENTS).join("references")), ["Tessel.pdf"]);
        assert_eq!(names(&folder.join(DOCUMENTS)), ["cv", "references"]);
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_name_clash_gets_a_number_within_its_subfolder_only() {
        let (_root, folder, desktop) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        fs::write(cv_folder(&folder).join("cv.pdf"), b"the one already there").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (2).pdf"));
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (3).pdf"));
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Reference).as_deref(), Ok("cv.pdf"), "another subfolder has no clash");
        assert_eq!(fs::read(cv_folder(&folder).join("cv.pdf")).unwrap(), b"the one already there");
        assert_eq!(names(&cv_folder(&folder)), ["cv (2).pdf", "cv (3).pdf", "cv.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_the_subfolder_counts_as_taken_and_is_never_written_through() {
        let (root, folder, desktop) = places();
        let outside = root.path().join("outside.pdf");
        fs::create_dir(cv_folder(&folder)).unwrap();
        std::os::unix::fs::symlink(&outside, cv_folder(&folder).join("cv.pdf")).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (2).pdf"));
        assert!(!outside.exists(), "the dangling link led nowhere");
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_hidden_character_or_a_leading_dot_never_reaches_the_copys_name() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("cv\u{202e}fdp.pdf"), pdf(1)).unwrap();
        fs::write(desktop.join(".profile.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv\u{202e}fdp.pdf"), &folder, Cv).as_deref(), Ok("cvfdp.pdf"));
        assert_eq!(copier(&folder).copy(&desktop.join(".profile.pdf"), &folder, Cv).as_deref(), Ok("profile.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cvfdp.pdf", "profile.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_missing_documents_folder_is_made_inside_the_folder() {
        let (_root, folder, desktop) = places();
        fs::remove_dir(folder.join(DOCUMENTS)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[cfg(unix)]
    #[test]
    fn a_documents_folder_that_is_a_link_is_never_written_into() {
        let (root, folder, desktop) = places();
        let elsewhere = root.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::remove_dir(folder.join(DOCUMENTS)).unwrap();
        std::os::unix::fs::symlink(&elsewhere, folder.join(DOCUMENTS)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(names(&elsewhere), Vec::<String>::new());
        let linked = root.path().join("Linked search");
        std::os::unix::fs::symlink(&folder, &linked).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &linked, Cv), Err(CopyFailure::NoDocumentsFolder), "nor a job search folder that is one");
    }

    #[cfg(unix)]
    #[test]
    fn a_subfolder_that_is_a_link_is_never_written_into() {
        let (root, folder, desktop) = places();
        let elsewhere = root.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, cv_folder(&folder)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(names(&elsewhere), Vec::<String>::new(), "the link's target stays empty");
    }

    #[test]
    fn a_subfolder_that_is_a_file_is_refused() {
        let (_root, folder, desktop) = places();
        fs::write(cv_folder(&folder), b"not a folder").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(fs::read(cv_folder(&folder)).unwrap(), b"not a folder");
    }

    fn part(pid: u32, count: u32) -> String {
        format!("{PART_PREFIX}{pid}-{count}{PART_SUFFIX}")
    }

    #[test]
    fn a_copy_clears_the_parts_of_stopped_copies_at_once_and_a_live_ones_only_when_old() {
        let (_root, folder, desktop) = places();
        let (cv, references) = (cv_folder(&folder), folder.join(DOCUMENTS).join("references"));
        fs::create_dir(&cv).unwrap();
        fs::create_dir(&references).unwrap();
        // A pid no running Jobdesk holds, this process's own, and another Jobdesk that still runs.
        let (gone, live) = (4_000_001, 4_000_002);
        let _held = own_folder::lock_folder(&claims(&folder).join(live.to_string())).unwrap();
        let old = |path: &Path| File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - STRAY_PART_AGE * 2).unwrap();
        for (at, name) in [(&references, part(gone, 0)), (&cv, part(process::id(), 7)), (&cv, part(live, 0)), (&references, part(live, 1))] {
            fs::write(at.join(name), b"half a PDF").unwrap();
        }
        old(&references.join(part(live, 1)));
        fs::write(cv.join(".jobdesk-copy-junk.part"), b"no pid").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv), [part(live, 0), "cv.pdf".to_owned()], "only the fresh part of a live copy stays");
        assert_eq!(names(&references), Vec::<String>::new(), "a stopped Jobdesk's part went at once, a live one's old part too");
        assert_eq!(fs::read(cv.join("cv.pdf")).unwrap(), pdf(1));
        root_holds_only_subfolders(&folder);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_disk_without_hard_links_still_gets_the_copy_and_never_a_file_written_over() {
        let (_root, folder, desktop) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        fs::write(cv_folder(&folder).join("cv.pdf"), b"the one already there").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(2)).unwrap();
        assert_eq!(linkless(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (2).pdf"));
        assert_eq!(linkless(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (3).pdf"));
        assert_eq!(fs::read(cv_folder(&folder).join("cv.pdf")).unwrap(), b"the one already there");
        assert_eq!(fs::read(cv_folder(&folder).join("cv (2).pdf")).unwrap(), pdf(2));
        assert_eq!(names(&cv_folder(&folder)), ["cv (2).pdf", "cv (3).pdf", "cv.pdf"], "no part file is left");
        root_holds_only_subfolders(&folder);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_rename_without_a_link_never_writes_over_a_file() {
        let (_root, folder, _) = places();
        let documents = folder.join(DOCUMENTS);
        fs::write(documents.join("from.part"), b"the bytes").unwrap();
        fs::write(documents.join("taken.pdf"), b"already there").unwrap();
        assert_eq!(rename_exclusive(&documents.join("from.part"), &documents.join("taken.pdf")).unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert_eq!(fs::read(documents.join("taken.pdf")).unwrap(), b"already there");
        rename_exclusive(&documents.join("from.part"), &documents.join("new.pdf")).unwrap();
        assert_eq!(fs::read(documents.join("new.pdf")).unwrap(), b"the bytes");
        assert!(!documents.join("from.part").exists(), "moved in one step");
    }

    /// A copier on a disk that has neither hard links nor a rename that refuses a taken name.
    fn unsafe_disk(beside: &Path) -> FileCopier {
        FileCopier { rename: |_, _| Err(io::Error::from(ErrorKind::Unsupported)), ..linkless(beside) }
    }

    /// Whatever way the copy gets its name, a file under a real name is whole or was there before: never a copy cut
    /// short. A disk that can take the name in no single step refuses the copy and keeps no part file.
    #[test]
    fn no_fallback_ever_leaves_a_named_file_shorter_than_the_source() {
        let renames = cfg!(target_os = "macos");
        let ways: [(&str, fn(&Path) -> FileCopier, bool); 3] = [("link", copier, true), ("rename", linkless, renames), ("neither", unsafe_disk, false)];
        for (way, make, places_it) in ways {
            let (_root, folder, desktop) = places();
            fs::create_dir(cv_folder(&folder)).unwrap();
            fs::write(cv_folder(&folder).join("cv.pdf"), b"the one already there").unwrap();
            fs::write(desktop.join("cv.pdf"), pdf(2)).unwrap();
            let copied = make(&folder).copy(&desktop.join("cv.pdf"), &folder, Cv);
            for name in names(&cv_folder(&folder)) {
                assert!(!name.starts_with(PART_PREFIX), "{way}: no part file is left");
                let bytes = fs::read(cv_folder(&folder).join(&name)).unwrap();
                let expected = if name == "cv.pdf" { b"the one already there".to_vec() } else { pdf(2) };
                assert_eq!(bytes, expected, "{way}: {name}");
            }
            if places_it {
                assert_eq!(copied.as_deref(), Ok("cv (2).pdf"), "{way}");
            } else {
                assert_eq!(copied, Err(CopyFailure::UnsafeDisk), "{way}");
                assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"], "{way}: nothing new");
            }
        }
        assert_eq!(CopyFailure::UnsafeDisk.line(), "This disk cannot take the copy safely. Choose a job search folder on your Mac's own disk.");
    }

    #[cfg(unix)]
    #[test]
    fn a_file_reached_through_a_link_does_not_stay_in_its_subfolder() {
        let (root, folder, _) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        let canonical = fs::canonicalize(cv_folder(&folder)).unwrap();
        fs::write(cv_folder(&folder).join("cv.pdf"), b"x").unwrap();
        assert!(stays_in(&cv_folder(&folder).join("cv.pdf"), &canonical));
        let elsewhere = root.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("cv.pdf"), b"x").unwrap();
        std::os::unix::fs::symlink(&elsewhere, folder.join("swapped")).unwrap();
        assert!(!stays_in(&folder.join("swapped").join("cv.pdf"), &canonical));
    }

    #[test]
    fn a_file_already_in_its_subfolder_is_not_copied_again() {
        let (_root, folder, _) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        fs::write(cv_folder(&folder).join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&cv_folder(&folder).join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_file_in_the_documents_root_is_copied_into_its_subfolder_and_left_as_it_was() {
        let (_root, folder, _) = places();
        let loose = folder.join(DOCUMENTS).join("cv.pdf");
        fs::write(&loose, pdf(1)).unwrap();
        assert_eq!(copier(&folder).copy(&loose, &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"]);
        assert_eq!(fs::read(&loose).unwrap(), pdf(1), "nothing moved");
        assert_eq!(names(&folder.join(DOCUMENTS)), ["cv", "cv.pdf"]);
    }
}
