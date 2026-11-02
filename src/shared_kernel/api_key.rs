use std::sync::Arc;

/// An Anthropic API key. It has no `Clone` and no `Display`, and its `Debug` shows no text,
/// so a key cannot reach a log, a panic message or the screen by accident.
pub struct ApiKey(String);

impl ApiKey {
    /// The key without the whitespace around it. None for text that is empty or holds whitespace or control characters inside.
    pub fn parse(text: &str) -> Option<ApiKey> {
        let key = text.trim();
        if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
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
}
