use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::settings::domain::{Preferences, PreferencesStore, StartScreen, TextSize, ThemeChoice};
use crate::shared_kernel::LoadError;

const FILE: &str = "settings.json";

/// Every field may be missing, and an unknown field is ignored, so an older or newer file still loads.
#[derive(Serialize, Deserialize)]
struct Record {
    #[serde(default)]
    theme: String,
    #[serde(default = "default_text_size")]
    text_size: i64,
    #[serde(default)]
    reduce_motion: bool,
    #[serde(default)]
    open_on: String,
}

fn default_text_size() -> i64 {
    i64::from(TextSize::DEFAULT)
}

/// The preferences in one file of the app's data folder, readable only by the user.
pub struct PreferencesFile {
    path: PathBuf,
}

impl PreferencesFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Goes through a temporary file, so a crash leaves the previous preferences whole.
    fn replace(&self, text: &str) -> std::io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.open(&temporary)?.write_all(text.as_bytes())?;
        fs::rename(temporary, &self.path)
    }
}

impl PreferencesStore for PreferencesFile {
    fn load(&self) -> Result<Option<Preferences>, LoadError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(LoadError::new(FILE, error)),
        };
        let record: Record = serde_json::from_str(&text).map_err(|error| LoadError::new(FILE, error))?;
        Ok(Some(preferences_from(record)))
    }

    fn save(&self, preferences: &Preferences) -> Result<(), LoadError> {
        let text = serde_json::to_string_pretty(&record_from(preferences)).map_err(|error| LoadError::new(FILE, error))?;
        self.replace(&text).map_err(|error| LoadError::new(FILE, error))
    }
}

fn preferences_from(record: Record) -> Preferences {
    let theme = match record.theme.as_str() {
        "light" => ThemeChoice::Light,
        "dark" => ThemeChoice::Dark,
        _ => ThemeChoice::System,
    };
    let open_on = match record.open_on.as_str() {
        "jobs" => StartScreen::Jobs,
        "applications" => StartScreen::Applications,
        "insights" => StartScreen::Insights,
        "tasks" => StartScreen::Tasks,
        _ => StartScreen::Today,
    };
    Preferences { theme, text_size: TextSize::from_percent(record.text_size), reduce_motion: record.reduce_motion, open_on }
}

fn record_from(preferences: &Preferences) -> Record {
    let theme = match preferences.theme {
        ThemeChoice::System => "system",
        ThemeChoice::Light => "light",
        ThemeChoice::Dark => "dark",
    };
    let open_on = match preferences.open_on {
        StartScreen::Today => "today",
        StartScreen::Jobs => "jobs",
        StartScreen::Applications => "applications",
        StartScreen::Insights => "insights",
        StartScreen::Tasks => "tasks",
    };
    Record { theme: theme.to_owned(), text_size: i64::from(preferences.text_size.percent()), reduce_motion: preferences.reduce_motion, open_on: open_on.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_in(folder: &tempfile::TempDir) -> PreferencesFile {
        PreferencesFile::new(folder.path().join("data").join(FILE))
    }

    fn write(file: &PreferencesFile, text: &str) {
        fs::create_dir_all(file.path.parent().unwrap()).unwrap();
        fs::write(&file.path, text).unwrap();
    }

    #[test]
    fn preferences_round_trip() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        let preferences = Preferences { theme: ThemeChoice::Dark, text_size: TextSize::from_percent(125), reduce_motion: true, open_on: StartScreen::Applications };
        file.save(&preferences).unwrap();
        assert_eq!(file.load().unwrap(), Some(preferences));
        assert!(!file.path.with_extension("json.tmp").exists());
    }

    #[test]
    fn the_file_holds_the_documented_keys() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        file.save(&Preferences::default()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file.path).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!({"theme": "system", "text_size": 100, "reduce_motion": false, "open_on": "today"}));
    }

    #[test]
    fn a_missing_file_gives_none() {
        let folder = tempfile::tempdir().unwrap();
        assert_eq!(file_in(&folder).load().unwrap(), None);
    }

    #[test]
    fn garbage_is_an_error() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, "{not json");
        let error = file.load().unwrap_err();
        assert_eq!(error.source, FILE);
    }

    #[test]
    fn missing_fields_give_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, "{}");
        assert_eq!(file.load().unwrap(), Some(Preferences::default()));
        write(&file, r#"{"theme": "dark"}"#);
        assert_eq!(file.load().unwrap(), Some(Preferences { theme: ThemeChoice::Dark, ..Preferences::default() }));
    }

    #[test]
    fn unknown_fields_and_values_are_ignored() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, r#"{"theme": "sepia", "open_on": "setup", "colour": "red", "reduce_motion": true}"#);
        assert_eq!(file.load().unwrap(), Some(Preferences { reduce_motion: true, ..Preferences::default() }));
    }

    #[test]
    fn a_text_size_out_of_range_clamps() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, r#"{"text_size": 999}"#);
        assert_eq!(file.load().unwrap().unwrap().text_size.percent(), 300);
        write(&file, r#"{"text_size": -4}"#);
        assert_eq!(file.load().unwrap().unwrap().text_size.percent(), 50);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        file.save(&Preferences::default()).unwrap();
        assert_eq!(fs::metadata(&file.path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
