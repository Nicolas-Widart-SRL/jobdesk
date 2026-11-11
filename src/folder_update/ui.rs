//! The folder update's notice in the status bar, beside Jobdesk's own update group and built as it is, and the details
//! it opens in the same popover as the release notes.

use egui::{Align, Layout, Ui};
use egui_commonmark::CommonMarkCache;

use crate::folder_update::application::{DETAILS_LABEL, DETAILS_TITLE, DETAILS_TRUST, DISMISS, ESC_CLOSES, Notice, NoticeMark, UNDO, UNDO_UPDATE};
use crate::ui_kit::tokens::ITEM_GAP;
use crate::ui_kit::{
    Ink, NotesAction, NotesBody, NotesList, ReleaseNotes, Tone, release_notes_popover, status_action, status_button, status_divider, status_mark,
    status_mark_button,
};

/// What stays between frames: whether the details are open.
#[derive(Default)]
pub struct NoticeScreen {
    details_open: bool,
    /// The popover's Markdown cache, which lists of names never use.
    cache: CommonMarkCache,
}

impl NoticeScreen {
    pub fn details_open(&self) -> bool {
        self.details_open
    }

    pub fn close_details(&mut self) {
        self.details_open = false;
    }
}

/// What the user asked of the notice or its details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeRequest {
    Undo,
    Dismiss,
}

/// The notice and the divider before it, for a right-to-left row that holds Settings and the update group already. The
/// divider shows only with the notice, and its words are cut first, so Undo and Dismiss keep their width. Words with
/// lists to show open them in a popover above, as the update's notes do. The paths in them are plain text only.
pub fn bar_group(ui: &mut Ui, screen: &mut NoticeScreen, notice: Option<&Notice>) -> Option<NoticeRequest> {
    let Some(notice) = notice else {
        screen.details_open = false;
        return None;
    };
    status_divider(ui);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = ITEM_GAP;
        let mut request = None;
        if notice.dismiss && status_button(ui, DISMISS, &[], false).clicked() {
            request = Some(NoticeRequest::Dismiss);
        }
        if notice.undo && status_action(ui, UNDO).clicked() {
            request = Some(NoticeRequest::Undo);
        }
        details(ui, screen, notice).or(request)
    })
    .inner
}

/// The square and the words. With lists to show they open them, and the details' Undo answers as the bar's does.
fn details(ui: &mut Ui, screen: &mut NoticeScreen, notice: &Notice) -> Option<NoticeRequest> {
    let square: Ink = match notice.mark {
        NoticeMark::News => Ink::Text,
        NoticeMark::Good => Tone::Hired.into(),
        NoticeMark::Failed => Tone::Closed.into(),
    };
    if notice.lists.is_empty() {
        screen.details_open = false;
        status_mark(ui, square, &notice.words);
        return None;
    }
    let mark = status_mark_button(ui, square, &notice.words, screen.details_open);
    if mark.clicked() {
        screen.details_open = !screen.details_open;
    }
    let lists: Vec<NotesList> = notice.lists.iter().map(|list| NotesList { lead: &list.lead, items: &list.items, more: list.more.as_deref() }).collect();
    let words = ReleaseNotes {
        title: DETAILS_TITLE,
        released: &notice.words,
        notes: NotesBody::Lists(&lists),
        notes_label: DETAILS_LABEL,
        trust: DETAILS_TRUST,
        slip: None,
        primary: notice.undo.then_some(UNDO_UPDATE),
        primary_wait: None,
        size: None,
        open_page: None,
        esc_note: ESC_CLOSES,
    };
    match release_notes_popover(&mark, &mut screen.details_open, &words, &mut screen.cache)? {
        NotesAction::Primary => Some(NoticeRequest::Undo),
        NotesAction::OpenPage => None,
    }
}
