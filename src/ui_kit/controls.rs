use std::sync::Arc;

use egui::{Color32, CornerRadius, Galley, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use crate::shared_kernel::WebUrl;
use crate::ui_kit::Palette;
use crate::ui_kit::marks::paint_key;
use crate::ui_kit::text::{line, paint_centred};
use crate::ui_kit::tokens::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_PADDING, FOCUS_OFFSET, FOCUS_RING, HAIRLINE, KEY_SIZE, NAV_TAB_GAP, NAV_TAB_HEIGHT, NAV_TAB_PADDING, PICK_ROW_HEIGHT, RADIUS_BUTTON,
    SEGMENT_HEIGHT, SEGMENT_PADDING, SEGMENT_TOP, SMALL_BUTTON_HEIGHT, SMALL_BUTTON_PADDING, STATUS_SEGMENT_HEIGHT, STATUS_SEGMENT_PADDING, TAB_LEFT, TAB_TOP, Type,
};

#[derive(Clone, Copy)]
enum Look {
    Plain,
    Primary,
    Danger,
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

fn focus_ring(ui: &Ui, response: &Response, radius: impl Into<CornerRadius>) {
    if response.has_focus() {
        let color = Palette::of(ui).active;
        ui.painter().rect_stroke(response.rect.expand(FOCUS_OFFSET), radius, Stroke::new(FOCUS_RING, color), StrokeKind::Outside);
    }
}

fn press(ui: &mut Ui, text: &str, look: Look, fit: Fit, key: Option<&str>) -> Response {
    let p = Palette::of(ui);
    let ink = match look {
        Look::Plain => p.text,
        Look::Primary => p.on_primary,
        Look::Danger => p.closed,
    };
    let keys = key.map_or(0.0, |_| KEY_SIZE + BUTTON_GAP);
    let room = ui.available_width() - fit.padding * 2.0 - keys;
    let galley = line(ui, text, fit.font, ink, room);
    let size = vec2(galley.size().x + fit.padding * 2.0 + keys, fit.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, border) = match look {
        Look::Primary => (p.primary, p.primary),
        Look::Plain if response.hovered() => (p.raised, p.line),
        Look::Danger if response.hovered() => (p.raised, p.closed),
        Look::Plain => (Color32::TRANSPARENT, p.line),
        Look::Danger => (Color32::TRANSPARENT, p.closed),
    };
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
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), text));
    response
}

pub fn button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Plain, NORMAL, None)
}

pub fn small_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Plain, SMALL, None)
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Primary, NORMAL, None)
}

pub fn danger_button(ui: &mut Ui, text: &str) -> Response {
    press(ui, text, Look::Danger, NORMAL, None)
}

/// A button that also answers to a single key.
pub fn key_button(ui: &mut Ui, text: &str, key: &str) -> Response {
    press(ui, text, Look::Plain, NORMAL, Some(key))
}

pub fn primary_key_button(ui: &mut Ui, text: &str, key: &str) -> Response {
    press(ui, text, Look::Primary, NORMAL, Some(key))
}

fn command_request_id() -> egui::Id {
    egui::Id::NULL.with("requested_command")
}

/// A command button on any screen. The shell picks the request up and opens the launcher.
pub fn command_button(ui: &mut Ui, command: &str) {
    let clicked = press(ui, command, Look::Plain, COMMAND, None).on_hover_text("Open in command launcher").clicked();
    if clicked {
        ui.ctx().data_mut(|data| data.insert_temp(command_request_id(), command.to_owned()));
    }
}

pub fn take_command_request(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|data| data.remove_temp(command_request_id()))
}

pub fn open_link(ui: &mut Ui, label: &str, url: Option<&WebUrl>) {
    let Some(url) = url else { return };
    let response = small_button(ui, label).on_hover_text(url.as_str());
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url.as_str()));
    }
}

/// The label of a tab, with its count in a quieter voice.
struct TabText {
    label: Arc<Galley>,
    count: Option<Arc<Galley>>,
}

impl TabText {
    fn layout(ui: &Ui, label: &str, count: Option<&str>, on: bool, ink: Color32, room: f32) -> Self {
        let label = line(ui, label, if on { Type::TAB_ON } else { Type::TAB }, ink, room);
        let faint = Palette::of(ui).faint;
        let count = count.map(|count| line(ui, count, Type::SMALL, faint, room - label.size().x - BUTTON_GAP));
        Self { label, count }
    }

