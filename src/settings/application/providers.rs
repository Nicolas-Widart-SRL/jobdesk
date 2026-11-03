use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};

use chrono::NaiveDateTime;

use crate::settings::application::settings::Settings;
use crate::settings::domain::{ConnectionTester, KeyMask, KeyStore, KeyStoreError, KeyTest, Outcome, Preferences, Saved, TestReport, task_sign_in};
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::{ApiKey, KeyReader, KeySource, SignIn, key_source, redacted};

/// Why Test connection is off while a task runs: one Claude process at a time.
const TASK_RUNNING: &str = "A task is running. Test once it ends.";
/// Why Test connection is off while a task's process is up but only waits for the user's reply.
const TASK_OPEN: &str = "A task is still open. Reply or end it, then test.";
/// What Test connection reads while a test runs, and why a second one is refused.
pub const TESTING: &str = "Testing…";
const NOT_A_KEY: &str = "This is not a key. Paste the whole key, with nothing inside it but its own characters.";
const NO_KEY: &str = "No key is saved.";
/// Why a test of the key waits: a task's start is reading the keychain, perhaps behind its prompt.
const KEY_BUSY: &str = "The keychain is being read for a task. Test once it answers.";
const NO_ANSWER: &str = "The test ended without an answer.";
/// What Claude Code said is cut to this, so a long error cannot push the page away.
const SAID_LINES: usize = 8;
const SAID_CHARS: usize = 500;

/// Whether Test connection can start now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestGate {
    Ready,
    Testing,
    TaskRunning,
}

impl TestGate {
    /// The words beside a Test connection that cannot start.
    pub fn reason(self) -> Option<&'static str> {
        match self {
            TestGate::Ready => None,
            TestGate::Testing => Some(TESTING),
            TestGate::TaskRunning => Some(TASK_RUNNING),
        }
    }
}

/// What a test checks: the saved key, or the sign-in Claude Code has on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestTarget {
    Key,
    ClaudeSignIn,
}

/// The last test of the Claude sign-in, kept for this session only.
#[derive(Debug, Clone, PartialEq)]
pub struct SignInCheck {
    pub outcome: Outcome,
    pub at: NaiveDateTime,
    pub cost_usd: f64,
    /// Clean already: what Claude Code said, for a failed test.
    pub said: String,
    /// The CLI's `apiKeySource`, for `sign_in_words` with `SignIn::ClaudeSignIn`: the test passes no key.
    pub source: Option<String>,
}

/// The last test of the saved key that did not pass, kept for this session only.
/// Only a `Refused` one is also recorded in the preferences and moves tasks off the key.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyFailure {
    /// `Refused` or `Unreached`.
    pub outcome: Outcome,
    pub at: NaiveDateTime,
    pub cost_usd: f64,
    /// Clean and masked already: what Claude Code said, or why it could not be asked.
    pub said: String,
}

/// The tone of the rail's word for Providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordTone {
    Muted,
    Closed,
}

struct Running {
    target: TestTarget,
    /// A copy of the key under test, kept only to mask it in what Claude Code says.
    key: Option<ApiKey>,
    report: Receiver<TestReport>,
    /// The user chose otherwise while it ran, so its result changes nothing. It still keeps a second test out.
    cancelled: bool,
}

/// What the keychain answered, held for the rest of the launch so it is not asked again.
enum Held {
    NotRead,
    Absent,
    Present(ApiKey),
}

impl Held {
    /// A copy of what is held. None while nothing was read.
    fn answer(&self) -> Option<Option<ApiKey>> {
        match self {
            Held::NotRead => None,
            Held::Absent => Some(None),
            Held::Present(key) => Some(ApiKey::parse(key.expose())),
        }
    }
}

struct Cell {
    held: Held,
    /// Why the last read failed, for the callers that waited on it.
    failure: Option<String>,
    /// Counts saves and removes, so a read that started before one never replaces what it wrote.
    generation: u64,
}

/// The saved key in process memory, shared with each task's start. The keychain is read once per launch, until a read succeeds:
/// an error is not held, so a later read asks again. Save key replaces what is held, and a removed key leaves nothing held.
/// `cell` is never locked across a keychain call. Only a read holds `turn` across it: a task's start waits for that turn,
/// and the window only tries it, so a prompt on one thread never makes the window wait.
struct KeyCache {
    keys: Arc<dyn KeyStore>,
    cell: Mutex<Cell>,
    turn: Mutex<()>,
    /// How many readers wait for another's read to end, so a test can tell one waits.
    waiting: AtomicUsize,
}

impl KeyCache {
    fn new(keys: Arc<dyn KeyStore>) -> Self {
        Self { keys, cell: Mutex::new(Cell { held: Held::NotRead, failure: None, generation: 0 }), turn: Mutex::new(()), waiting: AtomicUsize::new(0) }
    }

    /// The held key, or the keychain's answer when nothing is held. A read under way on another thread is waited for and shared,
    /// so two callers at once read the keychain once, and its failure is the answer of each. The window must not call this.
    fn read_waiting(&self) -> Result<Option<ApiKey>, KeyStoreError> {
        let (turn, waited) = match self.turn.try_lock() {
            Ok(turn) => (turn, false),
            Err(TryLockError::Poisoned(poisoned)) => (poisoned.into_inner(), false),
            Err(TryLockError::WouldBlock) => {
                self.waiting.fetch_add(1, Ordering::SeqCst);
                let turn = self.turn.lock().unwrap_or_else(PoisonError::into_inner);
                self.waiting.fetch_sub(1, Ordering::SeqCst);
                (turn, true)
            }
        };
        if waited {
            let cell = self.cell();
            if let (None, Some(message)) = (cell.held.answer(), &cell.failure) {
                return Err(KeyStoreError { message: message.clone() });
            }
        }
        self.answer_or_read(turn)
    }

