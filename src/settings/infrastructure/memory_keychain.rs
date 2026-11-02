use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::settings::domain::{KeyStore, KeyStoreError};
use crate::shared_kernel::ApiKey;

/// A key held in memory only, for tests and for a window that must never touch the system keychain.
pub struct MemoryKeychain {
    key: Mutex<Option<String>>,
}

impl MemoryKeychain {
    pub fn new(key: Option<&str>) -> Self {
        Self { key: Mutex::new(key.map(str::to_owned)) }
    }

    fn held(&self) -> MutexGuard<'_, Option<String>> {
        self.key.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl KeyStore for MemoryKeychain {
    fn read(&self) -> Result<Option<ApiKey>, KeyStoreError> {
        Ok(self.held().as_deref().and_then(ApiKey::parse))
    }

    fn write(&self, key: &ApiKey) -> Result<(), KeyStoreError> {
        *self.held() = Some(key.expose().to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), KeyStoreError> {
        *self.held() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    #[test]
    fn a_key_is_written_read_and_deleted() {
        let keychain = MemoryKeychain::new(None);
        assert!(keychain.read().unwrap().is_none());
        keychain.write(&ApiKey::parse(FAKE).unwrap()).unwrap();
        assert_eq!(keychain.read().unwrap().unwrap().expose(), FAKE);
        keychain.delete().unwrap();
        assert!(keychain.read().unwrap().is_none());
        keychain.delete().unwrap();
    }

    #[test]
    fn a_new_keychain_can_start_with_a_key() {
        assert_eq!(MemoryKeychain::new(Some(FAKE)).read().unwrap().unwrap().expose(), FAKE);
    }

    #[test]
    fn a_second_write_replaces_the_first() {
        let keychain = MemoryKeychain::new(Some(FAKE));
        keychain.write(&ApiKey::parse("sk-ant-other-FAKE").unwrap()).unwrap();
        assert_eq!(keychain.read().unwrap().unwrap().expose(), "sk-ant-other-FAKE");
    }
}
