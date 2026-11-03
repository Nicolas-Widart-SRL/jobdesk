use std::collections::VecDeque;
use std::time::Instant;

use crate::runs::domain::{AgentEvent, Decision, Elapsed, Prompt, PromptKind, RunRequest, StepRail, Usage, estimate_usd};
use crate::shared_kernel::SignIn;

#[derive(Debug, Clone, PartialEq)]
pub enum RunStatus {
    Starting,
    Working,
    Stopping,
    /// Claude ended its turn and the session can take a reply, as at `/apply`'s stop points.
    AwaitingReply,
    Stopped,
    Failed(String),
    Closed,
}

impl RunStatus {
    pub fn label(&self) -> &str {
        match self {
            Self::Starting => "Starting",
            Self::Working => "Running",
            Self::Stopping => "Stopping",
            Self::AwaitingReply => "Ready for reply",
            Self::Stopped => "Stopped",
            Self::Failed(_) => "Failed",
            Self::Closed => "Finished",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Done,
    /// Holds the reason, when the tool's output gave one.
    Failed(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivityKind {
    You(String),
    Claude(String),
    Tool {
        id: String,
        name: String,
        summary: String,
        state: ToolState,
    },
    Notice(String),
    /// A tool Jobdesk allowed without asking. Routine, unlike a notice that explains the run.
    Allowed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub kind: ActivityKind,
    pub nested: bool,
    /// The step the rail was on when the row arrived. `None` for a command without a rail.
    pub step: Option<usize>,
}

/// What a run has cost so far, and whether part of it is still a guess.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostFigure {
    pub usd: f64,
    /// True while some of it is priced from tokens, because the CLI has not reported its own figure yet.
    pub estimated: bool,
}

/// One command from start to finish, and any replies after it.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub id: String,
    pub line: String,
    pub started_at: String,
    pub requested_model: Option<String>,
    pub budget_usd: f64,
    pub session_id: Option<String>,
    pub model: Option<String>,
    /// How the CLI authenticated. `none` means its own Claude login.
    pub api_key_source: Option<String>,
    /// How Jobdesk started the process that reported `api_key_source`, which says whether a key it reports came from
    /// Jobdesk. None for a run read back from its log, which does not keep it, and before its process reports.
    pub sign_in: Option<SignIn>,
    /// How the process starting now signs in, until it reports how it did.
    starting_sign_in: Option<SignIn>,
    pub status: RunStatus,
    /// Whether the failure reason is the tail of the process's stderr, which ends on its error,
    /// rather than the turn's result text, which opens with it.
    pub failed_on_stderr: bool,
    pub activity: Vec<Activity>,
    pub streaming_text: String,
    pub prompts: VecDeque<Prompt>,
    pub cost_usd: f64,
    /// Spend of earlier processes. Each process reports its own running total.
    cost_of_ended_processes: f64,
    pub duration_ms: u64,
    /// How long the run has worked by the clock, which Claude's `duration_ms` is not.
    pub elapsed: Elapsed,
    /// When the stream last said anything. None before the first word.
    pub last_heard: Option<Instant>,
    /// What each message of the turn under way would cost at list price, by message id.
    /// The turn's result replaces them with the CLI's own figure.
    turn_estimates: Vec<(String, f64)>,
    /// The estimates of earlier processes that ended without a result to replace them.
    estimate_of_ended_processes: f64,
    pub steps: Option<StepRail>,
    pub files_changed: Vec<String>,
    pub usage: Usage,
    /// Subagents still working after the turn that started them ended.
    pub background_agents: usize,
    /// Index in `activity` of the tool that last moved the rail. 0, the command line, while it has never moved.
    rail_moved_at: usize,
    /// Index in `activity` of the first row of the current turn.
    turn_started_at: usize,
    /// How many `You` rows `activity` holds.
    turn_count: usize,
}

impl Run {
    pub fn new(request: &RunRequest) -> Self {
        let steps = StepRail::for_line(&request.line);
        let activity = vec![Activity { kind: ActivityKind::You(request.line.clone()), nested: false, step: steps.as_ref().map(|rail| rail.current) }];
        Self {
            id: request.run_id.clone(),
            line: request.line.clone(),
            started_at: request.started_at.clone(),
            requested_model: request.model.clone(),
            budget_usd: request.budget_usd,
            session_id: request.resume_session.clone(),
            model: None,
            api_key_source: None,
            sign_in: None,
            starting_sign_in: Some(request.sign_in),
            status: RunStatus::Starting,
            failed_on_stderr: false,
            rail_moved_at: 0,
            turn_started_at: activity.len(),
            turn_count: 1,
            activity,
            streaming_text: String::new(),
            prompts: VecDeque::new(),
            cost_usd: 0.0,
            cost_of_ended_processes: 0.0,
            duration_ms: 0,
            elapsed: Elapsed::default(),
            last_heard: None,
            turn_estimates: Vec::new(),
            estimate_of_ended_processes: 0.0,
            steps,
            files_changed: Vec::new(),
            usage: Usage::default(),
            background_agents: 0,
        }
    }

    /// Claude's own top-level messages, oldest first.
    pub fn replies(&self) -> Vec<String> {
        let reply = |activity: &Activity| match &activity.kind {
            ActivityKind::Claude(text) if !activity.nested => Some(text.clone()),
            _ => None,
        };
        self.activity.iter().filter_map(reply).collect()
    }

    /// Whether commands must wait, because two runs at once could overwrite each other's files.
    /// Background subagents count too: they can still write after the turn ended.
    pub fn is_busy(&self) -> bool {
        matches!(self.status, RunStatus::Starting | RunStatus::Working | RunStatus::Stopping) || self.background_agents > 0
    }

    /// The cost to show: what the CLI reported, plus the estimate of what it has not reported yet.
    pub fn shown_cost(&self) -> CostFigure {
        let estimate: f64 = self.turn_estimates.iter().map(|(_, usd)| usd).sum();
        CostFigure {
            usd: self.cost_usd + self.estimate_of_ended_processes + estimate,
            estimated: self.estimate_of_ended_processes > 0.0 || !self.turn_estimates.is_empty(),
        }
    }

    /// The stream said something at `now`.
    pub fn heard(&mut self, now: Instant) {
        self.last_heard = Some(now);
    }

    /// A new process continues the run, and its running total starts again from zero.
    /// What the old one spent is kept, and so is the estimate of what it never reported.
    pub fn note_new_process(&mut self) {
        self.cost_of_ended_processes = self.cost_usd;
        self.estimate_of_ended_processes += self.turn_estimates.drain(..).map(|(_, usd)| usd).sum::<f64>();
    }

    pub fn needs_user(&self) -> bool {
        !self.prompts.is_empty()
    }

    /// Whether Claude's turn ended, nothing else still works and no prompt is open, so only a reply moves the run on.
    pub fn awaits_reply(&self) -> bool {
        self.status == RunStatus::AwaitingReply && !self.is_busy() && !self.needs_user()
    }

    /// How many messages the user has sent, the command line included.
    pub fn turn_number(&self) -> usize {
        self.turn_count
    }

    /// Index in `activity` of the tool that last moved the rail. 0, the command line, while it has never moved.
    pub fn rail_moved_at(&self) -> usize {
        self.rail_moved_at
    }

    /// Index in `activity` of the current turn's first row: the one after the user's last message,
    /// or after the rows of the turn before when Claude began a turn on its own.
    pub fn current_turn_start(&self) -> usize {
        self.turn_started_at
    }

    pub fn current_prompt(&self) -> Option<&Prompt> {
        self.prompts.front()
    }

    /// A new process continues the run, signed in with `sign_in`, which holds once it reports.
    pub fn note_process_start(&mut self, sign_in: SignIn) {
        self.starting_sign_in = Some(sign_in);
    }

    /// A run read back from its log: the log does not keep how its processes signed in.
    pub fn forget_sign_in(&mut self) {
        (self.sign_in, self.starting_sign_in) = (None, None);
    }

    pub fn note_reply(&mut self, text: &str) {
        self.push(ActivityKind::You(text.to_owned()), false);
        self.turn_started_at = self.activity.len();
        self.turn_count += 1;
        self.status = RunStatus::Working;
    }

    pub fn note_stop_requested(&mut self) {
        self.status = RunStatus::Stopping;
        self.prompts.clear();
    }

    pub fn resolve_prompt(&mut self, request_id: &str, decision: &Decision) -> Option<Prompt> {
        let at = self.prompts.iter().position(|prompt| prompt.request_id == request_id)?;
        let prompt = self.prompts.remove(at)?;
        if *decision == Decision::Deny {
            self.notice("You denied that action.");
        }
        Some(prompt)
    }

    /// Jobdesk allows every tool for now. The notice keeps each allowance in the activity.
    pub fn allow_automatically(&mut self, request_id: &str) -> Option<Prompt> {
        let prompt = self.resolve_prompt(request_id, &Decision::Allow { remember: false })?;
        if let PromptKind::Permission { tool, .. } = &prompt.kind {
            self.push(ActivityKind::Allowed(format!("Allowed {tool} without asking.")), false);
        }
        Some(prompt)
    }

    pub fn notice(&mut self, text: &str) {
        self.push(ActivityKind::Notice(text.to_owned()), false);
    }

    /// Adds a row under the step the rail is on now.
    fn push(&mut self, kind: ActivityKind, nested: bool) {
        let step = self.steps.as_ref().map(|rail| rail.current);
        self.activity.push(Activity { kind, nested, step });
    }

    /// With no process left, nothing can still be running.
    fn close_open_tools(&mut self) {
        for activity in &mut self.activity {
            if let ActivityKind::Tool { state: state @ ToolState::Running, .. } = &mut activity.kind {
                *state = ToolState::Failed(None);
            }
        }
    }

    /// A run read back from its log has no live process.
    pub fn settle_after_restore(&mut self) {
        self.prompts.clear();
        self.background_agents = 0;
        self.streaming_text.clear();
        self.close_open_tools();
        self.elapsed = Elapsed::restored(self.duration_ms);
        if !matches!(self.status, RunStatus::Failed(_) | RunStatus::Stopped) {
            self.status = RunStatus::Closed;
        }
    }

    pub fn apply(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::SessionStarted { session_id, model, api_key_source } => {
                self.session_id = Some(session_id);
                self.model = Some(model);
                self.api_key_source = Some(api_key_source);
                // A live session reports again each turn, with the process it started with.
                if let Some(sign_in) = self.starting_sign_in.take() {
                    self.sign_in = Some(sign_in);
                }
                // Each turn starts with this, including the one Claude takes on its own when a background agent reports.
                if matches!(self.status, RunStatus::Starting | RunStatus::AwaitingReply | RunStatus::Stopped | RunStatus::Failed(_)) {
                    // The first turn begins with the command line. Any later one is Claude's own.
                    if self.status != RunStatus::Starting {
                        self.turn_started_at = self.activity.len();
                    }
                    self.status = RunStatus::Working;
                }
            }
            AgentEvent::TextDelta(text) => self.streaming_text.push_str(&text),
            AgentEvent::AssistantText { text, nested } => {
                self.streaming_text.clear();
                self.push(ActivityKind::Claude(text), nested);
            }
            AgentEvent::ToolStarted { id, name, summary, nested } => self.tool_started(id, name, summary, nested),
            AgentEvent::ToolFinished { id, outcome } => {
                let state = match outcome {
                    Ok(()) => ToolState::Done,
                    Err(reason) => ToolState::Failed(reason),
                };
                for activity in self.activity.iter_mut().rev() {
                    if let ActivityKind::Tool { id: known, state: current, .. } = &mut activity.kind
                        && *known == id
                    {
                        *current = state;
                        break;
                    }
                }
            }
            AgentEvent::PromptRaised(prompt) => self.prompts.push_back(prompt),
            AgentEvent::PromptWithdrawn { request_id } => self.prompts.retain(|prompt| prompt.request_id != request_id),
            AgentEvent::UsageChanged(usage) => self.usage = usage,
            AgentEvent::MessageUsage { id, model, tokens } => self.message_used(id, estimate_usd(&tokens, &model)),
            // The desk stamps the time, because the run never reads the clock.
            AgentEvent::Heard => {}
            AgentEvent::BackgroundTasksChanged { count } => self.background_agents = count,
            AgentEvent::TurnEnded { failed, message, cost_usd, duration_ms } => self.turn_ended(failed, message, cost_usd, duration_ms),
            AgentEvent::ProcessExited { failure } => self.process_exited(failure),
        }
    }

    fn tool_started(&mut self, id: String, name: String, summary: String, nested: bool) {
        self.streaming_text.clear();
        if let Some(steps) = &mut self.steps {
            let before = steps.current;
            steps.observe(&name, &summary);
            if steps.current != before {
                // The tool is pushed below, at the end of the activity.
                self.rail_moved_at = self.activity.len();
            }
        }
        if matches!(name.as_str(), "Write" | "Edit" | "MultiEdit") && !self.files_changed.contains(&summary) {
            self.files_changed.push(summary.clone());
        }
        self.push(ActivityKind::Tool { id, name, summary, state: ToolState::Running }, nested);
    }

    /// A message the CLI repeats keeps the larger of its figures, never both.
    fn message_used(&mut self, id: String, usd: f64) {
        match self.turn_estimates.iter_mut().find(|(known, _)| *known == id) {
            Some((_, known)) => *known = known.max(usd),
            None => self.turn_estimates.push((id, usd)),
        }
    }

    fn turn_ended(&mut self, failed: bool, message: Option<String>, cost_usd: f64, duration_ms: u64) {
        self.streaming_text.clear();
        self.cost_usd = self.cost_of_ended_processes + cost_usd;
        self.turn_estimates.clear();
        self.duration_ms += duration_ms;
        // A background subagent can still be waiting on a prompt after the main turn ends.
        if self.status == RunStatus::Stopping {
            self.status = RunStatus::Stopped;
            self.notice("You stopped this run.");
        } else if failed {
            let reason = message.unwrap_or_else(|| "The run ended with an error.".to_owned());
            self.notice(&reason);
            self.status = RunStatus::Failed(reason);
            self.failed_on_stderr = false;
        } else {
            self.status = RunStatus::AwaitingReply;
        }
    }

    fn process_exited(&mut self, failure: Option<String>) {
        self.note_new_process();
        self.prompts.clear();
        self.background_agents = 0;
        self.streaming_text.clear();
        self.close_open_tools();
        // A run that was stopped, closed or already failed keeps its status; a failure says its first reason once.
        match failure {
            Some(reason) if !matches!(self.status, RunStatus::Stopping | RunStatus::Stopped | RunStatus::Closed | RunStatus::Failed(_)) => {
                self.notice(&reason);
                self.status = RunStatus::Failed(reason);
                self.failed_on_stderr = true;
            }
            _ if matches!(self.status, RunStatus::Stopping) => self.status = RunStatus::Stopped,
            _ if !matches!(self.status, RunStatus::Failed(_) | RunStatus::Stopped) => self.status = RunStatus::Closed,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::domain::{PromptKind, Tokens};

    fn run(line: &str) -> Run {
        Run::new(&RunRequest {
            run_id: "1".into(),
            line: line.into(),
            model: None,
            budget_usd: 1.0,
            started_at: String::new(),
            resume_session: None,
            sign_in: crate::shared_kernel::SignIn::ClaudeSignIn,
        })
    }

    fn prompt(id: &str) -> Prompt {
        Prompt { request_id: id.into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None }
    }

    fn turn(cost_usd: f64) -> AgentEvent {
        AgentEvent::TurnEnded { failed: false, message: None, cost_usd, duration_ms: 10 }
    }

    #[test]
    fn prompts_queue_when_subagents_ask_together() {
        let mut run = run("/rank");
        run.apply(AgentEvent::PromptRaised(prompt("a")));
        run.apply(AgentEvent::PromptRaised(prompt("b")));
        assert_eq!(run.current_prompt().unwrap().request_id, "a");
        run.resolve_prompt("a", &Decision::Allow { remember: false });
        assert_eq!(run.current_prompt().unwrap().request_id, "b");
        run.apply(AgentEvent::PromptWithdrawn { request_id: "b".into() });
        assert!(!run.needs_user());
    }

    #[test]
    fn turn_end_is_a_stop_point_not_the_end() {
        let mut run = run("/apply x");
        run.apply(turn(0.5));
        assert_eq!(run.status, RunStatus::AwaitingReply);
        assert!(!run.is_busy());
        run.note_reply("yes");
        assert!(run.is_busy());
    }

    #[test]
    fn background_agents_keep_the_run_busy_past_its_turn() {
        let mut run = run("/rank");
        run.apply(AgentEvent::BackgroundTasksChanged { count: 1 });
        run.apply(turn(0.5));
        assert!(run.is_busy());
        run.apply(AgentEvent::BackgroundTasksChanged { count: 0 });
        assert!(!run.is_busy());
        run.apply(AgentEvent::SessionStarted { session_id: "s".into(), model: "m".into(), api_key_source: "none".into() });
        assert_eq!(run.status, RunStatus::Working);
    }

    #[test]
    fn an_automatic_allowance_is_noted() {
        let mut run = run("/rank");
        let permission = PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: false };
        run.apply(AgentEvent::PromptRaised(Prompt { kind: permission, ..prompt("a") }));
        assert!(run.allow_automatically("a").is_some());
        assert!(!run.needs_user());
        assert_eq!(run.activity.last().unwrap().kind, ActivityKind::Allowed("Allowed Bash without asking.".into()));
    }

    #[test]
    fn turns_count_the_messages_you_sent() {
        let mut run = run("/apply x");
        assert_eq!(run.turn_number(), 1);
        run.apply(turn(0.5));
        run.note_reply("yes");
        assert_eq!(run.turn_number(), 2);
    }

    #[test]
    fn background_prompt_outlives_the_turn() {
        let mut run = run("/rank");
        run.apply(AgentEvent::PromptRaised(prompt("a")));
        run.apply(turn(0.5));
        assert_eq!(run.current_prompt().unwrap().request_id, "a");
    }

    #[test]
    fn cost_adds_up_across_resumed_processes() {
        let mut run = run("/apply x");
        run.apply(turn(0.5));
        run.apply(turn(0.75));
        run.apply(AgentEvent::ProcessExited { failure: None });
        assert_eq!(run.status, RunStatus::Closed);
        run.note_reply("again");
        run.apply(turn(0.25));
        assert_eq!(run.cost_usd, 1.0);
    }

    fn used(id: &str, output: u64) -> AgentEvent {
        AgentEvent::MessageUsage { id: id.into(), model: "claude-opus-4-5".into(), tokens: Tokens { output, ..Tokens::default() } }
    }

    #[test]
    fn a_repeated_message_counts_once() {
        let mut run = run("/rank");
        run.apply(used("m1", 1_000_000));
        run.apply(used("m1", 1_000_000));
        assert_eq!(run.shown_cost(), CostFigure { usd: 25.0, estimated: true });
        run.apply(used("m1", 2_000_000));
        run.apply(used("m1", 1_000_000));
        assert_eq!(run.shown_cost().usd, 50.0, "the larger figure stays");
        run.apply(used("m2", 1_000_000));
        assert_eq!(run.shown_cost().usd, 75.0);
    }

    #[test]
    fn the_result_replaces_the_estimate() {
        let mut run = run("/rank");
        assert_eq!(run.shown_cost(), CostFigure { usd: 0.0, estimated: false });
        run.apply(used("m1", 1_000_000));
        run.apply(turn(0.5));
        assert_eq!(run.shown_cost(), CostFigure { usd: 0.5, estimated: false });
    }

    #[test]
    fn a_later_turn_adds_its_estimate_to_what_the_result_reported() {
        let mut run = run("/rank");
        run.apply(turn(0.5));
        run.note_reply("more");
        run.apply(used("m2", 1_000_000));
        assert_eq!(run.shown_cost(), CostFigure { usd: 25.5, estimated: true });
    }

    #[test]
    fn a_process_that_exits_without_a_result_keeps_its_estimate() {
        let mut run = run("/rank");
        run.apply(used("m1", 1_000_000));
        run.apply(AgentEvent::ProcessExited { failure: Some("killed".into()) });
        assert_eq!(run.shown_cost(), CostFigure { usd: 25.0, estimated: true });
        run.note_reply("again");
        run.apply(used("m2", 1_000_000));
        assert_eq!(run.shown_cost().usd, 50.0, "the new process adds its own");
    }

    #[test]
    fn a_new_process_keeps_the_old_total_and_adds_its_own() {
        let mut run = run("/apply x");
        run.apply(turn(0.5));
        run.note_new_process();
        assert_eq!(run.status, RunStatus::AwaitingReply, "the status stays");
        run.apply(turn(0.25));
        assert_eq!(run.cost_usd, 0.75);
        assert_eq!(run.shown_cost(), CostFigure { usd: 0.75, estimated: false });
    }

    #[test]
    fn a_new_process_keeps_the_estimate_of_a_turn_without_a_result() {
        let mut run = run("/apply x");
        run.apply(used("m1", 1_000_000));
        run.note_new_process();
        run.note_new_process();
        assert_eq!(run.shown_cost(), CostFigure { usd: 25.0, estimated: true }, "once, however often it is noted");
        run.apply(turn(0.25));
        assert_eq!(run.shown_cost(), CostFigure { usd: 25.25, estimated: true });
    }

    #[test]
    fn hearing_stamps_the_time_and_a_heard_event_changes_nothing() {
        let mut run = run("/rank");
        assert_eq!(run.last_heard, None);
        let now = Instant::now();
        run.heard(now);
        run.apply(AgentEvent::Heard);
        assert_eq!(run.last_heard, Some(now));
        assert_eq!(run.status, RunStatus::Starting);
    }

    #[test]
    fn a_restored_run_shows_the_time_claude_reported() {
        let mut run = run("/rank");
        run.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 860_000 });
        run.settle_after_restore();
        assert!(!run.elapsed.is_running());
        assert_eq!(run.elapsed.total(Instant::now()), std::time::Duration::from_millis(860_000));
    }

