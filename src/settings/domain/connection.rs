use crate::shared_kernel::ApiKey;

/// How a test ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    /// Claude Code answered that the sign-in under test was refused: an invalid key, or no sign-in at all.
    Refused,
    /// The test never got an answer about the sign-in: Claude Code missing, a timeout, a network error, another sign-in used.
    Unreached,
}

/// What one Test connection found. `said` is what Claude Code said, or why it could not be asked.
#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    pub outcome: Outcome,
    pub cost_usd: f64,
    pub said: String,
    /// The init line's `apiKeySource`: how Claude Code signed in. None when it never got that far.
    pub source: Option<String>,
}

/// Takes the report when a test ends, on any thread.
pub type TestDone = Box<dyn FnOnce(TestReport) + Send>;

/// Asks Claude Code once, with the key in its environment or, without a key, with the sign-in a task would use.
/// It is not a task: Jobdesk writes no run log for it, and Claude Code is told to save no session.
pub trait ConnectionTester {
    /// Returns at once. `done` gets the report when the test ends.
    fn start(&self, key: Option<ApiKey>, done: TestDone);
}
