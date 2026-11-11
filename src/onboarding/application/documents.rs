use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::onboarding::domain::{CopyFailure, DocumentCopier, DocumentKind, Refusal, shown_name};

/// A document the user added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    /// Where the user's own file is. It is only ever read.
    pub source: PathBuf,
    /// Its name as plain text, without hidden characters.
    pub name: String,
    /// Whether it has been looked at. Until then it has no page count, and Read my documents waits.
    pub looked: bool,
    /// Its page count, when the PDF reads.
    pub pages: Option<usize>,
    /// What it is, guessed from its name when added and changeable until it is copied. It decides the subfolder.
    pub kind: DocumentKind,
    /// The name its copy took in `documents/<kind folder>/`, once copied.
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

/// What a worker says about the documents.
enum News {
    Looked(PathBuf, Result<Option<usize>, Refusal>),
    Copied(PathBuf, Result<String, CopyFailure>),
    CopyEnded,
}

/// The documents the Profile chapter holds before Read my documents: what the user dropped or chose, and what was
/// refused the last time they added some. Reading a file, counting its pages and copying it run on worker threads,
/// never on the window's: `poll` takes their news, and each wakes the window through `repaint`.
pub struct Documents {
    copier: Arc<dyn DocumentCopier>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    added: Vec<Added>,
    refused: Vec<Refused>,
    copying: bool,
    copy_failure: Option<CopyFailure>,
    news: Receiver<News>,
    sender: Sender<News>,
}

