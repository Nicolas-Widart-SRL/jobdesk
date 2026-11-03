use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};

use crate::settings::domain::{
    AutoCheck, ComingUp, CommandDefault, Dollars, JobFilters, KeyTest, LaterLasts, PermissionMode, Preferences, PreferencesStore, Retention, RunDefaults,
    SkipReasons, SpendCap, StartScreen, TextSize, ThemeChoice, TodayQueue,
};
use crate::shared_kernel::file_text::without_bom;
use crate::shared_kernel::{LoadError, ModelChoice, SignIn};

const FILE: &str = "settings.json";
/// How a key test's time is written: to the minute, in local time.
const TEST_TIME: &str = "%Y-%m-%dT%H:%M";

/// Every field may be missing, and an unknown field is ignored, so an older or newer file still loads.
/// The run defaults, cap, permissions, retention, sign-in, key test, queue and Jobs view arrive as raw JSON and are read leniently below:
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
    #[serde(default)]
    sign_in: Value,
    #[serde(default)]
    key_test: Value,
    #[serde(default)]
    coming_up_days: Value,
    #[serde(default)]
    later: Value,
    #[serde(default)]
    skip_reasons: Value,
    #[serde(default)]
    hide_below_floor: Value,
    #[serde(default)]
    hide_too_far: Value,
    #[serde(default)]
    hide_language: Value,
    #[serde(default)]
    hide_flagged: Value,
    #[serde(default)]
    hide_under_score: Value,
    #[serde(default)]
    score_floor: Value,
    #[serde(default)]
    check_updates: Value,
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
        crate::shared_kernel::replace_file::rename_over(&temporary, &self.path)?;
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
        let record: Record = serde_json::from_str(without_bom(&text)).map_err(|error| LoadError::new(FILE, error))?;
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
        sign_in: sign_in_from(&record.sign_in),
        key_test: key_test_from(&record.key_test),
        queue: TodayQueue {
            coming_up: coming_up_from(&record.coming_up_days),
            later: later_from(&record.later),
            skip_reasons: skip_reasons_from(&record.skip_reasons),
        },
        jobs_view: JobFilters {
            below_floor: flag_from(&record.hide_below_floor),
            too_far: flag_from(&record.hide_too_far),
            language: flag_from(&record.hide_language),
            flagged: flag_from(&record.hide_flagged),
            under_score: flag_from(&record.hide_under_score),
            score: score_from(&record.score_floor),
        },
        check_updates: check_updates_from(&record.check_updates),
    }
}

/// Only `false` switches it off, so a missing key or one of the wrong type reads as on.
fn check_updates_from(value: &Value) -> AutoCheck {
    AutoCheck::from_on(value.as_bool().unwrap_or(true))
}

/// Only a span the app offers counts, so any other value keeps the two weeks.
fn coming_up_from(value: &Value) -> ComingUp {
    value.as_i64().and_then(|days| ComingUp::ALL.into_iter().find(|span| span.days() == days)).unwrap_or_default()
}

fn later_from(value: &Value) -> LaterLasts {
    match value.as_str() {
        Some("3_days") => LaterLasts::ThreeDays,
        Some("monday") => LaterLasts::UntilMonday,
        _ => LaterLasts::Tomorrow,
    }
}

/// A list goes through the rules of typing. An empty list stays empty, because the user may have taken every reason out.
/// Anything that is not a list gives the defaults.
fn skip_reasons_from(value: &Value) -> SkipReasons {
    match value.as_array() {
        Some(items) => SkipReasons::from_saved(items.iter().filter_map(Value::as_str)),
        None => SkipReasons::default(),
    }
}

fn flag_from(value: &Value) -> bool {
    value.as_bool().unwrap_or(false)
}

