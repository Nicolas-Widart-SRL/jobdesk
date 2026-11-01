use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

use chrono::NaiveDateTime;

use crate::runs::application::turn::wants_reply;
use crate::runs::domain::{AgentEvent, AgentGateway, AgentSession, Decision, EventSink, PromptKind, Run, RunHistory, RunRequest};

const HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StartOptions {
    pub model: Option<String>,
    pub budget_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    /// Two commands at once could overwrite each other's files.
    AnotherRunIsActive,
}

/// Owns every run of a workspace, newest first.
pub struct RunDesk {
    gateway: Box<dyn AgentGateway>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    runs: Vec<Run>,
    sessions: HashMap<String, Box<dyn AgentSession>>,
    events: Receiver<(String, AgentEvent)>,
    event_sender: Sender<(String, AgentEvent)>,
    started: u32,
    /// Off by default: every permission is allowed and noted in the activity. Questions always wait.
    asks_permission: bool,
}

impl RunDesk {
    pub fn new(gateway: Box<dyn AgentGateway>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (event_sender, events) = mpsc::channel();
        Self { gateway, repaint, runs: Vec::new(), sessions: HashMap::new(), events, event_sender, started: 0, asks_permission: false }
    }

    /// Sends each permission to the user instead of allowing it.
    pub fn ask_permission(&mut self) {
        self.asks_permission = true;
    }

    pub fn restore(&mut self, history: &dyn RunHistory) {
        self.runs = history.recent(HISTORY_LIMIT);
    }

    pub fn runs(&self) -> &[Run] {
        &self.runs
    }

    pub fn run(&self, id: &str) -> Option<&Run> {
        self.runs.iter().find(|run| run.id == id)
    }

    fn run_mut(&mut self, id: &str) -> Option<&mut Run> {
        self.runs.iter_mut().find(|run| run.id == id)
    }

    /// How the most recent run authenticated, as the CLI reported it.
    pub fn latest_auth_source(&self) -> Option<&str> {
        self.runs.iter().find_map(|run| run.api_key_source.as_deref())
    }

    pub fn is_busy(&self) -> bool {
        self.runs.iter().any(Run::is_busy)
    }

