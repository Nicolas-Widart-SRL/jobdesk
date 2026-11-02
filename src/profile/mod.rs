//! What the user wrote about themselves: the profile files of the checkout, read as topics and blocks.

pub mod application;
mod domain;
mod infrastructure;

use std::path::Path;

use crate::profile::application::{ProfileEditor, ProfileStore, ProfileView, profile_view};
use crate::profile::infrastructure::ProfileFiles;
use crate::shared_kernel::LoadError;

/// Each profile file that exists but cannot be read gives one error, and hides only the topics it feeds.
pub fn load(root: &Path) -> (ProfileView, Vec<LoadError>) {
    let files = ProfileFiles::in_workspace(root);
    profile_view(&|file| files.read(file))
}

/// The editor saves one block at a time into the checkout, the only write Jobdesk makes there.
pub fn open_editor(root: &Path) -> ProfileEditor {
    ProfileEditor::new(Box::new(ProfileFiles::in_workspace(root)))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::profile::application::{BlockView, Fill, ProfileFile, SaveOutcome, TopicId};

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().flatten() {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() { copy_tree(&entry.path(), &target) } else { fs::copy(entry.path(), target).map(|_| ()).unwrap() }
        }
    }

    /// The profile files of the sample workspace, with `CLAUDE.profile.md` as `CLAUDE.md`.
    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_workspace");
        copy_tree(&source.join(".claude"), &dir.path().join(".claude"));
        fs::copy(source.join("CLAUDE.profile.md"), dir.path().join("CLAUDE.md")).unwrap();
        dir
    }

    fn path(root: &Path, file: ProfileFile) -> PathBuf {
        root.join(file.relative_path())
    }

    /// The role of Experience whose heading starts with `title`.
    fn role(root: &Path, title: &str) -> BlockView {
        let (view, _) = load(root);
        let experience = view.topics.into_iter().find(|topic| topic.id == TopicId::Experience).unwrap();
        experience.sections.into_iter().flat_map(|section| section.children).find(|child| child.heading.starts_with(title)).unwrap()
    }

    fn edit(root: &Path, title: &str, change: impl Fn(&str) -> String) -> ProfileEditor {
        let role = role(root, title);
        let mut editor = open_editor(root);
        editor.open(role.key, role.text.clone());
        *editor.draft_mut().unwrap() = change(&role.text);
        editor
    }

    #[test]
    fn the_sample_reads_as_the_rail_expects() {
        let dir = sample();
        let (view, errors) = load(dir.path());
        assert!(errors.is_empty(), "{errors:?}");
        let rail: Vec<(TopicId, Fill)> = view.topics.iter().map(|topic| (topic.id, topic.fill)).collect();
        assert_eq!(
            rail,
            [
                (TopicId::Identity, Fill::Filled),
                (TopicId::Experience, Fill::ToFill(2)),
                (TopicId::Education, Fill::Filled),
                (TopicId::Skills, Fill::ToFill(1)),
                (TopicId::Publications, Fill::Filled),
                (TopicId::References, Fill::Filled),
                (TopicId::Statements, Fill::Filled),
                (TopicId::HowIWork, Fill::Filled),
                (TopicId::Goals, Fill::NotStarted),
                (TopicId::Stories, Fill::ToFill(3)),
                (TopicId::JobSearch, Fill::Filled),
            ]
        );
        assert!(view.topics.iter().all(|topic| !topic.missing_heading), "every named heading is in the sample");
        let roles: Vec<usize> = view.topics[1].sections[0].children.iter().map(|child| child.placeholders).collect();
        assert_eq!(roles, [1, 0, 1, 0]);
        let shown = format!("{view:?}");
        assert!(!shown.contains("Folder layout notes"), "the rest of CLAUDE.md never reaches the view");
    }

    #[test]
    fn an_unreadable_file_hides_only_its_topics() {
        let dir = sample();
        fs::write(path(dir.path(), ProfileFile::Behavioral), b"# Behavioral Profile\n\nCaf\xe9\n").unwrap();
        let (view, errors) = load(dir.path());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].source, "02-behavioral-profile.md");
        let hidden: Vec<TopicId> = view.topics.iter().filter(|topic| topic.fill == Fill::Hidden).map(|topic| topic.id).collect();
        assert_eq!(hidden, [TopicId::HowIWork]);
    }

    #[test]
    fn a_save_replaces_one_role_and_keeps_every_other_byte() {
        let dir = sample();
        let file = path(dir.path(), ProfileFile::Candidate);
        let before = fs::read_to_string(&file).unwrap();
        let role = role(dir.path(), "Software Engineer - Pemberton");
        let mut editor = edit(dir.path(), "Software Engineer - Pemberton", |text| text.replace("[PLACEHOLDER]", "the import ran in minutes, not days"));
        assert!(editor.is_dirty());
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        assert_eq!(editor.editing(), None);
        let after = fs::read_to_string(&file).unwrap();
        let start = before.find(&role.text).unwrap();
        let end = start + role.text.len();
        assert_eq!(&after[..start], &before[..start]);
        assert_eq!(&after[after.len() - (before.len() - end)..], &before[end..]);
        assert_eq!(&after[start..after.len() - (before.len() - end)], role.text.replace("[PLACEHOLDER]", "the import ran in minutes, not days"));
    }

    #[test]
    fn a_busy_save_leaves_the_file_alone() {
        let dir = sample();
        let file = path(dir.path(), ProfileFile::Candidate);
        let (bytes, modified) = (fs::read(&file).unwrap(), fs::metadata(&file).unwrap().modified().unwrap());
        let mut editor = edit(dir.path(), "Lead Software Engineer", |text| format!("{text}\n- More."));
        assert_eq!(editor.save(true, false), SaveOutcome::Busy);
        assert_eq!(editor.save(true, true), SaveOutcome::Busy);
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
        assert_eq!(fs::read(&file).unwrap(), bytes);
        assert!(editor.is_dirty(), "the draft waits for the run");
    }

    #[test]
    fn a_block_changed_on_disk_is_written_over_only_when_asked() {
        let dir = sample();
        let file = path(dir.path(), ProfileFile::Candidate);
        let mut editor = edit(dir.path(), "Lead Software Engineer", |text| text.replace("[PLACEHOLDER]", "EUR 1.2M a year"));
        let theirs = fs::read_to_string(&file).unwrap().replace("Antwerp. Freelance.", "Antwerp. Freelance, four days a week.");
        fs::write(&file, &theirs).unwrap();
        let SaveOutcome::Changed { now } = editor.save(false, false) else { panic!("the block changed") };
        assert!(now.starts_with("### Lead Software Engineer") && now.contains("four days a week"));
        assert_eq!(fs::read_to_string(&file).unwrap(), theirs, "nothing was written");
        assert_eq!(editor.save(false, true), SaveOutcome::Saved);
        let after = fs::read_to_string(&file).unwrap();
        assert!(after.contains("EUR 1.2M a year") && !after.contains("four days a week"));
    }

    #[test]
    fn a_deleted_block_comes_back_at_the_end_of_its_section() {
        let dir = sample();
        let file = path(dir.path(), ProfileFile::Candidate);
        let role = role(dir.path(), "Senior Backend Engineer");
        let mut editor = edit(dir.path(), "Senior Backend Engineer", |text| format!("{text}\n- Ran the on-call rota."));
        let without = fs::read_to_string(&file).unwrap().replace(&format!("{}\n\n", role.text), "");
        fs::write(&file, &without).unwrap();
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        let after = fs::read_to_string(&file).unwrap();
        let draft = format!("{}\n- Ran the on-call rota.\n\n## Independent Projects", role.text);
        assert!(after.contains(&draft), "{after}");
        assert_eq!(after.replace(&format!("{}\n- Ran the on-call rota.\n\n", role.text), ""), without);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_linked_outside_the_root_is_not_written() {
        let dir = sample();
        let outside = tempfile::tempdir().unwrap();
        let file = path(dir.path(), ProfileFile::Candidate);
        let elsewhere = outside.path().join("candidate.md");
        fs::copy(&file, &elsewhere).unwrap();
        fs::remove_file(&file).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
        let before = fs::read(&elsewhere).unwrap();
        let mut editor = edit(dir.path(), "Lead Software Engineer", |text| text.replace("[PLACEHOLDER]", "EUR 1.2M a year"));
        let SaveOutcome::Failed(error) = editor.save(false, false) else { panic!("the save must refuse") };
        assert_eq!(error.source, "01-candidate-profile.md");
        assert!(error.reason.contains("outside"), "{error}");
        assert_eq!(fs::read(&elsewhere).unwrap(), before);
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1, "no copy was left beside the target");
        assert!(editor.is_dirty(), "the draft stays");
    }

    #[test]
    fn a_crlf_file_stays_crlf() {
        let dir = sample();
        let file = path(dir.path(), ProfileFile::Candidate);
        let before = fs::read_to_string(&file).unwrap().replace('\n', "\r\n");
        fs::write(&file, &before).unwrap();
        let mut editor = edit(dir.path(), "Junior Developer", |text| format!("{}\n- Ran the studio's first accessibility audit.", text.replace("\r\n", "\n")));
        assert_eq!(editor.save(false, false), SaveOutcome::Saved);
        let after = fs::read_to_string(&file).unwrap();
        assert!(after.contains("venues.\r\n- Ran the studio's first accessibility audit.\r\n\r\n## Independent Projects"), "{after:?}");
        assert!(!after.replace("\r\n", "").contains('\n'), "every line still ends in CRLF");
    }
}