fn score_from(value: &Value) -> u8 {
    value.as_i64().and_then(|score| u8::try_from(score).ok()).filter(|score| *score <= 100).unwrap_or(JobFilters::DEFAULT_SCORE)
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

/// Only `"api_key"` picks the key, so a missing or unknown word keeps tasks on the Claude plan.
fn sign_in_from(value: &Value) -> SignIn {
    if value.as_str() == Some("api_key") { SignIn::ApiKey } else { SignIn::ClaudeSignIn }
}

/// A test of the wrong shape reads as none. The file holds when the test ran and what it cost, never the key.
fn key_test_from(value: &Value) -> Option<KeyTest> {
    let passed = value.get("passed")?.as_bool()?;
    let at = NaiveDateTime::parse_from_str(value.get("at")?.as_str()?, TEST_TIME).ok()?;
    let cost_usd = value.get("cost_usd")?.as_f64().filter(|cost| cost.is_finite() && *cost >= 0.0)?;
    Some(KeyTest { passed, at, cost_usd })
}

fn key_test_value(test: &KeyTest) -> Value {
    json!({"passed": test.passed, "at": test.at.format(TEST_TIME).to_string(), "cost_usd": number_value(test.cost_usd)})
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
        sign_in: Value::from(match preferences.sign_in {
            SignIn::ClaudeSignIn => "claude",
            SignIn::ApiKey => "api_key",
        }),
        key_test: preferences.key_test.as_ref().map_or(Value::Null, key_test_value),
        coming_up_days: Value::from(preferences.queue.coming_up.days()),
        later: Value::from(match preferences.queue.later {
            LaterLasts::Tomorrow => "tomorrow",
            LaterLasts::ThreeDays => "3_days",
            LaterLasts::UntilMonday => "monday",
        }),
        skip_reasons: Value::from(preferences.queue.skip_reasons.list().to_vec()),
        hide_below_floor: Value::Bool(preferences.jobs_view.below_floor),
        hide_too_far: Value::Bool(preferences.jobs_view.too_far),
        hide_language: Value::Bool(preferences.jobs_view.language),
        hide_flagged: Value::Bool(preferences.jobs_view.flagged),
        hide_under_score: Value::Bool(preferences.jobs_view.under_score),
        score_floor: Value::from(preferences.jobs_view.score),
        check_updates: Value::Bool(preferences.check_updates.is_on()),
    }
}

fn model_word(model: ModelChoice) -> &'static str {
    model.alias().unwrap_or("default")
}

fn number_from(dollars: Option<Dollars>) -> Value {
    dollars.map_or(Value::Null, |sum| number_value(sum.usd()))
}

