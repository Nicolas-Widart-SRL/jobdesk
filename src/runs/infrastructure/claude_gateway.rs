use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runs::domain::{AgentEvent, AgentGateway, AgentSession, Decision, EventSink, Prompt, RunRequest};
use crate::runs::infrastructure::run_logs::RunLog;
use crate::runs::infrastructure::stream_json::{decision_response, interrupt_request, parse_line, user_message};
use crate::shared_kernel::host_path::{find_executable, resolved_path};
use crate::shared_kernel::process_tree::{kill_tree, own_group};
use crate::shared_kernel::{ApiKey, ENV_THAT_OUTRANKS_THE_KEY, KeyReader, SignIn, redacted};

const STDERR_LINES_KEPT: usize = 12;

#[cfg(windows)]
const STORE: &str = "Credential Manager";
#[cfg(not(windows))]
const STORE: &str = "Keychain";

/// The saved key is gone from the system store, so a task under an API key cannot start.
#[cfg(windows)]
const KEY_GONE: &str = "Your API key is no longer in the Credential Manager. Save it again in Settings › Providers, or pick Claude sign-in there.";
#[cfg(not(windows))]
const KEY_GONE: &str = "Your API key is no longer in the Keychain. Save it again in Settings › Providers, or pick Claude sign-in there.";

/// What every process of the gateway shares.
struct Setup {
    workspace: PathBuf,
    logs: PathBuf,
    keys: KeyReader,
}

pub struct ClaudeGateway {
    setup: Arc<Setup>,
}

impl ClaudeGateway {
    pub fn new(workspace: PathBuf, logs: PathBuf, keys: KeyReader) -> Self {
        Self { setup: Arc::new(Setup { workspace, logs, keys }) }
    }
}

impl AgentGateway for ClaudeGateway {
    fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
        let (outbox, inbox) = mpsc::channel();
        let (setup, request) = (Arc::clone(&self.setup), request.clone());
        let to_self = outbox.clone();
        let process = Arc::new(Mutex::new(Process::default()));
        let supervised = Arc::clone(&process);
        thread::spawn(move || supervise(&setup, &request, sink, &inbox, to_self, &supervised));
        Box::new(ClaudeSession { outbox, interrupts: 0, process })
    }
}

enum Outgoing {
    Message(String),
    Control(String),
    /// The interrupt request, noted in the log as it goes, since the CLI writes nothing for it.
    Stop(String),
    StreamEnded,
}

/// The process a session started, shared by the session and its supervising thread.
#[derive(Default)]
struct Process {
    /// Set while the process runs. Its process group has the same id.
    pid: Option<u32>,
    /// The session was dropped, so a process spawned after this point is killed at once.
    ended: bool,
}

/// Dropping the session kills the process and every command it started, before the drop returns.
/// The app can exit right after, so this cannot wait for the supervising thread.
struct ClaudeSession {
    outbox: Sender<Outgoing>,
    interrupts: u32,
    process: Arc<Mutex<Process>>,
}

impl Drop for ClaudeSession {
    fn drop(&mut self) {
        let Ok(mut process) = self.process.lock() else { return };
        process.ended = true;
        if let Some(pid) = process.pid.take() {
            kill_tree(pid);
        }
    }
}

impl AgentSession for ClaudeSession {
    fn send_message(&mut self, text: &str) {
        let _ = self.outbox.send(Outgoing::Message(text.to_owned()));
    }

    fn respond(&mut self, prompt: &Prompt, decision: &Decision) {
        let _ = self.outbox.send(Outgoing::Control(decision_response(prompt, decision)));
    }

    fn interrupt(&mut self) {
        self.interrupts += 1;
        let _ = self.outbox.send(Outgoing::Stop(interrupt_request(&format!("jobdesk-interrupt-{}", self.interrupts))));
    }
}

/// The key a task's process gets, read once as it starts. The Claude sign-in reads none and leaves the inherited environment as it is.
fn task_key(sign_in: SignIn, keys: &KeyReader) -> Result<Option<ApiKey>, String> {
    match sign_in {
        SignIn::ClaudeSignIn => Ok(None),
        SignIn::ApiKey => match keys() {
            Ok(Some(key)) => Ok(Some(key)),
            Ok(None) => Err(KEY_GONE.to_owned()),
            Err(why) => Err(format!("Jobdesk could not read your API key from the {STORE}: {why}")),
        },
    }
}

