use std::fs;
use std::path::{Path, PathBuf};

use crate::shared_kernel::placeholders::placeholder_count;
use crate::workspace::domain::{PortalInfo, ProfileFile, ProfileState, SyncState, TemplateState, WorkspaceInspector, WorkspaceStatus};

const ASSISTANT: &str = ".claude/skills/job-application-assistant";
const PROFILE_FILES: [&str; 7] = [
    "01-candidate-profile.md",
    "02-behavioral-profile.md",
    "03-writing-style.md",
    "04-job-evaluation.md",
    "05-cv-templates.md",
    "06-cover-letter-templates.md",
    "07-interview-prep.md",
];
const ACTIVE_TEMPLATE: &str = "BEGIN ACTIVE-TEMPLATE";

pub struct CheckoutInspector {
    root: PathBuf,
}

impl CheckoutInspector {
    pub fn in_workspace(root: &Path) -> Self {
        Self { root: root.to_owned() }
    }

    fn read(&self, relative: &str) -> Option<String> {
        fs::read_to_string(self.root.join(relative)).ok()
    }

    fn profile_file(&self, name: &str) -> ProfileFile {
        let state = match self.read(&format!("{ASSISTANT}/{name}")) {
            None => ProfileState::Missing,
            Some(text) => match placeholder_count(&text) {
                0 => ProfileState::Filled,
                placeholders => ProfileState::NeedsSetup { placeholders },
            },
        };
        ProfileFile { name: name.to_owned(), state }
    }

    fn portals(&self) -> Vec<PortalInfo> {
        let mut names = folders_in(&self.root.join(".agents/skills"));
        names.sort();
        names
            .into_iter()
            .filter_map(|name| {
                let manifest = self.read(&format!(".agents/skills/{name}/SKILL.md"))?;
                Some(PortalInfo { enabled: is_enabled(&manifest), name })
            })
            .collect()
    }

    fn templates(&self) -> TemplateState {
        let custom = |file: &str| self.read(&format!("{ASSISTANT}/{file}")).is_some_and(|text| text.contains(ACTIVE_TEMPLATE));
        let templates = self.root.join("templates");
        let mut registered: Vec<String> = ["cv", "cover_letters"]
            .iter()
            .flat_map(|kind| {
                folders_in(&templates.join(kind)).into_iter().filter(|name| templates.join(kind).join(name).join("TEMPLATE.md").is_file()).collect::<Vec<_>>()
            })
            .collect();
        registered.sort();
        TemplateState { custom_cv: custom("05-cv-templates.md"), custom_letter: custom("06-cover-letter-templates.md"), registered }
    }

    fn sync(&self) -> SyncState {
        let gmail = self.read("gmail_sync/state.json");
        let last = gmail.as_deref().and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        SyncState {
            gmail_set_up: gmail.is_some(),
            gmail_last_sync: last.as_ref().and_then(|value| value.get("last_sync")).and_then(|value| value.as_str()).map(str::to_owned),
            notion_set_up: self.root.join("job_scraper/notion_sync.json").is_file(),
        }
    }
}

impl WorkspaceInspector for CheckoutInspector {
    fn status(&self) -> WorkspaceStatus {
        WorkspaceStatus {
            profile: PROFILE_FILES.iter().map(|name| self.profile_file(name)).collect(),
            portals: self.portals(),
            templates: self.templates(),
            sync: self.sync(),
            has_salary_data: self.root.join("salary_data.json").is_file(),
        }
    }
}

fn folders_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries.flatten().filter(|entry| entry.path().is_dir()).filter_map(|entry| entry.file_name().into_string().ok()).collect()
}

/// A portal is enabled unless its frontmatter sets `enabled: false`.
fn is_enabled(manifest: &str) -> bool {
    let frontmatter = manifest.strip_prefix("---").and_then(|rest| rest.split_once("\n---")).map_or("", |(front, _)| front);
    !frontmatter.lines().any(|line| line.trim() == "enabled: false")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, text: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn reads_a_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, ".claude/skills/job-application-assistant/01-candidate-profile.md", "Name: Sam");
        write(root, ".claude/skills/job-application-assistant/02-behavioral-profile.md", "[FIRST_NAME] [LAST_NAME]");
        write(root, ".claude/skills/job-application-assistant/05-cv-templates.md", "<!-- BEGIN ACTIVE-TEMPLATE (managed) -->");
        write(root, ".agents/skills/on-search/SKILL.md", "---\nname: on\n---\nenabled: false in prose\n");
        write(root, ".agents/skills/off-search/SKILL.md", "---\nname: off\nenabled: false\n---\n");
        write(root, "templates/cv/modern/TEMPLATE.md", "x");
        write(root, "gmail_sync/state.json", r#"{"last_sync":"2026-10-29T08:00:00Z","processed_message_ids":[]}"#);

        let status = CheckoutInspector::in_workspace(root).status();

        assert_eq!(status.profile[0].state, ProfileState::Filled);
        assert_eq!(status.profile[1].state, ProfileState::NeedsSetup { placeholders: 2 });
        assert_eq!(status.profile[2].state, ProfileState::Missing);
        assert_eq!(status.portals, [PortalInfo { name: "off-search".into(), enabled: false }, PortalInfo { name: "on-search".into(), enabled: true }]);
        assert!(status.templates.custom_cv && !status.templates.custom_letter);
        assert_eq!(status.templates.registered, ["modern"]);
        assert_eq!(status.sync.gmail_last_sync.as_deref(), Some("2026-10-29T08:00:00Z"));
        assert!(!status.sync.notion_set_up);
    }

    #[test]
    fn empty_checkout_has_no_portals() {
        let dir = tempfile::tempdir().unwrap();
        assert!(CheckoutInspector::in_workspace(dir.path()).status().portals.is_empty());
    }
}
