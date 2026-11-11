//! The first-run Profile chapter's parts: the drop zone and the files added to it, Claude's questions with a field each,
//! the changes Claude proposes with a checkbox each, the CV's pages, and the side slips beside them. Each takes plain
//! lists and returns what the user did. Claude's words and file names are untrusted, and render as
//! docs/architecture/trust.md says: Claude's Markdown by `release_markdown`'s rules, and a name as plain text with its
//! hidden characters removed.

use std::path::PathBuf;

use egui::text::LayoutJob;
use egui::{
    Align, Color32, Frame, Id, Layout, Margin, Rect, Response, RichText, Role, ScrollArea, Sense, Shape, Stroke, StrokeKind, TextEdit, TextWrapMode, Ui,
    UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};
use egui_commonmark::CommonMarkCache;

use crate::ui_kit::Palette;
use crate::ui_kit::controls::{button, link, small_button};
use crate::ui_kit::marks::tally_spoken;
use crate::ui_kit::pages::{PageSet, fitted_pages, paint_mini_page};
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{Choice, divider, folder_fill, paint_choice, paper};
use crate::ui_kit::text::{add_cut_text, add_job, add_text, claude_markdown, copyable, inline_job, line, paint_text, plain_name, wrapped_height};
use crate::ui_kit::tokens::{
    BODY_PADDING, CHANGE_GAP, CHANGE_PADDING, CHECK_BOX, DISABLED_OPACITY, DOC_MEASURE, DROP_GAP, DROP_PADDING, DROP_PAGE, DROP_PAGE_OFFSET, DROP_SLIM_GAP,
    DROP_SLIM_PADDING, FALLBACK_SHARE, FIELD_HEIGHT, FIELD_PADDING, FILE_PAGE, GROUP_GAP, GUTTER, HAIRLINE, INSET, ITEM_GAP, ITEM_NUMBER, PICK_PADDING,
    PROSE_GAP, QUESTION_GAP, RADIUS_BUTTON, RADIUS_CARD, RADIUS_PANEL, ROW_GAP, ROW_ITEM_GAP, ROW_PADDING, SPLIT_GAP, SUB_ITEM_LETTER, SWEEP_DASH, TALLY_FAT,
    Type,
};
use crate::ui_kit::wizard::well;

/// The room above a question's lettered sub-items.
const SUB_ITEM_GAP: f32 = 2.0;

/// What an empty answer field says: under a question Claude marked optional, and under any other.
const OPTIONAL_HINT: &str = "Optional. Leave it empty to skip.";
const ANSWER_HINT: &str = "Your answer, or leave it empty to skip";
/// What the fallback sheet's one field says while it is empty.
const FALLBACK_HINT: &str = "Write your answer here";

/// One numbered question in Claude's message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Question<'a> {
    /// Claude's own number for it.
    pub number: u32,
    /// Its words, in Markdown, as Claude wrote them.
    pub text: &'a str,
    /// Its lettered sub-items, such as a and b, which the user answers in its one field.
    pub sub_items: &'a [SubItem<'a>],
    /// Whether Claude marked it optional, which its empty field then says.
    pub optional: bool,
}

/// A lettered sub-item of a question, such as "a. A former manager".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubItem<'a> {
    pub letter: char,
    /// Its words, in Markdown.
    pub text: &'a str,
}

