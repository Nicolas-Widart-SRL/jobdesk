use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};

use crate::settings::domain::{
    CommandDefault, Dollars, PermissionMode, Preferences, PreferencesStore, Retention, RunDefaults, SpendCap, StartScreen, TextSize, ThemeChoice,
};
use crate::shared_kernel::{LoadError, ModelChoice};

const FILE: &str = "settings.json";

/// Every field may be missing, and an unknown field is ignored, so an older or newer file still loads.
/// The run defaults, cap, permissions and retention arrive as raw JSON and are read leniently below:
/// a field of the wrong shape falls back to its default and never fails the rest of the file.
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
    #[serde(default)]
    model: Value,
    #[serde(default)]
    budget_usd: Value,
    #[serde(default)]
    commands: Value,
    #[serde(default)]
    cap_on: Value,
    #[serde(default)]
    cap_usd: Value,
    #[serde(default)]
    permissions: Value,
    #[serde(default)]
    keep_run_logs: Value,
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
        let mut file = options.open(&temporary)?;
        file.write_all(text.as_bytes())?;
        // On disk before the rename, or a power loss could keep the rename and lose the data.
        file.sync_all()?;
        fs::rename(temporary, &self.path)?;
        // The rename lives in the folder, which a crash could still lose. Windows cannot open a folder this way.
        // Best effort: some mounts refuse to sync a folder, and the file is already in place.
        #[cfg(unix)]
        if let Some(folder) = self.path.parent().filter(|folder| !folder.as_os_str().is_empty()) {
            let _ = fs::File::open(folder).and_then(|folder| folder.sync_all());
        }
        Ok(())
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
    let run_defaults = RunDefaults { model: model_from(&record.model), budget: dollars_from(&record.budget_usd), commands: commands_from(&record.commands) };
    let cap = SpendCap { on: record.cap_on.as_bool().unwrap_or(false), amount: dollars_from(&record.cap_usd) };
    let permissions = if record.permissions.as_str() == Some("ask") { PermissionMode::Ask } else { PermissionMode::AllowAndNote };
    Preferences {
        theme,
        text_size: TextSize::from_percent(record.text_size),
        reduce_motion: record.reduce_motion,
        open_on,
        run_defaults,
        cap,
        permissions,
        retention: retention_from(&record.keep_run_logs),
    }
}

fn model_from(value: &Value) -> ModelChoice {
    value.as_str().map_or(ModelChoice::Default, ModelChoice::from_alias)
}

fn dollars_from(value: &Value) -> Option<Dollars> {
    value.as_f64().and_then(Dollars::from_usd)
}

/// A row without a command or a usable budget is dropped, and so is a second row for the same command.
fn commands_from(value: &Value) -> Vec<CommandDefault> {
    let mut found: Vec<CommandDefault> = Vec::new();
    for row in value.as_array().into_iter().flatten() {
        let Some(command) = row.get("command").and_then(Value::as_str).filter(|command| !command.trim().is_empty()) else { continue };
        let Some(budget) = row.get("budget_usd").and_then(dollars_from) else { continue };
        if found.iter().any(|kept| kept.command == command) {
            continue;
        }
        found.push(CommandDefault { command: command.to_owned(), model: row.get("model").map_or(ModelChoice::Default, model_from), budget });
    }
    found
}

/// Only the two shorter times delete anything, so every other value, whatever its type, keeps the logs forever.
fn retention_from(value: &Value) -> Retention {
    match value.as_str() {
        Some("30_days") => Retention::Days30,
        Some("90_days") => Retention::Days90,
        _ => Retention::Forever,
    }
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
    let commands = preferences
        .run_defaults
        .commands
        .iter()
        .map(|row| json!({"command": row.command, "model": model_word(row.model), "budget_usd": number_from(Some(row.budget))}))
        .collect();
    Record {
        theme: theme.to_owned(),
        text_size: i64::from(preferences.text_size.percent()),
        reduce_motion: preferences.reduce_motion,
        open_on: open_on.to_owned(),
        model: Value::from(model_word(preferences.run_defaults.model)),
        budget_usd: number_from(preferences.run_defaults.budget),
        commands: Value::Array(commands),
        cap_on: Value::Bool(preferences.cap.on),
        cap_usd: number_from(preferences.cap.amount),
        permissions: Value::from(if preferences.permissions.asks() { "ask" } else { "allow" }),
        keep_run_logs: Value::from(match preferences.retention {
            Retention::Days30 => "30_days",
            Retention::Days90 => "90_days",
            Retention::Forever => "forever",
        }),
    }
}

