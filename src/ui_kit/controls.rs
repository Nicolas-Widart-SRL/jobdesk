use std::sync::Arc;

use egui::{Color32, CornerRadius, Galley, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use crate::shared_kernel::WebUrl;
use crate::ui_kit::Palette;
use crate::ui_kit::marks::{CountChip, paint_key};
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{code_box, hairline, on_desk};
use crate::ui_kit::text::{add_text, line, paint_centred, paint_text, rich};
use crate::ui_kit::tokens::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_PADDING, CHIP_HEIGHT, COMMAND_NAME_WIDTH, FOCUS_OFFSET, FOCUS_RING, HAIRLINE, KEYS_GAP, KEY_SIZE, MARK_HEIGHT, MARK_LINE, MARK_OVERHANG, MARK_RULE, NAV_TAB_HEIGHT, NAV_TAB_PADDING, PICK_ROW_HEIGHT, PICK_ROW_INLINE, RADIUS_BUTTON, ROW_ITEM_GAP,
    SEGMENT_GAP, SEGMENT_HEIGHT, SEGMENT_PADDING, SEGMENT_TOP, SMALL_BUTTON_HEIGHT, SMALL_BUTTON_PADDING, STATUS_DOT, STATUS_SEGMENT_HEIGHT, STATUS_SEGMENT_PADDING, Type,
};

#[derive(Clone, Copy)]
enum Look {
    Plain,
    Primary,
    Danger,
    On,
}

#[derive(Clone, Copy)]
struct Fit {
    height: f32,
    padding: f32,
    font: Type,
}

const NORMAL: Fit = Fit { height: BUTTON_HEIGHT, padding: BUTTON_PADDING, font: Type::BUTTON };
const SMALL: Fit = Fit { height: SMALL_BUTTON_HEIGHT, padding: SMALL_BUTTON_PADDING, font: Type::SMALL_BUTTON };
const COMMAND: Fit = Fit { height: SMALL_BUTTON_HEIGHT, padding: SMALL_BUTTON_PADDING, font: Type::MONO };

pub(in crate::ui_kit) fn focus_ring(ui: &Ui, response: &Response, radius: impl Into<CornerRadius>) {
    if response.has_focus() {
        let color = Palette::of(ui).active;
        ui.painter().rect_stroke(response.rect.expand(FOCUS_OFFSET), radius, Stroke::new(FOCUS_RING, color), StrokeKind::Outside);
    }
}

/// A plain button at rest is clear on a surface, and fills with paper on the desk, so the grid never runs behind its label.
fn look_colors(p: &Palette, look: Look, hovered: bool, desk: bool) -> (Color32, Color32) {
    let rest = if desk { p.card } else { Color32::TRANSPARENT };
    match look {
        Look::Primary => (p.primary, p.primary),
        Look::On => (p.text, p.text),
        Look::Plain if hovered => (p.raised, p.line),
        Look::Danger if hovered => (p.raised, p.closed),
        Look::Plain => (rest, p.line),
        Look::Danger => (rest, p.closed),
    }
}

fn press(ui: &mut Ui, text: &str, look: Look, fit: Fit, key: Option<&str>) -> Response {
    press_as(ui, text, look, fit, key, None)
}