    /// As `read_waiting`, but None at once while another thread reads the keychain, so the window never waits on its prompt.
    fn read_now(&self) -> Option<Result<Option<ApiKey>, KeyStoreError>> {
        let turn = match self.turn.try_lock() {
            Ok(turn) => turn,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        Some(self.answer_or_read(turn))
    }

    /// What is known without asking the keychain: the held key, or why the last read failed.
    fn known(&self) -> Option<Result<Option<ApiKey>, KeyStoreError>> {
        let cell = self.cell();
        match (cell.held.answer(), &cell.failure) {
            (Some(answer), _) => Some(Ok(answer)),
            (None, Some(message)) => Some(Err(KeyStoreError { message: message.clone() })),
            (None, None) => None,
        }
    }

    /// Reads the keychain, with the turn held, only when nothing is held.
    fn answer_or_read(&self, _turn: MutexGuard<'_, ()>) -> Result<Option<ApiKey>, KeyStoreError> {
        let generation = {
            let cell = self.cell();
            if let Some(answer) = cell.held.answer() {
                return Ok(answer);
            }
            cell.generation
        };
        let read = self.keys.read();
        let mut cell = self.cell();
        match read {
            Ok(found) if cell.generation == generation => {
                cell.held = found.map_or(Held::Absent, Held::Present);
                cell.failure = None;
            }
            Err(error) if cell.generation == generation => {
                cell.failure = Some(error.message.clone());
                return Err(error);
            }
            // A save or a remove ended while it read, and what it wrote stands.
            _ => {}
        }
        Ok(cell.held.answer().flatten())
    }

    fn write(&self, key: &ApiKey) -> Result<(), KeyStoreError> {
        self.keys.write(key)?;
        self.hold(ApiKey::parse(key.expose()).map_or(Held::Absent, Held::Present));
        Ok(())
    }

    /// A key the keychain could not delete stays held, so memory, the page and the keychain still agree.
    fn delete(&self) -> Result<(), KeyStoreError> {
        self.keys.delete()?;
        self.hold(Held::Absent);
        Ok(())
    }

    fn hold(&self, held: Held) {
        let mut cell = self.cell();
        cell.held = held;
        cell.failure = None;
        cell.generation += 1;
    }

    fn cell(&self) -> MutexGuard<'_, Cell> {
        self.cell.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The page's word for a keychain answer.
fn saved_of(answer: Result<Option<ApiKey>, KeyStoreError>) -> Saved {
    match answer {
        Ok(None) => Saved::Absent,
        Ok(Some(key)) => Saved::Present(KeyMask::of(&key)),
        Err(error) => Saved::Unreadable(error.message),
    }
}

/// How tasks sign in, the saved API key and Test connection. The keychain is read once per launch, until a read succeeds, by whichever
/// comes first of Providers showing, a test and a task's start, and the key is then held in memory for the mask, tests and tasks.
pub struct Providers {
    key: Arc<KeyCache>,
    tester: Box<dyn ConnectionTester>,
    saved: Saved,
    running: Option<Running>,
    sign_in_check: Option<SignInCheck>,
    key_failure: Option<KeyFailure>,
}

impl Providers {
    pub fn new(keys: Arc<dyn KeyStore>, tester: Box<dyn ConnectionTester>) -> Self {
        let key = Arc::new(KeyCache::new(keys));
        Self { key, tester, saved: Saved::NotRead, running: None, sign_in_check: None, key_failure: None }
    }

    pub fn saved(&self) -> &Saved {
        &self.saved
    }

    /// The mask and the method, from what is known in memory, or from the keychain when nothing asked it yet.
    /// A key a task's start or a test read after a failed read replaces Unreadable. True while a task's start reads the keychain:
    /// the page then stays NotRead without waiting on that prompt, and should look again soon.
    pub fn read_once(&mut self) -> bool {
        if matches!(self.saved, Saved::Absent | Saved::Present(_)) {
            return false;
        }
        if let Some(known) = self.key.known() {
            self.saved = saved_of(known);
            return false;
        }
        match self.key.read_now() {
            Some(answer) => {
                self.saved = saved_of(answer);
                false
            }
            None => true,
        }
    }

    /// How the next task signs in.
    pub fn task_sign_in(&self, preferences: &Preferences) -> SignIn {
        task_sign_in(preferences.sign_in, &self.saved)
    }

    /// Gives the saved key when a task's process starts: the one held in memory, or read from the keychain when nothing read it yet.
    pub fn key_reader(&self) -> KeyReader {
        let key = Arc::clone(&self.key);
        Arc::new(move || key.read_waiting().map_err(|error| error.message))
    }

    /// The monthly cap is not asked: a test costs at most a cent and is not a task.
    pub fn test_gate(&self, task_running: bool) -> TestGate {
        if self.running.is_some() {
            TestGate::Testing
        } else if task_running {
            TestGate::TaskRunning
        } else {
            TestGate::Ready
        }
    }

    /// What a running test checks.
    /// None for a test the user turned away from: it still runs, so `test_gate` stays `Testing`, but nothing is under test.
    pub fn testing(&self) -> Option<TestTarget> {
        self.running.as_ref().filter(|running| !running.cancelled).map(|running| running.target)
    }

    /// The last test of the saved key this session, when it did not pass: refused, or never answered.
    pub fn key_failure(&self) -> Option<&KeyFailure> {
        self.key_failure.as_ref()
    }

    pub fn sign_in_check(&self) -> Option<&SignInCheck> {
        self.sign_in_check.as_ref()
    }

    /// Writes the key to the keychain, then tests it at once when nothing stops a test. Otherwise, with a task's process up or
    /// another test finishing, it waits untested until Test connection or Use for tasks.
    /// Tasks stay on the Claude sign-in until a test passes, so a mistyped key never breaks the next task. No `sk-ant-` prefix is required.
    pub fn save_key(&mut self, typed: &str, settings: &mut Settings, task_running: bool) -> Result<(), String> {
        let key = ApiKey::parse(typed).ok_or_else(|| NOT_A_KEY.to_owned())?;
        self.key.write(&key).map_err(|error| error.message)?;
        self.saved = Saved::Present(KeyMask::of(&key));
        self.key_failure = None;
        // A test still running, and the last one, were of the key this one replaces.
        self.cancel_key_test();
        settings.set_sign_in(SignIn::ClaudeSignIn);
        settings.set_key_test(None);
        if self.test_gate(task_running) == TestGate::Ready {
            self.start(TestTarget::Key, Some(key));
        }
        Ok(())
    }

    /// Tests the saved key, held in memory or read from the keychain when nothing read it yet. A pass puts tasks on the key, and a refusal moves them to the Claude sign-in.
    /// A test that never got an answer changes neither.
    /// It serves Test connection, Test again and Use for tasks on the key alike.
    pub fn test_key(&mut self, task_running: bool) -> Result<(), String> {
        self.gate(task_running)?;
        // A key held already needs no turn, so a task's start that holds it a moment never makes this wait.
        let read = match self.key.known() {
            Some(Ok(held)) => Ok(held),
            _ => self.key.read_now().ok_or_else(|| KEY_BUSY.to_owned())?,
        };
        let key = match read {
            Ok(Some(key)) => key,
            Ok(None) => {
                self.saved = Saved::Absent;
                return Err(NO_KEY.to_owned());
            }
            Err(error) => {
                self.saved = Saved::Unreadable(error.message.clone());
                return Err(error.message);
            }
        };
        self.saved = Saved::Present(KeyMask::of(&key));
        self.start(TestTarget::Key, Some(key));
        Ok(())
    }

    /// Tests Claude Code's own sign-in, with no key. It changes no method.
    pub fn test_sign_in(&mut self, task_running: bool) -> Result<(), String> {
        self.gate(task_running)?;
        self.start(TestTarget::ClaudeSignIn, None);
        Ok(())
    }

    /// The key stays saved. A key test still running no longer switches tasks to the key.
    pub fn use_claude_sign_in(&mut self, settings: &mut Settings) {
        self.cancel_key_test();
        settings.set_sign_in(SignIn::ClaudeSignIn);
    }

    /// Tasks go back to the Claude sign-in even when the keychain item cannot be deleted. The key leaves memory only once it is deleted.
    pub fn remove_key(&mut self, settings: &mut Settings) -> Result<(), String> {
        self.cancel_key_test();
        settings.set_sign_in(SignIn::ClaudeSignIn);
        self.key.delete().map_err(|error| error.message)?;
        self.saved = Saved::Absent;
        self.key_failure = None;
        settings.set_key_test(None);
        Ok(())
    }

    /// Takes a finished test's report. True when one ended, so the page has changed.
    pub fn poll(&mut self, settings: &mut Settings, now: NaiveDateTime) -> bool {
        let Some(running) = self.running.take() else { return false };
        let report = match running.report.try_recv() {
            Ok(report) => report,
            Err(TryRecvError::Empty) => {
                self.running = Some(running);
                return false;
            }
            Err(TryRecvError::Disconnected) => TestReport { outcome: Outcome::Unreached, cost_usd: 0.0, said: NO_ANSWER.to_owned(), source: None },
        };
        if running.cancelled {
            return true;
        }
        let said = said_text(&report.said, running.key.as_ref());
        match running.target {
            TestTarget::Key => {
                let cost_usd = report.cost_usd;
                match report.outcome {
                    Outcome::Passed => {
                        settings.set_key_test(Some(KeyTest { passed: true, at: now, cost_usd }));
                        settings.set_sign_in(SignIn::ApiKey);
                        self.key_failure = None;
                    }
                    Outcome::Refused => {
                        settings.set_key_test(Some(KeyTest { passed: false, at: now, cost_usd }));
                        settings.set_sign_in(SignIn::ClaudeSignIn);
                        self.key_failure = Some(KeyFailure { outcome: Outcome::Refused, at: now, cost_usd, said });
                    }
                    // Nothing was learnt about the key, so the method and the recorded test stay as they were.
                    Outcome::Unreached => self.key_failure = Some(KeyFailure { outcome: Outcome::Unreached, at: now, cost_usd, said }),
                }
            }
            TestTarget::ClaudeSignIn => {
                self.sign_in_check = Some(SignInCheck { outcome: report.outcome, at: now, cost_usd: report.cost_usd, said, source: report.source });
            }
        }
        true
    }

    /// The word at the right of the rail's Providers row.
    pub fn rail_word(&self, preferences: &Preferences) -> (String, WordTone) {
        let refused = preferences.key_test.is_some_and(|test| !test.passed) && self.saved != Saved::Absent;
        let (word, tone) = match self.task_sign_in(preferences) {
            SignIn::ApiKey => ("API key", WordTone::Muted),
            SignIn::ClaudeSignIn if refused => ("Key refused", WordTone::Closed),
            SignIn::ClaudeSignIn => ("Claude sign-in", WordTone::Muted),
        };
        (word.to_owned(), tone)
    }

    fn gate(&self, task_running: bool) -> Result<(), String> {
        match self.test_gate(task_running).reason() {
            Some(reason) => Err(reason.to_owned()),
            None => Ok(()),
        }
    }

    fn start(&mut self, target: TestTarget, key: Option<ApiKey>) {
        let (sender, report) = mpsc::channel();
        let copy = key.as_ref().and_then(|key| ApiKey::parse(key.expose()));
        self.tester.start(
            key,
            Box::new(move |report| {
                let _ = sender.send(report);
            }),
        );
        self.running = Some(Running { target, key: copy, report, cancelled: false });
    }

    fn cancel_key_test(&mut self) {
        if let Some(running) = self.running.as_mut().filter(|running| running.target == TestTarget::Key) {
            running.cancelled = true;
        }
    }
}

/// What Claude Code said, safe to show as plain text: the key masked wherever it appears, escape, control, bidi and zero-width
/// characters removed, blank lines dropped, and at most 8 lines and 500 characters.
fn said_text(raw: &str, key: Option<&ApiKey>) -> String {
    let lines: Vec<String> = redacted(raw, key)
        // Every line break, before the controls among them are removed and would join two lines.
        .split(['\r', '\n', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}'])
        // Masked again once clean, because a hidden character inside the key would have kept it from matching before.
        .map(|line| redacted(&visible_chars(line).collect::<String>(), key).trim_end().to_owned())
        .filter(|line| !line.trim().is_empty())
        .take(SAID_LINES)
        .collect();
    let text = lines.join("\n");
    if text.chars().count() <= SAID_CHARS {
        return text;
    }
    let mut cut: String = text.chars().take(SAID_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// How long ago a test ran, in words. Within the hour it counts minutes, then hours on the same day, then calendar days.
pub fn age_words(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let minutes = (now - at).num_minutes();
    let days = (now.date() - at.date()).num_days();
    match minutes {
        ..=0 => "just now".to_owned(),
        1 => "1 minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        _ if days == 0 && minutes < 120 => "1 hour ago".to_owned(),
        _ if days == 0 => format!("{} hours ago", minutes / 60),
        _ if days == 1 => "yesterday".to_owned(),
        _ => format!("{days} days ago"),
    }
}

/// The CLI's `apiKeySource` in words. A value it does not name never reaches the screen as it is.
/// `passed` is how Jobdesk started that process: only with `SignIn::ApiKey` did the key in its environment come from Jobdesk.
/// Otherwise it came from the shell that started Jobdesk, and must not read as the key saved here.
/// None is a process whose method is not known, such as a task read back from its log: a key then reads as no one's.
pub fn sign_in_words(source: &str, passed: Option<SignIn>) -> &'static str {
    match (key_source(source), passed) {
        (KeySource::ClaudeSignIn, _) => "your Claude sign-in",
        (KeySource::KeyVariable, Some(SignIn::ApiKey)) => "your API key",
        (KeySource::KeyVariable, Some(SignIn::ClaudeSignIn)) => "an API key from your environment",
        (KeySource::KeyVariable, None) => "an API key",
        (KeySource::Other, _) => "a sign-in Claude Code chose itself",
    }
}

/// Whether `source` is Claude Code's own sign-in, as a test of it hopes: not a key it found in its environment.
pub fn is_claude_sign_in(source: Option<&str>) -> bool {
    source.is_none_or(|source| key_source(source) == KeySource::ClaudeSignIn)
}

/// Why Test connection waits for a task's process: one that works, or one that only waits for the user's reply.
pub fn task_wait_reason(working: bool) -> &'static str {
    if working { TASK_RUNNING } else { TASK_OPEN }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use chrono::NaiveDate;

    use super::*;
    use crate::settings::domain::TestDone;
    use crate::settings::infrastructure::MemoryKeychain;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";
    const OTHER: &str = "sk-ant-api03-FAKE-another-fake-key-1111-Q7mz";

    type Asked = Arc<Mutex<Vec<(Option<String>, Option<TestDone>)>>>;

    /// Keeps each test it was asked for, with the key's text, until the test answers it.
    #[derive(Clone, Default)]
    struct FakeTester {
        asked: Asked,
    }

    impl ConnectionTester for FakeTester {
        fn start(&self, key: Option<ApiKey>, done: TestDone) {
            self.asked.lock().unwrap().push((key.map(|key| key.expose().to_owned()), Some(done)));
        }
    }

    impl FakeTester {
        fn calls(&self) -> usize {
            self.asked.lock().unwrap().len()
        }

        fn last_key(&self) -> Option<String> {
            self.asked.lock().unwrap().last().unwrap().0.clone()
        }

        fn answer(&self, outcome: Outcome, said: &str) {
            let done = self.asked.lock().unwrap().last_mut().unwrap().1.take().unwrap();
            done(TestReport { outcome, cost_usd: 0.004, said: said.to_owned(), source: Some("ANTHROPIC_API_KEY".to_owned()) });
        }
    }

    /// A keychain in memory that counts its reads, and can be told to fail a read or a delete,
    /// or to hold the next read until the test lets it answer, as a prompt would.
    struct CountingKeychain {
        inner: MemoryKeychain,
        reads: AtomicUsize,
        fail_reads: AtomicBool,
        fail_delete: AtomicBool,
        prompt: Mutex<Option<mpsc::Receiver<()>>>,
    }

    impl KeyStore for CountingKeychain {
        fn read(&self) -> Result<Option<ApiKey>, KeyStoreError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let prompt = self.prompt.lock().unwrap().take();
            if let Some(answered) = prompt {
                answered.recv().unwrap();
            }
            if self.fail_reads.load(Ordering::SeqCst) {
                return Err(KeyStoreError { message: "access denied".to_owned() });
            }
            self.inner.read()
        }

        fn write(&self, key: &ApiKey) -> Result<(), KeyStoreError> {
            self.inner.write(key)
        }

        fn delete(&self) -> Result<(), KeyStoreError> {
            if self.fail_delete.load(Ordering::SeqCst) {
                return Err(KeyStoreError { message: "the item is locked".to_owned() });
            }
            self.inner.delete()
        }
    }

    impl CountingKeychain {
        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }

        /// Holds the next read until the sender sends.
        fn prompt(&self) -> mpsc::Sender<()> {
            let (answer, answered) = mpsc::channel();
            *self.prompt.lock().unwrap() = Some(answered);
            answer
        }

        /// Waits until `count` reads have started.
        fn wait_for_reads(&self, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.reads() < count {
                assert!(Instant::now() < deadline, "the read never started");
                thread::yield_now();
            }
        }
    }

    struct Fixture {
        keys: Arc<CountingKeychain>,
        tester: FakeTester,
        providers: Providers,
        settings: Settings,
    }

    fn fixture(key: Option<&str>) -> Fixture {
        let keys = Arc::new(CountingKeychain {
            inner: MemoryKeychain::new(key),
            reads: AtomicUsize::new(0),
            fail_reads: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
            prompt: Mutex::new(None),
        });
        let tester = FakeTester::default();
        let providers = Providers::new(Arc::clone(&keys) as Arc<dyn KeyStore>, Box::new(tester.clone()));
        Fixture { keys, tester, providers, settings: Settings::unsaved() }
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 14).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    fn passed_test() -> KeyTest {
        KeyTest { passed: true, at: now() - chrono::Duration::days(2), cost_usd: 0.003 }
    }

    impl Fixture {
        fn poll(&mut self) -> bool {
            self.providers.poll(&mut self.settings, now())
        }

        fn method(&self) -> SignIn {
            self.settings.preferences().sign_in
        }

        /// What the keychain holds, read past the counter.
        fn stored(&self) -> Option<String> {
            self.keys.inner.read().unwrap().map(|key| key.expose().to_owned())
        }

        /// Waits until `count` readers wait for a read under way.
        fn wait_for_waiters(&self, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.providers.key.waiting.load(Ordering::SeqCst) < count {
                assert!(Instant::now() < deadline, "no reader waits");
                thread::yield_now();
            }
        }

        fn failure(&self) -> Option<(Outcome, &str)> {
            self.providers.key_failure().map(|failure| (failure.outcome, failure.said.as_str()))
        }
    }

    #[test]
    fn a_saved_key_that_passes_its_test_signs_tasks_in() {
        let mut f = fixture(None);
        f.providers.save_key(&format!("  {FAKE}\n"), &mut f.settings, false).unwrap();
        assert_eq!(f.stored().as_deref(), Some(FAKE));
        assert_eq!(f.providers.saved(), &Saved::Present(KeyMask::of(&ApiKey::parse(FAKE).unwrap())));
        assert_eq!((f.tester.calls(), f.tester.last_key().as_deref()), (1, Some(FAKE)), "the typed key is tested at once");
        assert_eq!(f.providers.testing(), Some(TestTarget::Key));
        assert_eq!(f.method(), SignIn::ClaudeSignIn, "not before the test passes");
        assert!(!f.poll(), "nothing ended yet");

        f.tester.answer(Outcome::Passed, "ready");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ApiKey);
        assert_eq!(f.settings.preferences().key_test, Some(KeyTest { passed: true, at: now(), cost_usd: 0.004 }));
        assert_eq!(f.providers.testing(), None);
        assert_eq!(f.failure(), None);
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("API key".to_owned(), WordTone::Muted));
    }

    #[test]
    fn a_saved_key_that_is_refused_leaves_tasks_on_the_claude_sign_in() {
        let mut f = fixture(None);
        f.providers.save_key(FAKE, &mut f.settings, false).unwrap();
        f.tester.answer(Outcome::Refused, "Invalid API key · Fix external API key");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        assert_eq!(f.settings.preferences().key_test, Some(KeyTest { passed: false, at: now(), cost_usd: 0.004 }));
        assert_eq!(f.failure(), Some((Outcome::Refused, "Invalid API key · Fix external API key")));
        assert_eq!(f.stored().as_deref(), Some(FAKE), "a refused key stays saved");
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("Key refused".to_owned(), WordTone::Closed));
    }

    #[test]
    fn a_saved_key_whose_test_is_unreached_stays_saved_and_unused() {
        let mut f = fixture(None);
        f.providers.save_key(FAKE, &mut f.settings, false).unwrap();
        f.tester.answer(Outcome::Unreached, "Claude Code did not answer within a minute.");
        assert!(f.poll());
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));
        assert_eq!(f.failure(), Some((Outcome::Unreached, "Claude Code did not answer within a minute.")));
        assert_eq!(f.stored().as_deref(), Some(FAKE));
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("Claude sign-in".to_owned(), WordTone::Muted), "not refused");
    }

    #[test]
    fn use_for_tasks_on_a_saved_key_switches_only_once_its_test_passes() {
        let mut f = fixture(Some(FAKE));
        f.providers.read_once();
        f.providers.test_key(false).unwrap();
        assert_eq!(f.tester.last_key().as_deref(), Some(FAKE), "the key is read when clicked");
        assert!(!f.poll());
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        f.tester.answer(Outcome::Passed, "ready");
        f.poll();
        assert_eq!(f.method(), SignIn::ApiKey);
    }

    #[test]
    fn a_refused_test_of_the_key_in_use_moves_tasks_back() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.providers.read_once();
        assert_eq!(f.providers.task_sign_in(f.settings.preferences()), SignIn::ApiKey);
        f.providers.test_key(false).unwrap();
        f.tester.answer(Outcome::Refused, "Invalid API key");
        f.poll();
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        assert_eq!(f.providers.task_sign_in(f.settings.preferences()), SignIn::ClaudeSignIn);
    }

    #[test]
    fn an_unreached_test_of_the_key_in_use_keeps_tasks_on_it() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.settings.set_key_test(Some(passed_test()));
        f.providers.test_key(false).unwrap();
        f.tester.answer(Outcome::Unreached, "Claude Code is not installed.");
        assert!(f.poll());
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ApiKey, Some(passed_test())), "nothing was learnt about the key");
        assert_eq!(f.failure(), Some((Outcome::Unreached, "Claude Code is not installed.")));
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("API key".to_owned(), WordTone::Muted));
        f.providers.test_key(false).unwrap();
        f.tester.answer(Outcome::Passed, "ready");
        f.poll();
        assert_eq!(f.failure(), None, "a pass clears the last failure");
    }

    #[test]
    fn saving_a_new_key_takes_tasks_off_the_old_one_until_the_new_one_passes() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.settings.set_key_test(Some(passed_test()));
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        assert_eq!(f.stored().as_deref(), Some(OTHER));
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));
    }

    #[test]
    fn remove_empties_the_keychain_and_goes_back_to_the_claude_sign_in() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.settings.set_key_test(Some(passed_test()));
        f.providers.read_once();
        f.providers.remove_key(&mut f.settings).unwrap();
        assert_eq!(f.stored(), None);
        assert_eq!(f.providers.saved(), &Saved::Absent);
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("Claude sign-in".to_owned(), WordTone::Muted));
    }

    #[test]
    fn a_key_test_that_ends_after_remove_a_switch_or_a_new_key_changes_nothing() {
        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.providers.remove_key(&mut f.settings).unwrap();
        assert_eq!(f.providers.test_gate(false), TestGate::Testing, "its process still runs");
        assert_eq!(f.providers.testing(), None, "but no key is under test");
        f.tester.answer(Outcome::Passed, "ready");
        assert!(f.poll());
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));

        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.providers.use_claude_sign_in(&mut f.settings);
        assert_eq!((f.providers.testing(), f.providers.test_gate(false)), (None, TestGate::Testing));
        f.tester.answer(Outcome::Passed, "ready");
        f.poll();
        assert_eq!(f.method(), SignIn::ClaudeSignIn);

        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        assert_eq!(f.tester.calls(), 1, "the new key waits for the old test to end");
        assert_eq!(f.providers.testing(), None, "the new key is not under test yet");
        f.tester.answer(Outcome::Passed, "ready");
        f.poll();
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None), "the old key's pass is not the new key's");
    }

    #[test]
    fn a_running_task_keeps_the_tester_from_being_called() {
        let mut f = fixture(Some(FAKE));
        assert_eq!(f.providers.test_gate(true), TestGate::TaskRunning);
        assert_eq!(f.providers.test_key(true), Err(TASK_RUNNING.to_owned()));
        assert_eq!(f.providers.test_sign_in(true), Err(TASK_RUNNING.to_owned()));
        f.providers.save_key(OTHER, &mut f.settings, true).unwrap();
        assert_eq!(f.stored().as_deref(), Some(OTHER), "the key is still saved");
        assert_eq!(f.tester.calls(), 0);
        assert_eq!(TestGate::TaskRunning.reason(), Some(TASK_RUNNING));
        assert_eq!((task_wait_reason(true), task_wait_reason(false)), (TASK_RUNNING, TASK_OPEN));
    }

    #[test]
    fn a_second_test_is_refused_while_one_runs_but_a_key_is_still_saved() {
        let mut f = fixture(Some(FAKE));
        f.providers.test_sign_in(false).unwrap();
        assert_eq!(f.providers.test_gate(false), TestGate::Testing);
        assert_eq!(f.providers.test_key(false), Err(TESTING.to_owned()));
        assert_eq!(f.providers.test_sign_in(false), Err(TESTING.to_owned()));
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        assert_eq!(f.stored().as_deref(), Some(OTHER));
        assert_eq!(f.tester.calls(), 1, "the saved key is not tested while another test runs");
        assert_eq!(f.providers.test_gate(false).reason(), Some(TESTING));
    }

    #[test]
    fn a_sign_in_test_changes_no_method() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.providers.test_sign_in(false).unwrap();
        assert_eq!(f.tester.last_key(), None, "no key goes to the Claude sign-in test");
        assert_eq!(f.providers.testing(), Some(TestTarget::ClaudeSignIn));
        f.tester.answer(Outcome::Refused, "Not logged in · Please run /login");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ApiKey);
        assert_eq!(f.settings.preferences().key_test, None);
        assert_eq!(f.failure(), None);
        let check = f.providers.sign_in_check().unwrap();
        assert_eq!((check.outcome, check.at, check.said.as_str()), (Outcome::Refused, now(), "Not logged in · Please run /login"));
    }

    #[test]
    fn a_tester_that_never_reports_is_unreached() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.providers.test_key(false).unwrap();
        f.tester.asked.lock().unwrap().clear();
        assert!(f.poll());
        assert_eq!(f.failure(), Some((Outcome::Unreached, NO_ANSWER)));
        assert_eq!(f.method(), SignIn::ApiKey);
    }

    #[test]
    fn keychain_problems_are_said_without_testing() {
        let mut f = fixture(None);
        assert_eq!(f.providers.test_key(false), Err(NO_KEY.to_owned()));
        assert_eq!(f.providers.saved(), &Saved::Absent);
        let mut f = fixture(Some("has a space"));
        assert!(f.providers.test_key(false).is_err());
        assert!(matches!(f.providers.saved(), Saved::Unreadable(_)));
        assert_eq!(f.providers.save_key("sk-ant- 1234", &mut f.settings, false), Err(NOT_A_KEY.to_owned()));
        assert_eq!(f.tester.calls(), 0);
    }

    #[test]
    fn the_keychain_is_read_once_per_launch() {
        let mut f = fixture(Some(FAKE));
        assert_eq!(f.providers.saved(), &Saved::NotRead);
        assert_eq!(f.keys.reads(), 0, "nothing reads it until asked");
        f.providers.read_once();
        f.keys.inner.delete().unwrap();
        f.providers.read_once();
        assert!(matches!(f.providers.saved(), Saved::Present(_)));
        f.providers.test_key(false).unwrap();
        assert_eq!(f.tester.last_key().as_deref(), Some(FAKE), "the test takes the key held in memory");
        let reader = f.providers.key_reader();
        for _ in 0..3 {
            assert_eq!(reader().unwrap().unwrap().expose(), FAKE);
        }
        assert_eq!(f.providers.key_reader()().unwrap().unwrap().expose(), FAKE);
        assert_eq!(f.keys.reads(), 1);

        let mut f = fixture(None);
        f.providers.read_once();
        assert_eq!(f.providers.saved(), &Saved::Absent);
        assert!(f.providers.key_reader()().unwrap().is_none(), "no key is an answer too");
        assert_eq!(f.providers.test_key(false), Err(NO_KEY.to_owned()));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_task_start_that_reads_first_spares_providers_a_second_read() {
        let mut f = fixture(Some(FAKE));
        let reader = f.providers.key_reader();
        assert_eq!(reader().unwrap().unwrap().expose(), FAKE);
        assert_eq!(f.providers.saved(), &Saved::NotRead, "the page has not shown yet");
        f.providers.read_once();
        assert_eq!(f.providers.saved(), &Saved::Present(KeyMask::of(&ApiKey::parse(FAKE).unwrap())));
        f.providers.test_key(false).unwrap();
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_saved_key_replaces_the_one_held_without_a_read() {
        let mut f = fixture(Some(FAKE));
        let reader = f.providers.key_reader();
        assert_eq!(reader().unwrap().unwrap().expose(), FAKE);
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        assert_eq!(reader().unwrap().unwrap().expose(), OTHER, "a reader made before the save gives the new key");
        assert_eq!(f.tester.last_key().as_deref(), Some(OTHER));

        let mut f = fixture(None);
        f.providers.save_key(FAKE, &mut f.settings, false).unwrap();
        f.tester.answer(Outcome::Passed, "ready");
        f.poll();
        f.providers.read_once();
        assert_eq!(f.providers.key_reader()().unwrap().unwrap().expose(), FAKE);
        f.providers.test_key(false).unwrap();
        assert_eq!(f.keys.reads(), 0, "a key saved this launch is never read back");
    }

    #[test]
    fn remove_drops_the_held_key() {
        let mut f = fixture(Some(FAKE));
        let reader = f.providers.key_reader();
        assert!(reader().unwrap().is_some());
        f.providers.remove_key(&mut f.settings).unwrap();
        assert!(reader().unwrap().is_none());
        assert_eq!(f.providers.test_key(false), Err(NO_KEY.to_owned()));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_key_the_keychain_could_not_delete_stays_held_and_testable() {
        let mut f = fixture(Some(FAKE));
        f.providers.read_once();
        f.keys.fail_delete.store(true, Ordering::SeqCst);
        assert_eq!(f.providers.remove_key(&mut f.settings), Err("the item is locked".to_owned()));
        assert_eq!(f.stored().as_deref(), Some(FAKE), "the keychain still holds it");
        assert!(matches!(f.providers.saved(), Saved::Present(_)), "the page still says what the keychain holds");
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        assert_eq!(f.providers.key_reader()().unwrap().unwrap().expose(), FAKE, "and so does memory");
        f.providers.test_key(false).unwrap();
        assert_eq!(f.tester.last_key().as_deref(), Some(FAKE), "Test connection and Use for tasks still find it");
        assert!(matches!(f.providers.saved(), Saved::Present(_)));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_failed_read_is_not_held_and_the_next_one_asks_again() {
        let mut f = fixture(Some(FAKE));
        f.keys.fail_reads.store(true, Ordering::SeqCst);
        f.providers.read_once();
        assert_eq!(f.providers.saved(), &Saved::Unreadable("access denied".to_owned()));
        let reader = f.providers.key_reader();
        assert_eq!(reader().unwrap_err(), "access denied");
        assert_eq!(f.keys.reads(), 2, "an error is not an answer to keep");

        f.providers.read_once();
        assert_eq!(f.keys.reads(), 2, "the page does not ask again by itself");

        f.keys.fail_reads.store(false, Ordering::SeqCst);
        assert_eq!(reader().unwrap().unwrap().expose(), FAKE);
        f.keys.fail_reads.store(true, Ordering::SeqCst);
        assert_eq!(reader().unwrap().unwrap().expose(), FAKE, "once a read succeeds, the keychain is not asked again");
        f.providers.read_once();
        assert!(matches!(f.providers.saved(), Saved::Present(_)), "the page picks up the key a task's start read");
        f.providers.test_key(false).unwrap();
        assert_eq!(f.keys.reads(), 3);
    }

    #[test]
    fn two_readers_at_once_read_the_keychain_once_and_the_page_never_waits() {
        let mut f = fixture(Some(FAKE));
        let answer = f.keys.prompt();
        let first = thread::spawn({
            let reader = f.providers.key_reader();
            move || reader().map(|key| key.map(|key| key.expose().to_owned()))
        });
        f.keys.wait_for_reads(1);
        let second = thread::spawn({
            let reader = f.providers.key_reader();
            move || reader().map(|key| key.map(|key| key.expose().to_owned()))
        });
        f.wait_for_waiters(1);
        // The window, meanwhile, never waits on the prompt the task's start opened.
        assert!(f.providers.read_once(), "the page looks again later");
        assert_eq!(f.providers.saved(), &Saved::NotRead);
        assert_eq!(f.providers.test_key(false), Err(KEY_BUSY.to_owned()));
        answer.send(()).unwrap();
        assert_eq!(first.join().unwrap().unwrap().as_deref(), Some(FAKE));
        assert_eq!(second.join().unwrap().unwrap().as_deref(), Some(FAKE));
        assert!(!f.providers.read_once());
        assert!(matches!(f.providers.saved(), Saved::Present(_)));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_denied_prompt_answers_every_reader_that_waited_on_it() {
        let mut f = fixture(Some(FAKE));
        f.keys.fail_reads.store(true, Ordering::SeqCst);
        let answer = f.keys.prompt();
        let first = thread::spawn({
            let reader = f.providers.key_reader();
            move || reader().map(|key| key.is_some())
        });
        f.keys.wait_for_reads(1);
        let reader = f.providers.key_reader();
        let second = thread::spawn(move || reader().map(|key| key.is_some()));
        f.wait_for_waiters(1);
        answer.send(()).unwrap();
        assert_eq!(first.join().unwrap(), Err("access denied".to_owned()));
        assert_eq!(second.join().unwrap(), Err("access denied".to_owned()), "the waiting reader takes the denial");
        assert_eq!(f.keys.reads(), 1, "and does not ask again");
        f.providers.read_once();
        assert_eq!(f.providers.saved(), &Saved::Unreadable("access denied".to_owned()), "the page takes the denial without a prompt");
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_read_that_fails_while_a_key_is_saved_leaves_the_new_key() {
        let mut f = fixture(Some(FAKE));
        f.keys.fail_reads.store(true, Ordering::SeqCst);
        let answer = f.keys.prompt();
        let reading = thread::spawn({
            let reader = f.providers.key_reader();
            move || reader().map(|key| key.map(|key| key.expose().to_owned()))
        });
        f.keys.wait_for_reads(1);
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        answer.send(()).unwrap();
        assert_eq!(reading.join().unwrap().unwrap().as_deref(), Some(OTHER), "the failure is not the answer once a key is held");
        assert!(f.providers.key.cell().failure.is_none(), "nor is it kept");
        f.providers.read_once();
        assert!(matches!(f.providers.saved(), Saved::Present(_)));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_test_of_a_held_key_never_waits_for_the_read_turn() {
        let mut f = fixture(Some(FAKE));
        f.providers.read_once();
        let key = Arc::clone(&f.providers.key);
        let turn = key.turn.lock().unwrap();
        f.providers.test_key(false).unwrap();
        drop(turn);
        assert_eq!(f.tester.last_key().as_deref(), Some(FAKE));
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn a_key_saved_while_a_task_reads_the_keychain_wins() {
        let mut f = fixture(Some(FAKE));
        let answer = f.keys.prompt();
        let reading = thread::spawn({
            let reader = f.providers.key_reader();
            move || reader().map(|key| key.map(|key| key.expose().to_owned()))
        });
        f.keys.wait_for_reads(1);
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        answer.send(()).unwrap();
        assert_eq!(reading.join().unwrap().unwrap().as_deref(), Some(OTHER), "the old key it read is not held over the new one");
        assert_eq!(f.providers.key_reader()().unwrap().unwrap().expose(), OTHER);
        assert_eq!(f.keys.reads(), 1);
    }

    #[test]
    fn what_claude_code_said_comes_out_clean_and_masked() {
        let key = ApiKey::parse(FAKE).unwrap();
        let raw = format!("Invalid \u{202E}key\u{200B} {FAKE}\n\n   \n\u{1b}[31mred\u{1b}[0m sk-ant-api03\u{200B}-FAKE-not-a-real-key-0000-R2kd\r\n");
        let said = said_text(&raw, Some(&key));
        assert_eq!(said, "Invalid key sk-ant-••••••••R2kd\nred sk-ant-••••••••R2kd");
        assert!(!said.contains("api03"));
    }

    #[test]
    fn every_line_break_keeps_its_lines_apart() {
        assert_eq!(said_text("progress\rerror: Invalid API key", None), "progress\nerror: Invalid API key");
        for brk in ['\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}'] {
            assert_eq!(said_text(&format!("one{brk}two"), None), "one\ntwo", "{brk:?}");
        }
    }

    #[test]
    fn what_claude_code_said_is_cut_to_eight_lines_and_500_characters() {
        let lines: Vec<String> = (1..=12).map(|n| format!("line {n}")).collect();
        let said = said_text(&lines.join("\n"), None);
        assert_eq!(said.lines().count(), 8);
        assert!(said.ends_with("line 8"));
        let said = said_text(&"x".repeat(900), None);
        assert_eq!(said.chars().count(), 500);
        assert!(said.ends_with('…'));
    }

    #[test]
    fn ages_read_in_words() {
        let at = now();
        let after = |minutes: i64| at + chrono::Duration::minutes(minutes);
        let cases = [
            (at - chrono::Duration::minutes(3), "just now"),
            (at, "just now"),
            (at + chrono::Duration::seconds(59), "just now"),
            (after(1), "1 minute ago"),
            (after(4), "4 minutes ago"),
            (after(59), "59 minutes ago"),
            (after(60), "1 hour ago"),
            (after(119), "1 hour ago"),
            (after(120), "2 hours ago"),
            (after(14 * 60 + 29), "14 hours ago"),
            (after(14 * 60 + 30), "yesterday"),
            (after(25 * 60 + 30), "yesterday"),
            (after(38 * 60 + 29), "yesterday"),
            (after(38 * 60 + 30), "2 days ago"),
            (after(47 * 60), "2 days ago"),
            (after(30 * 24 * 60), "30 days ago"),
        ];
        for (now, words) in cases {
            assert_eq!(age_words(at, now), words, "{now}");
        }
    }

    #[test]
    fn yesterday_is_the_day_before_on_the_calendar() {
        let day = |d: u32, h: u32, m: u32| NaiveDate::from_ymd_opt(2026, 3, d).unwrap().and_hms_opt(h, m, 0).unwrap();
        assert_eq!(age_words(day(14, 23, 30), day(15, 0, 10)), "40 minutes ago", "within the hour, across midnight");
        assert_eq!(age_words(day(14, 23, 0), day(15, 1, 0)), "yesterday");
        assert_eq!(age_words(day(14, 0, 30), day(15, 23, 30)), "yesterday", "47 hours, one calendar day");
        assert_eq!(age_words(day(14, 23, 30), day(16, 0, 30)), "2 days ago", "25 hours, two calendar days");
    }

    #[test]
    fn sign_in_sources_read_in_words() {
        let (key, sign_in) = (Some(SignIn::ApiKey), Some(SignIn::ClaudeSignIn));
        let cases = [
            ("", sign_in, "your Claude sign-in"),
            ("none", sign_in, "your Claude sign-in"),
            ("none", key, "your Claude sign-in"),
            ("none", None, "your Claude sign-in"),
            ("ANTHROPIC_API_KEY", key, "your API key"),
            ("ANTHROPIC_API_KEY", sign_in, "an API key from your environment"),
            ("ANTHROPIC_API_KEY", None, "an API key"),
            ("apiKeyHelper", key, "a sign-in Claude Code chose itself"),
            ("/login managed key", sign_in, "your Claude sign-in"),
            ("/login managed key", None, "your Claude sign-in"),
            ("apiKeyHelper", None, "a sign-in Claude Code chose itself"),
            ("\u{202E}evil", None, "a sign-in Claude Code chose itself"),
        ];
        for (source, passed, words) in cases {
            assert_eq!(sign_in_words(source, passed), words, "{source:?} {passed:?}");
        }
    }

    #[test]
    fn only_no_key_is_the_claude_sign_in() {
        assert!(is_claude_sign_in(None) && is_claude_sign_in(Some("none")) && is_claude_sign_in(Some("")));
        assert!(is_claude_sign_in(Some("/login managed key")));
        assert!(!is_claude_sign_in(Some("ANTHROPIC_API_KEY")) && !is_claude_sign_in(Some("apiKeyHelper")));
    }
}
