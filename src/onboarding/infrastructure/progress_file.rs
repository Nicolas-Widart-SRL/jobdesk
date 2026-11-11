use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::onboarding::domain::{Draft, Progress, ProgressStore, Step};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::dates::parse_iso_date;

pub const FILE: &str = "onboarding.json";
/// A record larger than this is not one Jobdesk wrote, and is read as no record.
const MAX_SIZE: u64 = 64 * 1024;

/// The fields of the Profile chapter are read one by one: a record from the first build has none of them, and one from
/// a later build may give one a shape this build does not know. Either way only that field is dropped, never the record.
#[derive(Serialize, Deserialize, Default)]
struct Record {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    folder: Option<PathBuf>,
    #[serde(default)]
    done: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stopped: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    run: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    path_a_sent: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    draft: Value,
}

/// A draft as the record keeps it.
#[derive(Serialize, Deserialize)]
struct DraftRecord {
    run: String,
    turn: usize,
    #[serde(default)]
    answers: Vec<AnswerRecord>,
    #[serde(default)]
    text: String,
}

#[derive(Serialize, Deserialize)]
struct AnswerRecord {
    number: u32,
    answer: String,
}

/// The progress record, `onboarding.json` in the app's data folder, readable only by the user.
pub struct ProgressFile {
    path: PathBuf,
}

impl ProgressFile {
    pub fn new(data: &Path) -> Self {
        Self { path: data.join(FILE) }
    }

    /// Goes through a temporary file with a new name each time, on disk before the rename, so a crash or a power loss
    /// leaves the previous record whole, and two Jobdesks saving at once never write into or remove each other's.
    fn replace(&self, text: &str) -> std::io::Result<()> {
        let folder = self.path.parent().filter(|folder| !folder.as_os_str().is_empty()).unwrap_or(Path::new("."));
        fs::create_dir_all(folder)?;
        // Readable only by the user, and deleted when anything below fails.
        let mut temporary = tempfile::Builder::new().prefix(".onboarding.json.").suffix(".tmp").tempfile_in(folder)?;
        temporary.write_all(text.as_bytes())?;
        // On disk before the rename, or a power loss could keep the rename and lose the data.
        temporary.as_file().sync_all()?;
        let temporary = temporary.into_temp_path();
        crate::shared_kernel::replace_file::rename_over(&temporary, &self.path)?;
        // The rename took the name, so nothing is left to delete.
        let _ = temporary.keep();
        // The rename lives in the folder, which a crash could still lose. Windows cannot open a folder this way.
        // Best effort: some mounts refuse to sync a folder, and the file is already in place.
        #[cfg(unix)]
        let _ = fs::File::open(folder).and_then(|folder| folder.sync_all());
        Ok(())
    }
}

impl ProgressStore for ProgressFile {
    fn load(&self) -> Result<Option<Progress>, LoadError> {
        let meta = match fs::symlink_metadata(&self.path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(LoadError::new(FILE, error)),
        };
        if !meta.is_file() || meta.len() > MAX_SIZE {
            return Err(LoadError::new(FILE, "not a record Jobdesk wrote"));
        }
        let text = fs::read_to_string(&self.path).map_err(|error| LoadError::new(FILE, error))?;
        let record: Record = serde_json::from_str(&text).map_err(|error| LoadError::new(FILE, error))?;
        Ok(Some(progress_from(record)))
    }

    /// A draft so long that the record would pass the size it is read back at is left out of the file, and stays only
    /// in memory: a record that no longer reads would lose every step done.
    fn save(&self, progress: &Progress) -> Result<(), LoadError> {
        let text = |record: &Record| serde_json::to_string_pretty(record).map_err(|error| LoadError::new(FILE, error));
        let mut record = record_from(progress);
        let mut written = text(&record)?;
        if written.len() as u64 > MAX_SIZE {
            record.draft = Value::Null;
            written = text(&record)?;
        }
        self.replace(&written).map_err(|error| LoadError::new(FILE, error))
    }

