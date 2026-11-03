use egui::{Align, Frame, Label, Layout, Margin, Rect, ScrollArea, Ui, UiBuilder, pos2, vec2};

use crate::profile::application::{Fill, Group, ProfileView, RailWord, TopicCounts, TopicId, TopicView, UpdatedMarks};
use crate::ui_kit::tokens::{BODY_PADDING, INSET, PICK_PADDING, PICK_ROW_HEIGHT, SKELETON_LINE};
use crate::ui_kit::{FillMarks, Ink, KeptHeight, Tone, Type, divider, fill_tally, rail_title, rich, row_tag, skeleton, slip, spoken_pick_row, tally_strip};

/// What the Updated mark says on hover, on the rail and in the head.
pub const UPDATED_HOVER: &str = "Updated by the last task. Open the topic to see it.";

/// While the Updated mark shows, the room on each side of it is tighter than a row's usual gap, so the longest title,
/// Identity and languages, fits beside it at `RUN_SIDE`. The title's side, where its own letters leave air, is the tighter.
const MARKED_GAPS: (f32, f32) = (3.0, 4.0);

/// Rows sit in so their words line up with the group titles.
const ROW_SIDE: i8 = INSET - PICK_PADDING.0;

/// The widths of the loading rail's grey titles, per group.
const LOADING_ROWS: [&[f32]; 2] = [&[150.0, 96.0, 120.0, 80.0, 170.0, 110.0, 140.0], &[100.0, 160.0, 130.0, 96.0]];
/// The width of a loading row's grey state word.
const LOADING_WORD: f32 = 44.0;

/// The pinned group's title, and its one row.
const BUILT: &str = "Built from your profile";
pub const CV_DOCUMENTS: &str = "CV documents";

/// What the paper shows: one topic, or the CV documents built from the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Topic(TopicId),
    CvDocuments,
}

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

/// The CV documents row's word: what is out of date in `interview`, an unread folder in `closed`, and the rest `muted`.
pub fn cv_word(word: RailWord) -> (String, Type, Ink) {
    match word {
        RailWord::OutOfDate(_) => (word.words(), Type::TAG, Tone::Interview.into()),
        RailWord::Documents(_) | RailWord::NoneYet => (word.words(), Type::NOTE, Ink::Muted),
        RailWord::Hidden => (word.words(), Type::TAG, Tone::Closed.into()),
    }
}

/// The rail: its title and count, the tally, then the topics in their groups, scrolling over the CV documents row pinned
/// at the foot. That row is not a topic, so the tally and the count leave it out. Returns what was picked this frame.
pub fn rail(ui: &mut Ui, view: &ProfileView, counts: TopicCounts, picked: Pick, marks: &UpdatedMarks, word: RailWord) -> Option<Pick> {
    let (mut topic, mut cv) = (None, false);
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        rail_head(ui, Some(&format!("{} of {} filled", counts.filled, counts.total)), |ui| {
            fill_tally(ui, fill_marks(counts), true);
        });
        over_pinned(
            ui,
            |ui| {
                ScrollArea::vertical().id_salt("profile_rail").auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for group in [Group::OnYourCv, Group::GuidesClaude] {
                        rail_title(ui, group_title(group));
                        rows(ui, |ui| {
                            for row in view.topics.iter().filter(|row| row.group == group) {
                                if topic_row(ui, row, picked == Pick::Topic(row.id), marks.rail_mark(row.id)).clicked() {
                                    topic = Some(row.id);
                                }
                            }
                        });
                    }
                });
            },
            |ui| rows(ui, |ui| cv = cv_row(ui, word, picked == Pick::CvDocuments).clicked()),
        );
    });
    if cv { Some(Pick::CvDocuments) } else { topic.map(Pick::Topic) }
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

