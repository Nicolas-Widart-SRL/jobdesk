use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hayro::hayro_syntax::Pdf;

use crate::onboarding::domain::{
    CopyFailure, DOCUMENTS, DocumentCopier, MAX_DOCUMENT, MAX_NUMBER, Refusal, by_name, copy_name, looks_like_pdf, numbered, shown_name,
};

/// Copies the user's documents into the job search folder. It reads each file once through a handle it checked is the
/// regular file it looked at, and writes only new files directly in `documents/`.
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

    fn copy(&self, source: &Path, folder: &Path) -> Result<String, CopyFailure> {
        let name = shown_name(source);
        by_name(&name).map_err(CopyFailure::Refused)?;
        let mut file = open_regular(source).map_err(CopyFailure::Refused)?;
        let documents = documents_folder(folder)?;
        if let Some(kept) = already_inside(source, &documents) {
            return Ok(kept);
        }
        let name = copy_name(&name);
        for number in 1..=MAX_NUMBER {
            let candidate = if number == 1 { name.clone() } else { numbered(&name, number) };
            let target = documents.join(&candidate);
            // Fails on any name already there, a link included, and never follows one.
            match OpenOptions::new().write(true).create_new(true).open(&target) {
                Ok(out) => return write_copy(&mut file, out, &target).map(|()| candidate),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(CopyFailure::Failed),
            }
        }
        Err(CopyFailure::NoFreeName)
    }
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

/// `documents/` of the job search folder, made when it is missing. The folder and `documents/` must both be real
/// folders, never links, so nothing is written outside them.
fn documents_folder(folder: &Path) -> Result<PathBuf, CopyFailure> {
    let real_folder = |path: &Path| fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir());
    if !real_folder(folder) {
        return Err(CopyFailure::NoDocumentsFolder);
    }
    let documents = folder.join(DOCUMENTS);
    match fs::symlink_metadata(&documents) {
        Ok(meta) if meta.is_dir() => Ok(documents),
        Ok(_) => Err(CopyFailure::NoDocumentsFolder),
        Err(error) if error.kind() == ErrorKind::NotFound => fs::create_dir(&documents).map(|()| documents).map_err(|_| CopyFailure::NoDocumentsFolder),
        Err(_) => Err(CopyFailure::NoDocumentsFolder),
    }
}

/// The name of a file the user added from `documents/` itself, which needs no copy.
fn already_inside(source: &Path, documents: &Path) -> Option<String> {
    let (source, documents) = (fs::canonicalize(source).ok()?, fs::canonicalize(documents).ok()?);
    (source.parent() == Some(documents.as_path())).then(|| source.file_name().map(|name| name.to_string_lossy().into_owned())).flatten()
}

/// Copies what `file` reads into the new `out`, on disk before it returns. A copy that fails or grows past the limit is
/// removed, so no half file stays.
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
        assert_eq!(FileCopier.copy(&desktop.join("letter.docx"), &folder), Err(CopyFailure::Refused(Refusal::Word)));
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
        assert_eq!(FileCopier.copy(&desktop.join("link.pdf"), &folder), Err(CopyFailure::Refused(Refusal::Link)));
    }

    #[test]
    fn a_copy_lands_in_documents_and_leaves_the_source_as_it_was() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("Sam Rivera CV.pdf"), pdf(2)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("Sam Rivera CV.pdf"), &folder).as_deref(), Ok("Sam Rivera CV.pdf"));
        assert_eq!(fs::read(folder.join(DOCUMENTS).join("Sam Rivera CV.pdf")).unwrap(), pdf(2));
        assert_eq!(fs::read(desktop.join("Sam Rivera CV.pdf")).unwrap(), pdf(2), "copied, never moved");
    }

    #[test]
    fn a_name_clash_gets_a_number_and_never_writes_over_a_file() {
        let (_root, folder, desktop) = places();
        let documents = folder.join(DOCUMENTS);
        fs::write(documents.join("cv.pdf"), b"the one already there").unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder).as_deref(), Ok("cv (2).pdf"));
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder).as_deref(), Ok("cv (3).pdf"));
        assert_eq!(fs::read(documents.join("cv.pdf")).unwrap(), b"the one already there");
        assert_eq!(names(&documents), ["cv (2).pdf", "cv (3).pdf", "cv.pdf"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_in_documents_counts_as_taken_and_is_never_written_through() {
        let (root, folder, desktop) = places();
        let outside = root.path().join("outside.pdf");
        std::os::unix::fs::symlink(&outside, folder.join(DOCUMENTS).join("cv.pdf")).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder).as_deref(), Ok("cv (2).pdf"));
        assert!(!outside.exists(), "the dangling link led nowhere");
    }

    #[test]
    fn a_hidden_character_or_a_leading_dot_never_reaches_the_copys_name() {
        let (_root, folder, desktop) = places();
        fs::write(desktop.join("cv\u{202e}fdp.pdf"), pdf(1)).unwrap();
        fs::write(desktop.join(".profile.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv\u{202e}fdp.pdf"), &folder).as_deref(), Ok("cvfdp.pdf"));
        assert_eq!(FileCopier.copy(&desktop.join(".profile.pdf"), &folder).as_deref(), Ok("profile.pdf"));
        assert_eq!(names(&folder.join(DOCUMENTS)), ["cvfdp.pdf", "profile.pdf"]);
    }

    #[test]
    fn a_missing_documents_folder_is_made_inside_the_folder() {
        let (_root, folder, desktop) = places();
        fs::remove_dir(folder.join(DOCUMENTS)).unwrap();
        fs::write(desktop.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&folder.join(DOCUMENTS)), ["cv.pdf"]);
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
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &folder), Err(CopyFailure::NoDocumentsFolder));
        assert_eq!(names(&elsewhere), Vec::<String>::new());
        let linked = root.path().join("Linked search");
        std::os::unix::fs::symlink(&folder, &linked).unwrap();
        assert_eq!(FileCopier.copy(&desktop.join("cv.pdf"), &linked), Err(CopyFailure::NoDocumentsFolder), "nor a job search folder that is one");
    }

    #[test]
    fn a_file_already_in_documents_is_not_copied_again() {
        let (_root, folder, _) = places();
        let documents = folder.join(DOCUMENTS);
        fs::write(documents.join("cv.pdf"), pdf(1)).unwrap();
        assert_eq!(FileCopier.copy(&documents.join("cv.pdf"), &folder).as_deref(), Ok("cv.pdf"));
        assert_eq!(names(&documents), ["cv.pdf"]);
    }
}
