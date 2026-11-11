use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::onboarding::domain::{CopyFailure, DocumentCopier, Refusal, shown_name};

/// A document the user added, waiting to be copied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// Where the user's own file is. It is only ever read.
    pub source: PathBuf,
    /// Its name as plain text, without hidden characters.
    pub name: String,
    /// Its page count, when the PDF reads.
    pub pages: Option<usize>,
    /// The name its copy took in `documents/`, once copied.
    pub copied_as: Option<String>,
}

/// A file the user added that is not taken, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Its name as plain text, without hidden characters.
    pub name: String,
    pub refusal: Refusal,
}

impl Refused {
    /// The plain line that says why.
    pub fn line(&self) -> &'static str {
        self.refusal.line()
    }
}

/// The documents the Profile chapter holds before Read my documents: what the user dropped or chose, and what was
/// refused the last time they added some.
pub struct Documents {
    copier: Arc<dyn DocumentCopier>,
    added: Vec<Added>,
    refused: Vec<Refused>,
}

impl Documents {
    pub fn new(copier: Arc<dyn DocumentCopier>) -> Self {
        Self { copier, added: Vec::new(), refused: Vec::new() }
    }

    /// Adds the files the user dropped or chose, in their order. Each is looked at once: a PDF is added with its page
    /// count, anything else is refused with its line. A file already added is not added twice. The refusals of an
    /// earlier add are dropped, so the lines speak of the files just added.
    pub fn add(&mut self, sources: impl IntoIterator<Item = PathBuf>) {
        self.refused.clear();
        for source in sources {
            if self.added.iter().any(|added| added.source == source) {
                continue;
            }
            let name = shown_name(&source);
            match self.copier.look(&source) {
                Ok(pages) => self.added.push(Added { source, name, pages, copied_as: None }),
                Err(refusal) => self.refused.push(Refused { name, refusal }),
            }
        }
    }

    /// Remove, on a document not copied yet. One already copied stays in the folder, and in the list.
    pub fn remove(&mut self, at: usize) {
        if self.added.get(at).is_some_and(|added| added.copied_as.is_none()) {
            self.added.remove(at);
        }
    }

    pub fn added(&self) -> &[Added] {
        &self.added
    }

    pub fn refused(&self) -> &[Refused] {
        &self.refused
    }

    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
    }

    /// Copies into `documents/` of `folder` each document not copied yet, in order, and stops at the first that fails.
    /// The ones copied before it stay copied, so trying again copies only the rest.
    pub(crate) fn copy_into(&mut self, folder: &Path) -> Result<(), CopyFailure> {
        for added in self.added.iter_mut().filter(|added| added.copied_as.is_none()) {
            added.copied_as = Some(self.copier.copy(&added.source, folder)?);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::onboarding::domain::by_name;

    /// Takes a PDF by its name alone, and copies under the file's own name, numbered after the first.
    #[derive(Default)]
    struct Shelf {
        copies: Mutex<Vec<String>>,
        broken: Mutex<Option<String>>,
    }

    impl DocumentCopier for Shelf {
        fn look(&self, source: &Path) -> Result<Option<usize>, Refusal> {
            by_name(&shown_name(source)).map(|()| Some(2))
        }

        fn copy(&self, source: &Path, _: &Path) -> Result<String, CopyFailure> {
            let name = shown_name(source);
            if self.broken.lock().unwrap().as_deref() == Some(name.as_str()) {
                return Err(CopyFailure::Failed);
            }
            self.copies.lock().unwrap().push(name.clone());
            Ok(name)
        }
    }

    fn documents() -> (Documents, Arc<Shelf>) {
        let shelf = Arc::new(Shelf::default());
        (Documents::new(Arc::clone(&shelf) as Arc<dyn DocumentCopier>), shelf)
    }

    fn desktop(name: &str) -> PathBuf {
        PathBuf::from("/Users/sam/Desktop").join(name)
    }

    #[test]
    fn a_word_file_is_refused_with_its_line_and_a_pdf_is_added() {
        let (mut documents, _) = documents();
        documents.add([desktop("Sam Rivera CV.pdf"), desktop("Reference.docx")]);
        assert_eq!(documents.added().iter().map(|added| (added.name.as_str(), added.pages)).collect::<Vec<_>>(), [("Sam Rivera CV.pdf", Some(2))]);
        assert_eq!(documents.refused(), [Refused { name: "Reference.docx".into(), refusal: Refusal::Word }]);
        assert_eq!(documents.refused()[0].line(), "Word files can't be read here. Save it as a PDF, then add the PDF.");
        documents.add([desktop("Profile.pdf")]);
        assert!(documents.refused().is_empty(), "the lines speak of the files just added");
    }

    #[test]
    fn a_name_shows_without_hidden_characters() {
        let (mut documents, _) = documents();
        documents.add([desktop("cv\u{202e}fdp.pdf"), desktop("notes\u{200b}.txt")]);
        assert_eq!(documents.added()[0].name, "cvfdp.pdf");
        assert_eq!(documents.refused()[0].name, "notes.txt");
    }

    #[test]
    fn a_file_added_twice_is_listed_once_and_remove_takes_it_out() {
        let (mut documents, _) = documents();
        documents.add([desktop("cv.pdf"), desktop("cv.pdf")]);
        documents.add([desktop("cv.pdf"), desktop("Profile.pdf")]);
        assert_eq!(documents.added().len(), 2);
        documents.remove(0);
        documents.remove(7);
        assert_eq!(documents.added().iter().map(|added| added.name.as_str()).collect::<Vec<_>>(), ["Profile.pdf"]);
        assert!(!documents.is_empty());
    }

    #[test]
    fn a_copy_that_stops_partway_copies_only_the_rest_next_time() {
        let (mut documents, shelf) = documents();
        documents.add([desktop("cv.pdf"), desktop("Profile.pdf"), desktop("Letter.pdf")]);
        *shelf.broken.lock().unwrap() = Some("Profile.pdf".into());
        assert_eq!(documents.copy_into(Path::new("/Users/sam/Documents/Job search")), Err(CopyFailure::Failed));
        assert_eq!(*shelf.copies.lock().unwrap(), ["cv.pdf"]);
        documents.remove(0);
        assert_eq!(documents.added().len(), 3, "a copied document stays");
        *shelf.broken.lock().unwrap() = None;
        documents.copy_into(Path::new("/Users/sam/Documents/Job search")).unwrap();
        assert_eq!(*shelf.copies.lock().unwrap(), ["cv.pdf", "Profile.pdf", "Letter.pdf"]);
        assert!(documents.added().iter().all(|added| added.copied_as.is_some()));
    }
}
