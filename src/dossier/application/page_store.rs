use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::dossier::domain::{PageRasterizer, PdfDocument, RenderedPages};

/// What a render that gives no page says instead.
pub const NO_PAGES: &str = "This PDF has no pages.";

/// How many rendered documents stay in memory. Each page is a few megabytes, and the dossier shows two or three at once.
const KEPT: usize = 8;

/// How long after a file changed a failed render of it may only mean the file was still being written.
const RECENT: u64 = 30;

#[derive(Clone, PartialEq, Eq)]
pub enum PageState {
    Loading,
    Ready(Arc<RenderedPages>),
    Failed(String),
}

type Key = (PathBuf, u64);

/// One document's render, and when it was last asked for.
struct Entry {
    state: PageState,
    asked: u64,
}

/// Renders PDFs on worker threads and keeps the results by path and modification time. Only the `KEPT` rendered
/// documents asked for last stay: an older one renders again when it is asked for again.
pub struct PageStore {
    rasterizer: Arc<dyn PageRasterizer>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    states: HashMap<Key, Entry>,
    /// Counts the asks, so the least recently asked document is the one to let go.
    asks: u64,
    finished: Receiver<(Key, PageState)>,
    finished_sender: Sender<(Key, PageState)>,
}

impl PageStore {
    pub fn new(rasterizer: Arc<dyn PageRasterizer>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (finished_sender, finished) = mpsc::channel();
        Self { rasterizer, repaint, states: HashMap::new(), asks: 0, finished, finished_sender }
    }

    /// Takes the renders that finished. One no longer asked for, such as an older version of a file, is dropped.
    pub fn poll(&mut self) {
        while let Ok((key, state)) = self.finished.try_recv() {
            if let Some(entry) = self.states.get_mut(&key) {
                entry.state = state;
            }
        }
        self.let_go();
    }

    /// Starts rendering on first request. A changed file gets a new key and renders again.
    pub fn pages(&mut self, document: &PdfDocument) -> PageState {
        let key = (document.path.clone(), document.modified);
        self.asks += 1;
        if let Some(entry) = self.states.get_mut(&key) {
            entry.asked = self.asks;
            return entry.state.clone();
        }
        self.states.retain(|(path, _), _| *path != document.path);
        self.states.insert(key.clone(), Entry { state: PageState::Loading, asked: self.asks });
        let (rasterizer, repaint, sender) = (Arc::clone(&self.rasterizer), Arc::clone(&self.repaint), self.finished_sender.clone());
        thread::spawn(move || {
            let state = match rasterizer.render(&key.0) {
                Ok(rendered) if rendered.pages.is_empty() => PageState::Failed(NO_PAGES.to_owned()),
                Ok(rendered) => PageState::Ready(Arc::new(rendered)),
                Err(reason) => PageState::Failed(reason),
            };
            let _ = sender.send((key, state));
            repaint();
        });
        PageState::Loading
    }

    /// What is known of the document's pages, without starting a render: nothing when it was never asked for, or was
    /// let go since.
    pub fn known(&self, document: &PdfDocument) -> Option<&PageState> {
        self.states.get(&(document.path.clone(), document.modified)).map(|entry| &entry.state)
    }

    /// Forgets the failed renders of files changed in the last `RECENT` seconds before `now`, in seconds since the epoch,
    /// so each renders again when next asked for. A file read while a task was still writing it fails, and can be whole
    /// a moment later with the same modification time to the second, so the shell calls this whenever the folder is read
    /// again. A file that failed and has not changed since stays failed.
    pub fn retry_failures(&mut self, now: u64) {
        self.states.retain(|(_, modified), entry| !matches!(entry.state, PageState::Failed(_)) || modified.saturating_add(RECENT) < now);
    }

