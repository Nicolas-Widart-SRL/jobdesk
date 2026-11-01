use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::intray::domain::{Decision, DecisionStore, Outcome, SkipReason};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::dates::parse_iso_date;

const FILE: &str = "decisions.json";

#[derive(Serialize, Deserialize)]
struct Record {
    key: String,
    name: String,
    outcome: String,
    reason: Option<String>,
    /// Absent in files from before a followed decision named its task, and left out when empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    on: String,
}

/// Decisions per workspace in one file of the app's data folder, readable only by the user.
pub struct DecisionFile {
    path: PathBuf,
    workspace: String,
}

impl DecisionFile {
    pub fn new(folder: PathBuf, workspace: &Path) -> Self {
        Self { path: folder.join(FILE), workspace: workspace.display().to_string() }
    }

    fn all(&self) -> Result<BTreeMap<String, Vec<Record>>, LoadError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|error| LoadError::new(FILE, error)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(LoadError::new(FILE, error)),
        }
    }

    /// Goes through a temporary file, so a crash leaves the previous decisions whole.
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

impl DecisionStore for DecisionFile {
    fn load(&self) -> Result<Vec<Decision>, LoadError> {
        let mut all = self.all()?;
        Ok(all.remove(&self.workspace).unwrap_or_default().into_iter().filter_map(decision_from).collect())
    }

    fn save(&self, decisions: &[Decision]) -> Result<(), LoadError> {
        let mut all = self.all()?;
        all.insert(self.workspace.clone(), decisions.iter().map(record_from).collect());
        let text = serde_json::to_string_pretty(&all).map_err(|error| LoadError::new(FILE, error))?;
        self.replace(&text).map_err(|error| LoadError::new(FILE, error))
    }
}

fn record_from(decision: &Decision) -> Record {
    let outcome = match decision.outcome {
        Outcome::Followed => "followed",
        Outcome::Later => "later",
        Outcome::Skipped => "skipped",
        Outcome::Kept => "kept",
    };
    Record {
        key: decision.key.clone(),
        name: decision.name.clone(),
        outcome: outcome.to_owned(),
        reason: decision.reason.map(|reason| reason.label().to_lowercase()),
        action: decision.action.clone(),
        on: decision.on.to_string(),
    }
}

/// A record this build cannot read is dropped from the tray, so its card simply returns.
fn decision_from(record: Record) -> Option<Decision> {
    let outcome = match record.outcome.as_str() {
        "followed" => Outcome::Followed,
        "later" => Outcome::Later,
        "skipped" => Outcome::Skipped,
        "kept" => Outcome::Kept,
        _ => return None,
    };
    let reason = record.reason.and_then(|raw| SkipReason::ALL.into_iter().find(|reason| reason.label().eq_ignore_ascii_case(&raw)));
    Some(Decision { key: record.key, name: record.name, outcome, reason, action: record.action, on: parse_iso_date(&record.on)? })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn decision(key: &str, outcome: Outcome, reason: Option<SkipReason>) -> Decision {
        Decision { key: key.into(), name: "Acme".into(), outcome, reason, action: None, on: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap() }
    }

    #[test]
    fn decisions_round_trip_per_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let first = DecisionFile::new(dir.path().join("intray"), Path::new("/one"));
        let second = DecisionFile::new(dir.path().join("intray"), Path::new("/two"));
        let saved = [decision("triage:a", Outcome::Skipped, Some(SkipReason::Rate)), decision("send:b", Outcome::Later, None)];

        first.save(&saved).unwrap();
        second.save(&saved[1..]).unwrap();

        assert_eq!(first.load().unwrap(), saved);
        assert_eq!(second.load().unwrap(), saved[1..]);
    }

    #[test]
    fn no_file_means_no_decisions() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(DecisionFile::new(dir.path().to_owned(), Path::new("/w")).load().unwrap(), vec![]);
    }

    #[test]
    fn a_corrupt_file_is_reported_and_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), "{").unwrap();
        let file = DecisionFile::new(dir.path().to_owned(), Path::new("/w"));
        assert_eq!(file.load().unwrap_err().source, FILE);
        assert!(file.save(&[]).is_err());
        assert_eq!(fs::read_to_string(dir.path().join(FILE)).unwrap(), "{");
    }

    #[test]
    fn an_unknown_outcome_is_dropped() {
        let record = Record { key: "k".into(), name: "n".into(), outcome: "maybe".into(), reason: None, action: None, on: "2026-10-31".into() };
        assert_eq!(decision_from(record), None);
    }

    #[test]
    fn a_file_from_before_the_action_field_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{"/w": [{"key": "send:a", "name": "Acme", "outcome": "followed", "reason": null, "on": "2026-10-31"}]}"#;
        fs::write(dir.path().join(FILE), old).unwrap();
        let loaded = DecisionFile::new(dir.path().to_owned(), Path::new("/w")).load().unwrap();
        assert_eq!(loaded, [decision("send:a", Outcome::Followed, None)]);
    }

    #[test]
    fn a_followed_decision_round_trips_its_action() {
        let dir = tempfile::tempdir().unwrap();
        let file = DecisionFile::new(dir.path().to_owned(), Path::new("/w"));
        let saved = [Decision { action: Some("Draft application".into()), ..decision("send:a", Outcome::Followed, None) }];
        file.save(&saved).unwrap();
        assert_eq!(file.load().unwrap(), saved);
    }

    #[test]
    fn a_record_without_action_writes_no_field() {
        let text = serde_json::to_string(&record_from(&decision("k", Outcome::Skipped, None))).unwrap();
        assert_eq!(text, r#"{"key":"k","name":"Acme","outcome":"skipped","reason":null,"on":"2026-10-31"}"#);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        DecisionFile::new(dir.path().to_owned(), Path::new("/w")).save(&[]).unwrap();
        assert_eq!(fs::metadata(dir.path().join(FILE)).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