/// A part of a message of Claude's that asks numbered questions, in the message's order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SheetPart<'a> {
    /// Claude's words around the questions, in Markdown, such as its lead or its closing line.
    Prose(&'a str),
    /// A bold line of Claude's that heads the questions under it, as plain text.
    Group(&'a str),
    Question(Question<'a>),
}

/// One numbered change Claude proposes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change<'a> {
    /// Claude's own number for it.
    pub number: u32,
    /// Its words, in Markdown.
    pub text: &'a str,
}

/// A part of a message of Claude's that proposes numbered changes, in the message's order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProposalPart<'a> {
    /// Claude's words around the changes, in Markdown.
    Prose(&'a str),
    /// A bold line of Claude's that heads the changes under it, as plain text.
    Group(&'a str),
    Change(Change<'a>),
}

/// The common shape of a message laid out part by part: Claude's prose, a group's title, or one of its numbered items.
enum Part<'a, T> {
    Prose(&'a str),
    Group(&'a str),
    Item(T),
}

/// Claude's message as one column at `DOC_MEASURE`: prose by `claude_markdown`, a group's title in `Type::TAKE_STRONG`,
/// and each item by `item`, handed its place among the items. `item_gap` is the room under an item.
fn message<'a, T>(ui: &mut Ui, cache: &mut CommonMarkCache, parts: impl Iterator<Item = Part<'a, T>>, item_gap: f32, mut item: impl FnMut(&mut Ui, usize, T)) {
    let width = ui.available_width().min(DOC_MEASURE);
    ui.scope(|ui| {
        ui.set_max_width(width);
        ui.spacing_mut().item_spacing.y = 0.0;
        // The room the last part left under it, so a group's title keeps `GROUP_GAP` above it, however it got there.
        let mut under: Option<f32> = None;
        let mut index = 0;
        for part in parts {
            match part {
                Part::Prose(text) => {
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = ITEM_GAP;
                        claude_markdown(ui, cache, text);
                    });
                    ui.add_space(PROSE_GAP);
                    under = Some(PROSE_GAP);
                }
                Part::Group(title) => {
                    if let Some(under) = under {
                        ui.add_space((GROUP_GAP.0 - under).max(0.0));
                    }
                    copyable(ui, |ui| add_text(ui, plain_name(title), Type::TAKE_STRONG, Ink::Text));
                    ui.add_space(GROUP_GAP.1);
                    under = Some(GROUP_GAP.1);
                }
                Part::Item(value) => {
                    item(ui, index, value);
                    index += 1;
                    ui.add_space(item_gap);
                    under = Some(item_gap);
                }
            }
        }
    });
}

/// Claude's number or letter for an item, such as "3.", in `muted` at `Type::TAKE`, in a cell `width` wide at least.
/// It is painted, with no node of its own: the item's words name what a screen reader hears.
fn number_cell(ui: &mut Ui, mark: &str, width: f32) {
    let p = Palette::of(ui);
    let galley = line(ui, mark, Type::TAKE, p.muted, f32::INFINITY);
    let (rect, _) = ui.allocate_exact_size(vec2(width.max(galley.size().x + ITEM_GAP), galley.size().y), Sense::hover());
    paint_text(ui, rect.min, galley, p.muted, Color32::TRANSPARENT);
}

/// Claude's message that ends in numbered questions, as a question sheet: the message as one column at `DOC_MEASURE`,
/// with each question followed by its own answer field. A question's lettered sub-items sit inside it, over its one
/// field. A field starts one line high and grows with its text. An empty one says it can be skipped, in Claude's words
/// for a question marked optional. Each field is named by its question for a screen reader.
///
/// `answers` holds one answer per question, in order: it grows or shrinks to the questions, with empty answers added.
/// Returns whether an answer changed this frame. Lay the sheet in a scroll area, such as `split_card`'s main column.
/// The screen takes its own keys, such as the command key and Enter for Send, before the sheet, so no field types them.
pub fn question_sheet(ui: &mut Ui, cache: &mut CommonMarkCache, parts: &[SheetPart], answers: &mut Vec<String>) -> bool {
    let count = parts.iter().filter(|part| matches!(part, SheetPart::Question(_))).count();
    answers.resize(count, String::new());
    let id = ui.scope_id().with("question_sheet");
    let mut changed = false;
    let pieces = parts.iter().map(|part| match part {
        SheetPart::Prose(text) => Part::Prose(text),
        SheetPart::Group(title) => Part::Group(title),
        SheetPart::Question(question) => Part::Item(question),
    });
    message(ui, cache, pieces, QUESTION_GAP, |ui, index, question| {
        if let Some(answer) = answers.get_mut(index) {
            changed |= question_item(ui, id.with(index), question, answer);
        }
    });
    changed
}

/// One question: its number, its words, its sub-items, then its field.
fn question_item(ui: &mut Ui, id: Id, question: &Question, answer: &mut String) -> bool {
    let p = Palette::of(ui);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        number_cell(ui, &format!("{}.", question.number), ITEM_NUMBER);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let words = copyable(ui, |ui| add_job(ui, inline_job(ui, question.text, Type::TAKE, p.text), TextWrapMode::Wrap));
            if !question.sub_items.is_empty() {
                ui.add_space(SUB_ITEM_GAP);
            }
            for sub in question.sub_items {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    number_cell(ui, &format!("{}.", plain_name(&sub.letter.to_string())), SUB_ITEM_LETTER);
                    copyable(ui, |ui| add_job(ui, inline_job(ui, sub.text, Type::TAKE, p.text), TextWrapMode::Wrap));
                });
            }
            ui.add_space(ITEM_GAP);
            let hint = if question.optional { OPTIONAL_HINT } else { ANSWER_HINT };
            answer_field(ui, id, answer, hint, None).labelled_by(words.id).changed()
        })
        .inner
    })
    .inner
}