    fn clear(&self) -> Result<(), LoadError> {
        match fs::remove_file(&self.path) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(LoadError::new(FILE, error)),
            _ => Ok(()),
        }
    }
}

const STEPS: [(Step, &str); 3] = [(Step::Folder, "folder"), (Step::Computer, "computer"), (Step::Profile, "profile")];

fn record_from(progress: &Progress) -> Record {
    let draft = progress.draft.as_ref().map(|draft| DraftRecord {
        run: draft.run.clone(),
        turn: draft.turn,
        answers: draft.answers.iter().map(|(number, answer)| AnswerRecord { number: *number, answer: answer.clone() }).collect(),
        text: draft.text.clone(),
    });
    Record {
        folder: progress.folder.clone(),
        done: progress.done.iter().filter_map(|step| STEPS.iter().find(|(known, _)| known == step)).map(|(_, word)| (*word).to_owned()).collect(),
        version: progress.version.clone(),
        stopped: progress.stopped.map(|day| day.to_string()),
        run: progress.run.clone().map_or(Value::Null, Value::String),
        // Left out until sent, as a record from before the Profile chapter has it.
        path_a_sent: if progress.path_a_sent { Value::Bool(true) } else { Value::Null },
        draft: draft.and_then(|draft| serde_json::to_value(draft).ok()).unwrap_or(Value::Null),
    }
}

