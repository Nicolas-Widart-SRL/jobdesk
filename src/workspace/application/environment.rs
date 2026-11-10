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
    /// The number of the latest check started. Only its results are kept.
    latest: u64,
    finished: Receiver<(u64, Vec<ToolCheck>)>,
    finished_sender: Sender<(u64, Vec<ToolCheck>)>,
}

impl EnvironmentChecks {
    pub fn new(prober: Arc<dyn ToolProber>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (finished_sender, finished) = mpsc::channel();
        Self { prober, repaint, results: None, running: false, latest: 0, finished, finished_sender }
    }

    pub fn poll(&mut self) {
        while let Ok((number, results)) = self.finished.try_recv() {
            if number == self.latest {
                self.results = Some(results);
                self.running = false;
            }
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

    /// Check again, which waits for a check still running.
    pub fn recheck(&mut self) {
        if !self.running {
            self.restart();
        }
    }

    /// Starts a new check now, even while one runs, whose results then never show: the tools may have changed since
    /// it started, as when first-run setup has just installed some. The last results stay until the new ones are in.
    pub fn restart(&mut self) {
        self.running = true;
        self.latest += 1;
        let (prober, repaint, sender, number) = (Arc::clone(&self.prober), Arc::clone(&self.repaint), self.finished_sender.clone(), self.latest);
        thread::spawn(move || {
            let results = TOOLS.iter().map(|spec| ToolCheck { spec: *spec, version: prober.probe(spec) }).collect();
            let _ = sender.send((number, results));
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
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

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

    /// Answers with the version set when each probe starts, after a pause when none is set yet.
    struct Changing(Mutex<Option<&'static str>>, Mutex<u32>);

    impl ToolProber for Changing {
        fn probe(&self, _: &ToolSpec) -> Option<String> {
            let answer = *self.0.lock().unwrap();
            if answer.is_none() {
                // The first check, from before the tools changed, ends after the second.
                thread::sleep(Duration::from_millis(30));
            }
            *self.1.lock().unwrap() += 1;
            answer.map(str::to_owned)
        }
    }

    #[test]
    fn a_restart_outruns_the_check_still_running() {
        let prober = Arc::new(Changing(Mutex::new(None), Mutex::new(0)));
        let mut checks = EnvironmentChecks::new(Arc::clone(&prober) as Arc<dyn ToolProber>, Arc::new(|| {}));
        checks.ensure_started();
        checks.recheck();
        thread::sleep(Duration::from_millis(5));
        *prober.0.lock().unwrap() = Some("1.0");
        checks.restart();
        let deadline = Instant::now() + Duration::from_secs(2);
        while checks.is_running() {
            assert!(Instant::now() < deadline, "the new check never ended");
            checks.poll();
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(checks.version_of("claude"), Some("1.0"));
        // The first check, which found no `claude`, ends after the new one, and is never shown.
        while *prober.1.lock().unwrap() < 2 * TOOLS.len() as u32 {
            assert!(Instant::now() < deadline, "the first check never ended");
            thread::sleep(Duration::from_millis(2));
        }
        thread::sleep(Duration::from_millis(5));
        checks.poll();
        assert_eq!(checks.version_of("claude"), Some("1.0"));
    }
}