/// `selected` makes a screen reader hear the button as a toggle that is on or off.
fn press_as(ui: &mut Ui, text: &str, look: Look, fit: Fit, key: Option<&str>, selected: Option<bool>) -> Response {
    let p = Palette::of(ui);
    let ink = match look {
        Look::Plain => p.text,
        Look::Primary => p.on_primary,
        Look::Danger => p.closed,
        Look::On => p.sheet,
    };
    let keys = key.map_or(0.0, |_| KEY_SIZE + BUTTON_GAP);
    let room = ui.available_width() - fit.padding * 2.0 - keys;
    let galley = line(ui, text, fit.font, ink, room);
    let size = vec2(galley.size().x + fit.padding * 2.0 + keys, fit.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, border) = look_colors(&p, look, response.hovered(), on_desk(ui));
    ui.painter().rect(rect, RADIUS_BUTTON, fill, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(fit.padding, (fit.height - galley.size().y) / 2.0), galley.size());
    if matches!(look, Look::Primary) && response.hovered() {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    paint_text(ui, text_rect.min, galley, ink, fill);
    if let Some(key) = key {
        let hint = Rect::from_center_size(egui::pos2(text_rect.right() + BUTTON_GAP + KEY_SIZE / 2.0, rect.center().y), vec2(KEY_SIZE, KEY_SIZE));
        paint_key(ui, hint, key, ink, fill);
    }
    focus_ring(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| match selected {
        Some(on) => WidgetInfo::selected(Role::Button, ui.is_enabled(), on, text),
        None => WidgetInfo::labeled(Role::Button, ui.is_enabled(), text),
    });
    response
}

/// The width a `button`, `primary_button` or `danger_button` with `text` takes, to keep room for one laid out later.
pub fn button_width(ui: &Ui, text: &str) -> f32 {
    line(ui, text, NORMAL.font, Palette::of(ui).text, f32::INFINITY).size().x + NORMAL.padding * 2.0
}

pub fn button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Plain, NORMAL, None)
}

pub fn small_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Plain, SMALL, None)
}

/// A small button that also answers to a single key.
pub fn small_key_button(ui: &mut Ui, text: &str, key: &str) -> Response {
    press(ui, text, Look::Plain, SMALL, Some(key))
}

/// A small button that stays pressed while `on`.
pub fn small_toggle(ui: &mut Ui, text: &str, on: bool) -> Response {
    press_as(ui, text, if on { Look::On } else { Look::Plain }, SMALL, None, Some(on))
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Primary, NORMAL, None)
}

pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Danger, NORMAL, None)
}

pub fn small_danger_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Danger, SMALL, None)
}

/// A button that also answers to a single key.
pub fn key_button(ui: &mut Ui, text: &str, key: &str) -> Response {
    press(ui, text, Look::Plain, NORMAL, Some(key))
}

pub fn primary_key_button(ui: &mut Ui, text: &str, key: &str) -> Response {
    press(ui, text, Look::Primary, NORMAL, Some(key))
}

/// A plain button in the menu that names its keyboard shortcut. The keys are plain glyphs, with no box.
pub fn shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    let p = Palette::of(ui);
    let ink = p.text;
    let label = line(ui, text, NORMAL.font, ink, f32::INFINITY);
    let glyphs: Vec<_> = keys.iter().map(|key| line(ui, key, Type::KEYS, p.muted, f32::INFINITY)).collect();
    let glyphs_width = glyphs.iter().map(|glyph| glyph.size().x).sum::<f32>() + KEYS_GAP * glyphs.len().saturating_sub(1) as f32;
    // The keys sit 2px further out than a button's own gap.
    let lead = if glyphs.is_empty() { 0.0 } else { BUTTON_GAP + 2.0 };
    let size = vec2(NORMAL.padding * 2.0 + label.size().x + lead + glyphs_width, NORMAL.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect(rect, RADIUS_BUTTON, p.card, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(NORMAL.padding, (NORMAL.height - label.size().y) / 2.0), label.size());
    if response.hovered() {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    let mut x = text_rect.right() + lead;
    paint_text(ui, text_rect.min, label, ink, p.card);
    for glyph in glyphs {
        let at = egui::pos2(x, rect.center().y - glyph.size().y / 2.0);
        x += glyph.size().x + KEYS_GAP;
        paint_text(ui, at, glyph, p.muted, p.card);
    }
    focus_ring(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), text));
    response
}

fn command_request_id() -> egui::Id {
    egui::Id::NULL.with("requested_command")
}

fn request_command(ui: &Ui, command: &str) {
    ui.ctx().data_mut(|data| data.insert_temp(command_request_id(), command.to_owned()));
}

/// A button showing `label` that opens `line` in the launcher. The shell picks the request up.
fn command_action_with(ui: &mut Ui, label: &str, line: &str, hint: &str, look: Look, fit: Fit) -> Response {
    let response = press(ui, label, look, fit, None).on_hover_text(hint);
    if response.clicked() {
        request_command(ui, line);
    }
    response
}

/// A command button on any screen, labelled with the command itself.
pub fn command_button(ui: &mut Ui, command: &str) {
    command_action_with(ui, command, command, "Open in command launcher", Look::Plain, COMMAND);
}