/// An answer field in the body face on `field`, in a `line` hairline that turns `text` while it has the focus, with
/// `hint` in `faint` while it is empty. It starts one line high, `FIELD_HEIGHT`, and grows with its text, or with `fixed`
/// it is that tall and its text scrolls inside it.
fn answer_field(ui: &mut Ui, id: Id, text: &mut String, hint: &str, fixed: Option<f32>) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let dim = |color: Color32| if enabled { color } else { color.gamma_multiply(DISABLED_OPACITY) };
    let row = ui.fonts_mut(|fonts| fonts.row_height(&Type::BODY.font()));
    // One line sits in the middle of a field of the usual height.
    let inset = Margin::symmetric(FIELD_PADDING as i8, ((FIELD_HEIGHT - row) / 2.0).floor().max(0.0) as i8);
    let edit = TextEdit::multiline(text)
        .id(id)
        .frame(Frame::NONE.inner_margin(inset))
        .font(Type::BODY.font())
        .text_color(dim(p.text))
        .hint_text(RichText::new(hint).font(Type::BODY.font()).color(dim(p.faint)))
        .desired_width(f32::INFINITY)
        .desired_rows(1)
        .min_size(vec2(0.0, fixed.unwrap_or(FIELD_HEIGHT)));
    // The field's box is painted last, when its focus is known, under what it holds.
    let under = ui.painter().add(Shape::Noop);
    let (response, rect) = match fixed {
        None => {
            let response = ui.add(edit);
            let rect = response.rect;
            (response, rect)
        }
        Some(height) => {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
            let mut inside = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
            let shown = ScrollArea::vertical().id_salt(id.with("scroll")).auto_shrink([false, false]).show(&mut inside, |ui| ui.add(edit));
            (shown.inner, rect)
        }
    };
    // The hint is painted only, so a screen reader hears it as the field's placeholder.
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_placeholder(hint));
    let border = if !enabled {
        dim(p.line)
    } else if response.has_focus() {
        p.text
    } else {
        p.line
    };
    ui.painter().set(under, Shape::rect_filled(rect, RADIUS_BUTTON, p.field));
    ui.painter().rect_stroke(rect, RADIUS_BUTTON, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    response
}

/// The sheet when Claude asked with no numbered questions to find, or numbers out of order: Claude's message at the left
/// under "Claude wrote", scrolling on its own, and one large field at the right under "Your answer", which takes the
/// height left over `help`. The answer column is `FALLBACK_SHARE` of the message column's width, `SPLIT_GAP` from it.
/// It takes all the room `ui` has, so lay it in `wide_card`. Returns whether the answer changed this frame.
pub fn question_fallback(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, answer: &mut String, help: &str) -> bool {
    let id = ui.scope_id().with("question_fallback");
    let area = ui.available_rect_before_wrap();
    let message_width = ((area.width() - SPLIT_GAP) / (1.0 + FALLBACK_SHARE)).max(0.0);
    let left = Rect::from_min_max(area.min, pos2(area.left() + message_width, area.bottom()));
    let right = Rect::from_min_max(pos2(left.right() + SPLIT_GAP, area.top()), area.max);
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(id.with(salt)).max_rect(rect).layout(Layout::top_down(Align::Min));

    let mut said = ui.new_child(column(left, "message"));
    said.shrink_clip_rect(left);
    said.spacing_mut().item_spacing.y = ITEM_GAP;
    add_text(&mut said, "Claude wrote", Type::TABLE_HEAD, Ink::Text);
    well(&mut said, "question_fallback_message", |ui| claude_markdown(ui, cache, text));

    let mut answering = ui.new_child(column(right, "answer"));
    answering.shrink_clip_rect(right);
    answering.spacing_mut().item_spacing.y = ITEM_GAP;
    let label = add_text(&mut answering, "Your answer", Type::TABLE_HEAD, Ink::Text);
    let help_height = if help.is_empty() { 0.0 } else { wrapped_height(&answering, help, Type::NOTE, right.width()) + ITEM_GAP };
    let height = (answering.available_height() - help_height).max(FIELD_HEIGHT);
    let changed = answer_field(&mut answering, id.with("field"), answer, FALLBACK_HINT, Some(height)).labelled_by(label.id).changed();
    if !help.is_empty() {
        add_text(&mut answering, help, Type::NOTE, Ink::Muted);
    }
    ui.advance_cursor_after_rect(area);
    changed
}

