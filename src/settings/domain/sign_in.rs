use chrono::NaiveDateTime;

use crate::shared_kernel::{ApiKey, SignIn};

/// The place the API key is kept: the system keychain, never a file.
pub trait KeyStore: Send + Sync {
    /// `Ok(None)` when no key is saved.
    fn read(&self) -> Result<Option<ApiKey>, KeyStoreError>;
    fn write(&self, key: &ApiKey) -> Result<(), KeyStoreError>;
    /// Succeeds when nothing is saved.
    fn delete(&self) -> Result<(), KeyStoreError>;
}

/// Why the keychain failed, in words that never hold the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyStoreError {
    pub message: String,
}

const MASK: &str = "••••••••";
const PREFIX: &str = "sk-ant-";
/// The last four show only when the secret part of the key, after the public `sk-ant-` prefix, has this many characters.
/// So at most a third of the secret is ever on the screen.
const LONG_ENOUGH: usize = 12;

/// What the screen may show of a saved key: whether it has the usual prefix, and the last four characters of a long key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMask {
    prefixed: bool,
    last_four: Option<String>,
}

impl KeyMask {
    pub fn of(key: &ApiKey) -> Self {
        let text = key.expose();
        let prefixed = text.starts_with(PREFIX);
        let count = text.chars().count();
        let secret = if prefixed { count - PREFIX.len() } else { count };
        let last_four = (secret >= LONG_ENOUGH).then(|| text.chars().skip(count - 4).collect());
        Self { prefixed, last_four }
    }

    /// The dots, after `sk-ant-` when the key has that prefix.
    pub fn shown(&self) -> String {
        if self.prefixed { format!("{PREFIX}{MASK}") } else { MASK.to_owned() }
    }

    /// The last four characters, for a key long enough to show them.
    pub fn ending(&self) -> Option<&str> {
        self.last_four.as_deref()
    }
}

/// What the keychain holds, as far as this session knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// The keychain was not asked yet.
    NotRead,
    Absent,
    Present(KeyMask),
    /// The keychain answered with an error, which is its message.
    Unreadable(String),
}

/// The last check of the saved key, with what it cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyTest {
    pub passed: bool,
    pub at: NaiveDateTime,
    pub cost_usd: f64,
}

/// How a task signs in. The API key is used only when it is chosen and not known to be missing.
/// A key not read yet or unreadable still gives the key: the read when the task's process starts then fails loudly,
/// so a task never quietly bills the Claude plan instead.
pub fn task_sign_in(method: SignIn, saved: &Saved) -> SignIn {
    match (method, saved) {
        (SignIn::ApiKey, Saved::NotRead | Saved::Present(_) | Saved::Unreadable(_)) => SignIn::ApiKey,
        _ => SignIn::ClaudeSignIn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    fn mask_of(text: &str) -> KeyMask {
        KeyMask::of(&ApiKey::parse(text).unwrap())
    }

    #[test]
    fn a_long_key_shows_only_its_last_four() {
        let mask = mask_of(FAKE);
        assert_eq!(mask.shown(), "sk-ant-••••••••");
        assert_eq!(mask.ending(), Some("R2kd"));
        assert!(!format!("{mask:?}{}", mask.shown()).contains("api03"));
    }

    #[test]
    fn a_short_key_shows_no_characters() {
        let mask = mask_of("sk-ant-1234");
        assert_eq!(mask.shown(), "sk-ant-••••••••");
        assert_eq!(mask.ending(), None);
        assert_eq!(mask_of("abc").ending(), None);
        assert!(!format!("{mask:?}").contains("1234"));
    }

    #[test]
    fn twelve_secret_characters_are_long_enough() {
        assert_eq!(mask_of("FAKE12345678").ending(), Some("5678"));
        assert_eq!(mask_of("FAKE1234567").ending(), None);
    }

    #[test]
    fn the_public_prefix_does_not_count_toward_the_secret() {
        assert_eq!(mask_of("sk-ant-12345").ending(), None, "five secret characters");
        assert_eq!(mask_of("sk-ant-12345678901").ending(), None, "eleven secret characters");
        assert_eq!(mask_of("sk-ant-123456789012").ending(), Some("9012"));
    }

    #[test]
    fn an_unprefixed_key_gets_only_dots() {
        let mask = mask_of("FAKE-not-a-real-key-0000-R2kd");
        assert_eq!(mask.shown(), "••••••••");
        assert_eq!(mask.ending(), Some("R2kd"));
    }

    #[test]
    fn the_last_four_count_characters_not_bytes() {
        assert_eq!(mask_of("FAKE-key-ünïcödé").ending(), Some("cödé"));
    }

    #[test]
    fn a_task_uses_the_key_only_when_chosen_and_not_known_to_be_missing() {
        let present = || Saved::Present(mask_of(FAKE));
        let unreadable = || Saved::Unreadable("locked".to_owned());
        let cases = [
            (SignIn::ClaudeSignIn, Saved::NotRead, SignIn::ClaudeSignIn),
            (SignIn::ClaudeSignIn, Saved::Absent, SignIn::ClaudeSignIn),
            (SignIn::ClaudeSignIn, present(), SignIn::ClaudeSignIn),
            (SignIn::ClaudeSignIn, unreadable(), SignIn::ClaudeSignIn),
            (SignIn::ApiKey, Saved::NotRead, SignIn::ApiKey),
            (SignIn::ApiKey, Saved::Absent, SignIn::ClaudeSignIn),
            (SignIn::ApiKey, present(), SignIn::ApiKey),
            (SignIn::ApiKey, unreadable(), SignIn::ApiKey),
        ];
        for (method, saved, expected) in cases {
            assert_eq!(task_sign_in(method, &saved), expected, "{method:?} with {saved:?}");
        }
    }
}
