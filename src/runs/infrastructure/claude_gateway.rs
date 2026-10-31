use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::runs::domain::{AgentEvent, AgentGateway, AgentSession, Decision, EventSink, Prompt, RunRequest};
use crate::runs::infrastructure::host_path::{find_claude, resolved_path};
use crate::runs::infrastructure::run_logs::RunLog;
use crate::runs::infrastructure::stream_json::{decision_response, interrupt_request, parse_line, user_message};

const STDERR_LINES_KEPT: usize = 12;

pub struct ClaudeGateway {
    workspace: PathBuf,
    logs: PathBuf,
}

impl ClaudeGateway {
    pub fn new(workspace: PathBuf, logs: PathBuf) -> Self {
        Self { workspace, logs }
    }
}

impl AgentGateway for ClaudeGateway {
    fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
        let (outbox, inbox) = mpsc::channel();
        let (workspace, logs, request) = (self.workspace.clone(), self.logs.clone(), request.clone());
        let to_self = outbox.clone();
        thread::spawn(move || supervise(&workspace, &logs, &request, sink, &inbox, to_self));
        Box::new(ClaudeSession { outbox, interrupts: 0 })
    }
}

enum Outgoing {
    Message(String),
    Control(String),
    StreamEnded,
}

/// Dropping the session closes the outbox, which ends the process.
struct ClaudeSession {
    outbox: Sender<Outgoing>,
    interrupts: u32,
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
        let _ = self.outbox.send(Outgoing::Control(interrupt_request(&format!("jobdesk-interrupt-{}", self.interrupts))));
    }
}

fn command_for(claude: &PathBuf, workspace: &PathBuf, path: &str, request: &RunRequest) -> Command {
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

fn supervise(workspace: &PathBuf, logs: &PathBuf, request: &RunRequest, sink: EventSink, inbox: &Receiver<Outgoing>, to_self: Sender<Outgoing>) {
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
    let Some(claude) = find_claude(&path) else {
        emit(AgentEvent::ProcessExited { failure: Some("Claude CLI not found. Install Claude Code, then run this again.".to_owned()) });
        return;
    };
    let mut child = match command_for(&claude, workspace, &path, request).spawn() {
        Ok(child) => child,
        Err(error) => {
            emit(AgentEvent::ProcessExited { failure: Some(format!("Could not start Claude: {error}")) });
            return;
        }
    };
    let log = RunLog::open(logs, workspace, request).ok().map(|log| Arc::new(Mutex::new(log)));
    let stderr_tail = collect_stderr(&mut child);
    let mut stdin = child.stdin.take();
    read_stdout(&mut child, log.clone(), emit.clone(), to_self);
    write_line(&mut stdin, &user_message(&request.line));

    loop {
        match inbox.recv() {
            Ok(Outgoing::Message(text)) => {
                if let Some(log) = &log {
                    if let Ok(mut log) = log.lock() {
                        log.append_user(&text);
                    }
                }
                write_line(&mut stdin, &user_message(&text));
            }
            Ok(Outgoing::Control(line)) => write_line(&mut stdin, &line),
            Ok(Outgoing::StreamEnded) => break,
            Err(_) => {
                let _ = child.kill();
                break;
            }
        }
    }
    drop(stdin);
    let status = child.wait();
    let failure = status.ok().filter(|status| !status.success()).map(|status| {
        let tail = stderr_tail.lock().map(|tail| tail.iter().cloned().collect::<Vec<_>>().join("\n")).unwrap_or_default();
        if tail.is_empty() { format!("Claude exited with {status}") } else { tail }
    });
    emit(AgentEvent::ProcessExited { failure });
}

fn write_line(stdin: &mut Option<ChildStdin>, line: &str) {
    if let Some(pipe) = stdin {
        if writeln!(pipe, "{line}").and_then(|()| pipe.flush()).is_err() {
            *stdin = None;
        }
    }
}

fn read_stdout(child: &mut Child, log: Option<Arc<Mutex<RunLog>>>, emit: impl Fn(AgentEvent) + Send + 'static, done: Sender<Outgoing>) {
    let Some(stdout) = child.stdout.take() else { return };
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(log) = &log {
                if let Ok(mut log) = log.lock() {
                    log.append(&line);
                }
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
