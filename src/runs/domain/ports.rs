use crate::runs::domain::{AgentEvent, Prompt, Run, TaskCost};
use crate::shared_kernel::SignIn;

pub type EventSink = Box<dyn Fn(AgentEvent) + Send>;

#[derive(Debug, Clone, PartialEq)]
pub struct RunRequest {
    pub run_id: String,
    pub line: String,
    pub model: Option<String>,
    pub budget_usd: f64,
    pub started_at: String,
    /// Continues an earlier session whose process has ended.
    pub resume_session: Option<String>,
    /// How the process signs in. The method only: the key itself never travels in a request.
    pub sign_in: SignIn,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Allow { remember: bool },
    Deny,
    Answer(Vec<(String, String)>),
}

pub trait AgentSession {
    fn send_message(&mut self, text: &str);
    fn respond(&mut self, prompt: &Prompt, decision: &Decision);
    fn interrupt(&mut self);
}

pub trait AgentGateway {
    fn start(&self, request: &RunRequest, sink: EventSink) -> Box<dyn AgentSession>;
}

pub trait RunHistory {
    fn recent(&self, limit: usize) -> Vec<Run>;
}

/// Reads what past tasks cost, from the logs they left.
pub trait UsageLogs: Send {
    fn tasks(&mut self) -> Vec<TaskCost>;
}
