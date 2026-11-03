use chrono::NaiveDateTime;
use egui::{Align, Frame, Label, Layout, Margin, Response, Ui, vec2};

use crate::profile::application::{CvDocument, CvShelf, CvTag, Shelf, meta_words, state_line};
use crate::profile::ui::rail::{CV_DOCUMENTS, row_words, rows};
use crate::profile::ui::screen::BUSY;
use crate::ui_kit::tokens::{DEAL_PADDING, ITEM_GAP, ROW_ITEM_GAP, RUN_SIDE, SCREEN_GAP};
use crate::ui_kit::{
    Ink, Tone, Type, desk_label, divider, empty_note, enabled_when, faint_mono, headed_paper, rail_title, rich, row_tag, small_command_action_hinted,
    split_paper, spoken_pick_row, tag,
};

/// What the head says under CV documents while no document shows.
const LEAD: &str = "The PDFs built from your profile, as they last built.";
const EMPTY_TITLE: &str = "No CV documents in your folder yet";
const EMPTY: &str =
    "This page shows each PDF in cv/, and each .tex there not built yet. /apply writes one for every application, and /build-cv builds its PDF.";
const HIDDEN_TITLE: &str = "Your CV documents are hidden, not gone";
const HIDDEN: &str =
    "Jobdesk could not read the folder that holds them, so it shows none. Nothing in it was changed. Jobdesk reads it again as soon as the folder changes.";
/// What the foot says at its right.
const SHOWS: &str = "Jobdesk shows these PDFs. /build-cv builds them.";

/// The tone each tag takes: what a build would fix in `interview`, a refused render in `closed`.
fn tone(tag: CvTag) -> Tone {
    match tag {
        CvTag::Current => Tone::Plain,
        CvTag::SourceNewer | CvTag::NotBuilt => Tone::Interview,
        CvTag::RenderFailed => Tone::Closed,
    }
}

/// What the action's hover says: the line it opens, and that nothing runs yet.
fn build_hint(line: &str) -> String {
    format!("Opens the launcher with {line}. Nothing runs until you start it there.")
}

/// The CV documents on paper: the list at the left, and the picked one beside it, which falls back to the first when
/// `picked` names none of them. A build only opens the launcher: nothing here starts a run. An empty or unread `cv/`
/// says so instead.
pub(super) fn cv_paper(ui: &mut Ui, shelf: &CvShelf, picked: &mut Option<String>, busy: bool, now: NaiveDateTime) {
    let documents = match shelf {
        CvShelf::Unread => return lone(ui, HIDDEN_TITLE, HIDDEN),
        CvShelf::Read(documents) => documents,
    };
    let Some(first) = documents.first() else { return lone(ui, EMPTY_TITLE, EMPTY) };
    let shown = picked.as_deref().and_then(|stem| documents.iter().find(|doc| doc.stem == stem)).unwrap_or(first);
    let mut chosen = None;
    split_paper(ui, RUN_SIDE, |ui| chosen = list(ui, documents, &shown.stem, now), |ui| head(ui, shown, busy, now), foot);
    if let Some(stem) = chosen {
        *picked = Some(stem);
    }
}

/// Masters, then each application's CV, as pickable rows. Returns the stem picked this frame.
fn list(ui: &mut Ui, documents: &[CvDocument], shown: &str, now: NaiveDateTime) -> Option<String> {
    let mut chosen = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    for (shelf, title) in [(Shelf::Master, "Masters"), (Shelf::Application, "For applications")] {
        let mut group = documents.iter().filter(|doc| doc.shelf == shelf).peekable();
        if group.peek().is_none() {
            continue;
        }
        rail_title(ui, title);
        rows(ui, |ui| {
            for doc in group {
                if doc_row(ui, doc, doc.stem == shown, now).clicked() {
                    chosen = Some(doc.stem.clone());
                }
            }
        });
    }
    chosen
}

