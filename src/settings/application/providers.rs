use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use chrono::NaiveDateTime;

use crate::settings::application::settings::Settings;
use crate::settings::domain::{ConnectionTester, KeyMask, KeyStore, KeyTest, Preferences, Saved, TestReport, redacted, task_sign_in};
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::{ApiKey, KeyReader, SignIn};

/// Why Test connection is off while a task runs: one Claude process at a time.
pub const TASK_RUNNING: &str = "A task is running. Test once it ends.";
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
    pub passed: bool,
    pub at: NaiveDateTime,
    pub cost_usd: f64,
    /// Clean already: what Claude Code said, for a failed test.
    pub said: String,
    /// The CLI's `apiKeySource`, for `sign_in_words`.
    pub source: Option<String>,
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
    /// What Claude Code said when it last refused the saved key, kept for this session only.
    refusal: Option<String>,
}

impl Providers {
    pub fn new(keys: Arc<dyn KeyStore>, tester: Box<dyn ConnectionTester>) -> Self {
        Self { keys, tester, saved: Saved::NotRead, running: None, sign_in_check: None, refusal: None }
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
    pub fn testing(&self) -> Option<TestTarget> {
        self.running.as_ref().map(|running| running.target)
    }

    /// What Claude Code said when it last refused the key, clean and masked.
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    pub fn sign_in_check(&self) -> Option<&SignInCheck> {
        self.sign_in_check.as_ref()
    }

    /// Writes the key to the keychain, then tests it at once when nothing stops a test. Tasks stay on the Claude sign-in
    /// until that test passes, so a mistyped key never breaks the next task. No `sk-ant-` prefix is required.
    pub fn save_key(&mut self, typed: &str, settings: &mut Settings, task_running: bool) -> Result<(), String> {
        if self.running.is_some() {
            return Err(TESTING.to_owned());
        }
        let key = ApiKey::parse(typed).ok_or_else(|| NOT_A_KEY.to_owned())?;
        self.keys.write(&key).map_err(|error| error.message)?;
        self.saved = Saved::Present(KeyMask::of(&key));
        self.refusal = None;
        // The last test was of the key this one replaces.
        settings.set_sign_in(SignIn::ClaudeSignIn);
        settings.set_key_test(None);
        if self.test_gate(task_running) == TestGate::Ready {
            self.start(TestTarget::Key, Some(key));
        }
        Ok(())
    }

    /// Tests the saved key, read from the keychain now. A pass puts tasks on the key, and a failure moves them to the Claude sign-in.
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
        self.refusal = None;
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
            Err(TryRecvError::Disconnected) => TestReport { passed: false, cost_usd: 0.0, said: NO_ANSWER.to_owned(), source: None },
        };
        if running.cancelled {
            return true;
        }
        let said = said_text(&report.said, running.key.as_ref());
        match running.target {
            TestTarget::Key => {
                settings.set_key_test(Some(KeyTest { passed: report.passed, at: now, cost_usd: report.cost_usd }));
                if report.passed {
                    settings.set_sign_in(SignIn::ApiKey);
                    self.refusal = None;
                } else {
                    settings.set_sign_in(SignIn::ClaudeSignIn);
                    self.refusal = Some(said);
                }
            }
            TestTarget::ClaudeSignIn => {
                self.sign_in_check = Some(SignInCheck { passed: report.passed, at: now, cost_usd: report.cost_usd, said, source: report.source });
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
        .lines()
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

/// How long ago a test ran, in words.
pub fn age_words(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let minutes = (now - at).num_minutes();
    match minutes {
        ..=0 => "just now".to_owned(),
        1 => "1 minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        60..120 => "1 hour ago".to_owned(),
        120..1_440 => format!("{} hours ago", minutes / 60),
        1_440..2_880 => "yesterday".to_owned(),
        _ => format!("{} days ago", minutes / 1_440),
    }
}

/// The CLI's `apiKeySource` in words. A value it does not name never reaches the screen as it is.
pub fn sign_in_words(source: &str) -> &'static str {
    match source {
        "" | "none" => "your Claude sign-in",
        "ANTHROPIC_API_KEY" => "your API key",
        _ => "a sign-in Claude Code chose itself",
    }
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

        fn answer(&self, passed: bool, said: &str) {
            let done = self.asked.lock().unwrap().last_mut().unwrap().1.take().unwrap();
            done(TestReport { passed, cost_usd: 0.004, said: said.to_owned(), source: Some("ANTHROPIC_API_KEY".to_owned()) });
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

        f.tester.answer(true, "ready");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ApiKey);
        assert_eq!(f.settings.preferences().key_test, Some(KeyTest { passed: true, at: now(), cost_usd: 0.004 }));
        assert_eq!(f.providers.testing(), None);
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("API key".to_owned(), WordTone::Muted));
    }

