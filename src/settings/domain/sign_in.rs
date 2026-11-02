use chrono::NaiveDateTime;

use crate::shared_kernel::{ApiKey, KeyMask, SignIn};

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

    #[test]
    fn a_task_uses_the_key_only_when_chosen_and_not_known_to_be_missing() {
        let present = || Saved::Present(KeyMask::of(&ApiKey::parse(FAKE).unwrap()));
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
