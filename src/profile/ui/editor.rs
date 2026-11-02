use chrono::NaiveDateTime;
use egui::{Id, Label, Modal, Ui};
use egui_commonmark::CommonMarkCache;

use crate::profile::application::{ProfileEditor, SaveOutcome, TopicId};
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::ITEM_GAP;
use crate::ui_kit::{
    BlockEditor, ChangedOnDisk, EditorAction, EditorBusy, Ink, Type, alert, block_editor, danger_button, primary_button, profile_document, rich, strong,
};

/// Why every other Edit waits while one is open.
pub(super) const ONE_AT_A_TIME: &str = "Save or cancel the open edit first.";
/// The heading a new role opens with.
pub(super) const ROLE_STUB: &str = "### Title - Company (dates)";
const HINT: &str = "Markdown. Placeholders read [PLACEHOLDER].";
/// Why Save waits while a run works, on hover, and the same beside Cancel, for a user who does not hover.
const RUN: EditorBusy = EditorBusy {
    why: "A task is running and may change this file. Save once it ends.",
    line: "A task is running, so Save waits until it ends. Your text stays here.",
};
/// Why Save waits, the same two ways, from a run's end until Jobdesk has read what it changed.
const READING: EditorBusy = EditorBusy {
    why: "Jobdesk is reading the files the task changed. Save once that is done.",
    line: "Reading the files the task changed, so Save waits a moment. Your text stays here.",
};
const SAVE_OVER: &str = "Save mine over it";
/// The discard ask's width, as End session's popover has.
const ASK_WIDTH: f32 = 320.0;

/// Why Save waits, if it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hold {
    /// A run works and may write the same file.
    Run,
    /// A run has ended, and the folder's load after it, which marks what it changed, has not arrived.
    Reading,
}

/// What an edit is of, in the words the editor uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Noun {
    Role,
    Section,
    Part,
}

impl Noun {
    fn word(self) -> &'static str {
        match self {
            Noun::Role => "role",
            Noun::Section => "section",
            Noun::Part => "part",
        }
    }
}

/// What the last save found on disk in place of the block the editor opened, and when.
enum Trouble {
    /// The block's lines as they are now, each with whether it is new.
    Changed(Vec<(String, bool)>),
    /// The block, or the section a new one goes in, is no longer in the file.
    Gone,
}

/// The one edit on show: the topic it sits in, what it is of, and what its last save found.
pub(super) struct Edit {
    pub topic: TopicId,
    noun: Noun,
    /// Counts the edits opened, so each block's field has an id, and an undo history, of its own.
    serial: u64,
    preview: bool,
    trouble: Option<(Trouble, NaiveDateTime)>,
    /// Why the last write failed, as plain text.
    failed: Option<String>,
    /// The field takes the focus on the next frame, as it does when the editor opens.
    focus: bool,
    /// Save scrolls into view on the next frame, as it does when the warning box opens above it.
    reveal: bool,
}

impl Edit {
    pub fn new(topic: TopicId, noun: Noun, serial: u64) -> Self {
        Self { topic, noun, serial, preview: false, trouble: None, failed: None, focus: true, reveal: false }
    }

    /// When the last save found the block changed or gone, which the foot of the paper ages as Changed on disk.
    pub fn changed_at(&self) -> Option<NaiveDateTime> {
        self.trouble.as_ref().map(|(_, at)| *at)
    }

    pub fn noun(&self) -> Noun {
        self.noun
    }
}

/// Saves the draft, over what is on disk when the last save found the block changed or gone, and says whether it
/// wrote. A draft with nothing changed has nothing to write, so the editor closes. While `busy` nothing is written,
/// whatever asked, and the draft stays. Over a change on disk it writes only while the file holds what the box shows:
/// anything newer opens the box again on it.
pub(super) fn save(editor: &mut ProfileEditor, edit: &mut Edit, held: bool, now: NaiveDateTime) -> bool {
    if !held && !editor.is_dirty() {
        editor.cancel();
        return false;
    }
    match editor.save(held, edit.trouble.is_some()) {
        SaveOutcome::Saved => return true,
        SaveOutcome::Changed { .. } => {
            edit.trouble = Some((Trouble::Changed(editor.theirs_lines().unwrap_or_default()), now));
            (edit.failed, edit.reveal) = (None, true);
        }
        SaveOutcome::Gone => {
            edit.trouble = Some((Trouble::Gone, now));
            (edit.failed, edit.reveal) = (None, true);
        }
        SaveOutcome::Failed(error) => edit.failed = Some(visible_chars(&error.to_string()).collect()),
        // Nothing was read or written, and the draft stays. An edit on show always has a block open.
        SaveOutcome::Busy | SaveOutcome::NothingOpen => {}
    }
    false
}

/// Takes the block as the last save found it on disk in place of the draft.
pub(super) fn take_theirs(editor: &mut ProfileEditor, edit: &mut Edit) {
    if matches!(edit.trouble, Some((Trouble::Changed(_), _))) {
        editor.use_new_version();
        edit.trouble = None;
    }
}

