use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use chrono::NaiveDateTime;

use crate::settings::application::settings::Settings;
use crate::settings::domain::{ConnectionTester, KeyMask, KeyStore, KeyTest, Outcome, Preferences, Saved, TestReport, redacted, task_sign_in};
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::{ApiKey, KeyReader, SignIn};

/// Why Test connection is off while a task runs: one Claude process at a time.
pub const TASK_RUNNING: &str = "A task is running. Test once it ends.";
/// Why Test connection is off while a task's process is up but only waits for the user's reply.
pub const TASK_OPEN: &str = "A task is still open. Reply or end it, then test.";
/// What Test connection reads while a test runs, and why a second one is refused.
pub const TESTING: &str = "Testing…";
const NOT_A_KEY: &str = "This is not a key. Paste the whole key, with nothing inside it but its own characters.";
const NO_KEY: &str = "No key is saved.";
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

/// How tasks sign in, the saved API key and Test connection. The key stays in the keychain: this reads it only to test it,
/// and a task's process reads it through `key_reader` when it starts.
pub struct Providers {
    keys: Arc<dyn KeyStore>,
    tester: Box<dyn ConnectionTester>,
    saved: Saved,
    running: Option<Running>,
    sign_in_check: Option<SignInCheck>,
    key_failure: Option<KeyFailure>,
}

impl Providers {
    pub fn new(keys: Arc<dyn KeyStore>, tester: Box<dyn ConnectionTester>) -> Self {
        Self { keys, tester, saved: Saved::NotRead, running: None, sign_in_check: None, key_failure: None }
    }

    pub fn saved(&self) -> &Saved {
        &self.saved
    }

    /// The one keychain read per launch, for the mask and the method. Later calls change nothing.
    pub fn read_once(&mut self) {
        if self.saved != Saved::NotRead {
            return;
        }
        self.saved = match self.keys.read() {
            Ok(None) => Saved::Absent,
            Ok(Some(key)) => Saved::Present(KeyMask::of(&key)),
            Err(error) => Saved::Unreadable(error.message),
        };
    }

    /// How the next task signs in.
    pub fn task_sign_in(&self, preferences: &Preferences) -> SignIn {
        task_sign_in(preferences.sign_in, &self.saved)
    }

    /// Reads the saved key from the keychain each time it is called, when a task's process starts.
    pub fn key_reader(&self) -> KeyReader {
        let keys = Arc::clone(&self.keys);
        Arc::new(move || keys.read().map_err(|error| error.message))
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

    /// Writes the key to the keychain, then tests it at once when nothing stops a test: otherwise the page gives `test_gate`'s reason.
    /// Tasks stay on the Claude sign-in until a test passes, so a mistyped key never breaks the next task. No `sk-ant-` prefix is required.
    pub fn save_key(&mut self, typed: &str, settings: &mut Settings, task_running: bool) -> Result<(), String> {
        let key = ApiKey::parse(typed).ok_or_else(|| NOT_A_KEY.to_owned())?;
        self.keys.write(&key).map_err(|error| error.message)?;
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

    /// Tests the saved key, read from the keychain now. A pass puts tasks on the key, and a refusal moves them to the Claude sign-in.
    /// A test that never got an answer changes neither.
    /// It serves Test connection, Test again and Use for tasks on the key alike.
    pub fn test_key(&mut self, task_running: bool) -> Result<(), String> {
        self.gate(task_running)?;
        let key = match self.keys.read() {
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

    /// Tasks go back to the Claude sign-in even when the keychain item cannot be deleted.
    pub fn remove_key(&mut self, settings: &mut Settings) -> Result<(), String> {
        self.cancel_key_test();
        settings.set_sign_in(SignIn::ClaudeSignIn);
        self.keys.delete().map_err(|error| error.message)?;
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
pub fn said_text(raw: &str, key: Option<&ApiKey>) -> String {
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

/// The `apiKeySource` Claude Code gives for the account signed in with `/login`.
const LOGIN_KEY: &str = "/login managed key";

/// The CLI's `apiKeySource` in words. A value it does not name never reaches the screen as it is.
/// `passed` is how Jobdesk started that process: only with `SignIn::ApiKey` did the key in its environment come from Jobdesk.
/// Otherwise it came from the shell that started Jobdesk, and must not read as the key saved here.
/// None is a process whose method is not known, such as a task read back from its log: a key then reads as no one's.
pub fn sign_in_words(source: &str, passed: Option<SignIn>) -> &'static str {
    match (source, passed) {
        ("" | "none" | LOGIN_KEY, _) => "your Claude sign-in",
        ("ANTHROPIC_API_KEY", Some(SignIn::ApiKey)) => "your API key",
        ("ANTHROPIC_API_KEY", Some(SignIn::ClaudeSignIn)) => "an API key from your environment",
        ("ANTHROPIC_API_KEY", None) => "an API key",
        _ => "a sign-in Claude Code chose itself",
    }
}

/// Whether `source` is Claude Code's own sign-in, as a test of it hopes: not a key it found in its environment.
pub fn is_claude_sign_in(source: Option<&str>) -> bool {
    matches!(source, None | Some("" | "none" | LOGIN_KEY))
}

/// Why Test connection waits for a task's process: one that works, or one that only waits for the user's reply.
pub fn task_wait_reason(working: bool) -> &'static str {
    if working { TASK_RUNNING } else { TASK_OPEN }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

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

    struct Fixture {
        keys: Arc<MemoryKeychain>,
        tester: FakeTester,
        providers: Providers,
        settings: Settings,
    }

    fn fixture(key: Option<&str>) -> Fixture {
        let keys = Arc::new(MemoryKeychain::new(key));
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

        fn stored(&self) -> Option<String> {
            self.keys.read().unwrap().map(|key| key.expose().to_owned())
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
        f.providers.read_once();
        f.keys.delete().unwrap();
        f.providers.read_once();
        assert!(matches!(f.providers.saved(), Saved::Present(_)));
        assert_eq!(fixture(None).providers.saved(), &Saved::NotRead);
        let mut f = fixture(None);
        f.providers.read_once();
        assert_eq!(f.providers.saved(), &Saved::Absent);
    }

    #[test]
    fn the_key_reader_reads_the_keychain_each_time() {
        let f = fixture(None);
        let reader = f.providers.key_reader();
        assert!(reader().unwrap().is_none());
        f.keys.write(&ApiKey::parse(FAKE).unwrap()).unwrap();
        assert_eq!(reader().unwrap().unwrap().expose(), FAKE);
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
