use std::sync::Arc;

use egui::{Color32, CornerRadius, Galley, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use crate::shared_kernel::WebUrl;
use crate::ui_kit::Palette;
use crate::ui_kit::marks::{CountChip, paint_key};
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{code_box, hairline};
use crate::ui_kit::text::{line, paint_centred, rich};
use crate::ui_kit::tokens::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_PADDING, CHIP_HEIGHT, COMMAND_NAME_WIDTH, FOCUS_OFFSET, FOCUS_RING, HAIRLINE, KEYS_GAP, KEY_SIZE, NAV_TAB_GAP, NAV_TAB_HEIGHT, NAV_TAB_PADDING, PICK_ROW_HEIGHT, PICK_ROW_INLINE, RADIUS_BUTTON, ROW_ITEM_GAP,
    SEGMENT_HEIGHT, SEGMENT_PADDING, SEGMENT_TOP, SMALL_BUTTON_HEIGHT, SMALL_BUTTON_PADDING, STATUS_DOT, STATUS_SEGMENT_HEIGHT, STATUS_SEGMENT_PADDING, TAB_TOP, Type,
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

fn look_colors(p: &Palette, look: Look, hovered: bool) -> (Color32, Color32) {
    match look {
        Look::Primary => (p.primary, p.primary),
        Look::On => (p.text, p.text),
        Look::Plain if hovered => (p.raised, p.line),
        Look::Danger if hovered => (p.raised, p.closed),
        Look::Plain => (Color32::TRANSPARENT, p.line),
        Look::Danger => (Color32::TRANSPARENT, p.closed),
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
    let (fill, border) = look_colors(&p, look, response.hovered());
    ui.painter().rect(rect, RADIUS_BUTTON, fill, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(fit.padding, (fit.height - galley.size().y) / 2.0), galley.size());
    if matches!(look, Look::Primary) && response.hovered() {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    ui.painter().galley(text_rect.min, galley, ink);
    if let Some(key) = key {
        let hint = Rect::from_center_size(egui::pos2(text_rect.right() + BUTTON_GAP + KEY_SIZE / 2.0, rect.center().y), vec2(KEY_SIZE, KEY_SIZE));
        paint_key(ui, hint, key, ink);
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

/// A button on the navy frame that names its keyboard shortcut. The keys are plain glyphs, with no box.
pub fn shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    let p = Palette::of(ui);
    let ink = p.chrome_ink;
    let label = line(ui, text, NORMAL.font, ink, f32::INFINITY);
    let glyphs: Vec<_> = keys.iter().map(|key| line(ui, key, Type::KEYS, p.chrome_muted, f32::INFINITY)).collect();
    let glyphs_width = glyphs.iter().map(|glyph| glyph.size().x).sum::<f32>() + KEYS_GAP * glyphs.len().saturating_sub(1) as f32;
    // The keys sit 2px further out than a button's own gap.
    let lead = if glyphs.is_empty() { 0.0 } else { BUTTON_GAP + 2.0 };
    let size = vec2(NORMAL.padding * 2.0 + label.size().x + lead + glyphs_width, NORMAL.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect(rect, RADIUS_BUTTON, p.chrome_tab, Stroke::new(HAIRLINE, p.chrome_line), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(NORMAL.padding, (NORMAL.height - label.size().y) / 2.0), label.size());
    if response.hovered() {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    let mut x = text_rect.right() + lead;
    ui.painter().galley(text_rect.min, label, ink);
    for glyph in glyphs {
        let at = egui::pos2(x, rect.center().y - glyph.size().y / 2.0);
        x += glyph.size().x + KEYS_GAP;
        ui.painter().galley(at, glyph, p.chrome_muted);
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
pub fn reveal_command(ui: &mut Ui, line: &str) -> Response {
    let id = egui::Id::NULL.with(("reveal_command", line));
    let open = ui.data(|data| data.get_temp::<bool>(id)).unwrap_or(false);
    let text = if open { "Hide the command" } else { "Show the command" };
    let link = rich(ui, text, Type::NOTE, Tone::Active);
    let response = ui.add(egui::Link::new(link));
    let open = open != response.clicked();
    ui.data_mut(|data| data.insert_temp(id, open));
    if open {
        code_box(ui, |ui| {
            let line = rich(ui, line, Type::MONO, Ink::Text);
            ui.add(egui::Label::new(line).wrap());
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

fn tab_look(ui: &Ui, response: &Response, on: bool) -> (Color32, Color32) {
    let p = Palette::of(ui);
    match (on, response.hovered()) {
        (true, _) => (p.sheet, p.text),
        (false, true) => (p.chrome_tab, p.chrome_ink),
        (false, false) => (Color32::TRANSPARENT, p.chrome_muted),
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

/// A folder tab on the navy frame. The active tab takes the sheet's fill, and its mark the darker stamp.
pub fn folder_tab(ui: &mut Ui, label: &str, mark: TabMark, on: bool) -> Response {
    let p = Palette::of(ui);
    let font = if on { Type::TAB_ON } else { Type::TAB };
    let width = line(ui, label, font, p.chrome_muted, f32::INFINITY).size().x;
    let chip = match mark {
        TabMark::Count(n) if n > 0 => Some(CountChip::layout(ui, n, !on)),
        _ => None,
    };
    let mark_width = match (&chip, mark) {
        (Some(chip), _) => chip.size().x,
        (None, TabMark::Failed) => STATUS_DOT,
        _ => 0.0,
    };
    let lead = if mark_width > 0.0 { BUTTON_GAP } else { 0.0 };
    let size = vec2(width + lead + mark_width + NAV_TAB_PADDING * 2.0, NAV_TAB_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, ink) = tab_look(ui, &response, on);
    ui.painter().rect_filled(rect, TAB_TOP, fill);
    let text = line(ui, label, font, ink, f32::INFINITY);
    let left = rect.left() + NAV_TAB_PADDING;
    let mark_left = left + width + lead;
    ui.painter().galley(egui::pos2(left, rect.center().y - text.size().y / 2.0), text, ink);
    if let Some(chip) = chip {
        let chip_rect = Rect::from_min_size(egui::pos2(mark_left, rect.center().y - CHIP_HEIGHT / 2.0), chip.size());
        chip.paint(ui, chip_rect);
    } else if mark == TabMark::Failed {
        let square = Rect::from_min_size(egui::pos2(mark_left, rect.center().y - STATUS_DOT / 2.0), vec2(STATUS_DOT, STATUS_DOT));
        ui.painter().rect_filled(square, 0.0, if on { p.closed } else { p.chrome_alert });
    }
    focus_ring(ui, &response, TAB_TOP);
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

fn segment(ui: &mut Ui, label: &str, on: bool, look: SegmentLook, inks: SegmentInks) -> Response {
    let font = if on { look.font_on } else { look.font };
    let ink = if on { inks.on.1 } else { inks.off };
    let galley = line(ui, label, font, ink, ui.available_width() - look.padding * 2.0);
    let size = vec2(galley.size().x + look.padding * 2.0, look.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if on {
        ui.painter().rect_filled(rect, look.radius, inks.on.0);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, look.radius, inks.hover);
    }
    paint_centred(ui, rect, galley, ink);
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
        ui.spacing_mut().item_spacing = vec2(NAV_TAB_GAP, 0.0);
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

/// The small switch that lives in the status bar, on the navy frame.
pub fn status_segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, String)]) -> Option<T> {
    let p = Palette::of(ui);
    let look = SegmentLook { height: STATUS_SEGMENT_HEIGHT, padding: STATUS_SEGMENT_PADDING, radius: CornerRadius::same(RADIUS_BUTTON), font: Type::SMALL, font_on: Type::SMALL };
    let inks = SegmentInks { on: (p.chrome_tab, p.chrome_ink), off: p.chrome_muted, hover: p.chrome_tab };
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = NAV_TAB_GAP;
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
    if on || response.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.panel);
    }
    let ink = if on { p.text } else { p.muted };
    let galley = line(ui, text, style, ink, rect.width() - crate::ui_kit::tokens::INSET as f32 * 2.0);
    let at = egui::pos2(rect.left() + crate::ui_kit::tokens::INSET as f32, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, ink);
    focus_ring(ui, &response, 0);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, text));
    response
}

/// A command in the catalogue: its name, what it does and what it writes. Painted whole, so a test finds one label.
pub fn command_row(ui: &mut Ui, name: &str, summary: &str, writes: &str, on: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), Sense::click());
    if on || response.hovered() {
        ui.painter().rect_filled(rect, 0.0, p.panel);
    }
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
    ui.painter().galley(named_at, named, p.text);
    ui.painter().galley(summed_at, summed, p.text);
    ui.painter().galley(written_at, written, p.faint);
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
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(|ui| {
            reveal_command(ui, LINE);
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