    /// Lets go of the rendered documents asked for longest ago, past the `KEPT` most recent.
    fn let_go(&mut self) {
        let mut rendered: Vec<(u64, Key)> =
            self.states.iter().filter(|(_, entry)| matches!(entry.state, PageState::Ready(_))).map(|(key, entry)| (entry.asked, key.clone())).collect();
        if rendered.len() <= KEPT {
            return;
        }
        rendered.sort();
        for (_, key) in &rendered[..rendered.len() - KEPT] {
            self.states.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::dossier::domain::PageImage;

    /// Counts its renders. It gives `pages` blank pages, or fails while `failing` holds.
    struct Counting {
        renders: Mutex<u32>,
        failing: Mutex<bool>,
        pages: usize,
    }

    impl Counting {
        fn new(failing: bool, pages: usize) -> Arc<Self> {
            Arc::new(Self { renders: Mutex::new(0), failing: Mutex::new(failing), pages })
        }

        fn renders(&self) -> u32 {
            *self.renders.lock().unwrap()
        }
    }

    impl PageRasterizer for Arc<Counting> {
        fn render(&self, _: &Path) -> Result<RenderedPages, String> {
            *self.renders.lock().unwrap() += 1;
            if *self.failing.lock().unwrap() {
                return Err("nope".into());
            }
            let page = PageImage { width: 1, height: 1, rgba: vec![255; 4] };
            Ok(RenderedPages { pages: vec![page; self.pages], total: self.pages })
        }
    }

    fn store(counting: &Arc<Counting>) -> PageStore {
        PageStore::new(Arc::new(Arc::clone(counting)), Arc::new(|| {}))
    }

    fn document(modified: u64) -> PdfDocument {
        named("/x.pdf", modified)
    }

    fn named(path: &str, modified: u64) -> PdfDocument {
        PdfDocument { label: "CV".into(), path: path.into(), modified }
    }

    fn settled(store: &mut PageStore, document: &PdfDocument) -> PageState {
        let start = Instant::now();
        loop {
            store.poll();
            let state = store.pages(document);
            if state != PageState::Loading || start.elapsed() > Duration::from_secs(5) {
                return state;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn renders_once_per_file_version_and_reports_failure() {
        let counting = Counting::new(true, 1);
        let mut store = store(&counting);

        assert!(settled(&mut store, &document(1)) == PageState::Failed("nope".into()));
        settled(&mut store, &document(1));
        assert_eq!(counting.renders(), 1);

        settled(&mut store, &document(2));
        assert_eq!(counting.renders(), 2);
    }

    /// A render that failed while the file was still being written renders again once the folder is read again.
    #[test]
    fn a_failed_render_is_tried_again_after_a_retry() {
        let counting = Counting::new(true, 2);
        let mut store = store(&counting);
        assert!(settled(&mut store, &document(1)) == PageState::Failed("nope".into()));
        assert!(matches!(store.known(&document(1)), Some(PageState::Failed(_))));
        *counting.failing.lock().unwrap() = false;
        store.retry_failures(1 + RECENT + 1);
        assert!(store.known(&document(1)).is_some(), "a file that has not changed for a while stays failed");
        store.retry_failures(1 + RECENT);
        assert!(store.known(&document(1)).is_none(), "the failure of a file just written is forgotten");
        assert!(matches!(settled(&mut store, &document(1)), PageState::Ready(pages) if pages.pages.len() == 2));
        assert_eq!(counting.renders(), 2);
        store.retry_failures(1);
        settled(&mut store, &document(1));
        assert_eq!(counting.renders(), 2, "a render that worked is kept");
    }

    #[test]
    fn a_pdf_with_no_pages_fails() {
        let counting = Counting::new(false, 0);
        let mut store = store(&counting);
        assert!(settled(&mut store, &document(1)) == PageState::Failed(NO_PAGES.into()));
    }

    /// Only the documents asked for last stay rendered, and one let go renders again when asked for.
    #[test]
    fn only_the_last_rendered_documents_stay() {
        let counting = Counting::new(false, 1);
        let mut store = store(&counting);
        let documents: Vec<PdfDocument> = (0..KEPT + 2).map(|index| named(&format!("/cv/{index}.pdf"), 1)).collect();
        for document in &documents {
            settled(&mut store, document);
        }
        store.poll();
        let kept: Vec<bool> = documents.iter().map(|document| store.known(document).is_some()).collect();
        assert_eq!(kept.iter().filter(|kept| **kept).count(), KEPT);
        assert!(!kept[0] && !kept[1] && kept[2..].iter().all(|kept| *kept), "{kept:?}");

        // Asking again keeps a document, so the next one let go is the oldest asked.
        settled(&mut store, &documents[2]);
        settled(&mut store, &documents[0]);
        store.poll();
        assert!(store.known(&documents[2]).is_some() && store.known(&documents[3]).is_none());
        assert_eq!(counting.renders(), u32::try_from(KEPT + 3).unwrap(), "only the document let go rendered again");
    }
}
