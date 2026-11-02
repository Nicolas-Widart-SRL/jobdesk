//! Preferences that belong to the app, kept in its own data folder and never in the checkout.

pub mod application;
mod domain;
mod infrastructure;
pub mod ui;

use std::path::Path;

use crate::settings::application::{Preferences, Settings};
use crate::settings::infrastructure::PreferencesFile;

/// `data` is the app's own data folder. The preferences never reach the workspace.
/// With no file there yet, `first` is kept and saved, so what the window restored carries over.
pub fn open_settings(data: &Path, first: Preferences) -> Settings {
    Settings::open(Box::new(PreferencesFile::new(data.join("settings.json"))), first)
}

/// Shows a folder in the file manager.
pub fn reveal_folder(path: &Path) {
    infrastructure::reveal_folder(path);
}
