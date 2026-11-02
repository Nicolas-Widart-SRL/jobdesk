use crate::settings::domain::{KeyStore, KeyStoreError};
use crate::shared_kernel::ApiKey;

const SERVICE: &str = "Jobdesk API key";
const ACCOUNT: &str = "Anthropic Console";
/// Said for every failure that could carry the stored bytes, so the key never reaches a message.
const UNREADABLE: &str = "The keychain item could not be read.";

/// The API key in the macOS Keychain or the Windows Credential Manager.
pub struct SystemKeychain;

impl SystemKeychain {
    pub fn new() -> Self {
        Self
    }

    fn entry() -> Result<keyring::Entry, KeyStoreError> {
        // Credential Manager lists an item by its target, so the target is the service name too.
        let entry = if cfg!(windows) { keyring::Entry::new_with_target(SERVICE, SERVICE, ACCOUNT) } else { keyring::Entry::new(SERVICE, ACCOUNT) };
        entry.map_err(failure)
    }
}

impl Default for SystemKeychain {
    fn default() -> Self {
        Self::new()
    }
}

/// Only the two platform failures give their own words. `BadEncoding` holds the stored bytes and `Ambiguous` holds
/// credentials, so those and every other error give the fixed text, and no `keyring::Error` is ever formatted whole.
fn failure(error: keyring::Error) -> KeyStoreError {
    let message = match error {
        keyring::Error::PlatformFailure(inner) | keyring::Error::NoStorageAccess(inner) => inner.to_string(),
        _ => UNREADABLE.to_owned(),
    };
    KeyStoreError { message }
}

impl KeyStore for SystemKeychain {
    fn read(&self) -> Result<Option<ApiKey>, KeyStoreError> {
        match Self::entry()?.get_password() {
            Ok(text) => ApiKey::parse(&text).map(Some).ok_or_else(|| KeyStoreError { message: UNREADABLE.to_owned() }),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(failure(error)),
        }
    }

    fn write(&self, key: &ApiKey) -> Result<(), KeyStoreError> {
        Self::entry()?.set_password(key.expose()).map_err(failure)
    }

    fn delete(&self) -> Result<(), KeyStoreError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(failure(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_that_holds_stored_bytes_gives_the_fixed_text() {
        let error = failure(keyring::Error::BadEncoding(b"sk-ant-api03-FAKE-not-a-real-key".to_vec()));
        assert_eq!(error.message, UNREADABLE);
        assert!(!error.message.contains("sk-ant"));
    }

    #[test]
    fn a_platform_failure_gives_its_own_words() {
        let error = failure(keyring::Error::PlatformFailure("The keychain is locked.".into()));
        assert_eq!(error.message, "The keychain is locked.");
        let error = failure(keyring::Error::NoStorageAccess("Access was denied.".into()));
        assert_eq!(error.message, "Access was denied.");
    }
}