pub fn danger_command_button(ui: &mut Ui, command: &str) {
    command_action_with(ui, command, command, "Open in command launcher", Look::Danger, COMMAND);
}

/// A button with a plain label that opens `line` in the command launcher.
pub fn command_action(ui: &mut Ui, label: &str, line: &str) -> Response {
    command_action_with(ui, label, line, line, Look::Plain, NORMAL)
}

pub fn primary_command_action(ui: &mut Ui, label: &str, line: &str) -> Response {
    command_action_with(ui, label, line, line, Look::Primary, NORMAL)
}

pub fn small_command_action(ui: &mut Ui, label: &str, line: &str) -> Response {
    command_action_with(ui, label, line, line, Look::Plain, SMALL)
}

pub fn take_command_request(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|data| data.remove_temp(command_request_id()))
}

/// "Show the command" under an action, which opens the command line it stands for in a code box.
/// The line shows as plain wrapped text, never as Markdown or a link, and clicking starts nothing.
/// The caller holds `open`, so it can close the line when what it belongs to goes away.
pub fn reveal_command(ui: &mut Ui, line: &str, open: &mut bool) -> Response {
    let text = if *open { "Hide the command" } else { "Show the command" };
    let link = rich(ui, text, Type::NOTE, Tone::Active);
    let response = ui.add(egui::Link::new(link));
    if response.clicked() {
        *open = !*open;
    }
    if *open {
        code_box(ui, |ui| {
            // A command reads from its start, wherever the link sits.
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                add_text(ui, line, Type::MONO, Ink::Text);
            });
        });
    }
    response
}

fn open_link_with(ui: &mut Ui, label: &str, url: Option<&WebUrl>, fit: Fit) {
    let Some(url) = url else { return };
    let response = press(ui, label, Look::Plain, fit, None).on_hover_text(url.as_str());
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url.as_str()));
    }
}

pub fn open_link(ui: &mut Ui, label: &str, url: Option<&WebUrl>) {
    open_link_with(ui, label, url, SMALL);
}

/// The same link at full button size, for a screen's head.
pub fn open_link_button(ui: &mut Ui, label: &str, url: Option<&WebUrl>) {
    open_link_with(ui, label, url, NORMAL);
}

/// What a folder tab shows after its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabMark {
    None,
    /// How many things there wait on the user. Zero shows nothing.
    Count(usize),
    /// The feature behind the tab could not be read.
    Failed,
}

/// The highlighter under a tab's label: `mark` with a `turn` rule when open, `line` with no rule under the pointer.
/// `line` is the palest neutral that stands off both the desk's base and its grid lines.
fn tab_swipe(ui: &Ui, response: &Response, on: bool) -> Option<(Color32, Option<Color32>)> {
    let p = Palette::of(ui);
    match (on, response.hovered()) {
        (true, _) => Some((p.mark, Some(p.turn))),
        (false, true) => Some((p.line, None)),
        (false, false) => None,
    }
}

fn tab_info(label: &str, mark: TabMark, on: bool) -> WidgetInfo {
    let title = match mark {
        TabMark::Count(n) if n > 0 => format!("{label}, {n} need you"),
        TabMark::Failed => format!("{label}, could not be read"),
        _ => label.to_owned(),
    };
    WidgetInfo::selected(Role::Tab, true, on, title)
}