fn number_value(amount: f64) -> Value {
    Number::from_f64(amount).map_or(Value::Null, Value::Number)
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

    #[test]
    fn a_file_saved_with_a_byte_order_mark_still_loads() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, "\u{feff}{\"theme\": \"dark\", \"open_on\": \"applications\"}\r\n");
        let loaded = file.load().unwrap().unwrap();
        assert_eq!((loaded.theme, loaded.open_on), (ThemeChoice::Dark, StartScreen::Applications));
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
            sign_in: SignIn::ApiKey,
            key_test: Some(key_test(true, 3, 0.04)),
            queue: TodayQueue { coming_up: ComingUp::Month, later: LaterLasts::UntilMonday, skip_reasons: SkipReasons::from_saved(["Pay", "Culture"]) },
            jobs_view: JobFilters { below_floor: true, too_far: true, language: true, flagged: true, under_score: true, score: 82 },
            check_updates: AutoCheck::Off,
        }
    }

    fn key_test(passed: bool, minute: u32, cost_usd: f64) -> KeyTest {
        let at = chrono::NaiveDate::from_ymd_opt(2026, 3, 9).unwrap().and_hms_opt(14, minute, 0).unwrap();
        KeyTest { passed, at, cost_usd }
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
                "cap_on": false, "cap_usd": null, "permissions": "allow", "keep_run_logs": "forever",
                "sign_in": "claude", "key_test": null,
                "coming_up_days": 14, "later": "tomorrow", "skip_reasons": ["Rate", "Scope", "Domain", "Location"],
                "hide_below_floor": false, "hide_too_far": false, "hide_language": false, "hide_flagged": false,
                "hide_under_score": false, "score_floor": 60, "check_updates": true
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
                "cap_on": true, "cap_usd": 60.0, "permissions": "ask", "keep_run_logs": "90_days",
                "sign_in": "api_key", "key_test": {"passed": true, "at": "2026-03-09T14:03", "cost_usd": 0.04},
                "coming_up_days": 30, "later": "monday", "skip_reasons": ["Pay", "Culture"],
                "hide_below_floor": true, "hide_too_far": true, "hide_language": true, "hide_flagged": true,
                "hide_under_score": true, "score_floor": 82, "check_updates": false
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

    fn sign_in_in(file: &PreferencesFile, field: &str) -> SignIn {
        write(file, &format!(r#"{{"sign_in": {field}}}"#));
        file.load().unwrap().unwrap().sign_in
    }

    #[test]
    fn the_sign_in_round_trips_and_only_api_key_picks_the_key() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for sign_in in [SignIn::ClaudeSignIn, SignIn::ApiKey] {
            let preferences = Preferences { sign_in, ..Preferences::default() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
        assert_eq!(sign_in_in(&file, r#""api_key""#), SignIn::ApiKey);
        for field in [r#""claude""#, r#""API_KEY""#, r#""apikey""#, r#""key""#, r#""""#, "true", "1", "null", "[]", r#"{"api_key": true}"#] {
            assert_eq!(sign_in_in(&file, field), SignIn::ClaudeSignIn, "{field}");
        }
        write(&file, "{}");
        assert_eq!(file.load().unwrap().unwrap().sign_in, SignIn::ClaudeSignIn, "a missing key keeps the Claude plan");
    }

    #[test]
    fn a_key_test_round_trips() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for test in [key_test(true, 3, 0.04), key_test(false, 59, 0.0)] {
            let preferences = Preferences { key_test: Some(test), ..Preferences::default() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
    }

    #[test]
    fn a_key_test_of_the_wrong_shape_reads_as_none_and_keeps_the_rest() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for field in [
            "null",
            "true",
            "7",
            r#""passed""#,
            "[]",
            "{}",
            r#"{"passed": true, "at": "2026-03-09T14:03"}"#,
            r#"{"passed": true, "cost_usd": 0.04}"#,
            r#"{"at": "2026-03-09T14:03", "cost_usd": 0.04}"#,
            r#"{"passed": "yes", "at": "2026-03-09T14:03", "cost_usd": 0.04}"#,
            r#"{"passed": true, "at": "yesterday", "cost_usd": 0.04}"#,
            r#"{"passed": true, "at": "2026-03-09", "cost_usd": 0.04}"#,
            r#"{"passed": true, "at": "2026-13-09T14:03", "cost_usd": 0.04}"#,
            r#"{"passed": true, "at": 5, "cost_usd": 0.04}"#,
            r#"{"passed": true, "at": "2026-03-09T14:03", "cost_usd": "0.04"}"#,
            r#"{"passed": true, "at": "2026-03-09T14:03", "cost_usd": -1}"#,
            r#"{"passed": true, "at": "2026-03-09T14:03", "cost_usd": null}"#,
        ] {
            write(&file, &format!(r#"{{"theme": "dark", "sign_in": "api_key", "key_test": {field}}}"#));
            let preferences = file.load().unwrap().unwrap();
            assert_eq!(preferences.key_test, None, "{field}");
            assert_eq!((preferences.theme, preferences.sign_in), (ThemeChoice::Dark, SignIn::ApiKey), "{field}");
        }
        write(&file, "{}");
        assert_eq!(file.load().unwrap().unwrap().key_test, None);
        write(&file, r#"{"key_test": {"passed": false, "at": "2026-03-09T14:03", "cost_usd": 2, "note": "extra"}}"#);
        assert_eq!(file.load().unwrap().unwrap().key_test, Some(key_test(false, 3, 2.0)), "an extra field is ignored");
    }

    #[test]
    fn the_file_never_holds_the_key() {
        const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        let key = crate::shared_kernel::ApiKey::parse(FAKE).unwrap();
        let preferences = Preferences { sign_in: SignIn::ApiKey, key_test: Some(key_test(true, 3, 0.04)), ..Preferences::default() };
        // Nothing in a preference can hold the key, so the proof is in the text of the whole file.
        file.save(&preferences).unwrap();
        let text = fs::read_to_string(&file.path).unwrap();
        assert!(!text.contains(key.expose()) && !text.contains("sk-ant") && !text.contains("R2kd"), "{text}");
        assert!(text.contains("api_key"));
    }

    /// A settings file from before the queue and Jobs view keys.
    const PART_FOUR: &str = r#"{"theme": "dark", "text_size": 110, "reduce_motion": false, "open_on": "jobs",
        "model": "opus", "budget_usd": 4.5, "commands": [], "cap_on": false, "cap_usd": null,
        "permissions": "allow", "keep_run_logs": "forever", "sign_in": "claude", "key_test": null}"#;

    #[test]
    fn a_file_without_the_new_keys_loads_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, PART_FOUR);
        let loaded = file.load().unwrap().unwrap();
        assert_eq!((loaded.theme, loaded.open_on), (ThemeChoice::Dark, StartScreen::Jobs));
        assert_eq!(loaded.queue, TodayQueue::default());
        assert_eq!(loaded.jobs_view, JobFilters::default());
        assert_eq!(loaded.queue.coming_up, ComingUp::TwoWeeks);
        assert_eq!(loaded.queue.later, LaterLasts::Tomorrow);
        assert_eq!(loaded.queue.skip_reasons.list(), ["Rate", "Scope", "Domain", "Location"]);
        assert_eq!(loaded.jobs_view.score, 60);
    }

    fn loaded_with(file: &PreferencesFile, fields: &str) -> Preferences {
        write(file, &format!("{{{fields}}}"));
        file.load().unwrap().unwrap()
    }

    #[test]
    fn the_queue_and_jobs_view_round_trip() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for coming_up in ComingUp::ALL {
            for later in LaterLasts::ALL {
                let preferences = Preferences { queue: TodayQueue { coming_up, later, ..TodayQueue::default() }, ..Preferences::default() };
                file.save(&preferences).unwrap();
                assert_eq!(file.load().unwrap(), Some(preferences));
            }
        }
        for flags in 0..32u8 {
            let on = |bit: u8| flags & (1 << bit) != 0;
            let jobs_view = JobFilters { below_floor: on(0), too_far: on(1), language: on(2), flagged: on(3), under_score: on(4), score: flags * 3 };
            let preferences = Preferences { jobs_view, ..Preferences::default() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
        for score in [0, 100] {
            let preferences = Preferences { jobs_view: JobFilters { score, ..JobFilters::default() }, ..Preferences::default() };
            file.save(&preferences).unwrap();
            assert_eq!(file.load().unwrap(), Some(preferences));
        }
        let empty = Preferences { queue: TodayQueue { skip_reasons: SkipReasons::from_saved([]), ..TodayQueue::default() }, ..Preferences::default() };
        file.save(&empty).unwrap();
        assert_eq!(file.load().unwrap(), Some(empty), "a list the user emptied stays empty");
    }

    #[test]
    fn an_unknown_span_or_word_loads_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for coming_up in ["0", "8", "-7", "365", "7.5", r#""14""#, r#""week""#, "null", "true", "[]", "{}"] {
            let loaded = loaded_with(&file, &format!(r#""coming_up_days": {coming_up}"#));
            assert_eq!(loaded.queue.coming_up, ComingUp::TwoWeeks, "{coming_up}");
        }
        for later in [r#""Tomorrow""#, r#""monday ""#, r#""friday""#, r#""""#, "1", "3", "null", "false", "[]", "{}"] {
            let loaded = loaded_with(&file, &format!(r#""later": {later}"#));
            assert_eq!(loaded.queue.later, LaterLasts::Tomorrow, "{later}");
        }
        assert_eq!(loaded_with(&file, r#""coming_up_days": 7"#).queue.coming_up, ComingUp::Week);
        assert_eq!(loaded_with(&file, r#""later": "3_days""#).queue.later, LaterLasts::ThreeDays);
    }

    #[test]
    fn wrong_types_for_the_flags_and_the_score_load_the_defaults() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        for flag in [r#""true""#, "1", "0", "null", "[]", "{}", r#""yes""#] {
            let loaded = loaded_with(
                &file,
                &format!(r#""hide_below_floor": {flag}, "hide_too_far": {flag}, "hide_language": {flag}, "hide_flagged": {flag}, "hide_under_score": {flag}"#),
            );
            assert_eq!(loaded.jobs_view, JobFilters::default(), "{flag}");
        }
        for score in ["-1", "101", "256", "1000", "60.5", r#""80""#, "null", "true", "[]", "{}"] {
            let loaded = loaded_with(&file, &format!(r#""score_floor": {score}"#));
            assert_eq!(loaded.jobs_view.score, 60, "{score}");
        }
        assert_eq!(loaded_with(&file, r#""score_floor": 0"#).jobs_view.score, 0);
        assert_eq!(loaded_with(&file, r#""score_floor": 100"#).jobs_view.score, 100);
        assert!(loaded_with(&file, r#""hide_flagged": true"#).jobs_view.flagged);
    }

    #[test]
    fn the_update_switch_is_off_only_for_false() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        assert_eq!(loaded_with(&file, r#""check_updates": false"#).check_updates, AutoCheck::Off);
        assert_eq!(loaded_with(&file, r#""check_updates": true"#).check_updates, AutoCheck::On);
        assert_eq!(loaded_with(&file, "").check_updates, AutoCheck::On, "a missing key");
        for field in ["0", "1", "null", r#""false""#, r#""off""#, "[]", "{}", "[false]"] {
            let loaded = loaded_with(&file, &format!(r#""theme": "dark", "check_updates": {field}"#));
            assert_eq!((loaded.check_updates, loaded.theme), (AutoCheck::On, ThemeChoice::Dark), "{field}");
        }
    }

    #[test]
    fn the_skip_reasons_read_through_the_rules_of_typing() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        let defaults = ["Rate", "Scope", "Domain", "Location"];
        for field in ["null", r#""Rate""#, "7", "true", "{}", r#"{"0": "Rate"}"#] {
            let loaded = loaded_with(&file, &format!(r#""skip_reasons": {field}"#));
            assert_eq!(loaded.queue.skip_reasons.list(), defaults, "{field}");
        }
        assert_eq!(loaded_with(&file, "").queue.skip_reasons.list(), defaults, "a missing key");
        assert!(loaded_with(&file, r#""skip_reasons": []"#).queue.skip_reasons.list().is_empty(), "an empty list stays empty");
        let loaded = loaded_with(&file, r#""skip_reasons": ["  Pay  low ", "pay low", "", 5, null, "Fit", "FIT", "Gap"]"#);
        assert_eq!(loaded.queue.skip_reasons.list(), ["Pay low", "Fit", "Gap"]);
        let long = "z".repeat(41);
        let loaded = loaded_with(&file, &format!(r#""skip_reasons": ["1","2","3","4","5","6","7","8","9","{long}"]"#));
        assert_eq!(loaded.queue.skip_reasons.list(), ["1", "2", "3", "4", "5", "6", "7", "8"]);
    }

    #[test]
    fn a_bad_queue_key_keeps_the_rest() {
        let folder = tempfile::tempdir().unwrap();
        let file = file_in(&folder);
        write(&file, r#"{"theme": "dark", "coming_up_days": "soon", "later": 3, "skip_reasons": 1, "hide_flagged": true, "score_floor": "high"}"#);
        assert_eq!(
            file.load().unwrap(),
            Some(Preferences { theme: ThemeChoice::Dark, jobs_view: JobFilters { flagged: true, ..JobFilters::default() }, ..Preferences::default() })
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
