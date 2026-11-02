//! Preferences that belong to the app, kept in its own data folder and never in the checkout.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use crate::settings::application::Settings;
use crate::settings::infrastructure::PreferencesFile;

/// `data` is the app's own data folder. The preferences never reach the workspace.
pub fn open_settings(data: &Path) -> Settings {
    Settings::open(Box::new(PreferencesFile::new(data.join("settings.json"))))
}

/// Shows a folder in the file manager.
pub fn reveal_folder(path: &Path) {
    infrastructure::reveal_folder(path);
}