/// A tab in the menu, on the desk. The open tab's label is highlighted: a pale swipe over a stamp-ink rule.
/// Its mark sits after the label, outside the swipe.
pub fn folder_tab(ui: &mut Ui, label: &str, mark: TabMark, on: bool) -> Response {
    let p = Palette::of(ui);
    let font = if on { Type::TAB_ON } else { Type::TAB };
    let ink = if on { p.text } else { p.muted };
    let text = line(ui, label, font, ink, f32::INFINITY);
    let width = text.size().x;
    let chip = match mark {
        TabMark::Count(n) if n > 0 => Some(CountChip::layout(ui, n)),
        _ => None,
    };
    let mark_width = match (&chip, mark) {
        (Some(chip), _) => chip.size().x,
        (None, TabMark::Failed) => STATUS_DOT,
        _ => 0.0,
    };
    // The mark clears the swipe by 2px, so a count never stacks on the highlighter.
    let lead = if mark_width > 0.0 { MARK_OVERHANG + 2.0 } else { 0.0 };
    let size = vec2(width + lead + mark_width + NAV_TAB_PADDING * 2.0, NAV_TAB_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let left = rect.left() + NAV_TAB_PADDING;
    let mark_left = left + width + lead;
    let at = egui::pos2(left, rect.center().y - text.size().y / 2.0);
    let swipe = tab_swipe(ui, &response, on);
    if let Some((fill, rule)) = swipe {
        // The swipe runs under the lower part of the label, its bottom 1px under the label's line box.
        let bottom = rect.center().y + MARK_LINE / 2.0 + 1.0;
        let swipe = Rect::from_min_max(egui::pos2(left - MARK_OVERHANG, bottom - MARK_HEIGHT), egui::pos2(left + width + MARK_OVERHANG, bottom));
        ui.painter().rect_filled(swipe, 0.0, fill);
        if let Some(rule) = rule {
            ui.painter().rect_filled(Rect::from_min_max(egui::pos2(swipe.left(), swipe.bottom() - MARK_RULE), swipe.max), 0.0, rule);
        }
    }
    // A resting tab's label stands on the bare grid, so it takes the desk's base. The swipe keeps it under a highlighted one.
    paint_text(ui, at, text, ink, swipe.map_or(Color32::TRANSPARENT, |(fill, _)| fill));
    if let Some(chip) = chip {
        let chip_rect = Rect::from_min_size(egui::pos2(mark_left, rect.center().y - CHIP_HEIGHT / 2.0), chip.size());
        chip.paint(ui, chip_rect);
    } else if mark == TabMark::Failed {
        let square = Rect::from_min_size(egui::pos2(mark_left, rect.center().y - STATUS_DOT / 2.0), vec2(STATUS_DOT, STATUS_DOT));
        ui.painter().rect_filled(square, 0.0, p.closed);
    }
    focus_ring(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| tab_info(label, mark, on));
    response
}

#[derive(Clone, Copy)]
struct SegmentLook {
    height: f32,
    padding: f32,
    radius: CornerRadius,
    font: Type,
    font_on: Type,
}

/// A segment's colours: fill and ink when on, ink when off, and the fill under the pointer.
#[derive(Clone, Copy)]
struct SegmentInks {
    on: (Color32, Color32),
    off: Color32,
    hover: Color32,
}

/// A segment is as wide as its label in the heavier face, on or off, so picking one never moves the others.
fn segment(ui: &mut Ui, label: &str, on: bool, look: SegmentLook, inks: SegmentInks) -> Response {
    let font = if on { look.font_on } else { look.font };
    let ink = if on { inks.on.1 } else { inks.off };
    let room = ui.available_width() - look.padding * 2.0;
    let galley = line(ui, label, font, ink, room);
    let widest = if on { galley.size().x } else { line(ui, label, look.font_on, ink, room).size().x.max(galley.size().x) };
    let size = vec2(widest + look.padding * 2.0, look.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = match (on, response.hovered()) {
        (true, _) => inks.on.0,
        (false, true) => inks.hover,
        (false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, look.radius, fill);
    paint_centred(ui, rect, galley, ink, fill);
    focus_ring(ui, &response, look.radius);
    response.widget_info(|| WidgetInfo::selected(Role::Tab, true, on, label));
    response
}

/// Dividers inside the folder. Returns the option picked this frame.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, String)]) -> Option<T> {
    let p = Palette::of(ui);
    let look = SegmentLook { height: SEGMENT_HEIGHT, padding: SEGMENT_PADDING, radius: SEGMENT_TOP, font: Type::TAB, font_on: Type::TAB_ON };
    let mut picked = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(SEGMENT_GAP, 0.0);
        ui.horizontal_wrapped(|ui| {
            for (value, label) in options {
                if segment(ui, label, *value == current, look, SegmentInks { on: (p.text, p.sheet), off: p.muted, hover: p.panel }).clicked() {
                    picked = Some(*value);
                }
            }
        });
        hairline(ui, p.text);
    });
    picked
}

