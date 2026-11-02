use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::settings::domain::{ConnectionTester, Outcome, TestDone, TestReport};
use crate::shared_kernel::host_path::{find_executable, resolved_path};
use crate::shared_kernel::process_tree::{kill_tree, own_group};
use crate::shared_kernel::{ApiKey, ENV_THAT_OUTRANKS_THE_KEY, redacted};

const PROMPT: &str = "Reply with the single word: ready";
const TIMEOUT: Duration = Duration::from_secs(60);
/// How long stderr may still take to close once the process ended.
const STDERR_GRACE: Duration = Duration::from_secs(2);
const STDERR_LINES_KEPT: usize = 12;
const KEY_VARIABLE: &str = "ANTHROPIC_API_KEY";
const NOT_INSTALLED: &str = "Claude Code is not installed.";
const TIMED_OUT: &str = "Claude Code did not answer within a minute.";
const KEY_NOT_USED: &str = "Claude Code did not use the key.";
/// Words in a failed result, lowercased, that say the sign-in itself was turned away.
const REFUSALS: [&str; 5] = ["invalid api key", "invalid x-api-key", "authentication_error", "not logged in", "please run /login"];

/// Runs one `claude -p` with no tools and a one-cent cap, in an empty temporary folder that is removed afterwards.
/// It takes no data folder, so it writes no run log: Tasks never lists it and Usage never counts it.
pub struct ClaudeTester {
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl ClaudeTester {
    pub fn new(repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { repaint }
    }
}

impl ConnectionTester for ClaudeTester {
    fn start(&self, key: Option<ApiKey>, done: TestDone) {
        let repaint = Arc::clone(&self.repaint);
        thread::spawn(move || {
            let mut report = test(key.as_ref());
            // Claude Code should never echo the key, but if it did, the report must not carry it.
            report.said = redacted(&report.said, key.as_ref());
            done(report);
            repaint();
        });
    }
}

/// A tester that never answers, for previews and snapshots.
#[derive(Default)]
pub struct IdleTester {
    /// Kept, so each test stays open and reads as running.
    waiting: Mutex<Vec<TestDone>>,
}

impl ConnectionTester for IdleTester {
    fn start(&self, _key: Option<ApiKey>, done: TestDone) {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner).push(done);
    }
}

/// How the process ended, when no result line said why.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ending {
    Exited(String),
    TimedOut,
    NotInstalled,
    NotStarted(String),
}

impl Ending {
    fn words(&self) -> String {
        match self {
            Ending::Exited(status) => format!("Claude Code exited with {status}."),
            Ending::TimedOut => TIMED_OUT.to_owned(),
            Ending::NotInstalled => NOT_INSTALLED.to_owned(),
            Ending::NotStarted(error) => format!("Could not start Claude Code: {error}"),
        }
    }
}

fn unreached(said: String) -> TestReport {
    TestReport { outcome: Outcome::Unreached, cost_usd: 0.0, said, source: None }
}

fn test(key: Option<&ApiKey>) -> TestReport {
    let folder = match tempfile::TempDir::new() {
        Ok(folder) => folder,
        Err(error) => return unreached(format!("Could not make a temporary folder: {error}")),
    };
    let path = resolved_path();
    let Some(claude) = find_executable(path, "claude") else {
        return report_of(&[], &[], &Ending::NotInstalled, key.is_some());
    };
    let (out, tail, ending) = run(command_for(&claude, folder.path(), path, key), TIMEOUT);
    // The folder goes only now, once nothing runs in it any more.
    drop(folder);
    report_of(&out, &tail, &ending, key.is_some())
}

/// Runs the command to its end, or kills it and everything it started once `timeout` has passed.
/// Returns what it printed on stdout, the last lines of stderr and how it ended.
fn run(mut command: Command, timeout: Duration) -> (Vec<String>, Vec<String>, Ending) {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return (Vec::new(), Vec::new(), Ending::NotStarted(error.to_string())),
    };
    let deadline = Instant::now() + timeout;
    let stdout = child.stdout.take().map(lines_of);
    let stderr = child.stderr.take().map(lines_of);
    let mut out = Vec::new();
    let mut timed_out = false;
    if let Some(stdout) = &stdout {
        loop {
            match stdout.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(line) => out.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    timed_out = true;
                    break;
                }
            }
        }
    }
    let status = if timed_out { None } else { wait_until(&mut child, deadline) };
    let ending = match status {
        Some(status) => Ending::Exited(status.code().map_or_else(|| status.to_string(), |code| format!("code {code}"))),
        None => {
            kill_tree(child.id());
            Ending::TimedOut
        }
    };
    // Reaped before the caller removes the folder it ran in.
    let _ = child.wait();
    let tail = stderr.map(|stderr| last_lines(&stderr)).unwrap_or_default();
    (out, tail, ending)
}

