/// How a task signs in to Anthropic: through the Claude plan the `claude` CLI is signed in to, or with an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignIn {
    #[default]
    ClaudeSignIn,
    ApiKey,
}

/// The `apiKeySource` Claude Code gives for the account signed in with `/login`.
const LOGIN_KEY: &str = "/login managed key";

/// What the CLI's `apiKeySource` says a process signed in with. Every page that names it reads it through `key_source`, so they agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// No key, or the key `/login` manages: the Claude sign-in.
    ClaudeSignIn,
    /// `ANTHROPIC_API_KEY` in the process's environment.
    KeyVariable,
    /// Anything else, such as an `apiKeyHelper` or a value the CLI added later: a sign-in Claude Code chose itself.
    Other,
}

pub fn key_source(api_key_source: &str) -> KeySource {
    match api_key_source {
        "" | "none" | LOGIN_KEY => KeySource::ClaudeSignIn,
        "ANTHROPIC_API_KEY" => KeySource::KeyVariable,
        _ => KeySource::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_sources_read_as_one_of_three() {
        let cases = [
            ("", KeySource::ClaudeSignIn),
            ("none", KeySource::ClaudeSignIn),
            ("/login managed key", KeySource::ClaudeSignIn),
            ("ANTHROPIC_API_KEY", KeySource::KeyVariable),
            ("apiKeyHelper", KeySource::Other),
            ("\u{202E}evil", KeySource::Other),
        ];
        for (source, kind) in cases {
            assert_eq!(key_source(source), kind, "{source:?}");
        }
    }
}
