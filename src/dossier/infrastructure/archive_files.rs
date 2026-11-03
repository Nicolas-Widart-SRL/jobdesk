use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::NaiveDate;

use crate::dossier::domain::{ApplicationRef, ArchiveContents, ArchiveRepository, NamedText, PdfDocument, PostingText, ResearchNote, within_workspace};
use crate::shared_kernel::byte_order_mark::without_bom;
use crate::shared_kernel::dates::{days_from, parse_iso_date};

const GUARD_HEADING: &str = "## posting_guard";
const RESEARCH_TTL_DAYS: i64 = 30;

pub struct ArchiveFiles {
    root: PathBuf,
}

impl ArchiveFiles {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    fn document(&self, label: &str, stored_source: &str, conventional: String) -> Option<PdfDocument> {
        let from_tracker = within_workspace(&self.root, stored_source).map(|path| path.with_extension("pdf"));
        let path = from_tracker.filter(|path| path.is_file()).or_else(|| Some(self.root.join(conventional)).filter(|path| path.is_file()))?;
        let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some(PdfDocument { label: label.to_owned(), path, modified })
    }

    fn research(&self, company: &str, today: NaiveDate) -> Option<ResearchNote> {
        let name = company.trim().to_lowercase().replace(' ', "-");
        let text = within_workspace(&self.root, &format!("company_research/{name}.json"))
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| without_bom(&text).to_owned())?;
        let value: Option<serde_json::Value> = serde_json::from_str(&text).ok();
        let fetched_on = value.as_ref().and_then(|value| value.get("fetched_date")).and_then(|date| date.as_str()).and_then(parse_iso_date);
        let pretty = value.and_then(|value| serde_json::to_string_pretty(&value).ok()).unwrap_or(text);
        Some(ResearchNote { fetched_on, is_stale: fetched_on.is_none_or(|date| days_from(date, today) > RESEARCH_TTL_DAYS), text: pretty })
    }
}

impl ArchiveRepository for ArchiveFiles {
    fn load(&self, application: &ApplicationRef, today: NaiveDate) -> ArchiveContents {
        let folder = self.root.join("documents/applications").join(&application.folder);
        let cv = self.document("CV", &application.cv_file, format!("cv/main_{}.pdf", application.folder));
        let letter = self.document("Cover letter", &application.cover_letter_file, format!("cover_letters/cover_{}.pdf", application.folder));
        ArchiveContents {
            posting: fs::read_to_string(folder.join("job_posting.md")).ok().map(|text| split_posting(&shown_text(&text))),
            prep_packs: texts_named(&folder, "interview_prep_"),
            follow_ups: texts_named(&folder, "followup_"),
            research: self.research(&application.company, today),
            documents: cv.into_iter().chain(letter).collect(),
        }
    }
}

/// An archived text as the dossier shows it: no byte order mark, and LF line endings, which every label lays out.
fn shown_text(text: &str) -> String {
    without_bom(text).replace("\r\n", "\n")
}

fn split_posting(text: &str) -> PostingText {
    match text.split_once(GUARD_HEADING) {
        Some((posting, guard)) => PostingText { text: posting.trim().to_owned(), guard: guard.trim().lines().next().map(str::to_owned) },
        None => PostingText { text: text.trim().to_owned(), guard: None },
    }
}

/// `<prefix>*.md` in one folder, newest name first.
fn texts_named(folder: &Path, prefix: &str) -> Vec<NamedText> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> =
        entries.flatten().filter_map(|entry| entry.file_name().into_string().ok()).filter(|name| name.starts_with(prefix) && name.ends_with(".md")).collect();
    names.sort();
    names.reverse();
    names
        .into_iter()
        .filter_map(|file| {
            let text = fs::read_to_string(folder.join(&file)).ok()?;
            let name = file.strip_prefix(prefix)?.strip_suffix(".md")?.replace('_', " ");
            Some(NamedText { name, text: shown_text(&text) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(cv_file: &str) -> ApplicationRef {
        ApplicationRef { folder: "acme_lead".into(), company: "Acme Corp".into(), cv_file: cv_file.into(), cover_letter_file: String::new() }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    #[test]
    fn empty_workspace_gives_an_empty_archive() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ArchiveFiles::in_workspace(dir.path()).load(&reference(""), today()), ArchiveContents::default());
    }

    #[test]
    fn loads_every_part_of_an_archive() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("documents/applications/acme_lead");
        fs::create_dir_all(&archive).unwrap();
        fs::create_dir_all(dir.path().join("cv")).unwrap();
        fs::create_dir_all(dir.path().join("company_research")).unwrap();
        fs::write(archive.join("job_posting.md"), "We need a lead\n\n## posting_guard\nclean\nhidden blocks: 0\n").unwrap();
        fs::write(archive.join("interview_prep_technical.md"), "# Prep").unwrap();
        fs::write(archive.join("followup_2026-10-01.md"), "Hello").unwrap();
        fs::write(dir.path().join("cv/main_custom.pdf"), "pdf").unwrap();
        fs::write(dir.path().join("company_research/acme-corp.json"), r#"{"fetched_date":"2026-08-01","about":"x"}"#).unwrap();

        let contents = ArchiveFiles::in_workspace(dir.path()).load(&reference("cv/main_custom.tex"), today());

        let posting = contents.posting.unwrap();
        assert_eq!((posting.text.as_str(), posting.guard.as_deref()), ("We need a lead", Some("clean")));
        assert_eq!(contents.prep_packs[0].name, "technical");
        assert_eq!(contents.follow_ups[0].name, "2026-10-01");
        assert_eq!(contents.documents[0].label, "CV");
        assert!(contents.research.unwrap().is_stale);
    }

    #[test]
    fn a_tracker_path_outside_the_workspace_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let contents = ArchiveFiles::in_workspace(&dir.path().join("w")).load(&reference("../secret.tex"), today());
        assert!(contents.documents.is_empty());
    }
}
