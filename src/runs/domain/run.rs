use std::collections::VecDeque;

use crate::runs::domain::{AgentEvent, Decision, Prompt, RunRequest, StepRail, Usage};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActivityKind {
    You(String),
    Claude(String),
    Tool { id: String, name: String, summary: String, state: ToolState },
    Notice(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub kind: ActivityKind,
    pub nested: bool,
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
    pub status: RunStatus,
    pub activity: Vec<Activity>,
    pub streaming_text: String,
    pub prompts: VecDeque<Prompt>,
    pub cost_usd: f64,
    /// Spend of earlier processes. Each process reports its own running total.
    cost_of_ended_processes: f64,
    pub duration_ms: u64,
    pub steps: Option<StepRail>,
    pub agents: Vec<String>,
    pub files_changed: Vec<String>,
    pub usage: Usage,
}

impl Run {
    pub fn new(request: &RunRequest) -> Self {
        Self {
            id: request.run_id.clone(),
            line: request.line.clone(),
            started_at: request.started_at.clone(),
            requested_model: request.model.clone(),
            budget_usd: request.budget_usd,
            session_id: request.resume_session.clone(),
            model: None,
            status: RunStatus::Starting,
            activity: vec![Activity { kind: ActivityKind::You(request.line.clone()), nested: false }],
            streaming_text: String::new(),
            prompts: VecDeque::new(),
            cost_usd: 0.0,
            cost_of_ended_processes: 0.0,
            duration_ms: 0,
            steps: StepRail::for_line(&request.line),
            agents: Vec::new(),
            files_changed: Vec::new(),
            usage: Usage::default(),
        }
    }

    /// Whether commands must wait, because two runs at once could overwrite each other's files.
    pub fn is_busy(&self) -> bool {
        matches!(self.status, RunStatus::Starting | RunStatus::Working | RunStatus::Stopping)
    }

    pub fn needs_user(&self) -> bool {
        !self.prompts.is_empty()
    }

    pub fn current_prompt(&self) -> Option<&Prompt> {
        self.prompts.front()
    }

    pub fn note_reply(&mut self, text: &str) {
        self.activity.push(Activity { kind: ActivityKind::You(text.to_owned()), nested: false });
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

    pub fn notice(&mut self, text: &str) {
        self.activity.push(Activity { kind: ActivityKind::Notice(text.to_owned()), nested: false });
    }

    /// With no process left, nothing can still be running.
    fn close_open_tools(&mut self) {
        for activity in &mut self.activity {
            if let ActivityKind::Tool { state: state @ ToolState::Running, .. } = &mut activity.kind {
                *state = ToolState::Failed;
            }
        }
    }

    /// A run read back from its log has no live process.
    pub fn settle_after_restore(&mut self) {
        self.prompts.clear();
        self.streaming_text.clear();
        self.close_open_tools();
        if !matches!(self.status, RunStatus::Failed(_) | RunStatus::Stopped) {
            self.status = RunStatus::Closed;
        }
    }

    pub fn apply(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::SessionStarted { session_id, model } => {
                self.session_id = Some(session_id);
                self.model = Some(model);
                if self.status == RunStatus::Starting {
                    self.status = RunStatus::Working;
                }
            }
            AgentEvent::TextDelta(text) => self.streaming_text.push_str(&text),
            AgentEvent::AssistantText { text, nested } => {
                self.streaming_text.clear();
                self.activity.push(Activity { kind: ActivityKind::Claude(text), nested });
            }
            AgentEvent::ToolStarted { id, name, summary, nested } => self.tool_started(id, name, summary, nested),
            AgentEvent::ToolFinished { id, failed } => {
                let state = if failed { ToolState::Failed } else { ToolState::Done };
                for activity in self.activity.iter_mut().rev() {
                    if let ActivityKind::Tool { id: known, state: current, .. } = &mut activity.kind {
                        if *known == id {
                            *current = state;
                            break;
                        }
                    }
                }
            }
            AgentEvent::PromptRaised(prompt) => self.prompts.push_back(prompt),
            AgentEvent::PromptWithdrawn { request_id } => self.prompts.retain(|prompt| prompt.request_id != request_id),
            AgentEvent::UsageChanged(usage) => self.usage = usage,
            AgentEvent::TurnEnded { failed, message, cost_usd, duration_ms } => self.turn_ended(failed, message, cost_usd, duration_ms),
            AgentEvent::ProcessExited { failure } => self.process_exited(failure),
        }
    }

    fn tool_started(&mut self, id: String, name: String, summary: String, nested: bool) {
        self.streaming_text.clear();
        if let Some(steps) = &mut self.steps {
            steps.observe(&name, &summary);
        }
        if matches!(name.as_str(), "Agent" | "Task") {
            self.agents.push(summary.clone());
        }
        if matches!(name.as_str(), "Write" | "Edit") && !self.files_changed.contains(&summary) {
            self.files_changed.push(summary.clone());
        }
        self.activity.push(Activity { kind: ActivityKind::Tool { id, name, summary, state: ToolState::Running }, nested });
    }

    fn turn_ended(&mut self, failed: bool, message: Option<String>, cost_usd: f64, duration_ms: u64) {
        self.streaming_text.clear();
        self.cost_usd = self.cost_of_ended_processes + cost_usd;
        self.duration_ms += duration_ms;
        self.prompts.clear();
        if self.status == RunStatus::Stopping {
            self.status = RunStatus::Stopped;
            self.notice("You stopped this run.");
        } else if failed {
            let reason = message.unwrap_or_else(|| "The run ended with an error.".to_owned());
            self.notice(&reason);
            self.status = RunStatus::Failed(reason);
        } else {
            self.status = RunStatus::AwaitingReply;
        }
    }

    fn process_exited(&mut self, failure: Option<String>) {
        self.cost_of_ended_processes = self.cost_usd;
        self.prompts.clear();
        self.streaming_text.clear();
        self.close_open_tools();
        match failure {
            Some(reason) if !matches!(self.status, RunStatus::Stopping | RunStatus::Stopped) => {
                self.notice(&reason);
                self.status = RunStatus::Failed(reason);
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
    use crate::runs::domain::PromptKind;

    fn run(line: &str) -> Run {
        Run::new(&RunRequest { run_id: "1".into(), line: line.into(), model: None, budget_usd: 1.0, started_at: String::new(), resume_session: None })
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

    #[test]
    fn stopping_wins_over_a_failed_exit() {
        let mut run = run("/rank");
        run.note_stop_requested();
        run.apply(AgentEvent::ProcessExited { failure: Some("killed".into()) });
        assert_eq!(run.status, RunStatus::Stopped);
    }

    #[test]
    fn budget_failure_reaches_the_user() {
        let mut run = run("/rank");
        run.apply(AgentEvent::TurnEnded { failed: true, message: Some("Budget cap reached.".into()), cost_usd: 1.0, duration_ms: 1 });
        assert_eq!(run.status, RunStatus::Failed("Budget cap reached.".into()));
    }

    #[test]
    fn tools_feed_steps_agents_and_files() {
        let mut run = run("/apply x");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Write".into(), summary: "cv/main_a.tex".into(), nested: false });
        run.apply(AgentEvent::ToolStarted { id: "2".into(), name: "Agent".into(), summary: "application-reviewer: critique".into(), nested: false });
        run.apply(AgentEvent::ToolFinished { id: "1".into(), failed: true });
        assert_eq!(run.steps.as_ref().unwrap().current, 3);
        assert_eq!(run.files_changed, ["cv/main_a.tex"]);
        assert_eq!(run.agents.len(), 1);
        assert!(matches!(run.activity[1].kind, ActivityKind::Tool { state: ToolState::Failed, .. }));
    }

    #[test]
    fn exit_closes_tools_left_running() {
        let mut run = run("/rank");
        run.apply(AgentEvent::ToolStarted { id: "1".into(), name: "Bash".into(), summary: "x".into(), nested: false });
        run.apply(AgentEvent::ProcessExited { failure: None });
        assert!(matches!(run.activity[1].kind, ActivityKind::Tool { state: ToolState::Failed, .. }));
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