impl Documents {
    pub fn new(copier: Arc<dyn DocumentCopier>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, news) = mpsc::channel();
        Self { copier, repaint, added: Vec::new(), refused: Vec::new(), copying: false, copy_failure: None, news, sender }
    }

    /// Adds the files the user dropped or chose, in their order, and looks at them on a worker: a PDF keeps its place
    /// with its page count, anything else leaves the list for the refused lines. A file already added is not added twice.
    /// The refusals of an earlier add are dropped, so the lines speak of the files just added. Nothing is added while
    /// a copy runs.
    ///
    /// Each new file's kind is guessed once, from its name. A file whose name says nothing is taken for the CV when it is
    /// the first such file of this add and neither the list nor this add holds a CV yet, and for a project otherwise.
    pub fn add(&mut self, sources: impl IntoIterator<Item = PathBuf>) {
        if self.copying {
            return;
        }
        self.refused.clear();
        let mut new: Vec<(PathBuf, String, Option<DocumentKind>)> = Vec::new();
        for source in sources {
            if self.added.iter().any(|added| added.source == source) || new.iter().any(|(known, _, _)| *known == source) {
                continue;
            }
            let name = shown_name(&source);
            let guessed = DocumentKind::guess(&name);
            new.push((source, name, guessed));
        }
        let mut has_cv = self.added.iter().any(|added| added.kind == DocumentKind::Cv) || new.iter().any(|(_, _, kind)| *kind == Some(DocumentKind::Cv));
        for (source, name, guessed) in &new {
            let kind = guessed.unwrap_or_else(|| if std::mem::replace(&mut has_cv, true) { DocumentKind::Project } else { DocumentKind::Cv });
            self.added.push(Added { source: source.clone(), name: name.clone(), looked: false, pages: None, kind, copied_as: None });
        }
        let new: Vec<PathBuf> = new.into_iter().map(|(source, _, _)| source).collect();
        if new.is_empty() {
            return;
        }
        self.spawn(move |copier, tell| {
            for source in new {
                let looked = copier.look(&source);
                tell(News::Looked(source, looked));
            }
        });
    }

    /// Remove, on a document not copied yet. One already copied stays in the folder, and in the list.
    pub fn remove(&mut self, at: usize) {
        if !self.copying && self.added.get(at).is_some_and(|added| added.copied_as.is_none()) {
            self.added.remove(at);
        }
    }

    /// Changes what a document not copied yet is. A copied one, or one being copied, keeps its kind.
    pub fn set_kind(&mut self, at: usize, kind: DocumentKind) {
        if self.copying {
            return;
        }
        if let Some(added) = self.added.get_mut(at).filter(|added| added.copied_as.is_none()) {
            added.kind = kind;
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

    /// Whether a document is still being looked at.
    pub fn is_looking(&self) -> bool {
        self.added.iter().any(|added| !added.looked)
    }

    pub fn is_copying(&self) -> bool {
        self.copying
    }

    /// Why the last copy stopped, until the next one starts.
    pub fn copy_failure(&self) -> Option<CopyFailure> {
        self.copy_failure
    }

    /// Whether every document is in `documents/`.
    pub fn all_copied(&self) -> bool {
        self.added.iter().all(|added| added.copied_as.is_some())
    }

    /// Takes what the workers said since the last call. Call it every frame.
    pub fn poll(&mut self) {
        while let Ok(news) = self.news.try_recv() {
            match news {
                News::Looked(source, looked) => {
                    let Some(at) = self.added.iter().position(|added| added.source == source && !added.looked) else { continue };
                    match looked {
                        Ok(pages) => (self.added[at].looked, self.added[at].pages) = (true, pages),
                        Err(refusal) => {
                            let gone = self.added.remove(at);
                            self.refused.push(Refused { name: gone.name, refusal });
                        }
                    }
                }
                News::Copied(source, copied) => match copied {
                    Ok(name) => {
                        if let Some(added) = self.added.iter_mut().find(|added| added.source == source) {
                            added.copied_as = Some(name);
                        }
                    }
                    Err(failure) => self.copy_failure = Some(failure),
                },
                News::CopyEnded => self.copying = false,
            }
        }
    }

    /// Copies into the subfolder of `documents/` of `folder` that each one's kind names, on a worker, each document not
    /// copied yet, in order, and stops at the first that fails. The ones copied before it stay copied, so trying again
    /// copies only the rest.
    pub(crate) fn start_copy(&mut self, folder: &Path) {
        if self.copying {
            return;
        }
        let waiting: Vec<(PathBuf, DocumentKind)> =
            self.added.iter().filter(|added| added.copied_as.is_none()).map(|added| (added.source.clone(), added.kind)).collect();
        self.copy_failure = None;
        self.copying = true;
        let folder = folder.to_owned();
        self.spawn(move |copier, tell| {
            for (source, kind) in waiting {
                let copied = copier.copy(&source, &folder, kind);
                let failed = copied.is_err();
                tell(News::Copied(source, copied));
                if failed {
                    break;
                }
            }
            tell(News::CopyEnded);
        });
    }

    fn spawn(&self, work: impl FnOnce(&dyn DocumentCopier, &dyn Fn(News)) + Send + 'static) {
        let (copier, sender, repaint) = (Arc::clone(&self.copier), self.sender.clone(), Arc::clone(&self.repaint));
        thread::spawn(move || {
            let tell = |news: News| {
                let _ = sender.send(news);
                repaint();
            };
            work(copier.as_ref(), &tell);
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::onboarding::domain::by_name;

    /// Takes a PDF by its name alone, and copies under the file's own name.
    #[derive(Default)]
    struct Shelf {
        copies: Mutex<Vec<String>>,
        kinds: Mutex<Vec<(String, DocumentKind)>>,
        broken: Mutex<Option<String>>,
    }

    impl DocumentCopier for Shelf {
        fn look(&self, source: &Path) -> Result<Option<usize>, Refusal> {
            by_name(&shown_name(source)).map(|()| Some(2))
        }

        fn copy(&self, source: &Path, _: &Path, kind: DocumentKind) -> Result<String, CopyFailure> {
            let name = shown_name(source);
            if self.broken.lock().unwrap().as_deref() == Some(name.as_str()) {
                return Err(CopyFailure::Failed);
            }
            self.copies.lock().unwrap().push(name.clone());
            self.kinds.lock().unwrap().push((name.clone(), kind));
            Ok(name)
        }
    }

    fn kinds(documents: &Documents) -> Vec<DocumentKind> {
        documents.added().iter().map(|added| added.kind).collect()
    }

    #[test]
    fn a_name_that_says_nothing_is_the_cv_only_when_no_cv_is_there() {
        use DocumentKind::*;
        let (mut with_cv, _) = documents();
        with_cv.add([desktop("Scan_0001.pdf"), desktop("Sam Rivera CV.pdf"), desktop("Notes.pdf")]);
        assert_eq!(kinds(&with_cv), [Project, Cv, Project], "this add holds a CV");
        let (mut unnamed, _) = documents();
        unnamed.add([desktop("Scan_0001.pdf"), desktop("Notes.pdf")]);
        assert_eq!(kinds(&unnamed), [Cv, Project], "the first is taken for the CV");
        unnamed.add([desktop("Scan_0002.pdf")]);
        assert_eq!(kinds(&unnamed), [Cv, Project, Project], "the list holds a CV");
        settle(&mut unnamed);
        assert_eq!(kinds(&unnamed), [Cv, Project, Project], "guessed once, never again");
    }

    #[test]
    fn a_kind_changes_until_the_document_is_copied() {
        use DocumentKind::*;
        let (mut documents, shelf) = documents();
        documents.add([desktop("Sam Rivera CV.pdf"), desktop("Profile.pdf"), desktop("Letter.pdf")]);
        settle(&mut documents);
        documents.set_kind(2, Reference);
        documents.set_kind(9, Diploma);
        assert_eq!(kinds(&documents), [Cv, LinkedIn, Reference]);
        documents.start_copy(Path::new("/Users/sam/Documents/Job search"));
        settle(&mut documents);
        let handed = shelf.kinds.lock().unwrap().clone();
        assert_eq!(handed, [("Sam Rivera CV.pdf".to_owned(), Cv), ("Profile.pdf".to_owned(), LinkedIn), ("Letter.pdf".to_owned(), Reference)]);
        documents.set_kind(0, Project);
        assert_eq!(kinds(&documents), [Cv, LinkedIn, Reference], "a copied one keeps its kind");
    }

    /// Polls until no worker has anything left to say, or fails after a few seconds.
    fn settle(documents: &mut Documents) {
        let deadline = Instant::now() + Duration::from_secs(5);
        documents.poll();
        while documents.is_looking() || documents.is_copying() {
            assert!(Instant::now() < deadline, "the workers never finished");
            thread::sleep(Duration::from_millis(2));
            documents.poll();
        }
    }

    fn documents() -> (Documents, Arc<Shelf>) {
        let shelf = Arc::new(Shelf::default());
        (Documents::new(Arc::clone(&shelf) as Arc<dyn DocumentCopier>, Arc::new(|| {})), shelf)
    }

    fn desktop(name: &str) -> PathBuf {
        PathBuf::from("/Users/sam/Desktop").join(name)
    }

    #[test]
    fn a_word_file_is_refused_with_its_line_and_a_pdf_is_added() {
        let (mut documents, _) = documents();
        documents.add([desktop("Sam Rivera CV.pdf"), desktop("Reference.docx")]);
        assert!(documents.is_looking(), "looked at on a worker");
        settle(&mut documents);
        assert_eq!(documents.added().iter().map(|added| (added.name.as_str(), added.pages)).collect::<Vec<_>>(), [("Sam Rivera CV.pdf", Some(2))]);
        assert_eq!(documents.refused(), [Refused { name: "Reference.docx".into(), refusal: Refusal::Word }]);
        assert_eq!(documents.refused()[0].line(), "Word files can't be read here. Save it as a PDF, then add the PDF.");
        documents.add([desktop("Profile.pdf")]);
        settle(&mut documents);
        assert!(documents.refused().is_empty(), "the lines speak of the files just added");
    }

    #[test]
    fn a_name_shows_without_hidden_characters() {
        let (mut documents, _) = documents();
        documents.add([desktop("cv\u{202e}fdp.pdf"), desktop("notes\u{200b}.txt")]);
        settle(&mut documents);
        assert_eq!(documents.added()[0].name, "cvfdp.pdf");
        assert_eq!(documents.refused()[0].name, "notes.txt");
    }

    #[test]
    fn a_file_added_twice_is_listed_once_and_remove_takes_it_out() {
        let (mut documents, _) = documents();
        documents.add([desktop("cv.pdf"), desktop("cv.pdf")]);
        documents.add([desktop("cv.pdf"), desktop("Profile.pdf")]);
        settle(&mut documents);
        assert_eq!(documents.added().len(), 2);
        documents.remove(0);
        documents.remove(7);
        assert_eq!(documents.added().iter().map(|added| added.name.as_str()).collect::<Vec<_>>(), ["Profile.pdf"]);
        assert!(!documents.is_empty());
    }

    #[test]
    fn a_copy_that_stops_partway_copies_only_the_rest_next_time() {
        let (mut documents, shelf) = documents();
        let folder = Path::new("/Users/sam/Documents/Job search");
        documents.add([desktop("cv.pdf"), desktop("Profile.pdf"), desktop("Letter.pdf")]);
        settle(&mut documents);
        *shelf.broken.lock().unwrap() = Some("Profile.pdf".into());
        documents.start_copy(folder);
        assert!(documents.is_copying(), "copied on a worker");
        settle(&mut documents);
        assert_eq!(documents.copy_failure(), Some(CopyFailure::Failed));
        assert_eq!(*shelf.copies.lock().unwrap(), ["cv.pdf"]);
        documents.remove(0);
        assert_eq!(documents.added().len(), 3, "a copied document stays");
        *shelf.broken.lock().unwrap() = None;
        documents.start_copy(folder);
        settle(&mut documents);
        assert_eq!(documents.copy_failure(), None);
        assert_eq!(*shelf.copies.lock().unwrap(), ["cv.pdf", "Profile.pdf", "Letter.pdf"]);
        assert!(documents.all_copied());
    }
}