/// `top` in the room the pinned group leaves, and the group at the foot under a hairline, so it stays in view when the
/// window is short. The group goes in first, at its height from the frame before, so the topics take what it leaves.
fn over_pinned(ui: &mut Ui, top: impl FnOnce(&mut Ui), pinned: impl FnOnce(&mut Ui)) {
    let area = ui.available_rect_before_wrap();
    let kept = KeptHeight::read(ui, ui.scope_id().with("profile_rail_pinned"), 0.0);
    let pin_rect = Rect::from_min_max(pos2(area.left(), (area.bottom() - kept.height).max(area.top())), area.max);
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut pin_ui = ui.new_child(column(pin_rect, "profile_rail_pinned"));
    pin_ui.spacing_mut().item_spacing.y = 0.0;
    divider(&mut pin_ui);
    rail_title(&mut pin_ui, BUILT);
    pinned(&mut pin_ui);
    pin_ui.add_space(f32::from(ROW_SIDE));
    kept.keep(ui, pin_ui.min_rect().height().min(area.height()), "the rail's pinned group changed height");
    let top_rect = Rect::from_min_max(area.min, pos2(area.right(), pin_rect.top()));
    let mut top_ui = ui.new_child(column(top_rect, "profile_rail_topics"));
    top_ui.shrink_clip_rect(top_rect);
    top(&mut top_ui);
    ui.advance_cursor_after_rect(area);
}

/// Rows set in so their words line up with the group titles.
pub(super) fn rows(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new().inner_margin(Margin::symmetric(ROW_SIDE, 0)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
    });
}

/// A row's words at the row's height, laid out from the right, so the state word goes in first and the title takes the rest.
pub(super) fn row_words(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let height = PICK_ROW_HEIGHT - f32::from(PICK_PADDING.1) * 2.0;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_height(height);
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
}

/// The row as focus hears it: the title, the state word, and what the Updated mark says on hover.
pub fn row_spoken(title: &str, word: &str, updated: bool) -> String {
    if updated { format!("{title}, {word}. {UPDATED_HOVER}") } else { format!("{title}, {word}") }
}

/// One topic: its title, the Updated mark while a run's change waits to be seen, and its state word.
fn topic_row(ui: &mut Ui, topic: &TopicView, on: bool, updated: bool) -> egui::Response {
    let (word, style, ink) = state_word(topic.fill);
    let spoken = row_spoken(topic.title, &word, updated);
    spoken_pick_row(ui, on, &spoken, updated.then_some(UPDATED_HOVER), |ui| {
        row_words(ui, |ui| {
            if updated {
                ui.spacing_mut().item_spacing.x = MARKED_GAPS.1;
            }
            ui.add(Label::new(rich(ui, word, style, ink)));
            if updated {
                row_tag(ui, "Updated", Tone::Active).on_hover_text(UPDATED_HOVER);
                ui.spacing_mut().item_spacing.x = MARKED_GAPS.0;
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, topic.title, Type::BODY, Ink::Text)).truncate());
            });
        });
    })
}

/// CV documents, with its word.
fn cv_row(ui: &mut Ui, word: RailWord, on: bool) -> egui::Response {
    let (word, style, ink) = cv_word(word);
    let spoken = row_spoken(CV_DOCUMENTS, &word, false);
    spoken_pick_row(ui, on, &spoken, None, |ui| {
        row_words(ui, |ui| {
            ui.add(Label::new(rich(ui, word, style, ink)));
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, CV_DOCUMENTS, Type::BODY, Ink::Text)).truncate());
            });
        });
    })
}

/// The rail before the first read: its title, an unlit tally, and the groups over grey rows, with CV documents pinned
/// at the foot over a grey word.
pub fn loading_rail(ui: &mut Ui, total: usize) {
    slip(ui, |ui| {
        ui.set_min_height(ui.available_height());
        rail_head(ui, None, |ui| {
            tally_strip(ui, &[(total, None)], true);
        });
        over_pinned(
            ui,
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (group, widths) in [Group::OnYourCv, Group::GuidesClaude].into_iter().zip(LOADING_ROWS) {
                    rail_title(ui, group_title(group));
                    for &width in widths {
                        loading_row(ui, |ui| skeleton(ui, width, SKELETON_LINE));
                    }
                }
            },
            |ui| {
                loading_row(ui, |ui| {
                    ui.label(rich(ui, CV_DOCUMENTS, Type::BODY, Ink::Text));
                });
            },
        );
    });
}

/// A grey row: `title` at the left, and a grey state word at the right.
fn loading_row(ui: &mut Ui, title: impl FnOnce(&mut Ui)) {
    let (x, y) = PICK_PADDING;
    Frame::new().inner_margin(Margin::symmetric(ROW_SIDE + x, y)).show(ui, |ui| {
        let height = PICK_ROW_HEIGHT - f32::from(y) * 2.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::left_to_right(Align::Center), |ui| {
            title(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| skeleton(ui, LOADING_WORD, SKELETON_LINE));
        });
    });
}