    pub fn has_live_process(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Runs where Claude waits on the user, as `wants_reply` says.
    pub fn waiting_count(&self) -> usize {
        self.runs.iter().filter(|run| wants_reply(run)).count()
    }

    /// Applies what arrived from the agent processes. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok((id, event)) = self.events.try_recv() {
            let exited = matches!(event, AgentEvent::ProcessExited { .. });
            let permission = match &event {
                AgentEvent::PromptRaised(prompt) if !self.asks_permission && matches!(prompt.kind, PromptKind::Permission { .. }) => Some(prompt.request_id.clone()),
                _ => None,
            };
            if let Some(run) = self.run_mut(&id) {
                run.apply(event);
            }
            if let Some(request_id) = permission
                && let Some(prompt) = self.run_mut(&id).and_then(|run| run.allow_automatically(&request_id))
                && let Some(session) = self.sessions.get_mut(&id)
            {
                session.respond(&prompt, &Decision::Allow { remember: false });
            }
            if exited {
                self.sessions.remove(&id);
            }
        }
    }

    fn sink_for(&self, id: &str) -> EventSink {
        let (id, sender, repaint) = (id.to_owned(), self.event_sender.clone(), Arc::clone(&self.repaint));
        Box::new(move |event| {
            let _ = sender.send((id.clone(), event));
            repaint();
        })
    }

    pub fn start(&mut self, line: &str, options: &StartOptions, now: NaiveDateTime) -> Result<String, StartError> {
        if self.is_busy() {
            return Err(StartError::AnotherRunIsActive);
        }
        self.started += 1;
        let request = RunRequest {
            run_id: format!("{}-{}", now.format("%Y%m%d-%H%M%S"), self.started),
            line: line.to_owned(),
            model: options.model.clone(),
            budget_usd: options.budget_usd,
            started_at: now.format("%Y-%m-%d %H:%M").to_string(),
            resume_session: None,
        };
        let session = self.gateway.start(&request, self.sink_for(&request.run_id));
        self.sessions.insert(request.run_id.clone(), session);
        self.runs.insert(0, Run::new(&request));
        Ok(request.run_id)
    }

    /// A finished run continues in its Claude session through a new process.
    pub fn reply(&mut self, id: &str, text: &str) -> Result<(), StartError> {
        let live = self.sessions.contains_key(id);
        if !live && self.is_busy() {
            return Err(StartError::AnotherRunIsActive);
        }
        let Some(run) = self.run_mut(id) else { return Ok(()) };
        run.note_reply(text);
        let request = RunRequest {
            run_id: run.id.clone(),
            line: text.to_owned(),
            model: run.requested_model.clone(),
            budget_usd: run.budget_usd,
            started_at: run.started_at.clone(),
            resume_session: run.session_id.clone(),
        };
        if live {
            if let Some(session) = self.sessions.get_mut(id) {
                session.send_message(text);
            }
        } else {
            let session = self.gateway.start(&request, self.sink_for(id));
            self.sessions.insert(id.to_owned(), session);
        }
        Ok(())
    }

    pub fn decide(&mut self, id: &str, request_id: &str, decision: Decision) {
        let Some(prompt) = self.run_mut(id).and_then(|run| run.resolve_prompt(request_id, &decision)) else { return };
        if let Some(session) = self.sessions.get_mut(id) {
            session.respond(&prompt, &decision);
        }
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(run) = self.run_mut(id) {
            run.note_stop_requested();
        }
        if let Some(session) = self.sessions.get_mut(id) {
            session.interrupt();
        }
    }

    /// Ends the process. The conversation stays in history and can be resumed.
    pub fn close(&mut self, id: &str) {
        self.sessions.remove(id);
        if let Some(run) = self.run_mut(id) {
            run.apply(AgentEvent::ProcessExited { failure: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::runs::domain::{Prompt, PromptKind, RunStatus};

    #[derive(Default)]
    struct Script {
        requests: Vec<RunRequest>,
        sent: Vec<String>,
        interrupted: bool,
    }

    struct FakeGateway(Rc<RefCell<Script>>);
    struct FakeSession(Rc<RefCell<Script>>);

    impl AgentGateway for FakeGateway {
        fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession> {
            self.0.borrow_mut().requests.push(request.clone());
            sink(AgentEvent::SessionStarted { session_id: "s1".into(), model: "m".into(), api_key_source: "none".into() });
            Box::new(FakeSession(Rc::clone(&self.0)))
        }
    }

    impl AgentSession for FakeSession {
        fn send_message(&mut self, text: &str) {
            self.0.borrow_mut().sent.push(text.to_owned());
        }
        fn respond(&mut self, _: &Prompt, decision: &Decision) {
            self.0.borrow_mut().sent.push(format!("{decision:?}"));
        }
        fn interrupt(&mut self) {
            self.0.borrow_mut().interrupted = true;
        }
    }

    fn desk() -> (RunDesk, Rc<RefCell<Script>>) {
        let script = Rc::new(RefCell::new(Script::default()));
        (RunDesk::new(Box::new(FakeGateway(Rc::clone(&script))), Arc::new(|| {})), script)
    }

    fn options() -> StartOptions {
        StartOptions { model: None, budget_usd: 1.0 }
    }

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    #[test]
    fn a_run_keeps_the_time_its_caller_gave() {
        let (mut desk, _) = desk();
        let id = desk.start("/rank", &options(), now()).unwrap();
        assert_eq!(desk.run(&id).unwrap().started_at, "2026-10-31 09:30");
    }

    #[test]
    fn only_one_run_works_at_a_time() {
        let (mut desk, _) = desk();
        desk.start("/rank", &options(), now()).unwrap();
        desk.poll();
        assert_eq!(desk.start("/scrape", &options(), now()), Err(StartError::AnotherRunIsActive));
    }

    #[test]
    fn a_stop_point_frees_the_desk_and_takes_a_reply_on_the_same_process() {
        let (mut desk, script) = desk();
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        assert!(!desk.is_busy());

        desk.reply(&id, "yes").unwrap();

        assert_eq!(script.borrow().requests.len(), 1);
        assert_eq!(script.borrow().sent, ["yes"]);
        assert!(desk.is_busy());
    }

    #[test]
    fn a_stop_point_counts_as_waiting() {
        let (mut desk, _) = desk();
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        assert_eq!(desk.waiting_count(), 1);

        desk.reply(&id, "yes").unwrap();
        assert_eq!(desk.waiting_count(), 0);

        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.2, duration_ms: 1 });
        desk.sink_for(&id)(AgentEvent::BackgroundTasksChanged { count: 1 });
        desk.poll();
        assert_eq!(desk.waiting_count(), 0, "background agents are still working");
    }

    #[test]
    fn reply_after_exit_resumes_the_session() {
        let (mut desk, script) = desk();
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::ProcessExited { failure: None });
        desk.poll();

        desk.reply(&id, "continue").unwrap();

        let requests = &script.borrow().requests;
        assert_eq!(requests[1].resume_session.as_deref(), Some("s1"));
        assert_eq!(requests[1].line, "continue");
    }

    #[test]
    fn decisions_reach_the_session_once() {
        let (mut desk, script) = desk();
        let id = desk.start("/rank", &options(), now()).unwrap();
        let prompt = Prompt { request_id: "r".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None };
        desk.sink_for(&id)(AgentEvent::PromptRaised(prompt));
        desk.poll();
        assert_eq!(desk.waiting_count(), 1);

        desk.decide(&id, "r", Decision::Deny);
        desk.decide(&id, "r", Decision::Deny);

        assert_eq!(script.borrow().sent.len(), 1);
    }

    #[test]
    fn permissions_are_allowed_and_questions_wait() {
        let (mut desk, script) = desk();
        let id = desk.start("/rank", &options(), now()).unwrap();
        let permission = PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: false };
        let prompt = |request_id: &str, kind| Prompt { request_id: request_id.into(), kind, input_json: "{}".into(), suggestions_json: None };
        desk.sink_for(&id)(AgentEvent::PromptRaised(prompt("p", permission)));
        desk.sink_for(&id)(AgentEvent::PromptRaised(prompt("q", PromptKind::Questions(vec![]))));
        desk.poll();
        assert_eq!(script.borrow().sent, ["Allow { remember: false }"]);
        assert_eq!(desk.run(&id).unwrap().current_prompt().unwrap().request_id, "q");
    }

    #[test]
    fn stop_interrupts_and_clears_prompts() {
        let (mut desk, script) = desk();
        let id = desk.start("/rank", &options(), now()).unwrap();
        desk.poll();
        desk.stop(&id);
        assert!(script.borrow().interrupted);
        assert_eq!(desk.run(&id).unwrap().status, RunStatus::Stopping);
    }
}
