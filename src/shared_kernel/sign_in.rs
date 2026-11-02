/// How a task signs in to Anthropic: through the Claude plan the `claude` CLI is signed in to, or with an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignIn {
    #[default]
    ClaudeSignIn,
    ApiKey,
}
