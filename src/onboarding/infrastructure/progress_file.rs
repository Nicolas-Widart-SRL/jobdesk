use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::onboarding::domain::{Progress, ProgressStore, Step};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::dates::parse_iso_date;

pub const FILE: &str = "onboarding.json";
/// A record larger than this is not one Jobdesk wrote, and is read as no record.
const MAX_SIZE: u64 = 64 * 1024;

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
    #[serde(default)]
    without_pdfs: bool,
}

/// The progress record, `onboarding.json` in the app's data folder, readable only by the user.
pub struct ProgressFile {
    path: PathBuf,
}

impl ProgressFile {
    pub fn new(data: &Path) -> Self {
        Self { path: data.join(FILE) }
    }

    /// Goes through a temporary file, so a crash leaves the previous record whole.
    fn replace(&self, text: &str) -> std::io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        let _ = fs::remove_file(&temporary);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        options.open(&temporary)?.write_all(text.as_bytes())?;
        crate::shared_kernel::replace_file::rename_over(&temporary, &self.path)
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

    fn save(&self, progress: &Progress) -> Result<(), LoadError> {
        let text = serde_json::to_string_pretty(&record_from(progress)).map_err(|error| LoadError::new(FILE, error))?;
        self.replace(&text).map_err(|error| LoadError::new(FILE, error))
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
    Record {
        folder: progress.folder.clone(),
        done: progress.done.iter().filter_map(|step| STEPS.iter().find(|(known, _)| known == step)).map(|(_, word)| (*word).to_owned()).collect(),
        version: progress.version.clone(),
        stopped: progress.stopped.map(|day| day.to_string()),
        without_pdfs: progress.without_pdfs,
    }
}

/// A step word this build does not know is dropped, so that step is simply done again.
fn progress_from(record: Record) -> Progress {
    Progress {
        folder: record.folder,
        done: record.done.iter().filter_map(|word| STEPS.iter().find(|(_, known)| known == word)).map(|(step, _)| *step).collect(),
        version: record.version,
        stopped: record.stopped.as_deref().and_then(parse_iso_date),
        without_pdfs: record.without_pdfs,
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
            without_pdfs: true,
        }
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
        file.clear().unwrap();
        assert_eq!(file.load().unwrap(), None);
        file.clear().unwrap();
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
        fs::write(data.path().join(FILE), r#"{"done": ["folder", "tea"], "run": "r1", "without_pdfs": false}"#).unwrap();
        let read = ProgressFile::new(data.path()).load().unwrap().unwrap();
        assert_eq!(read, Progress { done: [Step::Folder].into(), ..Progress::default() });
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
