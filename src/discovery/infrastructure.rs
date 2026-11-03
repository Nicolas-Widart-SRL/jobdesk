use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::discovery::domain::{CommuteTier, FetchFailure, FitLevel, Gate, Job, JobRepository, JobStatus, RankAssessment, Verdict};
use crate::shared_kernel::byte_order_mark::without_bom;
use crate::shared_kernel::dates::parse_iso_date;
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::{LoadError, WebUrl};

const SOURCE: &str = "job_scraper/seen_jobs.json";
const SKILLS: &str = ".claude/skills/job-application-assistant";
const CANDIDATE_PROFILE: &str = "01-candidate-profile.md";
const JOB_EVALUATION: &str = "04-job-evaluation.md";
const COMMUTE_LINE: &str = "Commute tiers";

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
        parse(without_bom(&text)).map_err(|error| LoadError::new(SOURCE, error))
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

/// What each profile file gave, or why it could not be read.
pub struct TermsRead {
    pub floor: Result<Option<u32>, LoadError>,
    /// The place the tiers are measured from, and the tiers.
    pub commute: Result<(Option<String>, Vec<CommuteTier>), LoadError>,
}

/// Reads each profile file once. A missing file only means the term is not stated, so it reads as empty.
/// A file that exists and cannot be read fails its own part, named by its bare file name.
pub fn read_your_terms(root: &Path) -> TermsRead {
    TermsRead { floor: read_part(root, CANDIDATE_PROFILE, floor_in), commute: read_part(root, JOB_EVALUATION, commute_tiers_in) }
}

fn read_part<T>(root: &Path, file: &str, parse: impl FnOnce(&str) -> T) -> Result<T, LoadError> {
    match fs::read_to_string(root.join(SKILLS).join(file)) {
        Ok(text) => Ok(parse(without_bom(&text))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(parse("")),
        Err(error) => Err(LoadError::new(file, error)),
    }
}

/// Reads `day-rate floor EUR 700` from anywhere in the profile.
fn floor_in(profile: &str) -> Option<u32> {
    profile.lines().find_map(|line| {
        let lowered = line.to_lowercase();
        let (before, after) = lowered.split_once("floor")?;
        let amount: String = after.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
        before.contains("rate").then(|| amount.parse().ok()).flatten()
    })
}

/// Reads the tiers line of the evaluation framework: `Commute tiers from Place (by car): **name** Place, Place (note); ...`.
/// Gives the origin, which is the text between the label and the colon, and the tiers.
fn commute_tiers_in(evaluation: &str) -> (Option<String>, Vec<CommuteTier>) {
    let Some(line) = evaluation.lines().find(|line| line.trim_start().starts_with(COMMUTE_LINE)) else { return (None, Vec::new()) };
    let Some((head, tiers)) = line.split_once(':') else { return (None, Vec::new()) };
    let origin = shown(head.trim_start().strip_prefix(COMMUTE_LINE).unwrap_or_default());
    ((!origin.is_empty()).then_some(origin), tiers.split(';').filter_map(commute_tier).collect())
}

/// A checkout's text is untrusted, so only what shows on screen is kept.
fn shown(text: &str) -> String {
    visible_chars(text).collect::<String>().trim().to_owned()
}

fn commute_tier(segment: &str) -> Option<CommuteTier> {
    let (_, rest) = segment.split_once("**")?;
    let (name, rest) = rest.split_once("**")?;
    let (places, note) = rest.split_once('(').unwrap_or((rest, ""));
    Some(CommuteTier {
        name: shown(name).to_lowercase(),
        places: places.split(',').map(shown).filter(|place| !place.is_empty()).collect(),
        note: shown(note.split(')').next().unwrap_or_default()),
    })
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
    fn reads_the_floor_from_the_profile_wording() {
        assert_eq!(floor_in("- **Status:** Freelance only, day-rate floor EUR 700."), Some(700));
        assert_eq!(floor_in("Day rate floor: €850 per day"), Some(850));
        assert_eq!(floor_in("Salary floor EUR 60000\nThe floor of the office"), None);
        assert_eq!(floor_in("Name: Sample"), None);
    }

    #[test]
    fn reads_commute_tiers() {
        let line = "Commute tiers from Denée (by car): **ideal** Namur, Dinant (<45 min); **borderline** Leuven (1h30-2h, only with hybrid); **too far** >2h unless fully remote.";
        let (origin, tiers) = commute_tiers_in(&format!("intro\n{line}\nafter"));
        assert_eq!(origin.as_deref(), Some("from Denée (by car)"));
        assert_eq!(tiers.len(), 3);
        assert_eq!(tiers[0], CommuteTier { name: "ideal".into(), places: vec!["Namur".into(), "Dinant".into()], note: "<45 min".into() });
        assert_eq!(tiers[1].note, "1h30-2h, only with hybrid");
    }

    #[test]
    fn a_blank_origin_is_none_and_hidden_characters_are_dropped() {
        assert_eq!(
            commute_tiers_in("Commute tiers: **ideal** Namur"),
            (None, vec![CommuteTier { name: "ideal".into(), places: vec!["Namur".into()], note: String::new() }])
        );
        let (origin, tiers) = commute_tiers_in("Commute tiers from Na\u{202e}mur: **id\u{202e}eal** Na\u{202e}mur (<45\u{202e} min)");
        assert_eq!(origin.as_deref(), Some("from Namur"));
        assert_eq!(tiers, vec![CommuteTier { name: "ideal".into(), places: vec!["Namur".into()], note: "<45 min".into() }]);
    }

    #[test]
    fn a_line_without_a_colon_has_no_origin_and_no_tiers() {
        assert_eq!(commute_tiers_in("Commute tiers not decided yet"), (None, vec![]));
    }

    #[test]
    fn a_place_of_hidden_characters_is_dropped() {
        let (_, tiers) = commute_tiers_in("Commute tiers: **ideal** Namur, \u{200b} , Dinant");
        assert_eq!(tiers[0].places, vec!["Namur".to_owned(), "Dinant".to_owned()]);
        let (_, tiers) = commute_tiers_in("Commute tiers: **ideal** , \u{200b} ,");
        assert!(tiers[0].places.is_empty());
    }

    #[test]
    fn a_missing_profile_reads_as_empty_and_an_unreadable_one_fails_by_its_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let read = read_your_terms(dir.path());
        assert_eq!((read.floor, read.commute), (Ok(None), Ok((None, vec![]))));
        let skills = dir.path().join(SKILLS);
        fs::create_dir_all(&skills).unwrap();
        fs::write(skills.join(CANDIDATE_PROFILE), [0xff, 0xfe]).unwrap();
        let read = read_your_terms(dir.path());
        assert_eq!(read.floor.unwrap_err().source, CANDIDATE_PROFILE);
        assert_eq!(read.commute, Ok((None, vec![])));
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