/// The slip in place of the block: the draft, Save and Cancel, and what the last save found. A failed write shows its
/// reason under it, as plain text.
pub(super) fn slip(ui: &mut Ui, cache: &mut CommonMarkCache, editor: &mut ProfileEditor, edit: &mut Edit, hold: Option<Hold>) -> Option<EditorAction> {
    let noun = edit.noun.word();
    let new = editor.is_new();
    let title = if new { "Add a role".to_owned() } else { format!("Edit this {noun}") };
    let unsaved = editor.is_dirty().then_some("Unsaved changes");
    let (changed, note) = match &edit.trouble {
        Some((Trouble::Changed(lines), _)) => {
            let (shown, part) = shown_lines(lines);
            let caption = match part {
                Part::Whole => "Now in the file".to_owned(),
                Part::Start => format!("Now in the file, the start of this {noun}"),
                Part::End => format!("Now in the file, the end of this {noun}"),
                Part::Middle => format!("Now in the file, part of this {noun}"),
            };
            let new_lines = lines.iter().filter(|(_, new)| *new).count();
            let what = match new_lines {
                0 => "The file holds a different version now.".to_owned(),
                1 => "It has a new line in the file.".to_owned(),
                count => format!("It has {count} new lines in the file."),
            };
            let take_note = match new_lines {
                0 => "Replaces your text with the file's.",
                1 => "Replaces your text with the file's. To keep both, copy the new line into your text, then save.",
                _ => "Replaces your text with the file's. To keep both, copy the new lines into your text, then save.",
            };
            let changed =
                Changed { title: format!("This {noun} changed while you were editing."), detail: format!("{what} {NOT_SAVED}"), caption, shown, take_note };
            (Some(changed), format!("Your text replaces the {noun} as it is now in the file."))
        }
        Some((Trouble::Gone, _)) => {
            let (title, note) = if new {
                ("The section this role goes in is no longer in the file.".to_owned(), "Your role goes at the end of the file.".to_owned())
            } else {
                let place = if editor.gone_into_section() { "its section" } else { "the file" };
                (format!("This {noun} is no longer in the file."), format!("Your text goes back at the end of {place}."))
            };
            let detail = format!("It was renamed or removed while you were editing. {NOT_SAVED}");
            (Some(Changed { title, detail, caption: String::new(), shown: Vec::new(), take_note: "" }), note)
        }
        None if new => (None, "The role goes after your last role. The rest of the file stays as it is.".to_owned()),
        None => (None, format!("Only this {noun} changes. The rest of the file stays as it is.")),
    };
    let gone = matches!(edit.trouble, Some((Trouble::Gone, _)));
    let lines: Vec<(&str, bool)> = changed.as_ref().map(|changed| changed.shown.iter().map(|(line, new)| (line.as_str(), *new)).collect()).unwrap_or_default();
    let on_disk = changed.as_ref().map(|changed| ChangedOnDisk {
        title: &changed.title,
        detail: &changed.detail,
        caption: &changed.caption,
        // A gone block has no new version to take, so the box offers none.
        lines: (!gone).then_some(lines.as_slice()),
        take: "Use the new version",
        take_note: changed.take_note,
    });
    let look = BlockEditor {
        title: &title,
        hint: HINT,
        save: if edit.trouble.is_some() { SAVE_OVER } else { "Save" },
        note: &note,
        unsaved,
        busy: hold.map(|hold| match hold {
            Hold::Run => RUN,
            Hold::Reading => READING,
        }),
        changed: on_disk.as_ref(),
        focus: edit.focus,
        reveal: edit.reveal,
    };
    let draft = editor.draft_mut()?;
    let preview = &mut edit.preview;
    let action = ui.push_id(("profile edit", edit.serial), |ui| block_editor(ui, &look, draft, preview, |ui, text| profile_document(ui, cache, text))).inner;
    (edit.focus, edit.reveal) = (false, false);
    if let Some(reason) = &edit.failed {
        ui.add_space(ITEM_GAP);
        alert(ui, format!("Not saved. {reason}"));
    }
    action
}

const NOT_SAVED: &str = "Your text below is not saved yet.";

/// The words of the box over a block that changed on disk.
struct Changed {
    title: String,
    detail: String,
    caption: String,
    shown: Vec<(String, bool)>,
    take_note: &'static str,
}

/// Which part of a block the lines shown are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Whole,
    Start,
    End,
    Middle,
}

/// The lines worth showing of a block that changed on disk: from the line before the first new one to the line after
/// the last. With no new line, the whole block.
fn shown_lines(lines: &[(String, bool)]) -> (Vec<(String, bool)>, Part) {
    let (Some(first), Some(last)) = (lines.iter().position(|(_, new)| *new), lines.iter().rposition(|(_, new)| *new)) else {
        return (lines.to_vec(), Part::Whole);
    };
    let (from, to) = (first.saturating_sub(1), (last + 2).min(lines.len()));
    let part = match (from == 0, to == lines.len()) {
        (true, true) => Part::Whole,
        (true, false) => Part::Start,
        (false, true) => Part::End,
        (false, false) => Part::Middle,
    };
    (lines[from..to].to_vec(), part)
}

