use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::dossier::domain::{PageImage, PageRasterizer, PdfDocument};

#[derive(Clone, PartialEq, Eq)]
pub enum PageState {
    Loading,
    Ready(Arc<Vec<PageImage>>),
    Failed(String),
}

type Key = (PathBuf, u64);

/// Renders PDFs on worker threads and keeps the results by path and modification time.
pub struct PageStore {
    rasterizer: Arc<dyn PageRasterizer>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    states: HashMap<Key, PageState>,
    finished: Receiver<(Key, PageState)>,
    finished_sender: Sender<(Key, PageState)>,
}

impl PageStore {
    pub fn new(rasterizer: Arc<dyn PageRasterizer>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (finished_sender, finished) = mpsc::channel();
        Self { rasterizer, repaint, states: HashMap::new(), finished, finished_sender }
    }

    pub fn poll(&mut self) {
        while let Ok((key, state)) = self.finished.try_recv() {
            self.states.insert(key, state);
        }
    }

    /// Starts rendering on first request. A changed file gets a new key and renders again.
    pub fn pages(&mut self, document: &PdfDocument) -> PageState {
        let key = (document.path.clone(), document.modified);
        if let Some(state) = self.states.get(&key) {
            return state.clone();
        }
        self.states.retain(|(path, _), _| *path != document.path);
        self.states.insert(key.clone(), PageState::Loading);
        let (rasterizer, repaint, sender) = (Arc::clone(&self.rasterizer), Arc::clone(&self.repaint), self.finished_sender.clone());
        thread::spawn(move || {
            let state = match rasterizer.render(&key.0) {
                Ok(pages) => PageState::Ready(Arc::new(pages)),
                Err(reason) => PageState::Failed(reason),
            };
            let _ = sender.send((key, state));
            repaint();
        });
        PageState::Loading
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use super::*;

    struct Counting(Mutex<u32>);

    impl PageRasterizer for Arc<Counting> {
        fn render(&self, _: &Path) -> Result<Vec<PageImage>, String> {
            *self.0.lock().unwrap() += 1;
            Err("nope".into())
        }
    }

    fn document(modified: u64) -> PdfDocument {
        PdfDocument { label: "CV".into(), path: "/x.pdf".into(), modified }
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
        let counting = Arc::new(Counting(Mutex::new(0)));
        let mut store = PageStore::new(Arc::new(Arc::clone(&counting)), Arc::new(|| {}));

        assert!(settled(&mut store, &document(1)) == PageState::Failed("nope".into()));
        settled(&mut store, &document(1));
        assert_eq!(*counting.0.lock().unwrap(), 1);

        settled(&mut store, &document(2));
        assert_eq!(*counting.0.lock().unwrap(), 2);
    }
}