    fn width(&self) -> f32 {
        self.label.size().x + self.count.as_ref().map_or(0.0, |count| count.size().x + BUTTON_GAP)
    }

    fn paint(self, ui: &Ui, left: f32, centre: f32, ink: Color32) {
        let right = left + self.label.size().x;
        ui.painter().galley(egui::pos2(left, centre - self.label.size().y / 2.0), self.label, ink);
        if let Some(count) = self.count {
            ui.painter().galley(egui::pos2(right + BUTTON_GAP, centre - count.size().y / 2.0), count, ink);
        }
    }
}

fn tab_look(ui: &Ui, response: &Response, on: bool) -> (Color32, Color32) {
    let p = Palette::of(ui);
    match (on, response.hovered()) {
        (true, _) => (p.sheet, p.text),
        (false, true) => (p.raised, p.text),
        (false, false) => (p.panel, p.muted),
    }
}

fn tab_info(label: &str, count: Option<&str>, on: bool) -> WidgetInfo {
    let title = count.map_or_else(|| label.to_owned(), |count| format!("{label}  {count}"));
    WidgetInfo::selected(Role::Tab, true, on, title)
}

/// A folder tab standing on the desk. The active tab takes the sheet's fill.
pub fn folder_tab(ui: &mut Ui, label: &str, count: Option<&str>, on: bool) -> Response {
    let muted = Palette::of(ui).muted;
    let measured = TabText::layout(ui, label, count, on, muted, f32::INFINITY);
    let size = vec2(measured.width() + NAV_TAB_PADDING * 2.0, NAV_TAB_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, ink) = tab_look(ui, &response, on);
    ui.painter().rect_filled(rect, TAB_TOP, fill);
    TabText::layout(ui, label, count, on, ink, f32::INFINITY).paint(ui, rect.left() + NAV_TAB_PADDING, rect.center().y, ink);
    focus_ring(ui, &response, TAB_TOP);
    response.widget_info(|| tab_info(label, count, on));
    response
}

/// The same tab on the sidebar's edge. It runs to the panel edge, so the active tab joins the sheet.
pub fn side_tab(ui: &mut Ui, label: &str, count: Option<&str>, on: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), NAV_TAB_HEIGHT), Sense::click());
    let (fill, ink) = tab_look(ui, &response, on);
    let mut tab = rect;
    tab.set_right(ui.clip_rect().right());
    ui.painter().rect_filled(tab, TAB_LEFT, fill);
    let padding = SEGMENT_PADDING;
    TabText::layout(ui, label, count, on, ink, tab.width() - padding * 2.0).paint(ui, rect.left() + padding, rect.center().y, ink);
    focus_ring(ui, &response, TAB_LEFT);
    response.widget_info(|| tab_info(label, count, on));
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

fn segment(ui: &mut Ui, label: &str, on: bool, look: SegmentLook, on_colors: (Color32, Color32)) -> Response {
    let p = Palette::of(ui);
    let font = if on { look.font_on } else { look.font };
    let ink = if on { on_colors.1 } else { p.muted };
    let galley = line(ui, label, font, ink, ui.available_width() - look.padding * 2.0);
    let size = vec2(galley.size().x + look.padding * 2.0, look.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if on {
        ui.painter().rect_filled(rect, look.radius, on_colors.0);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, look.radius, p.panel);
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
                if segment(ui, label, *value == current, look, (p.text, p.sheet)).clicked() {
                    picked = Some(*value);
                }
            }
        });
        let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), crate::ui_kit::tokens::HAIRLINE), Sense::hover());
        ui.painter().rect_filled(rule, 0.0, p.text);
    });
    picked
}

/// The small switch that lives in the status bar.
pub fn status_segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, String)]) -> Option<T> {
    let p = Palette::of(ui);
    let look = SegmentLook { height: STATUS_SEGMENT_HEIGHT, padding: STATUS_SEGMENT_PADDING, radius: CornerRadius::same(RADIUS_BUTTON), font: Type::SMALL, font_on: Type::SMALL };
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = NAV_TAB_GAP;
        for (value, label) in options {
            if segment(ui, label, *value == current, look, (p.sheet, p.text)).clicked() {
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
