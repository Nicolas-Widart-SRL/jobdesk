use egui::{Align, Frame, Label, Layout, Margin, Response, ScrollArea, Ui, vec2};

use crate::portals::application::{Badge, Origin, PortalView};
use crate::portals::ui::screen::BUSY;
use crate::ui_kit::tokens::{BODY_PADDING, INSET, ITEM_GAP, MIN_HIT, PICK_PADDING, RAIL_TITLE_PADDING, ROW_TAG_HEIGHT, SKELETON_LINE, Type};
use crate::ui_kit::{Ink, Tone, desk_label, divider, enabled_when, faded, name_line, rich, row_tag, skeleton, slip, spoken_pick_row, switch};

/// Rows sit in so their words line up with the group titles.
const ROW_SIDE: i8 = INSET - PICK_PADDING.0;
/// The room between a row's three lines.
const LINE_GAP: f32 = 2.0;
/// The widths of the loading list's grey names.
const LOADING_ROWS: [f32; 5] = [120.0, 90.0, 170.0, 130.0, 100.0];

/// What the user did in the list this frame: picked a row, or flipped a row's switch to the state given.
#[derive(Debug, Default)]
pub(super) struct RailAsk {
    pub pick: Option<String>,
    pub switch: Option<(String, bool)>,
}

/// The two groups, On then Off, on a slip that scrolls. A group with no portal is left out. `picked` is the folder on
/// paper. While `busy`, the switches wait and say why, and the rows still pick.
pub(super) fn rail(ui: &mut Ui, groups: [&[&PortalView]; 2], picked: &str, busy: bool) -> RailAsk {
    let mut ask = RailAsk::default();
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ScrollArea::vertical().id_salt("portals_list").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(f32::from(BODY_PADDING) / 2.0);
            for (portals, (word, note)) in groups.into_iter().zip([("On", "Claude searches these"), ("Off", "Claude skips these")]) {
                if portals.is_empty() {
                    continue;
                }
                group_title(ui, word, note);
                rows(ui, |ui| {
                    for (at, portal) in portals.iter().enumerate() {
                        if at > 0 {
                            divider(ui);
                        }
                        let (row, flipped) = portal_row(ui, portal, portal.folder == picked, busy);
                        if flipped {
                            ask.switch = Some((portal.folder.clone(), !portal.on));
                        } else if row.clicked() {
                            ask.pick = Some(portal.folder.clone());
                        }
                    }
                });
            }
        });
    });
    ask
}

/// A group's word, such as On, then what it means.
fn group_title(ui: &mut Ui, word: &str, note: &str) {
    let (top, bottom) = RAIL_TITLE_PADDING;
    Frame::new().inner_margin(Margin { left: INSET, right: INSET, top, bottom }).show(ui, |ui| {
        ui.horizontal(|ui| {
            desk_label(ui, word, Type::TABLE_HEAD, Ink::Text);
            desk_label(ui, note, Type::NOTE, Ink::Muted);
        });
    });
}

/// Rows set in so their words line up with the group titles.
fn rows(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().inner_margin(Margin::symmetric(ROW_SIDE, 0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
}

/// The portal's switch, which waits while a run works.
pub(super) fn portal_switch(ui: &mut Ui, portal: &PortalView, busy: bool) -> Response {
    enabled_when(ui, !busy, BUSY, |ui| switch(ui, portal.on, &portal.name))
}

/// What the portal's words look like: an off portal's are quieter, its switch at full ink.
pub(super) fn quiet<R>(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    if on { add(ui) } else { faded(ui, add) }
}

/// The source tag's word.
pub(super) fn origin_word(origin: Origin) -> &'static str {
    match origin {
        Origin::BuiltIn => "Built-in",
        Origin::Community => "Community",
        Origin::MadeByYou => "Made by you",
    }
}

pub(super) fn badge_word(badge: Badge) -> &'static str {
    match badge {
        Badge::NeedsKey => "Needs API key",
        Badge::PersonalUse => "Personal use only",
    }
}

