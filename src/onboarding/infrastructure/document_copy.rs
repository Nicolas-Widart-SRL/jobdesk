use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use hayro::hayro_syntax::Pdf;

use crate::onboarding::domain::{
    CopyFailure, DOCUMENTS, DocumentCopier, DocumentKind, MAX_DOCUMENT, MAX_NUMBER, Refusal, by_name, copy_name, looks_like_pdf, numbered, shown_name,
};

/// Copies the user's documents into the job search folder. It reads each file once through a handle it checked is the
/// regular file it looked at, and writes only new files directly in the subfolder of `documents/` that the document's
/// kind names.
pub struct FileCopier;

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
    /// already there, a link included, so nothing is written over. Part files a copy left more than `STRAY_PART_AGE`
    /// ago are removed first. Nothing is written in the `documents/` root.
    fn copy(&self, source: &Path, folder: &Path, kind: DocumentKind) -> Result<String, CopyFailure> {
        let name = shown_name(source);
        by_name(&name).map_err(CopyFailure::Refused)?;
        let mut file = open_regular(source).map_err(CopyFailure::Refused)?;
        let documents = kind_folder(folder, kind)?;
        let canonical = fs::canonicalize(&documents).map_err(|_| CopyFailure::NoDocumentsFolder)?;
        if let Some(kept) = already_inside(source, &canonical) {
            return Ok(kept);
        }
        for each in DocumentKind::ALL {
            clear_stray_parts(&folder.join(DOCUMENTS).join(each.folder()));
        }
        let part = documents.join(part_name());
        let out = OpenOptions::new().write(true).create_new(true).open(&part).map_err(|_| CopyFailure::Failed)?;
        if !stays_in(&part, &canonical) {
            let _ = fs::remove_file(&part);
            return Err(CopyFailure::NoDocumentsFolder);
        }
        write_copy(&mut file, out, &part)?;
        let placed = place(&part, &documents, &canonical, &copy_name(&name));
        let _ = fs::remove_file(&part);
        placed
    }
}

/// What a part file's name starts and ends with.
const PART_PREFIX: &str = ".jobdesk-copy-";
const PART_SUFFIX: &str = ".part";

/// A part file older than this belongs to a copy that never finished: a copy takes seconds.
const STRAY_PART_AGE: Duration = Duration::from_secs(10 * 60);

/// A part file name no other copy uses: this process's id and a count.
fn part_name() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    format!("{PART_PREFIX}{}-{}{PART_SUFFIX}", process::id(), COUNT.fetch_add(1, Ordering::Relaxed))
}

