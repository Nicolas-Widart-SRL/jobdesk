//! Preferences that belong to the app, kept in its own data folder and never in the checkout.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;
use std::sync::Arc;

use crate::settings::application::{DataSizes, KeyStore, Preferences, Settings};
use crate::settings::infrastructure::{MemoryKeychain, PreferencesFile, SystemKeychain};

/// `data` is the app's own data folder. The preferences never reach the workspace.
/// With no file there yet, `first` is kept and saved, so what the window restored carries over.
pub fn open_settings(data: &Path, first: Preferences) -> Settings {
    Settings::open(Box::new(PreferencesFile::new(data.join("settings.json"))), first)
}

/// What the app keeps in `data`, in bytes: the run logs and the tray's decisions.
pub fn measure_data(data: &Path) -> DataSizes {
    infrastructure::measure(data)
}

/// Shows a folder in the file manager.
pub fn reveal_folder(path: &Path) {
    infrastructure::reveal_folder(path);
}

/// The API key in the system keychain: the macOS Keychain or the Windows Credential Manager.
pub fn system_keychain() -> Arc<dyn KeyStore> {
    Arc::new(SystemKeychain::new())
}

/// A key kept in memory only, for tests and for a window that must not touch the system keychain.
pub fn memory_keychain(key: Option<&str>) -> Arc<dyn KeyStore> {
    Arc::new(MemoryKeychain::new(key))
}