/// One document: its label, cut to the row with the whole on hover, and its tag when it is not Current.
fn doc_row(ui: &mut Ui, doc: &CvDocument, on: bool, now: NaiveDateTime) -> Response {
    let mark = Some(state_line(doc, false, now).tag).filter(|&tag| tag != CvTag::Current);
    let spoken = match mark {
        Some(tag) => format!("{}, {}", doc.label, tag.word()),
        None => doc.label.clone(),
    };
    spoken_pick_row(ui, on, &spoken, None, |ui| {
        row_words(ui, |ui| {
            if let Some(tag) = mark {
                row_tag(ui, tag.word(), tone(tag));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, &doc.label, Type::BODY, Ink::Text)).truncate().show_tooltip_when_elided(false));
            });
        });
    })
    .on_hover_text(&doc.label)
}

/// The picked document's head: its label, its file name and what it weighs, then its state line, over a hairline.
fn head(ui: &mut Ui, doc: &CvDocument, busy: bool, now: NaiveDateTime) {
    let (x, y) = DEAL_PADDING;
    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
        ui.add(Label::new(rich(ui, &doc.label, Type::DEAL, Ink::Text)).wrap());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            desk_label(ui, doc.shown_name(), Type::MONO, Ink::Text);
            desk_label(ui, meta_words(doc, None, now), Type::BODY, Ink::Muted);
        });
        ui.add_space(ITEM_GAP);
        state(ui, doc, busy, now);
    });
    divider(ui);
}

/// The tag, what it means, and at the right the build that would help, which opens the launcher and waits while a run works.
fn state(ui: &mut Ui, doc: &CvDocument, busy: bool, now: NaiveDateTime) {
    let line = state_line(doc, false, now);
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::right_to_left(Align::Center), |ui| {
        if let Some((label, command)) = &line.action {
            enabled_when(ui, !busy, BUSY, |ui| small_command_action_hinted(ui, label, command, &build_hint(command)));
            ui.add_space(ITEM_GAP * 2.0);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            tag(ui, line.tag.word(), tone(line.tag));
            ui.add(Label::new(rich(ui, &line.sentence, Type::NOTE, Ink::Muted)).wrap());
        });
    });
}

/// CV documents with no document to show: the head, then why, over the foot.
fn lone(ui: &mut Ui, title: &str, body: &str) {
    headed_paper(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            ui.label(rich(ui, CV_DOCUMENTS, Type::DEAL, Ink::Text));
            ui.add(Label::new(rich(ui, LEAD, Type::BODY, Ink::Muted)).wrap());
        },
        |ui| {
            ui.add_space(SCREEN_GAP / 2.0);
            empty_note(ui, title, body);
        },
        foot,
    );
}

/// Where the documents live, and that Jobdesk only shows them.
fn foot(ui: &mut Ui) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        desk_label(ui, SHOWS, Type::NOTE, Ink::Muted);
        ui.add_space(ITEM_GAP * 2.0);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            desk_label(ui, "Kept in ", Type::NOTE, Ink::Muted);
            faint_mono(ui, "cv/");
            desk_label(ui, ", beside the ", Type::NOTE, Ink::Muted);
            faint_mono(ui, ".tex");
            desk_label(ui, " each PDF is built from", Type::NOTE, Ink::Muted);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tag_takes_its_tone() {
        assert_eq!(tone(CvTag::Current), Tone::Plain);
        assert_eq!(tone(CvTag::SourceNewer), Tone::Interview);
        assert_eq!(tone(CvTag::NotBuilt), Tone::Interview);
        assert_eq!(tone(CvTag::RenderFailed), Tone::Closed);
    }

    #[test]
    fn the_hint_names_the_line_and_says_nothing_runs() {
        assert_eq!(
            build_hint("/build-cv main_tessel_platform_lead"),
            "Opens the launcher with /build-cv main_tessel_platform_lead. Nothing runs until you start it there."
        );
    }
}
