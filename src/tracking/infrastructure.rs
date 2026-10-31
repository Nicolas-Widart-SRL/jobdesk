use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::shared_kernel::dates::{iso_dates_in, parse_iso_date};
use crate::shared_kernel::{LoadError, WebUrl};
use crate::tracking::domain::{
    AppliedOn, ApplicationRepository, ApplicationStatus, OutcomeRecord, Stage, TrackedApplication, archive_folder,
};

const TRACKER: &str = "job_search_tracker.csv";

/// The tracker CSV joined with each application's `outcome.md`.
pub struct TrackerFiles {
    root: PathBuf,
}

impl TrackerFiles {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    fn outcome_of(&self, row: &Row) -> Option<OutcomeRecord> {
        let folder = archive_folder(&row.company, &row.role)?;
        let path = self.root.join("documents/applications").join(folder).join("outcome.md");
        fs::read_to_string(path).ok().map(|text| parse_outcome(&text))
    }
}

impl ApplicationRepository for TrackerFiles {
    fn all(&self) -> Result<Vec<TrackedApplication>, LoadError> {
        let bytes = match fs::read(self.root.join(TRACKER)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LoadError::new(TRACKER, error)),
        };
        let text = String::from_utf8(bytes).map_err(|_| LoadError::new(TRACKER, "not UTF-8 text, re-save it as CSV UTF-8"))?;
        let rows = parse_rows(&text).map_err(|error| LoadError::new(TRACKER, error))?;
        Ok(rows
            .into_iter()
            .filter(|row| !row.company.trim().is_empty())
            .map(|row| {
                let outcome = self.outcome_of(&row);
                application_from(row, outcome)
            })
            .collect())
    }
}

#[derive(Default)]
struct Row {
    date: String,
    company: String,
    sector: String,
    role: String,
    channel: String,
    status: String,
    contact_person: String,
    fit_rating: String,
    notes: String,
    source: String,
    deadline: String,
}

/// Columns are read by name. Rows from before `deadline`, or cut short, read as empty cells.
fn parse_rows(text: &str) -> csv::Result<Vec<Row>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(text.as_bytes());
    let headers = reader.headers()?.clone();
    reader
        .records()
        .map(|record| {
            let record = record?;
            let cell = |name: &str| headers.iter().position(|header| header.trim() == name).and_then(|at| record.get(at)).unwrap_or_default().to_owned();
            Ok(Row {
                date: cell("date"),
                company: cell("company"),
                sector: cell("sector"),
                role: cell("role"),
                channel: cell("channel"),
                status: cell("status"),
                contact_person: cell("contact_person"),
                fit_rating: cell("fit_rating"),
                notes: cell("notes"),
                source: cell("source"),
                deadline: cell("deadline"),
            })
        })
        .collect()
}

fn application_from(row: Row, outcome: Option<OutcomeRecord>) -> TrackedApplication {
    TrackedApplication {
        status: ApplicationStatus::parse(&row.status),
        applied_on: AppliedOn::parse(&row.date),
        deadline: parse_iso_date(&row.deadline),
        fit_rating: row.fit_rating.trim().parse::<f32>().ok().map(|fit| fit.clamp(0.0, 100.0).round() as u8),
        source: WebUrl::parse(&row.source),
        company: row.company.trim().to_owned(),
        role: row.role.trim().to_owned(),
        sector: row.sector.trim().to_owned(),
        channel: row.channel.trim().to_owned(),
        contact_person: row.contact_person.trim().to_owned(),
        notes: row.notes,
        outcome,
    }
}

fn parse_outcome(text: &str) -> OutcomeRecord {
    let mut record = OutcomeRecord::default();
    let mut section = "";
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix("## ") {
            section = heading.trim();
        } else if let Some(value) = trimmed.strip_prefix("**Status:**") {
            record.status = Some(value.trim().to_owned());
        } else if let Some(value) = trimmed.strip_prefix("**Date resolved:**") {
            record.resolved_on = iso_dates_in(value).first().copied();
        } else if section == "Interview stages reached" {
            record.stages.extend(parse_stage(trimmed));
        } else if section == "Notes" {
            record.notes.push_str(line);
            record.notes.push('\n');
        }
    }
    record.notes = record.notes.trim().to_owned();
    record
}

fn parse_stage(line: &str) -> Option<Stage> {
    let (reached, rest) = line
        .strip_prefix("- [x]")
        .or_else(|| line.strip_prefix("- [X]"))
        .map(|rest| (true, rest))
        .or_else(|| line.strip_prefix("- [ ]").map(|rest| (false, rest)))?;
    let name = rest.split('(').next().unwrap_or(rest).trim().to_owned();
    Some(Stage { name, reached, date: iso_dates_in(rest).first().copied() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "date,company,sector,role,role_type,channel,status,contact_person,fit_rating,notes,cv_file,cover_letter_file,source,deadline\n";

    #[test]
    fn missing_tracker_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(TrackerFiles::in_workspace(dir.path()).all().unwrap(), vec![]);
    }

    #[test]
    fn reads_legacy_thirteen_column_rows_and_a_bom() {
        let legacy = "\u{feff}date,company,sector,role,role_type,channel,status,contact_person,fit_rating,notes,cv_file,cover_letter_file,source\n2026-01-02,Acme,Tech,Lead,,portal,no response,,72,,,,https://acme.example/1\n";
        let row = &parse_rows(legacy).unwrap()[0];
        assert_eq!((row.company.as_str(), row.deadline.as_str()), ("Acme", ""));
    }

    #[test]
    fn joins_the_outcome_file_and_keeps_short_rows() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("documents/applications/acme_lead");
        fs::create_dir_all(&archive).unwrap();
        fs::write(archive.join("outcome.md"), "**Status:** rejected\n\n## Interview stages reached\n- [x] Phone screen (2026-02-01)\n- [ ] Offer received\n\n## Notes\nThey liked the replay story\n").unwrap();
        fs::write(dir.path().join(TRACKER), format!("{HEADER}2026-01-02,Acme,Tech,Lead,,portal,rejected,,not a number\n")).unwrap();

        let applications = TrackerFiles::in_workspace(dir.path()).all().unwrap();

        let outcome = applications[0].outcome.as_ref().unwrap();
        assert!(outcome.reached_interview() && !outcome.reached_offer());
        assert_eq!(outcome.stages[0].date, parse_iso_date("2026-02-01"));
        assert_eq!(outcome.notes, "They liked the replay story");
        assert_eq!(applications[0].fit_rating, None);
    }

    #[test]
    fn non_utf8_tracker_says_how_to_fix_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(TRACKER), b"date,company\n2026,Caf\xe9\n").unwrap();
        let error = TrackerFiles::in_workspace(dir.path()).all().unwrap_err();
        assert!(error.reason.contains("CSV UTF-8"));
    }
}