/// A key goes in the environment only, never on the command line.
fn command_for(claude: &Path, workspace: &Path, path: &str, request: &RunRequest, key: Option<&ApiKey>) -> Command {
    let mut command = Command::new(claude);
    command
        .current_dir(workspace)
        .env("PATH", path)
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--permission-prompt-tool", "stdio", "--setting-sources", "project"])
        .args(["--max-budget-usd", &format!("{:.2}", request.budget_usd)])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    own_group(&mut command);
    if let Some(key) = key {
        command.env("ANTHROPIC_API_KEY", key.expose());
        // The CLI strips the key from the environment of the tools, hooks and MCP servers it starts, so a command cannot print it.
        // Its own environment still holds the key.
        command.env("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB", "1");
        // Another inherited credential or provider switch could outrank the key.
        for name in ENV_THAT_OUTRANKS_THE_KEY {
            command.env_remove(name);
        }
    }
    if let Some(model) = &request.model {
        command.args(["--model", model]);
    }
    if let Some(session) = &request.resume_session {
        command.args(["--resume", session]);
    }
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
    command
}

/// Starts the process. A key that cannot be had ends it before any process exists, and never falls back to the Claude plan.
/// The key comes back with the process, kept only to mask it in what the process says on stderr.
fn spawn_claude(claude: &Path, workspace: &Path, path: &str, request: &RunRequest, keys: &KeyReader) -> Result<(Child, Option<ApiKey>), String> {
    let key = task_key(request.sign_in, keys)?;
    // The command holds the key only until it is spawned.
    let spawned = command_for(claude, workspace, path, request, key.as_ref()).spawn();
    spawned.map(|child| (child, key)).map_err(|error| format!("Could not start Claude: {error}"))
}

/// Why a process that did not succeed failed: the last lines of its stderr, with the key it was given masked, or else its exit status.
fn failure_of(status: &ExitStatus, tail: &[String], key: Option<&ApiKey>) -> String {
    let tail = tail.join("\n");
    if tail.is_empty() { format!("Claude exited with {status}") } else { redacted(&tail, key) }
}

fn supervise(setup: &Setup, request: &RunRequest, sink: EventSink, inbox: &Receiver<Outgoing>, to_self: Sender<Outgoing>, process: &Mutex<Process>) {
    let Setup { workspace, logs, keys } = setup;
    let sink = Arc::new(Mutex::new(sink));
    let emit = {
        let sink = Arc::clone(&sink);
        move |event| {
            if let Ok(sink) = sink.lock() {
                sink(event);
            }
        }
    };
    let path = resolved_path();
    let Some(claude) = find_executable(path, "claude") else {
        emit(AgentEvent::ProcessExited { failure: Some("Claude CLI not found. Install Claude Code, then run this again.".to_owned()) });
        return;
    };
    let (mut child, key) = match spawn_claude(&claude, workspace, path, request, keys) {
        Ok(spawned) => spawned,
        Err(failure) => {
            emit(AgentEvent::ProcessExited { failure: Some(failure) });
            return;
        }
    };
    if let Ok(mut process) = process.lock() {
        if process.ended {
            kill_tree(child.id());
        } else {
            process.pid = Some(child.id());
        }
    }
    let log = RunLog::open(logs, workspace, request).ok().map(|log| Arc::new(Mutex::new(log)));
    let stderr_tail = collect_stderr(&mut child);
    let mut stdin = child.stdin.take();
    read_stdout(&mut child, log.clone(), emit.clone(), to_self);
    write_line(&mut stdin, &user_message(&request.line));

    let note = |write: &dyn Fn(&mut RunLog)| {
        if let Some(log) = &log
            && let Ok(mut log) = log.lock()
        {
            write(&mut log);
        }
    };
    loop {
        match inbox.recv() {
            Ok(Outgoing::Message(text)) => {
                note(&|log| log.append_user(&text));
                write_line(&mut stdin, &user_message(&text));
            }
            Ok(Outgoing::Control(line)) => write_line(&mut stdin, &line),
            Ok(Outgoing::Stop(line)) => {
                note(&RunLog::note_stop);
                write_line(&mut stdin, &line);
            }
            Ok(Outgoing::StreamEnded) => break,
            // The dropped session already killed the process.
            Err(_) => break,
        }
    }
    drop(stdin);
    let status = child.wait();
    // The id is free for reuse from here, so the session must not kill it any more.
    if let Ok(mut process) = process.lock() {
        process.pid = None;
    }
    let failure = status.ok().filter(|status| !status.success()).map(|status| {
        let tail = stderr_tail.lock().map(|tail| tail.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
        failure_of(&status, &tail, key.as_ref())
    });
    drop(key);
    emit(AgentEvent::ProcessExited { failure });
}

fn write_line(stdin: &mut Option<ChildStdin>, line: &str) {
    if let Some(pipe) = stdin
        && writeln!(pipe, "{line}").and_then(|()| pipe.flush()).is_err()
    {
        *stdin = None;
    }
}

fn read_stdout(child: &mut Child, log: Option<Arc<Mutex<RunLog>>>, emit: impl Fn(AgentEvent) + Send + 'static, done: Sender<Outgoing>) {
    let Some(stdout) = child.stdout.take() else { return };
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(log) = &log
                && let Ok(mut log) = log.lock()
            {
                log.append(&line);
            }
            parse_line(&line).into_iter().for_each(&emit);
        }
        let _ = done.send(Outgoing::StreamEnded);
    });
}

