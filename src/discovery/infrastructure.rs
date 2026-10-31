use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::discovery::domain::{FetchFailure, FitLevel, Gate, Job, JobRepository, JobStatus, RankAssessment, Verdict};
use crate::shared_kernel::dates::parse_iso_date;
use crate::shared_kernel::{LoadError, WebUrl};

const SOURCE: &str = "job_scraper/seen_jobs.json";

/// `seen_jobs.json`. Old checkouts store the map bare, without the `seen` wrapper.
pub struct SeenJobsFile {
    path: PathBuf,
}

impl SeenJobsFile {
    pub fn in_workspace(root: &Path) -> Self {
        Self { path: root.join(SOURCE) }
    }
}

impl JobRepository for SeenJobsFile {
    fn all(&self) -> Result<Vec<Job>, LoadError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(LoadError::new(SOURCE, error)),
        };
        parse(&text).map_err(|error| LoadError::new(SOURCE, error))
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Document {
    Wrapped { seen: BTreeMap<String, Entry> },
    Bare(BTreeMap<String, Entry>),
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    title: Option<String>,
    company: Option<String>,
    url: Option<String>,
    location: Option<String>,
    portal: Option<String>,
    first_seen: Option<String>,
    posted_date: Option<String>,
    deadline: Option<String>,
    fit: Option<String>,
    status: Option<String>,
    rank_score: Option<f64>,
    rank_verdict: Option<String>,
    rank_date: Option<String>,
    location_verdict: Option<String>,
    language_gate: Option<String>,
    language_note: Option<String>,
    strengths: Vec<String>,
    gaps: Vec<String>,
    guard: Option<String>,
    fetch_attempts: Option<u32>,
    last_fetch_error: Option<String>,
}

fn parse(text: &str) -> serde_json::Result<Vec<Job>> {
    let entries = match serde_json::from_str(text)? {
        Document::Wrapped { seen } | Document::Bare(seen) => seen,
    };
    Ok(entries.into_iter().map(|(key, entry)| job_from(key, entry)).collect())
}

fn job_from(key: String, entry: Entry) -> Job {
    // Entries ranked before the rename keep a verdict in the scraper's `location` field.
    let legacy_verdict = entry.location.as_deref().and_then(Gate::parse);
    let location = entry.location.filter(|_| legacy_verdict.is_none());
    let location_gate = entry.location_verdict.as_deref().and_then(Gate::parse).or(legacy_verdict);

    let assessment = entry.rank_score.map(|raw| {
        let score = raw.clamp(0.0, 100.0).round() as u8;
        RankAssessment {
            score,
            verdict: entry.rank_verdict.as_deref().and_then(Verdict::parse).unwrap_or(Verdict::from_score(score)),
            location: location_gate.unwrap_or(Gate::Pass),
            language: entry.language_gate.as_deref().and_then(Gate::parse).unwrap_or(Gate::Pass),
            language_note: entry.language_note,
            strengths: entry.strengths,
            gaps: entry.gaps,
            ranked_on: entry.rank_date.as_deref().and_then(parse_iso_date),
        }
    });
    let fetch_failure = entry.fetch_attempts.map(|attempts| FetchFailure { attempts, last_error: entry.last_fetch_error });

    Job {
        title: entry.title.unwrap_or_else(|| key.clone()),
        company: entry.company.unwrap_or_default(),
        location,
        url: entry.url.as_deref().and_then(WebUrl::parse),
        portal: entry.portal,
        first_seen: entry.first_seen.as_deref().and_then(parse_iso_date),
        posted_on: entry.posted_date.as_deref().and_then(parse_iso_date),
        deadline: entry.deadline.as_deref().and_then(parse_iso_date),
        scraper_fit: entry.fit.as_deref().and_then(FitLevel::parse),
        status: JobStatus::parse(entry.status.as_deref().unwrap_or("new")),
        assessment,
        fetch_failure,
        guard: entry.guard,
        key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wrapped_and_bare_documents() {
        let wrapped = parse(r#"{"seen":{"a":{"title":"T","status":"new"}}}"#).unwrap();
        let bare = parse(r#"{"a":{"title":"T","status":"new"}}"#).unwrap();
        assert_eq!(wrapped, bare);
        assert_eq!(wrapped[0].key, "a");
    }

    #[test]
    fn legacy_location_verdict_moves_to_the_gate() {
        let jobs = parse(r#"{"seen":{"a":{"status":"ranked","rank_score":70,"location":"FAIL"}}}"#).unwrap();
        assert_eq!(jobs[0].location, None);
        assert_eq!(jobs[0].assessment.as_ref().unwrap().location, Gate::Fail);
    }

    #[test]
    fn keeps_a_real_place_and_ignores_bad_dates() {
        let jobs = parse(r#"{"seen":{"a":{"location":"Namur","deadline":"ASAP","url":"file:///x","rank_score":71.6,"rank_verdict":"odd"}}}"#).unwrap();
        assert_eq!(jobs[0].location.as_deref(), Some("Namur"));
        assert_eq!(jobs[0].deadline, None);
        assert_eq!(jobs[0].url, None);
        let assessment = jobs[0].assessment.as_ref().unwrap();
        assert_eq!((assessment.score, assessment.verdict), (72, Verdict::Good));
    }

    #[test]
    fn missing_file_is_an_empty_board() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(SeenJobsFile::in_workspace(dir.path()).all().unwrap(), vec![]);
    }

    #[test]
    fn malformed_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("job_scraper")).unwrap();
        fs::write(dir.path().join(SOURCE), "{").unwrap();
        assert_eq!(SeenJobsFile::in_workspace(dir.path()).all().unwrap_err().source, SOURCE);
    }
}
