use crate::runs::domain::{AgentEvent, Prompt, Run};

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
