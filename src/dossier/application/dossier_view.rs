use crate::dossier::domain::{ArchiveContents, RunFindings, extract_findings};
use crate::runs::application::Run;
use crate::tracking::application::ApplicationCard;

/// What this context needs from a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunTranscript {
    pub line: String,
    pub started_at: String,
    pub files_changed: Vec<String>,
    pub replies: Vec<String>,
}

impl From<&Run> for RunTranscript {
    fn from(run: &Run) -> Self {
        Self { line: run.line.clone(), started_at: run.started_at.clone(), files_changed: run.files_changed.clone(), replies: run.replies() }
    }
}

impl RunTranscript {
    fn belongs_to(&self, card: &ApplicationCard) -> bool {
        let is_apply = self.line.starts_with("/apply");
        let same_posting = card.source.as_ref().is_some_and(|source| self.line.contains(source.as_str()));
        let wrote_its_files = [&card.cv_file, &card.cover_letter_file]
            .iter()
            .any(|file| !file.is_empty() && self.files_changed.iter().any(|changed| changed.ends_with(file.as_str())));
        is_apply && (same_posting || wrote_its_files)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dossier {
    pub card: ApplicationCard,
    pub contents: ArchiveContents,
    pub findings: RunFindings,
    pub findings_from: Option<String>,
}

/// `runs` is newest first. The newest `/apply` of this application supplies the findings.
pub fn compose_dossier(card: &ApplicationCard, contents: &ArchiveContents, runs: &[RunTranscript]) -> Dossier {
    let source = runs.iter().find(|run| run.belongs_to(card));
    Dossier {
        card: card.clone(),
        contents: contents.clone(),
        findings: source.map(|run| extract_findings(&run.replies)).unwrap_or_default(),
        findings_from: source.map(|run| run.started_at.clone()),
    }
}