/// Removes the part files that copies which never finished left in `documents/`. Only regular files of that name and
/// age go: a link is left alone, and so is the part file of a copy under way.
fn clear_stray_parts(documents: &Path) {
    let Ok(entries) = fs::read_dir(documents) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(PART_PREFIX) || !name.ends_with(PART_SUFFIX) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(entry.path()) else { continue };
        let old = meta.modified().ok().and_then(|at| SystemTime::now().duration_since(at).ok()).is_some_and(|age| age > STRAY_PART_AGE);
        if meta.is_file() && old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Gives the copy in `part` its name, `name` or the first free numbered one, never over a file.
fn place(part: &Path, documents: &Path, canonical: &Path, name: &str) -> Result<String, CopyFailure> {
    for number in 1..=MAX_NUMBER {
        let candidate = if number == 1 { name.to_owned() } else { numbered(name, number) };
        let target = documents.join(&candidate);
        match fs::hard_link(part, &target) {
            Ok(()) if stays_in(&target, canonical) => return Ok(candidate),
            Ok(()) => {
                let _ = fs::remove_file(&target);
                return Err(CopyFailure::NoDocumentsFolder);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(CopyFailure::Failed),
        }
    }
    Err(CopyFailure::NoFreeName)
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

    use DocumentKind::{Cv, Reference};

    #[test]
    fn a_pdf_shows_its_page_count() {
        let (_root, _, desktop) = places();
        fs::write(desktop.join("cv.pdf"), pdf(2)).unwrap();
        assert_eq!(FileCopier.look(&desktop.join("cv.pdf")), Ok(Some(2)));
        fs::write(desktop.join("odd.pdf"), b"%PDF-1.4\nnothing else").unwrap();
        assert_eq!(FileCopier.look(&desktop.join("odd.pdf")), Ok(None), "a PDF the renderer cannot count is still taken");
    }

    #[test]
    fn a_word_file_or_a_file_that_is_no_pdf_is_refused() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("letter.docx"), b"PK\x03\x04").unwrap();
        assert_eq!(FileCopier.look(&desktop.join("letter.docx")), Err(Refusal::Word));
        assert_eq!(FileCopier.copy(&desktop.join("letter.docx"), &folder, Reference), Err(CopyFailure::Refused(Refusal::Word)));
        fs::write(desktop.join("renamed.pdf"), b"PK\x03\x04word/document.xml").unwrap();
        assert_eq!(FileCopier.look(&desktop.join("renamed.pdf")), Err(Refusal::NotPdf));
        fs::create_dir(desktop.join("folder.pdf")).unwrap();
        assert_eq!(FileCopier.look(&desktop.join("folder.pdf")), Err(Refusal::NotAFile));
        assert_eq!(FileCopier.look(&desktop.join("gone.pdf")), Err(Refusal::Unreadable));
        assert_eq!(names(&folder.join(DOCUMENTS)), Vec::<String>::new(), "nothing was copied");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_refused_and_never_followed() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        std::os::unix::fs::symlink(desktop.join("cv.pdf"), desktop.join("link.pdf")).unwrap();
        assert_eq!(FileCopier.look(&desktop.join("link.pdf")), Err(Refusal::Link));
        assert_eq!(FileCopier.copy(&desktop.join("link.pdf"), &folder, Cv), Err(CopyFailure::Refused(Refusal::Link)));
    }

    #[test]
    fn a_cv_lands_in_documents_cv_and_a_missing_subfolder_is_made() {
        let (_root, folder, desktop) = places();
        fs::create_dir(cv_folder(&folder)).unwrap();
        fs::write(desktop.join("Sam Rivera CV.pdf"), pdf(2)).unwrap();
        fs::write(desktop.join("Tessel.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("Sam Rivera CV.pdf"), &folder, Cv).as_deref(), Ok("Sam Rivera CV.pdf"));
        assert_eq!(fs::read(cv_folder(&folder).join("Sam Rivera CV.pdf")).unwrap(), pdf(2));
        assert_eq!(fs::read(desktop.join("Sam Rivera CV.pdf")).unwrap(), pdf(2), "copied, never moved");
        assert_eq!(FileCopier.copy(&desktop.join("Tessel.pdf"), &folder, Reference).as_deref(), Ok("Tessel.pdf"));
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
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (2).pdf"));
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (3).pdf"));
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Reference).as_deref(), Ok("cv.pdf"), "another subfolder has no clash");
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
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv (2).pdf"));
        assert!(!outside.exists(), "the dangling link led nowhere");
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_hidden_character_or_a_leading_dot_never_reaches_the_copys_name() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("cv\u{202e}fdp.pdf"), pdf(1)).unwrap();
        fs::write(desktop.join(".profile.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv\u{202e}fdp.pdf"), &folder, Cv).as_deref(), Ok("cvfdp.pdf"));
        assert_eq!(FileCopier.copy(&desktop.join(".profile.pdf"), &folder, Cv).as_deref(), Ok("profile.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cvfdp.pdf", "profile.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_missing_documents_folder_is_made_inside_the_folder() {
        let (_root, folder, desktop) = places();
        fs::remove_dir(folder.join(DOCUMENTS)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
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
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(names(&elsewhere), Vec::<String>::new());
        let linked = root.path().join("Linked search");
        std::os::unix::fs::symlink(&folder, &linked).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &linked, Cv), Err(CopyFailure::NoDocumentsFolder), "nor a job search folder that is one");
    }

    #[cfg(unix)]
    #[test]
    fn a_subfolder_that_is_a_link_is_never_written_into() {
        let (root, folder, desktop) = places();
        let elsewhere = root.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, cv_folder(&folder)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(names(&elsewhere), Vec::<String>::new(), "the link's target stays empty");
    }

    #[test]
    fn a_subfolder_that_is_a_file_is_refused() {
        let (_root, folder, desktop) = places();
        fs::write(cv_folder(&folder), b"not a folder").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(fs::read(cv_folder(&folder)).unwrap(), b"not a folder");
    }

    #[test]
    fn a_copy_leaves_no_part_file_and_clears_a_stray_one() {
        let (_root, folder, desktop) = places();
        let (cv, references) = (cv_folder(&folder), folder.join(DOCUMENTS).join("references"));
        fs::create_dir(&cv).unwrap();
        fs::create_dir(&references).unwrap();
        let stray = references.join(format!("{PART_PREFIX}1-0{PART_SUFFIX}"));
        let under_way = cv.join(format!("{PART_PREFIX}2-0{PART_SUFFIX}"));
        fs::write(&stray, b"half a PDF").unwrap();
        fs::write(&under_way, b"a copy under way").unwrap();
        File::options().write(true).open(&stray).unwrap().set_modified(SystemTime::now() - STRAY_PART_AGE * 2).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv), [format!("{PART_PREFIX}2-0{PART_SUFFIX}"), "cv.pdf".to_owned()], "the fresh part stayed");
        assert_eq!(names(&references), Vec::<String>::new(), "the stray part in another subfolder went");
        assert_eq!(fs::read(cv.join("cv.pdf")).unwrap(), pdf(1));
        root_holds_only_subfolders(&folder);
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
        assert_eq!(FileCopier.copy(&cv_folder(&folder).join("cv.pdf"), &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"]);
        root_holds_only_subfolders(&folder);
    }

    #[test]
    fn a_file_in_the_documents_root_is_copied_into_its_subfolder_and_left_as_it_was() {
        let (_root, folder, _) = places();
        let loose = folder.join(DOCUMENTS).join("cv.pdf");
        fs::write(&loose, pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&loose, &folder, Cv).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&cv_folder(&folder)), ["cv.pdf"]);
        assert_eq!(fs::read(&loose).unwrap(), pdf(1), "nothing moved");
        assert_eq!(names(&folder.join(DOCUMENTS)), ["cv", "cv.pdf"]);
    }
}
