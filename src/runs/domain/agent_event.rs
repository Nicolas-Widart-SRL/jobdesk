#[derive(Debug, Clone, PartialEq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub text: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PromptKind {
    Permission { tool: String, detail: String, can_remember: bool },
    Questions(Vec<Question>),
}

/// Something Claude cannot continue without. The opaque fields go back to the CLI unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Prompt {
    pub request_id: String,
    pub kind: PromptKind,
    pub input_json: String,
    pub suggestions_json: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Usage {
    pub five_hour: Option<f32>,
    pub seven_day: Option<f32>,
}

/// What the CLI reported, in this feature's terms.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    SessionStarted { session_id: String, model: String, api_key_source: String },
    TextDelta(String),
    AssistantText { text: String, nested: bool },
    ToolStarted { id: String, name: String, summary: String, nested: bool },
    /// `Err` means the tool failed, and holds the reason when its output said why.
    ToolFinished { id: String, outcome: Result<(), Option<String>> },
    PromptRaised(Prompt),
    PromptWithdrawn { request_id: String },
    UsageChanged(Usage),
    /// How many subagents still work in the background. They can outlive the turn that started them.
    BackgroundTasksChanged { count: usize },
    TurnEnded { failed: bool, message: Option<String>, cost_usd: f64, duration_ms: u64 },
    ProcessExited { failure: Option<String> },
}