/// A slip in a card's side column: on `folder_fill`, its words `INSET` in, with `title` in the section face and `note`
/// in `muted` at its right, such as a count. An empty `title` leaves the head out, and an empty `note` the count.
pub fn side_slip<R>(ui: &mut Ui, title: &str, note: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = folder_fill(ui);
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(Margin::symmetric(INSET, BODY_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            if !title.is_empty() {
                ui.horizontal(|ui| {
                    add_cut_text(ui, title, Type::SECTION, Ink::Text);
                    if !note.is_empty() {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| add_text(ui, note, Type::NOTE, Ink::Muted));
                    }
                });
            }
            add(ui)
        })
        .inner
}

/// A short paragraph on a side slip: `strong` in `text`, such as "Short answers are fine.", then `rest` in `muted`, both
/// at the note size. Either may be empty.
pub fn slip_text(ui: &mut Ui, strong: &str, rest: &str) -> Response {
    let p = Palette::of(ui);
    let mut job = LayoutJob::default();
    job.append(strong, 0.0, egui::TextFormat::simple(Type::TABLE_HEAD.font(), p.text));
    let rest = if strong.is_empty() || rest.is_empty() { rest.to_owned() } else { format!(" {rest}") };
    job.append(&rest, 0.0, egui::TextFormat::simple(Type::NOTE.font(), p.muted));
    add_job(ui, job, TextWrapMode::Wrap)
}

/// What the answers slip shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnswersSlip<'a> {
    /// Whether each question has an answer, in Claude's order.
    pub answered: &'a [bool],
    /// When the draft last saved, such as "14:32", or `None` before it has.
    pub saved_at: Option<&'a str>,
    /// The short form for a round of follow-ups: the count and one line, with no tally, draft slip or Show all.
    pub short: bool,
}

/// The question sheet's side slips: "Your answers" with how many of the questions have one and a `TALLY_FAT` mark
/// for each, lit where it is answered, then that short answers are fine and an empty one is sent as Skipped. Then when
/// the draft saved, and what Send does, with Show all Claude wrote. Returns whether Show all Claude wrote was clicked.
pub fn answers_slip(ui: &mut Ui, slip: &AnswersSlip) -> bool {
    let p = Palette::of(ui);
    let total = slip.answered.len();
    let done = slip.answered.iter().filter(|answered| **answered).count();
    side_slip(ui, "Your answers", &format!("{done} of {total}"), |ui| {
        if slip.short {
            slip_text(ui, "Short answers are fine.", "Saved as you type. An empty one is sent as Skipped.");
            return;
        }
        let colors: Vec<Color32> = slip.answered.iter().map(|&answered| if answered { p.text } else { p.off }).collect();
        tally_spoken(ui, &colors, TALLY_FAT, format!("{done} of {total} answered"));
        slip_text(ui, "Short answers are fine.", "Answer in any order, and skip any you like. An empty one is sent as Skipped.");
    });
    if slip.short {
        return false;
    }
    let when = slip.saved_at.map_or_else(|| "Saved as you type.".to_owned(), |at| format!("Saved as you type, last at {at}."));
    side_slip(ui, "Draft saved", "", |ui| {
        slip_text(ui, "", &format!("{when} Take your time: close Jobdesk and come back, and your answers wait here."));
    });
    side_slip(ui, "", "", |ui| {
        slip_text(ui, "", "Send joins your answers into one numbered reply, in Claude's order.");
        link(ui, "Show all Claude wrote").clicked()
    })
}

