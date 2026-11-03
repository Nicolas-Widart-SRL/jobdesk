use crate::settings::domain::{KeyStore, KeyStoreError};
use crate::shared_kernel::ApiKey;

#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
const SERVICE: &str = "Jobdesk API key";
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
const ACCOUNT: &str = "Anthropic Console";
/// Said when a stored item is no usable key.
const UNREADABLE: &str = "The keychain item could not be read.";
/// Said for every other failure that could carry the stored bytes, whatever the operation, so the key never reaches a message.
const FAILED: &str = "The keychain item could not be used.";
/// Only the macOS Keychain and the Windows Credential Manager are backed. Elsewhere the crate keeps a mock that forgets.
#[cfg(not(any(target_os = "macos", windows)))]
const NO_KEYCHAIN: &str = "No system keychain on this platform.";

/// The API key in the macOS Keychain or the Windows Credential Manager.
pub struct SystemKeychain;

impl SystemKeychain {
    pub fn new() -> Self {
        Self
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    fn entry() -> Result<keyring::Entry, KeyStoreError> {
        Err(KeyStoreError { message: NO_KEYCHAIN.to_owned() })
    }

    #[cfg(any(target_os = "macos", windows))]
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
/// credentials, so those and every other error give a fixed text, and no `keyring::Error` is ever formatted whole.
fn failure(error: keyring::Error) -> KeyStoreError {
    let message = match error {
        keyring::Error::PlatformFailure(inner) | keyring::Error::NoStorageAccess(inner) => inner.to_string(),
        _ => FAILED.to_owned(),
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
        assert_eq!(error.message, FAILED);
        assert!(!error.message.contains("sk-ant"));
        assert_eq!(failure(keyring::Error::Ambiguous(Vec::new())).message, FAILED);
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn without_a_backed_keychain_every_operation_fails() {
        let keychain = SystemKeychain::new();
        let key = ApiKey::parse("sk-ant-FAKE-not-a-real-key").unwrap();
        assert_eq!(keychain.write(&key).unwrap_err().message, NO_KEYCHAIN);
        assert_eq!(keychain.read().unwrap_err().message, NO_KEYCHAIN);
        assert_eq!(keychain.delete().unwrap_err().message, NO_KEYCHAIN);
    }

    #[test]
    fn a_platform_failure_gives_its_own_words() {
        let error = failure(keyring::Error::PlatformFailure("The keychain is locked.".into()));
        assert_eq!(error.message, "The keychain is locked.");
        let error = failure(keyring::Error::NoStorageAccess("Access was denied.".into()));
        assert_eq!(error.message, "Access was denied.");
    }
}