    #[test]
    fn a_saved_key_that_fails_its_test_leaves_tasks_on_the_claude_sign_in() {
        let mut f = fixture(None);
        f.providers.save_key(FAKE, &mut f.settings, false).unwrap();
        f.tester.answer(false, "Invalid API key · Fix external API key");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        assert_eq!(f.settings.preferences().key_test.map(|test| test.passed), Some(false));
        assert_eq!(f.providers.refusal(), Some("Invalid API key · Fix external API key"));
        assert_eq!(f.stored().as_deref(), Some(FAKE), "a refused key stays saved");
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("Key refused".to_owned(), WordTone::Closed));
    }

    #[test]
    fn use_for_tasks_on_a_saved_key_switches_only_once_its_test_passes() {
        let mut f = fixture(Some(FAKE));
        f.providers.read_once();
        f.providers.test_key(false).unwrap();
        assert_eq!(f.tester.last_key().as_deref(), Some(FAKE), "the key is read when clicked");
        assert!(!f.poll());
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        f.tester.answer(true, "ready");
        f.poll();
        assert_eq!(f.method(), SignIn::ApiKey);
    }

    #[test]
    fn a_failed_test_of_the_key_in_use_moves_tasks_back() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.providers.read_once();
        assert_eq!(f.providers.task_sign_in(f.settings.preferences()), SignIn::ApiKey);
        f.providers.test_key(false).unwrap();
        f.tester.answer(false, "Credit balance is too low");
        f.poll();
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
        assert_eq!(f.providers.task_sign_in(f.settings.preferences()), SignIn::ClaudeSignIn);
    }

    #[test]
    fn saving_a_new_key_takes_tasks_off_the_old_one_until_the_new_one_passes() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.settings.set_key_test(Some(KeyTest { passed: true, at: now(), cost_usd: 0.004 }));
        f.providers.save_key(OTHER, &mut f.settings, false).unwrap();
        assert_eq!(f.stored().as_deref(), Some(OTHER));
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));
    }

    #[test]
    fn remove_empties_the_keychain_and_goes_back_to_the_claude_sign_in() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.settings.set_key_test(Some(KeyTest { passed: true, at: now(), cost_usd: 0.004 }));
        f.providers.read_once();
        f.providers.remove_key(&mut f.settings).unwrap();
        assert_eq!(f.stored(), None);
        assert_eq!(f.providers.saved(), &Saved::Absent);
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));
        assert_eq!(f.providers.rail_word(f.settings.preferences()), ("Claude sign-in".to_owned(), WordTone::Muted));
    }

    #[test]
    fn a_key_test_that_ends_after_remove_or_a_switch_changes_nothing() {
        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.providers.remove_key(&mut f.settings).unwrap();
        assert_eq!(f.providers.test_gate(false), TestGate::Testing, "its process still runs");
        f.tester.answer(true, "ready");
        assert!(f.poll());
        assert_eq!((f.method(), f.settings.preferences().key_test), (SignIn::ClaudeSignIn, None));

        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.providers.use_claude_sign_in(&mut f.settings);
        f.tester.answer(true, "ready");
        f.poll();
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
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
    }

    #[test]
    fn a_second_test_is_refused_while_one_runs() {
        let mut f = fixture(Some(FAKE));
        f.providers.test_sign_in(false).unwrap();
        assert_eq!(f.providers.test_gate(false), TestGate::Testing);
        assert_eq!(f.providers.test_key(false), Err(TESTING.to_owned()));
        assert_eq!(f.providers.test_sign_in(false), Err(TESTING.to_owned()));
        assert_eq!(f.providers.save_key(OTHER, &mut f.settings, false), Err(TESTING.to_owned()));
        assert_eq!(f.stored().as_deref(), Some(FAKE));
        assert_eq!(f.tester.calls(), 1);
    }

    #[test]
    fn a_sign_in_test_changes_no_method() {
        let mut f = fixture(Some(FAKE));
        f.settings.set_sign_in(SignIn::ApiKey);
        f.providers.test_sign_in(false).unwrap();
        assert_eq!(f.tester.last_key(), None, "no key goes to the Claude sign-in test");
        assert_eq!(f.providers.testing(), Some(TestTarget::ClaudeSignIn));
        f.tester.answer(false, "Not logged in · Please run /login");
        assert!(f.poll());
        assert_eq!(f.method(), SignIn::ApiKey);
        assert_eq!(f.settings.preferences().key_test, None);
        let check = f.providers.sign_in_check().unwrap();
        assert_eq!((check.passed, check.at, check.said.as_str()), (false, now(), "Not logged in · Please run /login"));
    }

    #[test]
    fn a_tester_that_never_reports_ends_as_a_failure() {
        let mut f = fixture(Some(FAKE));
        f.providers.test_key(false).unwrap();
        f.tester.asked.lock().unwrap().clear();
        assert!(f.poll());
        assert_eq!(f.providers.refusal(), Some(NO_ANSWER));
        assert_eq!(f.method(), SignIn::ClaudeSignIn);
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
            (after(23 * 60 + 59), "23 hours ago"),
            (after(24 * 60), "yesterday"),
            (after(47 * 60), "yesterday"),
            (after(48 * 60), "2 days ago"),
            (after(30 * 24 * 60), "30 days ago"),
        ];
        for (now, words) in cases {
            assert_eq!(age_words(at, now), words, "{now}");
        }
    }

    #[test]
    fn sign_in_sources_read_in_words() {
        let cases = [
            ("", "your Claude sign-in"),
            ("none", "your Claude sign-in"),
            ("ANTHROPIC_API_KEY", "your API key"),
            ("apiKeyHelper", "a sign-in Claude Code chose itself"),
            ("/login managed key", "a sign-in Claude Code chose itself"),
            ("\u{202E}evil", "a sign-in Claude Code chose itself"),
        ];
        for (source, words) in cases {
            assert_eq!(sign_in_words(source), words, "{source:?}");
        }
    }
}