/// A proposed change in the review card: a checkbox, painted as `setting_check` paints its own, Claude's number in
/// `muted`, then the change's words at `Type::TAKE`, held to `DOC_MEASURE`, by `release_markdown`'s rules. The box,
/// the number and the words are one control: a click on any of them flips `on`, Tab stops on it once with the focus
/// ring round the box, and a screen reader hears one checkbox named by the number and the words, ticked or not.
/// Unticked, the words go `muted`.
pub fn change_check(ui: &mut Ui, on: &mut bool, number: u32, text: &str) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let full = ui.available_width().min(DOC_MEASURE);
    let mark = format!("{number}.");
    let numbered = line(ui, &mark, Type::TAKE, p.muted, f32::INFINITY);
    let number_width = ITEM_NUMBER.max(numbered.size().x + ITEM_GAP);
    let lead = CHECK_BOX + ROW_ITEM_GAP + number_width;
    let mut job = inline_job(ui, text, Type::TAKE, Color32::PLACEHOLDER);
    job.wrap.max_width = (full - lead).max(0.0);
    let words = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let first_line = words.rows.first().map_or(numbered.size().y, |row| row.rect().height());
    let height = words.size().y.max(first_line).max(CHECK_BOX) + CHANGE_PADDING * 2.0;
    let (rect, mut response) = ui.allocate_exact_size(vec2(full, height), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let name = format!("{mark} {}", words.text());
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, *on, &name));
    let top = rect.top() + CHANGE_PADDING;
    let boxed = Rect::from_min_size(pos2(rect.left(), top + ((first_line - CHECK_BOX) / 2.0).max(0.0)), Vec2::splat(CHECK_BOX));
    paint_choice(ui, &response, Choice::Check, boxed, *on);
    let ink = if !enabled {
        p.muted.gamma_multiply(DISABLED_OPACITY)
    } else if *on {
        p.text
    } else {
        p.muted
    };
    paint_text(ui, pos2(rect.left() + CHECK_BOX + ROW_ITEM_GAP, top), numbered, p.muted, Color32::TRANSPARENT);
    paint_text(ui, pos2(rect.left() + lead, top), words, ink, Color32::TRANSPARENT);
    response
}

/// Claude's proposal as the review card's document: the message as one column at `DOC_MEASURE`, with each numbered
/// change a `change_check`. `ticked` holds one flag per change, in order: it grows or shrinks to the changes, with new
/// ones ticked, as every change starts. Returns whether a change was flipped this frame. Lay it in a scroll area.
pub fn review_changes(ui: &mut Ui, cache: &mut CommonMarkCache, parts: &[ProposalPart], ticked: &mut Vec<bool>) -> bool {
    let count = parts.iter().filter(|part| matches!(part, ProposalPart::Change(_))).count();
    ticked.resize(count, true);
    let mut changed = false;
    let pieces = parts.iter().map(|part| match part {
        ProposalPart::Prose(text) => Part::Prose(text),
        ProposalPart::Group(title) => Part::Group(title),
        ProposalPart::Change(change) => Part::Item(change),
    });
    message(ui, cache, pieces, CHANGE_GAP, |ui, index, change| {
        if let Some(on) = ticked.get_mut(index) {
            changed |= ui.push_id(("review_change", index), |ui| change_check(ui, on, change.number, change.text)).inner.changed();
        }
    });
    changed
}

/// Numbers as words in a sentence: "3", "3 and 7", "3, 7 and 9".
fn listed(numbers: &[u32]) -> String {
    match numbers {
        [] => String::new(),
        [only] => only.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")),
    }
}

/// The review card's side slips: "N of M changes" with a `TALLY_FAT` mark for each change, lit where it is ticked, and
/// which are left out; then `reply`, the words Apply these changes sends, in `mono`. `changes` holds Claude's number for
/// each change and whether it is ticked, in Claude's order.
pub fn review_slip(ui: &mut Ui, changes: &[(u32, bool)], reply: &str) {
    let p = Palette::of(ui);
    let total = changes.len();
    let kept = changes.iter().filter(|(_, ticked)| *ticked).count();
    let out: Vec<u32> = changes.iter().filter(|(_, ticked)| !*ticked).map(|(number, _)| *number).collect();
    side_slip(ui, &format!("{kept} of {total} changes"), "", |ui| {
        let colors: Vec<Color32> = changes.iter().map(|&(_, ticked)| if ticked { p.text } else { p.off }).collect();
        tally_spoken(ui, &colors, TALLY_FAT, format!("{kept} of {total} ticked"));
        if out.is_empty() {
            slip_text(ui, "", "Untick any change you don't want. The rest go ahead when you press Apply these changes.");
        } else {
            slip_text(ui, &format!("{} left out: {}.", out.len(), listed(&out)), "Claude skips them, and they stay out of your profile.");
        }
    });
    reply_slip(ui, "Apply these changes sends Claude this reply:", reply);
}