    #[test]
    fn stopping_wins_over_a_failed_exit() {
        let mut run = run("/rank");
        run.note_stop_requested();
        run.apply(AgentEvent::ProcessExited { failure: Some("killed".into()) });
        assert_eq!(run.status, RunStatus::Stopped);
    }

    #[test]
    fn a_failed_exit_after_closing_leaves_the_run_closed() {
        let mut run = run("/rank");
        run.apply(turn(0.5));
        run.apply(AgentEvent::ProcessExited { failure: None });
        assert_eq!(run.status, RunStatus::Closed);
        run.apply(AgentEvent::ProcessExited { failure: Some("killed".into()) });
        assert_eq!(run.status, RunStatus::Closed);
        assert!(!run.activity.iter().any(|activity| matches!(activity.kind, ActivityKind::Notice(_))));
    }

    #[test]
    fn budget_failure_reaches_the_user() {
        let mut run = run("/rank");
        run.apply(AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.".into()), cost_usd: 1.0, duration_ms: 1 });
        assert_eq!(run.status, RunStatus::Failed("Budget cap reached.".into()));
    }

    #[test]
    fn tools_feed_steps_and_files() {
        let mut run = run("/apply x");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Write".into(), summary: "cv/main_a.tex".into(), nested: false });
        run.apply(AgentEvent::ToolStarted { id: "2".into(), name: "Agent".into(), summary: "application-reviewer: critique".into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: "1".into(), outcome: Err(None) });
        assert_eq!(run.steps.as_ref().unwrap().current, 3);
        assert_eq!(run.files_changed, ["cv/main_a.tex"]);
        assert!(matches!(run.activity[1].kind, ActivityKind::Tool { state: ToolState::Failed(None), .. }));
    }

    #[test]
    fn a_failed_run_keeps_its_first_reason() {
        let mut run = run("/rank");
        run.apply(AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.".into()), cost_usd: 1.0, duration_ms: 1 });
        run.apply(AgentEvent::ProcessExited { failure: Some("exit code 1".into()) });
        assert_eq!(run.status, RunStatus::Failed("Budget cap reached.".into()));
        let notices: Vec<_> = run.activity.iter().filter(|activity| matches!(activity.kind, ActivityKind::Notice(_))).collect();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, ActivityKind::Notice("Budget cap reached.".into()));
    }

    #[test]
    fn multi_edit_counts_as_a_changed_file() {
        let mut run = run("/apply x");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "MultiEdit".into(), summary: "job_search_tracker.csv".into(), nested: false });
        assert_eq!(run.files_changed, ["job_search_tracker.csv"]);
    }

    #[test]
    fn failed_tool_keeps_its_reason() {
        let mut run = run("/rank");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Bash".into(), summary: "ls -Z".into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: "1".into(), outcome: Err(Some("ls: invalid option -- 'Z'".into())) });
        assert_eq!(
            run.activity[1].kind,
            ActivityKind::Tool {
                id: "1".into(),
                name: "Bash".into(),
                summary: "ls -Z".into(),
                state: ToolState::Failed(Some("ls: invalid option -- 'Z'".into()))
            }
        );
    }

    #[test]
    fn finished_tool_is_done() {
        let mut run = run("/rank");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Bash".into(), summary: "ls".into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: "1".into(), outcome: Ok(()) });
        assert!(matches!(run.activity[1].kind, ActivityKind::Tool { state: ToolState::Done, .. }));
    }

    #[test]
    fn exit_closes_tools_left_running() {
        let mut run = run("/rank");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Bash".into(), summary: "x".into(), nested: false });
        run.apply(AgentEvent::ProcessExited { failure: None });
        assert!(matches!(run.activity[1].kind, ActivityKind::Tool { state: ToolState::Failed(None), .. }));
    }

    #[test]
    fn restored_run_has_no_live_state() {
        let mut run = run("/rank");
        run.apply(AgentEvent::PromptRaised(prompt("a")));
        run.settle_after_restore();
        assert_eq!(run.status, RunStatus::Closed);
        assert!(!run.needs_user());
    }
}
