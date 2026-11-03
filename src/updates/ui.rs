//! The update group at the right of the status bar, and the release notes it opens over the desk.

use egui::{Align, Layout, Ui};
use egui_commonmark::CommonMarkCache;

use crate::ui_kit::tokens::ITEM_GAP;
use crate::ui_kit::{
    Ink, NotesAction, ReleaseNotes, Tone, enabled_when, release_notes_popover, status_action, status_button, status_divider, status_mark, status_mark_button,
    status_progress,
};
use crate::updates::application::{BarAction, BarGroup, ESC_CLOSES, Mark, NotesView, UpdateView};

/// What stays between frames: whether the release notes are open, and their Markdown cache.
#[derive(Default)]
pub struct UpdateScreen {
    notes_open: bool,
    cache: CommonMarkCache,
}

impl UpdateScreen {
    /// Opens the notes over the desk, as About's What's new does. They show while the bar has a group to anchor them.
    pub fn open_notes(&mut self) {
        self.notes_open = true;
    }

    pub fn close_notes(&mut self) {
        self.notes_open = false;
    }

    pub fn notes_open(&self) -> bool {
        self.notes_open
    }
}

/// What the user asked of the update group or its notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateRequest {
    Act(BarAction),
    /// The release page, at the address the view built from the version.
    OpenPage(String),
}

/// The update group and the divider after it, for a right-to-left row that holds Settings already. The divider shows
/// only with the group. The group's words are cut first, so the action and Settings keep their width.
/// `restart_wait` is why Restart to update waits, if it does. The notes open from the group's words, in a popover
/// above them.
pub fn bar_group(ui: &mut Ui, screen: &mut UpdateScreen, view: &UpdateView, restart_wait: Option<&str>) -> Option<UpdateRequest> {
    let Some(group) = &view.bar else {
        screen.notes_open = false;
        return None;
    };
    status_divider(ui);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = ITEM_GAP;
        let mut request = group.action.and_then(|action| bar_action(ui, action, restart_wait)).map(UpdateRequest::Act);
        match &group.progress {
            // A download that runs draws its bar instead of the words, and has no notes to open.
            Some(progress) => {
                screen.notes_open = false;
                status_progress(ui, &group.words, progress.fraction, &format!("{}, {}", progress.share, progress.size));
            }
            None => request = notes_button(ui, screen, group, view.notes.as_ref(), restart_wait).or(request),
        }
        request
    })
    .inner
}

/// The group's one action. Cancel is a plain status button, and Restart to update waits while a task is open.
fn bar_action(ui: &mut Ui, action: BarAction, restart_wait: Option<&str>) -> Option<BarAction> {
    let pressed = match action {
        BarAction::Cancel => status_button(ui, action.label(), &[], false),
        BarAction::Restart => enabled_when(ui, restart_wait.is_none(), restart_wait.unwrap_or_default(), |ui| status_action(ui, action.label())),
        BarAction::Download | BarAction::Retry => status_action(ui, action.label()),
    };
    pressed.clicked().then_some(action)
}

/// The square and the words, which open the notes in a popover above them, their right edges level.
fn notes_button(ui: &mut Ui, screen: &mut UpdateScreen, group: &BarGroup, notes: Option<&NotesView>, restart_wait: Option<&str>) -> Option<UpdateRequest> {
    let square: Ink = match group.mark {
        Mark::News => Ink::Text,
        Mark::Good => Tone::Hired.into(),
        Mark::Failed => Tone::Closed.into(),
    };
    // With no notes to open, such as after a Restart to update that did not install and before the release is known
    // again, the words are plain words that take no focus.
    let Some(notes) = notes else {
        screen.notes_open = false;
        status_mark(ui, square, &group.words);
        return None;
    };
    let mark = status_mark_button(ui, square, &group.words, screen.notes_open);
    if mark.clicked() {
        screen.notes_open = !screen.notes_open;
    }
    let action = notes.action.as_ref();
    // A slip says what Restart does, or why there is no step to take.
    let slip = notes.slip.as_deref().or(notes.problem.as_deref()).or(notes.blocked.as_deref());
    // A Restart that waits says why in the slip too, so the keyboard learns it as well as the pointer.
    let primary_wait = action.filter(|action| action.action == BarAction::Restart).and(restart_wait);
    let words = ReleaseNotes {
        title: &notes.title,
        released: &notes.meta,
        notes: &notes.notes,
        notes_label: &notes.notes_label,
        trust: notes.trust,
        slip,
        primary: action.map(|action| action.label.as_str()),
        primary_wait,
        size: Some(&notes.size),
        open_page: notes.release_page.as_ref().map(|_| notes.release_page_label),
        esc_note: if slip.is_some() || primary_wait.is_some() { ESC_CLOSES } else { notes.foot },
    };
    match release_notes_popover(&mark, &mut screen.notes_open, &words, &mut screen.cache)? {
        NotesAction::Primary => action.map(|action| UpdateRequest::Act(action.action)),
        NotesAction::OpenPage => notes.release_page.clone().map(UpdateRequest::OpenPage),
    }
}
