use std::sync::Arc;

/// An Anthropic API key. It has no `Clone` and no `Display`, and its `Debug` shows no text,
/// so a key cannot reach a log, a panic message or the screen by accident.
pub struct ApiKey(String);

impl ApiKey {
    /// The key without the whitespace around it. None for text that is empty or holds anything but visible ASCII inside:
    /// real keys are ASCII, and a space, a control, a zero-width or a bidi character could hide in a pasted key.
    pub fn parse(text: &str) -> Option<ApiKey> {
        let key = text.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_graphic()) {
            return None;
        }
        Some(ApiKey(key.to_owned()))
    }

    /// The key itself. Only the keychain and the process that needs it should call this.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(…)")
    }
}

/// Variables from the shell that started Jobdesk that make the `claude` CLI sign in some other way than with the saved key:
/// a bearer token, or the switch of another provider (Bedrock, Vertex, Foundry, Mantle, a gateway, Anthropic on AWS or Google Cloud).
/// A process that runs under the key removes these. `ANTHROPIC_BASE_URL` is left alone on purpose: it moves where requests go, not how they sign in,
/// and a user may set it for a proxy they trust.
pub const ENV_THAT_OUTRANKS_THE_KEY: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_USE_ANTHROPIC_AWS",
    "CLAUDE_CODE_USE_ANTHROPIC_GOOGLE_CLOUD",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_USE_GATEWAY",
    "CLAUDE_CODE_USE_MANTLE",
    "CLAUDE_CODE_USE_VERTEX",
];

/// Reads the saved key when a task's process starts. `Ok(None)` is no key saved, and `Err` says why it could not be read.
pub type KeyReader = Arc<dyn Fn() -> Result<Option<ApiKey>, String> + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    #[test]
    fn debug_shows_no_key_text() {
        let key = ApiKey::parse(FAKE).unwrap();
        assert_eq!(format!("{key:?}"), "ApiKey(…)");
        let wrapped = format!("{:?} {:#?}", Some(&key), Ok::<_, String>(&key));
        assert!(!wrapped.contains("sk-ant") && !wrapped.contains("R2kd"));
    }

    #[test]
    fn parse_trims_the_ends() {
        let key = ApiKey::parse(&format!("  \t{FAKE}\r\n")).unwrap();
        assert_eq!(key.expose(), FAKE);
    }

    #[test]
    fn parse_refuses_empty_text() {
        for text in ["", "   ", "\n\t "] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn parse_refuses_whitespace_or_control_characters_inside() {
        for text in ["sk-ant- 1234", "sk-ant-\n1234", "sk-ant-\t1234", "sk-ant-\u{a0}1234", "sk-ant-\u{0}1234", "sk-ant-\u{7f}1234"] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn parse_refuses_letters_outside_ascii() {
        for text in ["sk-ant-é1234", "sk-ant-日本1234", "sk-ant-1234😀"] {
            assert!(ApiKey::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn parse_refuses_invisible_characters_anywhere() {
        for hidden in ['\u{200B}', '\u{FEFF}', '\u{200E}', '\u{202E}', '\u{2066}', '\u{2060}', '\u{00AD}', '\u{FE0F}', '\u{E0100}', '\u{1BCA0}'] {
            for text in [format!("{FAKE}{hidden}"), format!("{hidden}{FAKE}"), format!("sk-ant-{hidden}api03-FAKE"), format!("{FAKE}{hidden}  ")] {
                assert!(ApiKey::parse(&text).is_none(), "{text:?}");
            }
        }
    }
}