/// What waits on the discard ask: Cancel, another topic, another tab, another folder, or quitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ask {
    Cancel,
    Topic(TopicId),
    Tab,
    Folder,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Answer {
    Keep,
    Discard,
}

/// Asks before unsaved text goes. Keep editing is the default: it takes the focus as the ask opens, and Esc or a click
/// outside keeps editing too. Nothing behind the ask takes a click while it shows.
pub(super) fn discard_ask(ctx: &egui::Context, ask: Ask, noun: Noun, focus: &mut bool) -> Option<Answer> {
    let noun = noun.word();
    let then = match ask {
        Ask::Cancel => "Cancel drops them.",
        Ask::Topic(_) => "Opening another topic drops them.",
        Ask::Tab => "Leaving Profile drops them.",
        Ask::Folder => "Opening another folder drops them.",
        Ask::Quit => "Quitting drops them.",
    };
    let shown = Modal::new(Id::NULL.with("profile_discard_ask")).show(ctx, |ui| {
        ui.set_max_width(ASK_WIDTH);
        strong(ui, "Discard your changes?");
        let body = format!("Your changes to this {noun} are not saved. {then}");
        ui.add(Label::new(rich(ui, body, Type::BODY, Ink::Muted)).wrap());
        ui.add_space(ITEM_GAP);
        let answer = ui
            .horizontal(|ui| {
                let keep = primary_button(ui, "Keep editing");
                if *focus && !ui.is_sizing_pass() {
                    keep.request_focus();
                    *focus = false;
                }
                if keep.clicked() {
                    Some(Answer::Keep)
                } else if danger_button(ui, "Discard changes").clicked() {
                    Some(Answer::Discard)
                } else {
                    None
                }
            })
            .inner;
        ui.label(rich(ui, "Esc keeps editing.", Type::NOTE, Ink::Faint));
        answer
    });
    if shown.inner.is_none() && shown.should_close() { Some(Answer::Keep) } else { shown.inner }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(marks: &[bool]) -> Vec<(String, bool)> {
        marks.iter().enumerate().map(|(at, new)| (format!("line {at}"), *new)).collect()
    }

    #[test]
    fn a_change_shows_with_a_line_around_it() {
        let shown = |marks: &[bool]| {
            let (shown, part) = shown_lines(&lines(marks));
            (shown.iter().map(|(line, _)| line.trim_start_matches("line ").parse::<usize>().unwrap()).collect::<Vec<_>>(), part)
        };
        assert_eq!(shown(&[false, false, false, true]), (vec![2, 3], Part::End));
        assert_eq!(shown(&[true, false, false, false]), (vec![0, 1], Part::Start));
        assert_eq!(shown(&[false, false, true, false, false]), (vec![1, 2, 3], Part::Middle));
        assert_eq!(shown(&[false, true, false]), (vec![0, 1, 2], Part::Whole));
        assert_eq!(shown(&[false, false]), (vec![0, 1], Part::Whole), "with no new line, all of it");
    }

    /// A file that reads and refuses every write, with a reason that hides characters.
    struct Refusing;

    impl crate::profile::application::ProfileStore for Refusing {
        fn read(&self, _: crate::profile::application::ProfileFile) -> Result<Option<String>, crate::shared_kernel::LoadError> {
            Ok(Some("## Education\nMSc\n".to_owned()))
        }

        fn write(&self, _: crate::profile::application::ProfileFile, _: &str) -> Result<(), crate::shared_kernel::LoadError> {
            Err(crate::shared_kernel::LoadError { source: "01-candidate-profile.md".to_owned(), reason: "disk\u{202e} full\u{200b}".to_owned() })
        }
    }

    /// A failed write keeps the editor open and the draft, and gives its reason as plain text, hidden characters removed.
    #[test]
    fn a_failed_write_says_why_in_plain_text() {
        let mut editor = ProfileEditor::new(Box::new(Refusing));
        let key = crate::profile::application::BlockKey {
            file: crate::profile::application::ProfileFile::Candidate,
            level: 2,
            heading: "Education".to_owned(),
            parent: None,
            nth: 0,
        };
        editor.open(key, "## Education\nMSc".to_owned());
        *editor.draft_mut().unwrap() = "## Education\nPhD".to_owned();
        let mut edit = Edit::new(TopicId::Education, Noun::Section, 1);
        let at = chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap();
        assert!(!save(&mut editor, &mut edit, false, at));
        assert_eq!(edit.failed.as_deref(), Some("01-candidate-profile.md: disk full"));
        assert!(editor.is_dirty() && editor.editing().is_some(), "the editor stays open with the draft");
    }
}
