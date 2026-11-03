use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use chrono::NaiveDateTime;

use crate::runs::application::turn::{clock_runs, wants_reply};
use crate::runs::domain::{AgentEvent, AgentGateway, AgentSession, Decision, EventSink, PromptKind, Run, RunHistory, RunRequest};
use crate::shared_kernel::SignIn;

/// Where the desk reads the time, so a test can script it.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

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
    /// Settings' Test connection runs Claude Code, and one Claude process runs at a time.
    TestRunning,
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
    /// How the next process signs in. A run already working keeps the method it started with.
    sign_in: SignIn,
    /// Settings' Test connection is running, so no new process starts.
    testing: bool,
    clock: Clock,
}

impl RunDesk {
    pub fn new(gateway: Box<dyn AgentGateway>, repaint: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (event_sender, events) = mpsc::channel();
        Self {
            gateway,
            repaint,
            runs: Vec::new(),
            sessions: HashMap::new(),
            events,
            event_sender,
            started: 0,
            asks_permission: false,
            sign_in: SignIn::ClaudeSignIn,
            testing: false,
            clock: Arc::new(Instant::now),
        }
    }

    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    pub fn now(&self) -> Instant {
        (self.clock)()
    }

    /// On, each permission goes to the user instead of being allowed. It holds for every prompt from the next poll on.
    pub fn set_asks_permission(&mut self, asks: bool) {
        self.asks_permission = asks;
    }

    pub fn asks_permission(&self) -> bool {
        self.asks_permission
    }

    /// Holds for every process started from now on, not for one already running.
    pub fn set_sign_in(&mut self, sign_in: SignIn) {
        self.sign_in = sign_in;
    }

    pub fn sign_in(&self) -> SignIn {
        self.sign_in
    }

    /// While a test runs, no task process starts: no new run, and no reply that needs a new process.
    pub fn set_testing(&mut self, testing: bool) {
        self.testing = testing;
    }