/// The day a job was first seen, as the screens write a day.
pub(super) fn day(date: chrono::NaiveDate) -> String {
    date.format("%a %-d %b").to_string()
}

/// "412 jobs · last new Mon 9 Nov", or that it found none yet. Nothing while the saved jobs cannot be read.
fn jobs_words(jobs: Option<(usize, Option<chrono::NaiveDate>)>) -> Option<String> {
    let (count, last) = jobs?;
    let jobs = if count == 1 { "1 job".to_owned() } else { format!("{count} jobs") };
    Some(match (count, last) {
        (0, _) => "No jobs yet".to_owned(),
        (_, Some(last)) => format!("{jobs} · last new {}", day(last)),
        (_, None) => jobs,
    })
}

/// One portal in three lines: its name and country with the switch at the right, the sentence on what it searches, then
/// the jobs it found and its tags. Returns the row, and whether its switch was clicked.
fn portal_row(ui: &mut Ui, portal: &PortalView, picked: bool, busy: bool) -> (Response, bool) {
    let mut flipped = false;
    let spoken = match &portal.country {
        Some(country) => format!("{}, {country}", portal.name),
        None => portal.name.clone(),
    };
    let row = spoken_pick_row(ui, picked, &spoken, None, |ui| {
        ui.spacing_mut().item_spacing.y = LINE_GAP;
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, MIN_HIT), Layout::right_to_left(Align::Center), |ui| {
            flipped = portal_switch(ui, portal, busy).clicked();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                quiet(ui, portal.on, |ui| name_line(ui, &portal.name, portal.country.as_deref()));
            });
        });
        quiet(ui, portal.on, |ui| {
            if let Some(sentence) = &portal.sentence {
                ui.add(Label::new(rich(ui, sentence, Type::NOTE, Ink::Muted)).truncate());
            }
            ui.add_space(ITEM_GAP / 2.0);
            ui.allocate_ui_with_layout(vec2(width, ROW_TAG_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
                for badge in portal.badges.iter().rev() {
                    row_tag(ui, badge_word(*badge), Tone::Interview);
                }
                row_tag(ui, origin_word(portal.origin), Tone::Plain);
                if let Some(words) = jobs_words(portal.jobs) {
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(Label::new(rich(ui, words, Type::NOTE, Ink::Muted)).truncate());
                    });
                }
            });
        });
    });
    (row, flipped)
}

/// The list before the first read: grey rows in a slip.
pub(super) fn loading_rail(ui: &mut Ui) {
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(f32::from(BODY_PADDING) / 2.0);
        let (top, bottom) = RAIL_TITLE_PADDING;
        Frame::new().inner_margin(Margin { left: INSET, right: INSET, top, bottom }).show(ui, |ui| skeleton(ui, 30.0, SKELETON_LINE));
        rows(ui, |ui| {
            for (at, width) in LOADING_ROWS.into_iter().enumerate() {
                if at > 0 {
                    divider(ui);
                }
                let (x, y) = PICK_PADDING;
                Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = ITEM_GAP;
                    skeleton(ui, width, SKELETON_LINE + 2.0);
                    skeleton(ui, width + 140.0, SKELETON_LINE);
                    skeleton(ui, 140.0, SKELETON_LINE);
                });
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn a_row_counts_the_jobs_found_and_the_last_new_one() {
        let day = NaiveDate::from_ymd_opt(2026, 11, 9);
        assert_eq!(jobs_words(Some((412, day))).as_deref(), Some("412 jobs · last new Mon 9 Nov"));
        assert_eq!(jobs_words(Some((1, day))).as_deref(), Some("1 job · last new Mon 9 Nov"));
        assert_eq!(jobs_words(Some((0, None))).as_deref(), Some("No jobs yet"));
        assert_eq!(jobs_words(None), None, "the saved jobs could not be read");
    }
}