fn command_for(claude: &Path, folder: &Path, path: &str, key: Option<&ApiKey>) -> Command {
    let mut command = Command::new(claude);
    command
        .current_dir(folder)
        .env("PATH", path)
        .args(["-p", PROMPT, "--output-format", "stream-json", "--verbose", "--model", "haiku", "--max-budget-usd", "0.01"])
        .args(["--tools", "", "--strict-mcp-config", "--setting-sources", "project", "--no-session-persistence"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_group(&mut command);
    // Like a task: the key goes in the environment only, never on the command line, and what could outrank it is removed.
    // Without a key the environment stays as a task on the Claude sign-in has it, and the init line says what was used.
    if let Some(key) = key {
        command.env(KEY_VARIABLE, key.expose());
        // The test starts no tools, but if the CLI starts a hook or an MCP server it keeps the key out of their environment.
        command.env("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB", "1");
        for name in ENV_THAT_OUTRANKS_THE_KEY {
            command.env_remove(name);
        }
    }
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
    command
}

fn lines_of(pipe: impl Read + Send + 'static) -> Receiver<String> {
    let (sender, lines) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    lines
}

fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// The last lines stderr wrote, waiting a moment for it to close.
fn last_lines(stderr: &Receiver<String>) -> Vec<String> {
    let deadline = Instant::now() + STDERR_GRACE;
    let mut tail = VecDeque::new();
    while let Ok(line) = stderr.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        tail.push_back(line);
        if tail.len() > STDERR_LINES_KEPT {
            tail.pop_front();
        }
    }
    tail.into()
}

/// Whether a failed result says the sign-in itself was turned away, rather than that the request never got through.
fn is_refusal(events: &[Value], said: &str) -> bool {
    let said = said.to_lowercase();
    REFUSALS.iter().any(|words| said.contains(words)) || events.iter().any(|event| event.get("error").and_then(Value::as_str) == Some("authentication_failed"))
}

/// Reads the test from what the process printed. The key was accepted when the result is no error, or when it ran out of
/// its cent, which Anthropic only charges to a key it took. A key test passes only when Claude Code says it used that key.
/// A failed result is a refusal only when it says the sign-in was turned away. Everything else never reached that question.
fn report_of(stdout: &[String], stderr: &[String], ending: &Ending, with_key: bool) -> TestReport {
    let events: Vec<Value> = stdout.iter().filter_map(|line| serde_json::from_str(line).ok()).collect();
    let text = |event: &Value, field: &str| event.get(field).and_then(Value::as_str).unwrap_or_default().to_owned();
    let source = events.iter().find(|event| text(event, "type") == "system" && text(event, "subtype") == "init").map(|init| text(init, "apiKeySource"));
    let Some(result) = events.iter().rev().find(|event| text(event, "type") == "result") else {
        let said = if stderr.is_empty() { ending.words() } else { stderr.join("\n") };
        return TestReport { outcome: Outcome::Unreached, cost_usd: 0.0, said, source };
    };
    let cost_usd = result.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0);
    let subtype = text(result, "subtype");
    let is_error = result.get("is_error").and_then(Value::as_bool).unwrap_or(false) || subtype.starts_with("error");
    let accepted = !is_error || subtype == "error_max_budget_usd";
    let said = text(result, "result");
    // A key test where Claude Code named another sign-in never tried the key, whatever the result. With no init line,
    // only a pass needs that proof: a refusal still says something was turned away.
    let other_sign_in = source.as_deref().is_some_and(|source| source != KEY_VARIABLE);
    if with_key && (other_sign_in || accepted && source.is_none()) {
        return TestReport { outcome: Outcome::Unreached, cost_usd, said: KEY_NOT_USED.to_owned(), source };
    }
    let outcome = if accepted {
        Outcome::Passed
    } else if is_refusal(&events, &said) {
        Outcome::Refused
    } else {
        Outcome::Unreached
    };
    let said = if accepted || !said.trim().is_empty() {
        said
    } else if !stderr.is_empty() {
        stderr.join("\n")
    } else {
        ending.words()
    };
    TestReport { outcome, cost_usd, said, source }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    fn init(source: &str) -> String {
        format!(r#"{{"type":"system","subtype":"init","session_id":"s","model":"claude-haiku-4-5","apiKeySource":"{source}"}}"#)
    }

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|line| (*line).to_owned()).collect()
    }

    fn exited() -> Ending {
        Ending::Exited("code 1".to_owned())
    }

    #[test]
    fn a_ready_answer_passes_with_its_cost() {
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ready","total_cost_usd":0.0031}"#;
        let report = report_of(&[init("none"), result.to_owned()], &[], &Ending::Exited("code 0".to_owned()), false);
        assert_eq!(report, TestReport { outcome: Outcome::Passed, cost_usd: 0.0031, said: "ready".to_owned(), source: Some("none".to_owned()) });
        let report = report_of(&[init(KEY_VARIABLE), result.to_owned()], &[], &exited(), true);
        assert_eq!(report.outcome, Outcome::Passed, "a key test passes when Claude Code used the key");
    }

    #[test]
    fn an_invalid_key_is_refused_with_what_claude_code_said() {
        let result = r#"{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Fix external API key","total_cost_usd":0}"#;
        let report = report_of(&[init(KEY_VARIABLE), result.to_owned()], &lines(&["noise"]), &exited(), true);
        assert_eq!(report.outcome, Outcome::Refused);
        assert_eq!(report.said, "Invalid API key · Fix external API key");
    }

    #[test]
    fn an_authentication_error_anywhere_in_the_stream_is_a_refusal() {
        let assistant = r#"{"type":"assistant","error":"authentication_failed","message":{"content":[{"type":"text","text":"Bad key"}]}}"#;
        let result = r#"{"type":"result","subtype":"success","is_error":true,"result":"Bad key","total_cost_usd":0}"#;
        assert_eq!(report_of(&lines(&[assistant, result]), &[], &exited(), true).outcome, Outcome::Refused);
        let result = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login","total_cost_usd":0}"#;
        assert_eq!(report_of(&lines(&[result]), &[], &exited(), false).outcome, Outcome::Refused, "no Claude sign-in");
    }

    fn failed_result(said: &str) -> String {
        serde_json::json!({"type": "result", "subtype": "success", "is_error": true, "result": said, "total_cost_usd": 0}).to_string()
    }

    #[test]
    fn each_refusal_phrase_is_a_refusal_on_its_own() {
        for said in [
            "Invalid API key · Fix external API key",
            "API Error: 401 invalid x-api-key",
            r#"API Error: 401 {"type":"error","error":{"type":"authentication_error"}}"#,
            "Not logged in",
            "Your session expired. Please run /login",
        ] {
            let report = report_of(&[init("none"), failed_result(said)], &[], &exited(), false);
            assert_eq!(report.outcome, Outcome::Refused, "{said}");
        }
    }

    #[test]
    fn a_proxy_that_says_authentication_failed_is_no_refusal() {
        let report = report_of(&[failed_result("API Error: 407 Proxy authentication failed")], &[], &exited(), false);
        assert_eq!(report.outcome, Outcome::Unreached);
    }

    #[test]
    fn a_refusal_of_another_sign_in_in_a_key_test_is_unreached() {
        for source in ["none", "apiKeyHelper"] {
            let report = report_of(&[init(source), failed_result("Invalid API key · Fix external API key")], &[], &exited(), true);
            assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, KEY_NOT_USED), "{source}");
        }
        let report = report_of(&[failed_result("Invalid API key")], &[], &exited(), true);
        assert_eq!(report.outcome, Outcome::Refused, "with no init line a refusal still counts");
    }

    #[test]
    fn a_failed_result_that_names_no_sign_in_problem_is_unreached() {
        let result = r#"{"type":"result","subtype":"success","is_error":true,"result":"API Error: Connection error.","total_cost_usd":0}"#;
        let report = report_of(&[init(KEY_VARIABLE), result.to_owned()], &[], &exited(), true);
        assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, "API Error: Connection error."));
    }

    #[test]
    fn running_out_of_the_cent_still_passes() {
        let result = r#"{"type":"result","subtype":"error_max_budget_usd","is_error":true,"total_cost_usd":0.012}"#;
        let report = report_of(&[init(KEY_VARIABLE), result.to_owned()], &[], &exited(), true);
        assert_eq!(report.outcome, Outcome::Passed, "the key was accepted and charged");
        assert_eq!(report.cost_usd, 0.012);
    }

    #[test]
    fn a_key_test_is_unreached_when_claude_code_signed_in_another_way() {
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"ready","total_cost_usd":0.003}"#;
        for source in [init("none"), init("apiKeyHelper")] {
            let report = report_of(&[source, result.to_owned()], &[], &exited(), true);
            assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, KEY_NOT_USED));
            assert_eq!(report.cost_usd, 0.003);
        }
        let report = report_of(&[result.to_owned()], &[], &exited(), true);
        assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, KEY_NOT_USED), "no init line is no proof either");
    }

    #[test]
    fn output_that_is_no_json_gives_the_stderr_tail() {
        let report = report_of(&lines(&["Usage: claude [options]", "{not json"]), &lines(&["error: unknown option '--tools'", "see --help"]), &exited(), false);
        assert_eq!(
            report,
            TestReport { outcome: Outcome::Unreached, cost_usd: 0.0, said: "error: unknown option '--tools'\nsee --help".to_owned(), source: None }
        );
        let report = report_of(&lines(&["garbage"]), &[], &exited(), false);
        assert_eq!(report.said, "Claude Code exited with code 1.");
    }

    #[test]
    fn a_failed_result_without_text_falls_back_to_stderr_then_the_ending() {
        let result = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"total_cost_usd":0}"#;
        assert_eq!(report_of(&[result.to_owned()], &lines(&["boom"]), &exited(), false).said, "boom");
        let report = report_of(&[result.to_owned()], &[], &exited(), false);
        assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, "Claude Code exited with code 1."));
    }

    #[test]
    fn a_timeout_and_a_missing_cli_are_unreached() {
        let report = report_of(&[init("none")], &[], &Ending::TimedOut, false);
        assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, TIMED_OUT));
        assert_eq!(report.source.as_deref(), Some("none"));
        let report = report_of(&[], &[], &Ending::NotInstalled, true);
        assert_eq!((report.outcome, report.said.as_str()), (Outcome::Unreached, NOT_INSTALLED));
        assert_eq!(report_of(&[], &[], &Ending::NotStarted("denied".to_owned()), true).said, "Could not start Claude Code: denied");
    }

    fn args(command: &Command) -> Vec<&str> {
        command.get_args().map(|arg| arg.to_str().unwrap()).collect()
    }

    fn env<'a>(command: &'a Command, name: &str) -> Option<Option<&'a OsStr>> {
        command.get_envs().find(|(key, _)| *key == OsStr::new(name)).map(|(_, value)| value)
    }

    #[test]
    fn the_command_has_no_tools_a_cent_and_the_key_only_in_its_environment() {
        let folder = tempfile::tempdir().unwrap();
        let key = ApiKey::parse(FAKE).unwrap();
        let command = command_for(Path::new("/bin/claude"), folder.path(), "/bin", Some(&key));
        assert_eq!(
            args(&command),
            [
                "-p",
                PROMPT,
                "--output-format",
                "stream-json",
                "--verbose",
                "--model",
                "haiku",
                "--max-budget-usd",
                "0.01",
                "--tools",
                "",
                "--strict-mcp-config",
                "--setting-sources",
                "project",
                "--no-session-persistence"
            ]
        );
        assert!(!args(&command).iter().any(|arg| arg.contains("sk-ant") || arg.contains("R2kd")), "no key on the command line");
        assert_eq!(env(&command, KEY_VARIABLE), Some(Some(OsStr::new(FAKE))));
        assert_eq!(env(&command, "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"), Some(Some(OsStr::new("1"))), "as a task's");
        for name in ENV_THAT_OUTRANKS_THE_KEY {
            assert_eq!(env(&command, name), Some(None), "{name} could outrank the key");
        }
        assert_eq!(env(&command, "PATH"), Some(Some(OsStr::new("/bin"))));
        assert_eq!(command.get_current_dir(), Some(folder.path()));
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0, "the folder is empty");
    }

    #[test]
    fn the_sign_in_command_leaves_the_environment_as_a_task_has_it() {
        let folder = tempfile::tempdir().unwrap();
        let command = command_for(Path::new("/bin/claude"), folder.path(), "/bin", None);
        assert_eq!(env(&command, KEY_VARIABLE), None, "an inherited key stays, as it does for a task, and the init line names it");
        for name in ENV_THAT_OUTRANKS_THE_KEY {
            assert_eq!(env(&command, name), None, "{name} stays");
        }
        assert_eq!(command.get_envs().count(), 1, "only PATH is set");
    }

    #[cfg(unix)]
    #[test]
    fn a_test_past_its_time_is_killed_with_everything_it_started() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60 & echo $!; wait"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        own_group(&mut command);
        let started = Instant::now();
        let (out, _, ending) = run(command, Duration::from_millis(500));
        assert_eq!(ending, Ending::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
        let grandchild: libc::pid_t = out[0].trim().parse().unwrap();
        let alive = || {
            // SAFETY: signal 0 only checks that the process exists.
            unsafe { libc::kill(grandchild, 0) == 0 }
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while alive() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(), "the background sleep died too");
    }

    #[cfg(unix)]
    #[test]
    fn a_test_that_ends_in_time_gives_its_output_and_exit_code() {
        let mut command = Command::new("sh");
        command.args(["-c", "echo out; echo err >&2; exit 3"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        assert_eq!(run(command, TIMEOUT), (lines(&["out"]), lines(&["err"]), Ending::Exited("code 3".to_owned())));
    }

    #[test]
    fn the_idle_tester_never_answers() {
        let (sender, answers) = mpsc::channel();
        let tester = IdleTester::default();
        tester.start(None, Box::new(move |report| sender.send(report).unwrap()));
        assert_eq!(answers.try_recv(), Err(mpsc::TryRecvError::Empty));
    }
}