/// A step word this build does not know is dropped, so that step is simply done again. A Profile chapter field of a
/// shape this build does not know reads as absent.
fn progress_from(record: Record) -> Progress {
    let draft = serde_json::from_value::<DraftRecord>(record.draft).ok().map(|draft| Draft {
        run: draft.run,
        turn: draft.turn,
        answers: draft.answers.into_iter().map(|answer| (answer.number, answer.answer)).collect(),
        text: draft.text,
    });
    Progress {
        folder: record.folder,
        done: record.done.iter().filter_map(|word| STEPS.iter().find(|(_, known)| known == word)).map(|(step, _)| *step).collect(),
        version: record.version,
        stopped: record.stopped.as_deref().and_then(parse_iso_date),
        run: record.run.as_str().map(str::to_owned),
        path_a_sent: record.path_a_sent.as_bool().unwrap_or(false),
        draft,
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn progress() -> Progress {
        Progress {
            folder: Some(PathBuf::from("/Users/sam/Documents/Job search")),
            done: [Step::Folder, Step::Computer].into(),
            version: Some("1.7.5".into()),
            stopped: NaiveDate::from_ymd_opt(2026, 11, 10),
            ..Progress::default()
        }
    }

    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn the_record_reads_back_after_a_write() {
        let data = tempfile::tempdir().unwrap();
        let file = ProgressFile::new(data.path());
        assert_eq!(file.load().unwrap(), None, "no record before setup starts");
        file.save(&progress()).unwrap();
        assert_eq!(file.load().unwrap(), Some(progress()));
        file.save(&Progress::default()).unwrap();
        assert_eq!(file.load().unwrap(), Some(Progress::default()), "a second write replaces the first");
        assert_eq!(names(data.path()), [FILE], "no temporary file is left");
        file.clear().unwrap();
        assert_eq!(file.load().unwrap(), None);
        file.clear().unwrap();
    }

    #[test]
    fn a_save_leaves_another_jobdesks_temporary_file_alone() {
        let data = tempfile::tempdir().unwrap();
        // What another Jobdesk could be writing at the same moment, under a name of its own.
        let theirs = data.path().join(".onboarding.json.other.tmp");
        fs::write(&theirs, "half").unwrap();
        ProgressFile::new(data.path()).save(&progress()).unwrap();
        assert_eq!(fs::read_to_string(&theirs).unwrap(), "half");
        assert_eq!(ProgressFile::new(data.path()).load().unwrap(), Some(progress()));
    }

    #[cfg(unix)]
    #[test]
    fn the_record_is_readable_only_by_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let data = tempfile::tempdir().unwrap();
        ProgressFile::new(data.path()).save(&progress()).unwrap();
        assert_eq!(fs::metadata(data.path().join(FILE)).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn an_unknown_step_and_fields_from_a_later_build_are_left_out() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join(FILE), r#"{"done": ["folder", "tea"], "first_search": "r9", "without_pdfs": false}"#).unwrap();
        let read = ProgressFile::new(data.path()).load().unwrap().unwrap();
        assert_eq!(read, Progress { done: [Step::Folder].into(), ..Progress::default() });
    }

    fn profile_progress() -> Progress {
        let draft = Draft {
            run: "20261111-093000-1".into(),
            turn: 3,
            answers: vec![(1, "Design Lead".into()), (2, "The whole studio,\nnot only my team".into()), (4, String::new())],
            text: "Leave out the Lowmere years.".into(),
        };
        Progress { run: Some("20261111-093000-1".into()), path_a_sent: true, draft: Some(draft), ..progress() }
    }

    #[test]
    fn a_draft_survives_a_record_round_trip() {
        let data = tempfile::tempdir().unwrap();
        let file = ProgressFile::new(data.path());
        file.save(&profile_progress()).unwrap();
        assert_eq!(file.load().unwrap(), Some(profile_progress()));
        let unsent = Progress { path_a_sent: false, draft: None, ..profile_progress() };
        file.save(&unsent).unwrap();
        assert_eq!(file.load().unwrap(), Some(unsent));
    }

    #[test]
    fn a_record_from_before_the_profile_chapter_reads_with_nothing_of_it() {
        let data = tempfile::tempdir().unwrap();
        let first_build = r#"{"folder": "/Users/sam/Documents/Job search", "done": ["folder", "computer"], "version": "1.7.5", "stopped": "2026-11-10"}"#;
        fs::write(data.path().join(FILE), first_build).unwrap();
        let read = ProgressFile::new(data.path()).load().unwrap().unwrap();
        assert_eq!(read, progress());
        assert_eq!((read.run, read.path_a_sent, read.draft), (None, false, None));
    }

    #[test]
    fn a_profile_field_of_an_unknown_shape_drops_only_that_field() {
        let data = tempfile::tempdir().unwrap();
        let later = r#"{"done": ["folder"], "run": {"id": "r1"}, "path_a_sent": "yes", "draft": [{"turn": "three"}]}"#;
        fs::write(data.path().join(FILE), later).unwrap();
        let read = ProgressFile::new(data.path()).load().unwrap().unwrap();
        assert_eq!(read, Progress { done: [Step::Folder].into(), ..Progress::default() });
        let partial = r#"{"run": "r1", "path_a_sent": true, "draft": {"run": "r1", "turn": 2, "answers": [{"number": 1, "answer": "a", "kind": "new"}]}}"#;
        fs::write(data.path().join(FILE), partial).unwrap();
        let read = ProgressFile::new(data.path()).load().unwrap().unwrap();
        assert_eq!(
            read.draft,
            Some(Draft { run: "r1".into(), turn: 2, answers: vec![(1, "a".into())], text: String::new() }),
            "an unknown field inside is left out"
        );
        assert!(read.path_a_sent && read.run.as_deref() == Some("r1"));
    }

    #[test]
    fn a_draft_too_long_for_the_record_stays_out_of_it() {
        let data = tempfile::tempdir().unwrap();
        let file = ProgressFile::new(data.path());
        let long = Draft { run: "r1".into(), turn: 2, answers: vec![(1, "a".repeat(MAX_SIZE as usize))], text: String::new() };
        let progress = Progress { run: Some("r1".into()), draft: Some(long), ..progress() };
        file.save(&progress).unwrap();
        assert_eq!(file.load().unwrap(), Some(Progress { draft: None, ..progress }), "every step done still reads");
    }

    #[test]
    fn a_record_that_is_not_one_jobdesk_wrote_is_an_error() {
        let data = tempfile::tempdir().unwrap();
        let file = ProgressFile::new(data.path());
        fs::write(data.path().join(FILE), "not json").unwrap();
        assert!(file.load().is_err());
        fs::write(data.path().join(FILE), " ".repeat(MAX_SIZE as usize + 1)).unwrap();
        assert!(file.load().is_err());
    }
}