/// A side slip that shows the reply an action sends Claude: `lead` in `muted`, then `reply` as it will go, in `mono` on
/// `field`, so the user reads exactly what is sent.
pub fn reply_slip(ui: &mut Ui, lead: &str, reply: &str) {
    let field = Palette::of(ui).field;
    side_slip(ui, "", "", |ui| {
        slip_text(ui, "", lead);
        let (x, y) = PICK_PADDING;
        Frame::new().fill(field).corner_radius(RADIUS_CARD).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            copyable(ui, |ui| add_text(ui, reply, Type::MONO, Ink::Text));
        });
    });
}

/// Which drop zone to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DropForm<'a> {
    /// The zone before any file is added: two stacked pages, `title` in the slip face over `sentence`, and Choose files….
    Full { title: &'a str, sentence: &'a str },
    /// The slim zone under a file list, to add more: `sentence` and a small Choose files….
    Slim { sentence: &'a str },
}

/// What the user did with a drop zone this frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DropZoneAction {
    /// Choose files… was clicked: the screen opens the system's file picker.
    pub choose: bool,
    /// The files dropped on the window this frame, each by its path.
    pub dropped: Vec<PathBuf>,
}

/// Where the user adds documents: paper with a dashed `line` edge, in `SWEEP_DASH` dashes, that takes `active` while
/// files are dragged over the window, holding `form`'s words and its Choose files…. Returns whether Choose files… was
/// clicked, and the paths of the files dropped on the window this frame.
pub fn drop_zone(ui: &mut Ui, form: DropForm) -> DropZoneAction {
    let p = Palette::of(ui);
    let (dragging, dropped) =
        ui.input(|input| (!input.raw.hovered_files.is_empty(), input.raw.dropped_files.iter().map(|file| file.path().to_path_buf()).collect()));
    let ((x, y), gap) = match form {
        DropForm::Full { .. } => (DROP_PADDING, DROP_GAP),
        DropForm::Slim { .. } => (DROP_SLIM_PADDING, DROP_SLIM_GAP),
    };
    let shown = Frame::new().fill(p.card).corner_radius(RADIUS_PANEL).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            match form {
                DropForm::Full { title, sentence } => {
                    let stack = vec2(DROP_PAGE.0 + DROP_PAGE_OFFSET.0, DROP_PAGE.1 + DROP_PAGE_OFFSET.1);
                    let (rect, _) = ui.allocate_exact_size(stack, Sense::hover());
                    let page = Vec2::from(DROP_PAGE);
                    paint_mini_page(ui, Rect::from_min_size(rect.min + vec2(0.0, DROP_PAGE_OFFSET.1), page), false);
                    paint_mini_page(ui, Rect::from_min_size(rect.min + vec2(DROP_PAGE_OFFSET.0, 0.0), page), false);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = CHANGE_GAP;
                        add_text(ui, title, Type::SLIP_TITLE, Ink::Text);
                        add_text(ui, sentence, Type::BODY, Ink::Muted);
                        ui.add_space(ITEM_GAP);
                        button(ui, "Choose files…").clicked()
                    })
                    .inner
                }
                DropForm::Slim { sentence } => {
                    add_text(ui, sentence, Type::BODY, Ink::Muted);
                    small_button(ui, "Choose files…").clicked()
                }
            }
        })
        .inner
    });
    let edge = shown.response.rect.shrink(HAIRLINE / 2.0);
    let corners = [edge.left_top(), edge.right_top(), edge.right_bottom(), edge.left_bottom(), edge.left_top()];
    let ink = if dragging { p.active } else { p.line };
    ui.painter().extend(Shape::dashed_line(&corners, Stroke::new(HAIRLINE, ink), SWEEP_DASH.0, SWEEP_DASH.1));
    DropZoneAction { choose: shown.inner, dropped }
}

/// A file the user added, as the file list shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AddedFile<'a> {
    /// Its name, as it came from the user's disk.
    pub name: &'a str,
    /// How many pages it has, when known.
    pub pages: Option<usize>,
}