/// The small switch that lives in the status strip.
pub fn status_segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, String)]) -> Option<T> {
    let p = Palette::of(ui);
    let look = SegmentLook { height: STATUS_SEGMENT_HEIGHT, padding: STATUS_SEGMENT_PADDING, radius: CornerRadius::same(RADIUS_BUTTON), font: Type::SMALL, font_on: Type::TAG };
    let inks = SegmentInks { on: (p.raised, p.text), off: p.muted, hover: p.panel };
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SEGMENT_GAP;
        for (value, label) in options {
            if segment(ui, label, *value == current, look, inks).clicked() {
                picked = Some(*value);
            }
        }
    });
    picked
}

/// A pickable row in a list. `text` is cut to one line.
pub fn select_row(ui: &mut Ui, text: &str, style: Type, on: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), Sense::click());
    let fill = if on || response.hovered() { p.panel } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, 0.0, fill);
    let ink = if on { p.text } else { p.muted };
    let galley = line(ui, text, style, ink, rect.width() - crate::ui_kit::tokens::INSET as f32 * 2.0);
    let at = egui::pos2(rect.left() + crate::ui_kit::tokens::INSET as f32, rect.center().y - galley.size().y / 2.0);
    paint_text(ui, at, galley, ink, fill);
    focus_ring(ui, &response, 0);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, text));
    response
}

/// A command in the catalogue: its name, what it does and what it writes. Painted whole, so a test finds one label.
pub fn command_row(ui: &mut Ui, name: &str, summary: &str, writes: &str, on: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), Sense::click());
    let fill = if on || response.hovered() { p.panel } else { Color32::TRANSPARENT };
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), HAIRLINE)), 0.0, p.line);
    let left = rect.left() + PICK_ROW_INLINE;
    let right = rect.right() - PICK_ROW_INLINE;
    let summary_left = left + COMMAND_NAME_WIDTH;
    let written = line(ui, writes, Type::MONO, p.faint, rect.width() / 2.0);
    let named = line(ui, name, Type::MONO, p.text, COMMAND_NAME_WIDTH - ROW_ITEM_GAP);
    let room = (right - written.size().x - ROW_ITEM_GAP - summary_left).max(0.0);
    let summed = line(ui, summary, Type::BODY, p.text, room);
    let centred = |galley: &Arc<Galley>, x: f32| egui::pos2(x, rect.center().y - galley.size().y / 2.0);
    let (named_at, summed_at, written_at) = (centred(&named, left), centred(&summed, summary_left), centred(&written, right - written.size().x));
    paint_text(ui, named_at, named, p.text, fill);
    paint_text(ui, summed_at, summed, p.text, fill);
    paint_text(ui, written_at, written, p.faint, fill);
    focus_ring(ui, &response, 0);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, format!("{name} {summary}")));
    response
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    const LINE: &str = "/apply https://tessel.example/jobs/1";

    #[test]
    fn a_tab_reads_its_count_as_need_you() {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([600.0, 80.0]).with_context_setup(setup).build_ui(|ui| {
            ui.horizontal(|ui| {
                folder_tab(ui, "Today", TabMark::Count(8), true);
                folder_tab(ui, "Jobs", TabMark::Count(0), false);
                folder_tab(ui, "Applications", TabMark::Failed, false);
                folder_tab(ui, "Tasks", TabMark::Count(1), false);
            });
        });
        harness.run();
        harness.get_by_label("Today, 8 need you");
        harness.get_by_label("Jobs");
        harness.get_by_label("Applications, could not be read");
        harness.get_by_label("Tasks, 1 need you");
    }

    #[test]
    fn the_command_shows_only_after_a_click() {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut open = false;
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
            reveal_command(ui, LINE, &mut open);
        });
        harness.run();
        assert!(harness.query_by_label(LINE).is_none());
        harness.get_by_label("Show the command").click();
        harness.run();
        harness.get_by_label(LINE);
        harness.get_by_label("Hide the command").click();
        harness.run();
        assert!(harness.query_by_label(LINE).is_none());
        assert!(take_command_request(&harness.ctx).is_none());
    }
}