fn collect_stderr(child: &mut Child) -> Arc<Mutex<VecDeque<String>>> {
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = child.stderr.take() {
        let tail = Arc::clone(&tail);
        thread::spawn(move || read_tail(stderr, &tail));
    }
    tail
}

fn read_tail(stderr: impl Read, tail: &Mutex<VecDeque<String>>) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        if let Ok(mut tail) = tail.lock() {
            tail.push_back(line);
            if tail.len() > STDERR_LINES_KEPT {
                tail.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use crate::settings::application::{ConnectionTester, Outcome, Preferences, TestDone, TestReport};
    use crate::settings::{memory_keychain, open_providers, open_settings};
    use crate::shared_kernel::KeyMask;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    fn request() -> RunRequest {
        RunRequest {
            run_id: "r".into(),
            line: "/rank".into(),
            model: Some("opus".into()),
            budget_usd: 2.0,
            started_at: String::new(),
            resume_session: Some("s1".into()),
            sign_in: SignIn::ApiKey,
        }
    }

    fn command(key: Option<&ApiKey>) -> Command {
        command_for(Path::new("/bin/claude"), Path::new("/w"), "/bin", &request(), key)
    }

    /// None when the command leaves the variable alone, Some(None) when it removes it.
    fn env_of(command: &Command, name: &str) -> Option<Option<String>> {
        command.get_envs().find(|(var, _)| *var == name).map(|(_, value)| value.map(|v| v.to_string_lossy().into_owned()))
    }

    fn api_key_in(command: &Command) -> Option<Option<String>> {
        env_of(command, "ANTHROPIC_API_KEY")
    }

    #[test]
    fn a_key_goes_in_the_environment_and_never_on_the_command_line() {
        let key = ApiKey::parse(FAKE).unwrap();
        let command = command(Some(&key));
        for arg in command.get_args() {
            let arg = arg.to_string_lossy();
            assert!(!arg.contains("FAKE") && !arg.contains("sk-ant"), "{arg}");
        }
        assert_eq!(api_key_in(&command), Some(Some(FAKE.to_owned())));
    }

    #[test]
    fn no_key_leaves_the_environment_alone() {
        let command = command(None);
        assert_eq!(command.get_envs().map(|(name, _)| name.to_string_lossy().into_owned()).collect::<Vec<_>>(), ["PATH"]);
    }

    #[test]
    fn a_key_keeps_the_tools_from_inheriting_it() {
        let key = ApiKey::parse(FAKE).unwrap();
        assert_eq!(env_of(&command(Some(&key)), "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"), Some(Some("1".to_owned())));
        assert_eq!(env_of(&command(None), "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB"), None);
    }

    #[test]
    fn a_key_clears_what_would_outrank_it() {
        let key = ApiKey::parse(FAKE).unwrap();
        let with = command(Some(&key));
        let named = ["ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY", "CLAUDE_CODE_USE_GATEWAY"];
        for name in named {
            assert!(ENV_THAT_OUTRANKS_THE_KEY.contains(&name), "{name} is on the list");
        }
        assert!(!ENV_THAT_OUTRANKS_THE_KEY.contains(&"ANTHROPIC_BASE_URL"), "the base URL stays");
        for name in ENV_THAT_OUTRANKS_THE_KEY.iter().copied() {
            assert_eq!(env_of(&with, name), Some(None), "{name} is removed");
            assert_eq!(env_of(&command(None), name), None, "{name} is left alone without a key");
        }
    }

    #[test]
    fn a_task_whose_key_cannot_be_had_spawns_nothing() {
        // A spawn attempt at this path would fail with "Could not start Claude", so the key error shows none was made.
        let nowhere = Path::new("/nonexistent/claude");
        let mut request = request();
        for (keys, expected) in [
            (Arc::new(|| Ok(None)) as KeyReader, KEY_GONE.to_owned()),
            (Arc::new(|| Err("access denied".to_owned())) as KeyReader, format!("Jobdesk could not read your API key from the {STORE}: access denied")),
        ] {
            let failure = spawn_claude(nowhere, Path::new("/w"), "/bin", &request, &keys).unwrap_err();
            assert_eq!(failure, expected);
        }
        request.sign_in = SignIn::ClaudeSignIn;
        let keys: KeyReader = Arc::new(|| panic!("the reader must not be called"));
        let failure = spawn_claude(nowhere, Path::new("/w"), "/bin", &request, &keys).unwrap_err();
        assert!(failure.starts_with("Could not start Claude"), "the Claude sign-in goes on to the spawn");
    }

    #[test]
    fn the_claude_sign_in_never_reads_the_key() {
        let keys: KeyReader = Arc::new(|| panic!("the reader must not be called"));
        assert!(matches!(task_key(SignIn::ClaudeSignIn, &keys), Ok(None)));
    }

    #[test]
    fn an_api_key_run_gets_the_saved_key() {
        let keys: KeyReader = Arc::new(|| Ok(ApiKey::parse(FAKE)));
        assert_eq!(task_key(SignIn::ApiKey, &keys).unwrap().unwrap().expose(), FAKE);
    }

    #[test]
    fn a_key_gone_from_the_store_stops_the_task() {
        let keys: KeyReader = Arc::new(|| Ok(None));
        assert_eq!(task_key(SignIn::ApiKey, &keys).unwrap_err(), KEY_GONE);
        assert!(KEY_GONE.contains("Settings › Providers"));
    }

    #[test]
    fn an_unreadable_store_says_why_without_a_key() {
        let keys: KeyReader = Arc::new(|| Err("access denied".to_owned()));
        let text = task_key(SignIn::ApiKey, &keys).unwrap_err();
        assert!(text.contains("access denied") && text.starts_with("Jobdesk could not read your API key from the "));
        assert!(!text.contains("sk-ant"));
    }

    #[cfg(unix)]
    fn exit_one() -> ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(256)
    }

    #[cfg(windows)]
    fn exit_one() -> ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(1)
    }

    #[test]
    fn a_failure_masks_the_key_the_process_said() {
        let key = ApiKey::parse(FAKE).unwrap();
        let tail = [format!("error: invalid x-api-key {FAKE}"), format!("key was {FAKE}")];
        assert_eq!(failure_of(&exit_one(), &tail, Some(&key)), "error: invalid x-api-key sk-ant-••••••••R2kd\nkey was sk-ant-••••••••R2kd");
        assert_eq!(failure_of(&exit_one(), &tail[..1], None), format!("error: invalid x-api-key {FAKE}"), "no key was passed, so none is known");
        assert!(failure_of(&exit_one(), &[], Some(&key)).starts_with("Claude exited with "));
    }

    /// Answers every test at once with a pass, and says the key back, so the report has to be masked.
    struct PassingTester;

    impl ConnectionTester for PassingTester {
        fn start(&self, key: Option<ApiKey>, done: TestDone) {
            let said = key.map(|key| format!("ready {}", key.expose())).unwrap_or_default();
            done(TestReport { outcome: Outcome::Passed, cost_usd: 0.004, said, source: Some("ANTHROPIC_API_KEY".to_owned()) });
        }
    }

    /// Whether `text` holds the key, or any `sk-ant-` text but its mask: the dots and at most the last four.
    fn leaks(text: &str) -> bool {
        let mask = KeyMask::of(&ApiKey::parse(FAKE).unwrap()).shown();
        text.contains(FAKE)
            || text.contains("api03")
            || text.match_indices("sk-ant-").any(|(at, _)| {
                let Some(rest) = text[at..].strip_prefix(&mask) else { return true };
                rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).count() > 4
            })
    }

    fn files_in(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() { files_in(&path, found) } else { found.push(path) }
        }
    }

    /// A key typed on Providers, tested, then used by a task, leaves no trace in the data folder, the command line or the failure.
    #[cfg(unix)]
    #[test]
    fn a_saved_key_reaches_no_file_argument_or_failure() {
        use std::os::unix::fs::PermissionsExt;

        assert!(!leaks("key sk-ant-••••••••R2kd.") && leaks("sk-ant-••••••••R2kdX") && leaks("sk-ant-api") && leaks(FAKE), "the scan itself");

        let data = tempfile::tempdir().unwrap();
        let mut settings = open_settings(data.path(), Preferences::default());
        let mut providers = open_providers(memory_keychain(None), Box::new(PassingTester));
        providers.save_key(FAKE, &mut settings, false).unwrap();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 3, 14).unwrap().and_hms_opt(9, 30, 0).unwrap();
        assert!(providers.poll(&mut settings, now));
        let mut request = request();
        request.sign_in = providers.task_sign_in(settings.preferences());
        assert_eq!(request.sign_in, SignIn::ApiKey, "the pass put tasks on the key");

        let reader = providers.key_reader();
        let key = task_key(request.sign_in, &reader).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let command = command_for(Path::new("/bin/claude"), workspace.path(), "/bin", &request, key.as_ref());
        for arg in command.get_args() {
            assert!(!leaks(&arg.to_string_lossy()), "{arg:?}");
        }
        drop(key);

        // A stand-in for claude that gets the same command: it answers, says its key on stderr, and fails.
        let bin = tempfile::tempdir().unwrap();
        let claude = bin.path().join("claude");
        let script = "#!/bin/sh\nread line\necho '{\"type\":\"system\",\"subtype\":\"init\",\"apiKeySource\":\"ANTHROPIC_API_KEY\"}'\necho \"error: bad key $ANTHROPIC_API_KEY\" >&2\nexit 1\n";
        std::fs::write(&claude, script).unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (mut child, key) = spawn_claude(&claude, workspace.path(), "/bin:/usr/bin", &request, &reader).unwrap();
        let mut log = RunLog::open(&data.path().join("runs"), workspace.path(), &request).unwrap();
        let mut stdin = child.stdin.take();
        write_line(&mut stdin, &user_message(&request.line));
        drop(stdin);
        for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
            log.append(&line);
        }
        log.append_user("Thanks");
        log.note_stop();
        drop(log);
        // Its stderr is a line, far less than a pipe holds, so it can be read after stdout ended.
        let tail = Mutex::new(VecDeque::new());
        read_tail(child.stderr.take().unwrap(), &tail);
        let status = child.wait().unwrap();
        let tail: Vec<String> = tail.into_inner().unwrap().into();
        assert!(tail.iter().any(|line| line.contains(FAKE)), "the stand-in got the key");
        let failure = failure_of(&status, &tail, key.as_ref());
        assert_eq!(failure, "error: bad key sk-ant-••••••••R2kd");

        let mut files = Vec::new();
        files_in(data.path(), &mut files);
        let names: Vec<_> = files.iter().map(|path| path.strip_prefix(data.path()).unwrap().to_string_lossy().into_owned()).collect();
        assert!(names.contains(&"settings.json".to_owned()) && names.contains(&"runs/r.jsonl".to_owned()), "{names:?}");
        for path in &files {
            let text = String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned();
            assert!(!leaks(&text), "{} holds the key", path.display());
        }
    }
}
