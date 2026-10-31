use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::workspace::domain::{TOOLS, ToolCheck, ToolProber};

/// Checks installed tools on a worker, because a slow tool must not freeze the window.
pub struct EnvironmentChecks {
    prober: Arc<dyn ToolProber>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    results: Option<Vec<ToolCheck>>,
    running: bool,
    finished: Receiver<Vec<ToolCheck>>,
    finished_sender: Sender<Vec<ToolCheck>>,
}

impl EnvironmentChecks {
    pub fn new(prober: Arc<dyn ToolProber>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (finished_sender, finished) = mpsc::channel();
        Self { prober, repaint, results: None, running: false, finished, finished_sender }
    }

    pub fn poll(&mut self) {
        while let Ok(results) = self.finished.try_recv() {
            self.results = Some(results);
            self.running = false;
        }
    }

    pub fn results(&self) -> Option<&[ToolCheck]> {
        self.results.as_deref()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn recheck(&mut self) {
        if self.running {
            return;
        }
        self.running = true;
        let (prober, repaint, sender) = (Arc::clone(&self.prober), Arc::clone(&self.repaint), self.finished_sender.clone());
        thread::spawn(move || {
            let results = TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: prober.probe(spec) }).collect();
            let _ = sender.send(results);
            repaint();
        });
    }

    /// First visit starts the check.
    pub fn ensure_started(&mut self) {
        if self.results.is_none() {
            self.recheck();
        }
    }
}
