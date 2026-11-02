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

    /// The first word a tool printed about itself, such as `2.1.3` from `2.1.3 (Claude Code)`.
    pub fn version_of(&self, name: &str) -> Option<&str> {
        let check = self.results()?.iter().find(|check| check.spec.name == name)?;
        check.version.as_deref()?.split_whitespace().next()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::domain::ToolSpec;

    struct NoTools;

    impl ToolProber for NoTools {
        fn probe(&self, _: &ToolSpec) -> Option<String> {
            None
        }
    }

    #[test]
    fn version_of_reads_the_first_word_of_a_found_tool() {
        let mut checks = EnvironmentChecks::new(Arc::new(NoTools), Arc::new(|| {}));
        assert_eq!(checks.version_of("claude"), None);
        checks.results =
            Some(vec![ToolCheck { spec: TOOLS[0], version: Some("2.1.308 (Claude Code)".to_owned()) }, ToolCheck { spec: TOOLS[1], version: None }]);
        assert_eq!(checks.version_of("claude"), Some("2.1.308"));
        assert_eq!(checks.version_of("python3"), None);
        assert_eq!(checks.version_of("bun"), None);
    }
}
