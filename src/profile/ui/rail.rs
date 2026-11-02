use egui::{Align, Frame, Label, Layout, Margin, ScrollArea, Ui, vec2};

use crate::profile::application::{Fill, Group, ProfileView, TopicCounts, TopicId, TopicView, UpdatedMarks};
use crate::ui_kit::tokens::{BODY_PADDING, INSET, PICK_PADDING, PICK_ROW_HEIGHT, SKELETON_LINE};
use crate::ui_kit::{FillMarks, Ink, Tone, Type, divider, fill_tally, pick_row, rail_title, rich, skeleton, slip, tag, tally_strip};

/// What the Updated mark says on hover, on the rail and in the head.
pub const UPDATED_HOVER: &str = "Updated by the last task. Open the topic to see it.";

/// Rows sit in so their words line up with the group titles.
const ROW_SIDE: i8 = INSET - PICK_PADDING.0;

/// The widths of the loading rail's grey titles, per group.
const LOADING_ROWS: [&[f32]; 2] = [&[150.0, 96.0, 120.0, 80.0, 170.0, 110.0, 140.0], &[100.0, 160.0, 130.0, 96.0]];
/// The width of a loading row's grey state word.
const LOADING_WORD: f32 = 44.0;

pub fn group_title(group: Group) -> &'static str {
    match group {
        Group::OnYourCv => "On your CV",
        Group::GuidesClaude => "What guides Claude",
    }
}

/// The tally's runs: filled topics in `text`, those with something left to fill in `interview`, and hidden ones apart.
pub fn fill_marks(counts: TopicCounts) -> FillMarks {
    let open = counts.total.saturating_sub(counts.filled + counts.hidden);
    FillMarks { filled: counts.filled, open, hidden: counts.hidden, open_ink: Some(Tone::Interview.into()) }
}

/// The word at a row's right, saying how much of the topic is written.
pub fn state_word(fill: Fill) -> (String, Type, Ink) {
    match fill {
        Fill::Filled => ("Filled".to_owned(), Type::NOTE, Ink::Muted),
        Fill::ToFill(left) => (format!("{left} to fill"), Type::TAG, Tone::Interview.into()),
        Fill::NotStarted => ("Not started".to_owned(), Type::TAG, Tone::Interview.into()),
        Fill::Hidden => ("Hidden".to_owned(), Type::TAG, Tone::Closed.into()),
    }
}

/// The rail: its title and count, the tally, then the topics in their groups. Returns the topic picked this frame.
pub fn rail(ui: &mut Ui, view: &ProfileView, counts: TopicCounts, picked: TopicId, marks: &UpdatedMarks) -> Option<TopicId> {
    let mut chosen = None;
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        rail_head(ui, Some(&format!("{} of {} filled", counts.filled, counts.total)), |ui| {
            fill_tally(ui, fill_marks(counts), true);
        });
        ScrollArea::vertical().id_salt("profile_rail").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for group in [Group::OnYourCv, Group::GuidesClaude] {
                rail_title(ui, group_title(group));
                Frame::new().inner_margin(Margin::symmetric(ROW_SIDE, 0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for topic in view.topics.iter().filter(|topic| topic.group == group) {
                        if topic_row(ui, topic, topic.id == picked, marks.rail_mark(topic.id)).clicked() {
                            chosen = Some(topic.id);
                        }
                    }
                });
            }
        });
    });
    chosen
}

/// "Topics" with its note at the right, then `under` it, such as the tally, over a hairline.
fn rail_head(ui: &mut Ui, note: Option<&str>, under: impl FnOnce(&mut Ui)) {
    Frame::new().inner_margin(Margin { left: INSET, right: INSET, top: BODY_PADDING, bottom: BODY_PADDING }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = BODY_PADDING.into();
        ui.horizontal(|ui| {
            ui.label(rich(ui, "Topics", Type::SECTION, Ink::Text));
            if let Some(note) = note {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(rich(ui, note, Type::NOTE, Ink::Muted));
                });
            }
        });
        under(ui);
    });
    divider(ui);
}

/// One topic: its title, the Updated mark while a run's change waits to be seen, and its state word.
fn topic_row(ui: &mut Ui, topic: &TopicView, on: bool, updated: bool) -> egui::Response {
    let (word, style, ink) = state_word(topic.fill);
    pick_row(ui, on, |ui| {
        let height = PICK_ROW_HEIGHT - f32::from(PICK_PADDING.1) * 2.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_height(height);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, word, style, ink)));
                if updated {
                    tag(ui, "Updated", Tone::Active).on_hover_text(UPDATED_HOVER);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(Label::new(rich(ui, topic.title, Type::BODY, Ink::Text)).truncate());
                });
            });
        });
    })
}

/// The rail before the first read: its title, an unlit tally, and the groups over grey rows.
pub fn loading_rail(ui: &mut Ui, total: usize) {
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        rail_head(ui, None, |ui| {
            tally_strip(ui, &[(total, None)], true);
        });
        ui.spacing_mut().item_spacing.y = 0.0;
        for (group, widths) in [Group::OnYourCv, Group::GuidesClaude].into_iter().zip(LOADING_ROWS) {
            rail_title(ui, group_title(group));
            for &width in widths {
                let (x, y) = PICK_PADDING;
                Frame::new().inner_margin(Margin::symmetric(ROW_SIDE + x, y)).show(ui, |ui| {
                    let height = PICK_ROW_HEIGHT - f32::from(y) * 2.0;
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
                        skeleton(ui, width, SKELETON_LINE);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| skeleton(ui, LOADING_WORD, SKELETON_LINE));
                    });
                });
            }
        }
    });
}