fn model_word(model: ModelChoice) -> &'static str {
    model.alias().unwrap_or("default")
}

fn number_from(dollars: Option<Dollars>) -> Value {
    dollars.and_then(|sum| Number::from_f64(sum.usd())).map_or(Value::Null, Value::Number)
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
        let preferences = Preferences {
            theme: ThemeChoice::Dark,
            text_size: TextSize::from_percent(125),
            reduce_motion: true,
            open_on: StartScreen::Applications,
            ..Preferences::default()
        };
        file.save(&preferences).unwrap();
        assert_eq!(file.load().unwrap(), Some(preferences));
        assert!(!file.path.with_extension("json.tmp").exists());
    }

    fn dollars(text: &str) -> Dollars {
        Dollars::parse(text).unwrap()
    }

    fn row(command: &str, model: ModelChoice, budget: &str) -> CommandDefault {
        CommandDefault { command: command.to_owned(), model, budget: dollars(budget) }
    }

    fn every_key_set() -> Preferences {
        Preferences {
            theme: ThemeChoice::Light,
            text_size: TextSize::from_percent(110),
            reduce_motion: true,
            open_on: StartScreen::Tasks,
            run_defaults: RunDefaults {
                model: ModelChoice::Opus,
                budget: Some(dollars("4.5")),
                commands: vec![row("/scrape", ModelChoice::Haiku, "1"), row("/rank", ModelChoice::Default, "3.25")],
            },
            cap: SpendCap { on: true, amount: Some(dollars("60")) },
            permissions: PermissionMode::Ask,
            retention: Retention::Days90,
        }
    }

    #[test]
    fn every_key_round_trips() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        let preferences = every_key_set();
        file.save(&preferences).unwrap();
        assert_eq!(file.load().unwrap(), Some(preferences));
        for retention in Retention::ALL {
            let preferences = Preferences { retention, ..every_key_set() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
        for model in ModelChoice::ALL {
            let preferences = Preferences { run_defaults: RunDefaults { model, ..RunDefaults::default() }, ..Preferences::default() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
    }

    #[test]
    fn the_file_holds_the_documented_keys() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        file.save(&Preferences::default()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file.path).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "theme": "system", "text_size": 100, "reduce_motion": false, "open_on": "today",
                "model": "default", "budget_usd": null, "commands": [],
                "cap_on": false, "cap_usd": null, "permissions": "allow", "keep_run_logs": "forever"
            })
        );
        file.save(&every_key_set()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file.path).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "theme": "light", "text_size": 110, "reduce_motion": true, "open_on": "tasks",
                "model": "opus", "budget_usd": 4.5,
                "commands": [
                    {"command": "/scrape", "model": "haiku", "budget_usd": 1.0},
                    {"command": "/rank", "model": "default", "budget_usd": 3.25},
                ],
                "cap_on": true, "cap_usd": 60.0, "permissions": "ask", "keep_run_logs": "90_days"
            })
        );
    }

    fn retention_in(file: &PreferencesFile, field: &str) -> Retention {
        write(file, &format!(r#"{{"keep_run_logs": {field}}}"#));
        file.load().unwrap().unwrap().retention
    }

    #[test]
    fn only_the_two_shorter_times_delete_anything() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        assert_eq!(retention_in(&file, r#""30_days""#), Retention::Days30);
        assert_eq!(retention_in(&file, r#""90_days""#), Retention::Days90);
        assert_eq!(retention_in(&file, r#""forever""#), Retention::Forever);
    }

    #[test]
    fn an_unknown_retention_keeps_logs_forever() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for field in [
            r#""7_days""#,
            r#""30 days""#,
            r#""30_DAYS""#,
            r#" "30_days " "#,
            r#""""#,
            r#""never""#,
            "30",
            "90",
            "0",
            "-1",
            "30.5",
            "null",
            "true",
            "false",
            "[]",
            r#"["30_days"]"#,
            "{}",
            r#"{"days": 30}"#,
        ] {
            assert_eq!(retention_in(&file, field), Retention::Forever, "{field}");
        }
        write(&file, "{}");
        assert_eq!(file.load().unwrap().unwrap().retention, Retention::Forever, "a missing key keeps logs too");
    }

    #[test]
    fn a_bad_new_field_keeps_the_rest() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(
            &file,
            r#"{"theme": "dark", "text_size": 125, "reduce_motion": true, "open_on": "jobs",
                "model": 7, "budget_usd": "lots", "commands": "none",
                "cap_on": "yes", "cap_usd": [1], "permissions": {"mode": "ask"}, "keep_run_logs": 30}"#,
        );
        let preferences = file.load().unwrap().unwrap();
        assert_eq!(
            preferences,
            Preferences {
                theme: ThemeChoice::Dark,
                text_size: TextSize::from_percent(125),
                reduce_motion: true,
                open_on: StartScreen::Jobs,
                ..Preferences::default()
            }
        );
        write(&file, r#"{"theme": "dark", "model": null, "budget_usd": null, "commands": null, "cap_on": null, "cap_usd": null, "permissions": null}"#);
        assert_eq!(file.load().unwrap(), Some(Preferences { theme: ThemeChoice::Dark, ..Preferences::default() }));
    }

    #[test]
    fn a_bad_model_or_budget_falls_back() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, r#"{"model": "gpt-9", "budget_usd": 0}"#);
        assert_eq!(file.load().unwrap().unwrap().run_defaults, RunDefaults::default());
        for budget in ["-3", "0.001", "100000.5", "1e9", r#""3""#, "true"] {
            write(&file, &format!(r#"{{"budget_usd": {budget}, "cap_on": true, "cap_usd": {budget}}}"#));
            let preferences = file.load().unwrap().unwrap();
            assert_eq!((preferences.run_defaults.budget, preferences.cap.amount, preferences.cap.on), (None, None, true), "{budget}");
        }
        write(&file, r#"{"model": "sonnet", "budget_usd": 2.5, "cap_usd": 12}"#);
        let preferences = file.load().unwrap().unwrap();
        assert_eq!((preferences.run_defaults.model, preferences.run_defaults.budget), (ModelChoice::Sonnet, Some(dollars("2.50"))));
        assert_eq!(preferences.cap, SpendCap { on: false, amount: Some(dollars("12")) });
    }

    #[test]
    fn permissions_ask_only_on_ask() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for (field, expected) in [
            (r#""ask""#, PermissionMode::Ask),
            (r#""allow""#, PermissionMode::AllowAndNote),
            (r#""Ask""#, PermissionMode::AllowAndNote),
            ("true", PermissionMode::AllowAndNote),
        ] {
            write(&file, &format!(r#"{{"permissions": {field}}}"#));
            assert_eq!(file.load().unwrap().unwrap().permissions, expected, "{field}");
        }
    }

    #[test]
    fn bad_command_rows_are_dropped_and_the_first_duplicate_wins() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(
            &file,
            r#"{"commands": [
                {"command": "/scrape", "model": "haiku", "budget_usd": 1},
                {"command": "/scrape", "model": "opus", "budget_usd": 9},
                {"command": "/rank", "model": "sonnet"},
                {"command": "/apply", "model": "sonnet", "budget_usd": 0},
                {"command": "", "model": "opus", "budget_usd": 2},
                {"command": 5, "model": "opus", "budget_usd": 2},
                {"model": "opus", "budget_usd": 2},
                "/cv",
                null,
                {"command": "/cv", "model": "nonsense", "budget_usd": 2.5},
                {"command": "/followup", "budget_usd": 3}
            ]}"#,
        );
        let commands = file.load().unwrap().unwrap().run_defaults.commands;
        assert_eq!(
            commands,
            vec![row("/scrape", ModelChoice::Haiku, "1"), row("/cv", ModelChoice::Default, "2.5"), row("/followup", ModelChoice::Default, "3")]
        );
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