    /// Whether a reply to `id` would wait for Test connection: it needs a new process, and a test runs.
    pub fn reply_waits_for_test(&self, id: &str) -> bool {
        self.testing && !self.sessions.contains_key(id)
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

    /// The newest run whose process reported how it signed in.
    pub fn latest_signed_in(&self) -> Option<&Run> {
        self.runs.iter().find(|run| run.api_key_source.is_some())
    }

    pub fn is_busy(&self) -> bool {
        self.runs.iter().any(Run::is_busy)
    }

    pub fn has_live_process(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Whether the run's process is still up, so its session can be ended.
    pub fn has_session(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    /// Runs where Claude waits on the user, as `wants_reply` says.
    pub fn waiting_count(&self) -> usize {
        self.runs.iter().filter(|run| wants_reply(run)).count()
    }

    /// Applies what arrived from the agent processes. Call once per frame.
    pub fn poll(&mut self) {
        while let Ok((id, event)) = self.events.try_recv() {
            let now = self.now();
            let exited = matches!(event, AgentEvent::ProcessExited { .. });
            let permission = match &event {
                AgentEvent::PromptRaised(prompt) if !self.asks_permission && matches!(prompt.kind, PromptKind::Permission { .. }) => {
                    Some(prompt.request_id.clone())
                }
                _ => None,
            };
            if let Some(run) = self.run_mut(&id) {
                run.heard(now);
                run.apply(event);
                settle_clock(run, now);
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
        if self.testing {
            return Err(StartError::TestRunning);
        }
        self.started += 1;
        let request = RunRequest {
            run_id: format!("{}-{}", now.format("%Y%m%d-%H%M%S"), self.started),
            line: line.to_owned(),
            model: options.model.clone(),
            budget_usd: options.budget_usd,
            started_at: now.format("%Y-%m-%d %H:%M").to_string(),
            resume_session: None,
            sign_in: self.sign_in,
        };
        let session = self.gateway.start(&request, self.sink_for(&request.run_id));
        self.sessions.insert(request.run_id.clone(), session);
        let mut run = Run::new(&request);
        let now = self.now();
        run.elapsed.start(now);
        run.heard(now);
        self.runs.insert(0, run);
        Ok(request.run_id)
    }

    /// A finished run continues in its Claude session through a new process.
    /// One run works at a time: while another run is busy, the reply is refused, even on a live session.
    pub fn reply(&mut self, id: &str, text: &str) -> Result<(), StartError> {
        if self.runs.iter().any(|run| run.id != id && run.is_busy()) {
            return Err(StartError::AnotherRunIsActive);
        }
        if self.reply_waits_for_test(id) {
            return Err(StartError::TestRunning);
        }
        let sign_in = self.sign_in;
        let live = self.sessions.contains_key(id);
        let now = self.now();
        let Some(run) = self.run_mut(id) else { return Ok(()) };
        run.note_reply(text);
        run.elapsed.start(now);
        run.heard(now);
        if !live {
            run.note_process_start(sign_in);
        }
        let request = RunRequest {
            run_id: run.id.clone(),
            line: text.to_owned(),
            model: run.requested_model.clone(),
            budget_usd: run.budget_usd,
            started_at: run.started_at.clone(),
            resume_session: run.session_id.clone(),
            sign_in,
        };
        if let Some(session) = self.sessions.get_mut(id) {
            session.send_message(text);
        } else {
            let session = self.gateway.start(&request, self.sink_for(id));
            self.sessions.insert(id.to_owned(), session);
        }
        Ok(())
    }

    /// An answer to a prompt is a send: the last update is now, and the clock follows whose turn it is.
    pub fn decide(&mut self, id: &str, request_id: &str, decision: Decision) {
        let now = self.now();
        let Some(run) = self.run_mut(id) else { return };
        let Some(prompt) = run.resolve_prompt(request_id, &decision) else { return };
        run.heard(now);
        settle_clock(run, now);
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
        let now = self.now();
        if let Some(run) = self.run_mut(id) {
            run.apply(AgentEvent::ProcessExited { failure: None });
            settle_clock(run, now);
        }
    }
}

/// The clock ticks while the run works or waits on the user, and holds when it does not.
fn settle_clock(run: &mut Run, now: Instant) {
    match (clock_runs(run), run.elapsed.is_running()) {
        (true, false) => run.elapsed.start(now),
        (false, true) => run.elapsed.hold(now),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

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

    /// A desk whose clock stands still until the test sets it, in seconds after `base`.
    fn desk_on_a_scripted_clock() -> (RunDesk, Instant, impl Fn(u64)) {
        let (mut desk, _) = desk();
        let (base, seconds) = (Instant::now(), Arc::new(AtomicU64::new(0)));
        let reader = Arc::clone(&seconds);
        desk.set_clock(Arc::new(move || base + Duration::from_secs(reader.load(Ordering::SeqCst))));
        (desk, base, move |at| seconds.store(at, Ordering::SeqCst))
    }

    fn options() -> StartOptions {
        StartOptions { model: None, budget_usd: 1.0 }
    }

    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    #[test]
    fn a_failed_turn_keeps_its_session_until_closed() {
        let (mut desk, _) = desk();
        let id = desk.start("/rank", &options(), now()).unwrap();
        desk.event_sender
            .send((id.clone(), AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.".into()), cost_usd: 1.0, duration_ms: 1 }))
            .unwrap();
        desk.poll();
        assert!(matches!(desk.run(&id).unwrap().status, RunStatus::Failed(_)));
        assert!(desk.has_session(&id));
        desk.close(&id);
        assert!(!desk.has_session(&id));
        assert!(!desk.has_session("unknown"));
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
    fn a_live_session_takes_no_reply_while_another_run_works() {
        let (mut desk, script) = desk();
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        let rank = desk.start("/rank", &options(), now()).unwrap();
        desk.poll();
        assert!(desk.has_session(&id) && desk.run(&rank).unwrap().is_busy());

        assert_eq!(desk.reply(&id, "yes"), Err(StartError::AnotherRunIsActive));

        assert!(script.borrow().sent.is_empty(), "nothing reached the live session");
        assert_eq!(desk.run(&id).unwrap().turn_number(), 1, "the reply is not in the story");
        desk.reply(&rank, "more").unwrap();
        assert_eq!(script.borrow().sent, ["more"], "a busy run's own session still takes a message");
    }

    #[test]
    fn a_stop_point_counts_as_waiting() {
        let (mut desk, _) = desk();
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::AssistantText { text: "Shall I draft it?".into(), nested: false });
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
    fn the_sign_in_reaches_each_request_it_is_set_for() {
        let (mut desk, script) = desk();
        assert_eq!(desk.sign_in(), SignIn::ClaudeSignIn);
        desk.set_sign_in(SignIn::ApiKey);
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&id)(AgentEvent::ProcessExited { failure: None });
        desk.poll();
        desk.reply(&id, "continue").unwrap();
        assert_eq!(desk.sign_in(), SignIn::ApiKey);
        assert_eq!(script.borrow().requests[0].sign_in, SignIn::ApiKey);
        assert_eq!(script.borrow().requests[1].sign_in, SignIn::ApiKey, "a resumed reply");

        desk.sink_for(&id)(AgentEvent::ProcessExited { failure: None });
        desk.poll();
        desk.set_sign_in(SignIn::ClaudeSignIn);
        desk.reply(&id, "again").unwrap();
        assert_eq!(script.borrow().requests[2].sign_in, SignIn::ClaudeSignIn, "a resumed reply");
        desk.sink_for(&id)(AgentEvent::ProcessExited { failure: None });
        desk.poll();
        desk.start("/rank", &options(), now()).unwrap();
        assert_eq!(script.borrow().requests[3].sign_in, SignIn::ClaudeSignIn);
    }

    #[test]
    fn the_latest_signed_in_run_is_the_newest_with_a_source() {
        let (mut desk, _) = desk();
        assert!(desk.latest_signed_in().is_none());
        let first = desk.start("/rank", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&first)(AgentEvent::ProcessExited { failure: None });
        desk.poll();
        let second = desk.start("/scrape", &options(), now()).unwrap();
        assert_eq!(desk.latest_signed_in().map(|run| run.id.as_str()), Some(first.as_str()), "the newest run has reported none yet");
        desk.poll();
        assert_eq!(desk.latest_signed_in().map(|run| run.id.as_str()), Some(second.as_str()));
    }

    #[test]
    fn a_run_keeps_the_sign_in_its_process_started_with() {
        let (mut desk, _) = desk();
        desk.set_sign_in(SignIn::ApiKey);
        let id = desk.start("/apply x", &options(), now()).unwrap();
        assert_eq!(desk.run(&id).unwrap().sign_in, None, "not before its process reports");
        desk.poll();
        assert_eq!(desk.run(&id).unwrap().sign_in, Some(SignIn::ApiKey));
        desk.set_sign_in(SignIn::ClaudeSignIn);
        assert_eq!(desk.run(&id).unwrap().sign_in, Some(SignIn::ApiKey), "a later change of method is not the run's");

        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.sink_for(&id)(AgentEvent::ProcessExited { failure: None });
        desk.poll();
        desk.reply(&id, "continue").unwrap();
        assert_eq!(desk.run(&id).unwrap().sign_in, Some(SignIn::ApiKey), "until the new process reports");
        desk.poll();
        assert_eq!(desk.run(&id).unwrap().sign_in, Some(SignIn::ClaudeSignIn), "a resumed run takes its new process's");
    }

    #[test]
    fn no_task_process_starts_while_a_test_runs() {
        let (mut desk, script) = desk();
        let live = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        desk.sink_for(&live)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        desk.set_testing(true);
        assert!(!desk.reply_waits_for_test(&live), "its process is up, so a reply starts none");
        desk.reply(&live, "yes").unwrap();
        desk.sink_for(&live)(AgentEvent::ProcessExited { failure: None });
        desk.poll();

        assert_eq!(desk.start("/rank", &options(), now()), Err(StartError::TestRunning));
        assert!(desk.reply_waits_for_test(&live));
        assert_eq!(desk.reply(&live, "again"), Err(StartError::TestRunning));
        assert_eq!(script.borrow().requests.len(), 1, "no process started");
        assert_eq!(desk.run(&live).unwrap().turn_number(), 2, "the refused reply is not in the story");

        desk.set_testing(false);
        desk.reply(&live, "again").unwrap();
        assert_eq!(script.borrow().requests.len(), 2);
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
    fn an_asking_desk_leaves_permissions_to_the_user_until_turned_off() {
        let (mut desk, script) = desk();
        desk.set_asks_permission(true);
        let id = desk.start("/rank", &options(), now()).unwrap();
        let permission = |request_id: &str| Prompt {
            request_id: request_id.into(),
            kind: PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: false },
            input_json: "{}".into(),
            suggestions_json: None,
        };
        desk.sink_for(&id)(AgentEvent::PromptRaised(permission("p")));
        desk.poll();
        assert!(script.borrow().sent.is_empty(), "the permission waits for the user");
        assert_eq!(desk.run(&id).unwrap().current_prompt().unwrap().request_id, "p");

        desk.set_asks_permission(false);
        desk.sink_for(&id)(AgentEvent::PromptRaised(permission("p2")));
        desk.poll();
        assert_eq!(script.borrow().sent, ["Allow { remember: false }"], "the next one is allowed");
    }

    #[test]
    fn elapsed_ticks_through_the_users_turn_and_holds_when_done() {
        let (mut desk, base, at) = desk_on_a_scripted_clock();
        let secs = Duration::from_secs;
        let id = desk.start("/apply x", &options(), now()).unwrap();
        desk.poll();
        at(10);
        desk.sink_for(&id)(AgentEvent::AssistantText { text: "Shall I draft it?".into(), nested: false });
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        assert!(desk.run(&id).unwrap().elapsed.is_running(), "the stop point is the user's turn");
        at(30);
        assert_eq!(desk.run(&id).unwrap().elapsed.total(desk.now()), secs(30));

        at(40);
        desk.reply(&id, "yes").unwrap();
        at(60);
        desk.sink_for(&id)(AgentEvent::AssistantText { text: "Done.".into(), nested: false });
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.2, duration_ms: 1 });
        desk.poll();
        let run = desk.run(&id).unwrap();
        assert!(!run.elapsed.is_running(), "done for now");
        assert_eq!(run.elapsed.total(base + secs(60)), secs(60));
        at(100);
        assert_eq!(desk.run(&id).unwrap().elapsed.total(desk.now()), secs(60), "it stands still");

        desk.reply(&id, "one more thing").unwrap();
        at(105);
        assert_eq!(desk.run(&id).unwrap().elapsed.total(desk.now()), secs(65), "a reply adds a span");

        at(110);
        desk.close(&id);
        at(200);
        let run = desk.run(&id).unwrap();
        assert!(!run.elapsed.is_running(), "closing holds it");
        assert_eq!(run.elapsed.total(desk.now()), secs(70));
    }

    #[test]
    fn a_new_event_or_a_send_resets_the_last_update() {
        let (mut desk, base, at) = desk_on_a_scripted_clock();
        let secs = Duration::from_secs;
        at(5);
        let id = desk.start("/apply x", &options(), now()).unwrap();
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(5)), "the send");
        at(20);
        desk.poll();
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(20)), "the event");
        at(50);
        desk.poll();
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(20)), "no event, no change");

        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        at(90);
        desk.reply(&id, "yes").unwrap();
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(90)), "a reply");
    }

    #[test]
    fn a_decision_resets_the_quiet_and_the_clock_keeps_ticking() {
        let (mut desk, base, at) = desk_on_a_scripted_clock();
        let secs = Duration::from_secs;
        let id = desk.start("/rank", &options(), now()).unwrap();
        let prompt = Prompt { request_id: "r".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None };
        desk.sink_for(&id)(AgentEvent::PromptRaised(prompt));
        desk.poll();
        at(180);
        desk.decide(&id, "r", Decision::Allow { remember: false });
        let run = desk.run(&id).unwrap();
        assert_eq!(run.last_heard, Some(base + secs(180)), "the answer is an update");
        assert!(run.elapsed.is_running(), "Claude's turn again");
        at(200);
        assert_eq!(desk.run(&id).unwrap().elapsed.total(desk.now()), secs(200));
        at(210);
        desk.decide(&id, "r", Decision::Deny);
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(180)), "no prompt, no send");
    }

    #[test]
    fn a_decision_that_leaves_the_turn_done_for_now_holds_the_clock() {
        let (mut desk, base, at) = desk_on_a_scripted_clock();
        let secs = Duration::from_secs;
        let id = desk.start("/rank", &options(), now()).unwrap();
        let prompt = Prompt { request_id: "r".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None };
        desk.sink_for(&id)(AgentEvent::PromptRaised(prompt));
        desk.sink_for(&id)(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.1, duration_ms: 1 });
        desk.poll();
        assert!(desk.run(&id).unwrap().elapsed.is_running(), "the open prompt waits on the user");
        at(30);
        desk.decide(&id, "r", Decision::Allow { remember: false });
        let run = desk.run(&id).unwrap();
        assert!(!run.elapsed.is_running(), "nothing works and nothing is asked: done for now");
        at(90);
        assert_eq!(desk.run(&id).unwrap().elapsed.total(desk.now()), secs(30), "it stands still");
        assert_eq!(desk.run(&id).unwrap().last_heard, Some(base + secs(30)));
    }

    struct Restored(Run);

    impl RunHistory for Restored {
        fn recent(&self, _: usize) -> Vec<Run> {
            vec![self.0.clone()]
        }
    }

    /// A Closed `/apply` read back from its log, its last turn's result $1.31, and an estimate its last process left without one.
    fn restored_apply(estimate: bool) -> Run {
        let mut run = Run::new(&RunRequest {
            run_id: "old".into(),
            line: "/apply x".into(),
            model: None,
            budget_usd: 3.0,
            started_at: String::new(),
            resume_session: Some("s0".into()),
            sign_in: SignIn::ClaudeSignIn,
        });
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 1.31, duration_ms: 1 });
        if estimate {
            let tokens = crate::runs::domain::Tokens { output: 4_000, ..Default::default() };
            run.apply(AgentEvent::MessageUsage { id: "m9".into(), model: "claude-opus-4-5".into(), tokens });
        }
        run.settle_after_restore();
        run
    }

    #[test]
    fn a_reply_to_a_restored_run_adds_its_cost_to_the_old() {
        for estimate in [false, true] {
            let (mut desk, _) = desk();
            desk.restore(&Restored(restored_apply(estimate)));
            let before = desk.run("old").unwrap().shown_cost();
            desk.reply("old", "continue").unwrap();
            desk.sink_for("old")(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.2, duration_ms: 1 });
            desk.poll();
            let shown = desk.run("old").unwrap().shown_cost();
            assert!((shown.usd - (before.usd + 0.2)).abs() < 1e-9, "{} after {}", shown.usd, before.usd);
            assert_eq!(shown.estimated, estimate, "an estimate the old process left stays");
        }
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