/// The files added so far, on paper, one row each: a small page, the name in `mono` as plain text with its hidden
/// characters removed, cut at the right with the whole name on hover, its page count in `muted`, and a small Remove,
/// which a screen reader hears as Remove and the name. Rows are ruled off from each other. Returns the row whose Remove
/// was clicked.
pub fn file_list(ui: &mut Ui, files: &[AddedFile]) -> Option<usize> {
    let mut removed = None;
    paper(ui, false, |ui| {
        for (index, file) in files.iter().enumerate() {
            if index > 0 {
                divider(ui);
            }
            let name = plain_name(file.name);
            ui.push_id(("added_file", index), |ui| {
                Frame::new().inner_margin(Margin::symmetric(INSET, ROW_PADDING)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = f32::from(GUTTER);
                        let (page, _) = ui.allocate_exact_size(Vec2::from(FILE_PAGE), Sense::hover());
                        paint_mini_page(ui, page, true);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let enabled = ui.is_enabled();
                            let remove = small_button(ui, "Remove");
                            remove.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, format!("Remove {name}")));
                            if remove.clicked() {
                                removed = Some(index);
                            }
                            if let Some(pages) = file.pages {
                                add_text(ui, if pages == 1 { "1 page".to_owned() } else { format!("{pages} pages") }, Type::NOTE, Ink::Muted);
                            }
                            ui.with_layout(Layout::left_to_right(Align::Center), |ui| add_cut_text(ui, name.as_str(), Type::MONO, Ink::Text));
                        });
                    });
                });
            });
        }
    });
    removed
}

/// The CV's pages for the user to check, as Profile's CV documents draw them: page images only, fitted to the room left,
/// each over "Page N of M".
pub fn cv_check(ui: &mut Ui, pages: PageSet) {
    fitted_pages(ui, pages);
}

