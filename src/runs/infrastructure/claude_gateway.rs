use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runs::domain::{AgentEvent, AgentGateway, AgentSession, Decision, EventSink, Prompt, RunRequest};
use crate::runs::infrastructure::run_logs::RunLog;
use crate::runs::infrastructure::stream_json::{decision_response, interrupt_request, parse_line, user_message};
use crate::shared_kernel::host_path::{find_executable, resolved_path};
use crate::shared_kernel::{ApiKey, KeyReader, SignIn};

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

/// Variables from the shell that started Jobdesk that would make the CLI sign in some other way than the saved key.
const OUTRANKING: [&str; 3] = ["ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"];

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
    // Its own process group, so one kill reaches the tools it runs too.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    if let Some(key) = key {
        command.env("ANTHROPIC_API_KEY", key.expose());
        // The CLI strips the key from the environment of the tools, hooks and servers it starts, so a command cannot print it.
        command.env("CLAUDE_CODE_SUBPROCESS_ENV_SCRUB", "1");
        // Another inherited credential or provider switch could outrank the key.
        for name in OUTRANKING {
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
fn spawn_claude(claude: &Path, workspace: &Path, path: &str, request: &RunRequest, keys: &KeyReader) -> Result<Child, String> {
    let key = task_key(request.sign_in, keys)?;
    // The command holds the key only until it is spawned, and the key goes with it.
    let spawned = command_for(claude, workspace, path, request, key.as_ref()).spawn();
    drop(key);
    spawned.map_err(|error| format!("Could not start Claude: {error}"))
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
    let mut child = match spawn_claude(&claude, workspace, path, request, keys) {
        Ok(child) => child,
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
        let tail = stderr_tail.lock().map(|tail| tail.iter().cloned().collect::<Vec<_>>().join("\n")).unwrap_or_default();
        if tail.is_empty() { format!("Claude exited with {status}") } else { tail }
    });
    emit(AgentEvent::ProcessExited { failure });
}

#[cfg(unix)]
fn kill_tree(pid: u32) {
    let Ok(group) = libc::pid_t::try_from(pid) else { return };
    // SAFETY: kill only sends a signal. A negative id names the process group the child leads.
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    let mut command = Command::new("taskkill");
    command.args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // CREATE_NO_WINDOW
    let _ = command.status();
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
#[cfg(unix)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn alive(pid: libc::pid_t) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid, 0) == 0 }
    }

    /// A tool Claude started, here a background `sleep`, dies with Claude.
    #[test]
    fn killing_the_tree_reaches_grandchildren() {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 60 & echo $!; wait"]).stdout(Stdio::piped());
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn().unwrap();
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let grandchild: libc::pid_t = line.trim().parse().unwrap();

        kill_tree(child.id());
        child.wait().unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while alive(grandchild) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(grandchild));
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

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
        for name in ["ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX"] {
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
}