/// The CV check's side slip: the CV's `name` as plain text with its hidden characters removed, `sentence` in `muted`,
/// then Open PDF. Returns Open PDF's response.
pub fn cv_slip(ui: &mut Ui, name: &str, sentence: &str) -> Response {
    side_slip(ui, &plain_name(name), "", |ui| {
        slip_text(ui, "", sentence);
        ui.horizontal(|ui| small_button(ui, "Open PDF")).inner
    })
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    const CHANGES: [(u32, &str); 2] = [(3, "Add the years from the **profile**: 2016 to now."), (7, "Group the two freelance years under one heading.")];

    /// The box, the number and the words are one control: one checkbox named by the number and the words, one Tab stop
    /// each, whose ticked state is what a screen reader hears, and which Space flips.
    #[test]
    fn a_change_check_is_one_tab_stop_whose_state_is_announced() {
        let ticks = Rc::new(Cell::new([true, false]));
        let seen = Rc::clone(&ticks);
        let mut harness = Harness::builder().with_size([700.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            let _ = ui.button("Before");
            let mut now = seen.get();
            for (index, (number, text)) in CHANGES.iter().enumerate() {
                change_check(ui, &mut now[index], *number, text);
            }
            seen.set(now);
            let _ = ui.button("After");
        });
        harness.run();
        let checks: Vec<_> = harness.query_all_by_role(egui::accesskit::Role::CheckBox).collect();
        assert_eq!(checks.len(), 2, "one checkbox per change");
        let first = harness.get_by_label("3. Add the years from the profile: 2016 to now.");
        assert_eq!(first.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "the ticked change says so");
        let second = harness.get_by_label("7. Group the two freelance years under one heading.");
        assert_eq!(second.accesskit_node().toggled(), Some(egui::accesskit::Toggled::False), "the unticked one too");
        assert!(harness.query_by_label("3.").is_none(), "the number is no node of its own");

        harness.get_by_label("Before").focus();
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_label("3. Add the years from the profile: 2016 to now.").is_focused(), "Tab reaches the first change");
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_label("7. Group the two freelance years under one heading.").is_focused(), "one Tab moves to the next change");
        harness.key_press(egui::Key::Space);
        harness.run();
        assert_eq!(ticks.get(), [true, true], "Space ticks it");
        let second = harness.get_by_label("7. Group the two freelance years under one heading.");
        assert_eq!(second.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True), "and it is heard as ticked");
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_label("After").is_focused(), "the next Tab leaves the list");
    }

    /// A click on a change's words flips it, as a click on its box does.
    #[test]
    fn a_click_on_the_words_flips_the_change() {
        let on = Rc::new(Cell::new(true));
        let seen = Rc::clone(&on);
        let mut harness = Harness::builder().with_size([700.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
            let mut now = seen.get();
            change_check(ui, &mut now, CHANGES[1].0, CHANGES[1].1);
            seen.set(now);
        });
        harness.run();
        let words = harness.get_by_role(egui::accesskit::Role::CheckBox).rect().left_center() + egui::vec2(120.0, 0.0);
        harness.hover_at(words);
        harness.step();
        assert_eq!(harness.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand, "the words show the hand");
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: words,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
        assert!(!on.get(), "the click on the words unticks it");
    }

    const QUESTIONS: [SheetPart<'static>; 4] = [
        SheetPart::Prose("Answer in any order."),
        SheetPart::Group("Career direction"),
        SheetPart::Question(Question { number: 1, text: "What kind of role do you want next?", sub_items: &[], optional: false }),
        SheetPart::Question(Question {
            number: 2,
            text: "Which team brings out your best? (optional)",
            sub_items: &[SubItem { letter: 'a', text: "A former manager" }],
            optional: true,
        }),
    ];

    /// One field per question, named by its question, saying it can be skipped in the words its question needs.
    /// The answers grow to the questions, and typing in a field changes its own answer.
    #[test]
    fn each_question_gets_a_field_named_by_it() {
        let answers = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&answers);
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([800.0, 600.0]).with_context_setup(setup).build_ui(move |ui| {
            question_sheet(ui, &mut cache, &QUESTIONS, &mut seen.borrow_mut());
        });
        harness.run();
        assert_eq!(answers.borrow().len(), 2, "one answer per question");
        let fields: Vec<_> = harness.query_all_by_role(egui::accesskit::Role::MultilineTextInput).collect();
        assert_eq!(fields.len(), 2, "one field per question");
        for question in ["What kind of role do you want next?", "Which team brings out your best? (optional)"] {
            assert!(harness.query_by_role_and_label(egui::accesskit::Role::MultilineTextInput, question).is_some(), "{question}: its field is named by it");
        }
        let placeholder = |question: &str| {
            harness.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, question).accesskit_node().placeholder().map(str::to_owned)
        };
        assert_eq!(
            placeholder("Which team brings out your best? (optional)").as_deref(),
            Some(OPTIONAL_HINT),
            "the optional question's field says it may stay empty"
        );
        assert_eq!(placeholder("What kind of role do you want next?").as_deref(), Some(ANSWER_HINT), "the other field says it can be skipped");

        let first = harness.query_all_by_role(egui::accesskit::Role::MultilineTextInput).next().expect("a field");
        first.focus();
        harness.run();
        harness.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, "What kind of role do you want next?").type_text("Product designer");
        harness.run();
        assert_eq!(*answers.borrow(), ["Product designer".to_owned(), String::new()]);
    }

    /// Claude's words lose their hidden characters, keep a link's words and an image's alt text, and open nothing.
    #[test]
    fn claude_words_inside_a_control_are_plain_words() {
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(|ui| {
            let job = inline_job(
                ui,
                "Add **[the site](https://example.com)**\u{200B} and ![a chart](https://example.com/c.png)\u{202E}.",
                Type::TAKE,
                Color32::PLACEHOLDER,
            );
            assert_eq!(job.text, "Add the site and a chart.");
        });
        harness.run();
    }

    #[test]
    fn numbers_left_out_read_as_a_sentence() {
        assert_eq!(listed(&[3]), "3");
        assert_eq!(listed(&[3, 7]), "3 and 7");
        assert_eq!(listed(&[3, 7, 9]), "3, 7 and 9");
    }

    /// A file's name loses its hidden characters, and its Remove names it for a screen reader.
    #[test]
    fn a_file_name_shows_without_hidden_characters() {
        let removed = Rc::new(Cell::new(None));
        let seen = Rc::clone(&removed);
        let mut harness = Harness::builder().with_size([700.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
            let files = [AddedFile { name: "Sample\u{202E}fdp.CV.pdf", pages: Some(2) }, AddedFile { name: "Profile.pdf", pages: Some(1) }];
            if let Some(index) = file_list(ui, &files) {
                seen.set(Some(index));
            }
        });
        harness.run();
        assert!(harness.query_by_label("Samplefdp.CV.pdf").is_some(), "the reordering mark is gone");
        assert!(harness.query_by_label("1 page").is_some());
        harness.get_by_label("Remove Profile.pdf").click();
        harness.run();
        assert_eq!(removed.get(), Some(1));
    }
}
