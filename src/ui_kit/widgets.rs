use egui::{Button, Color32, CornerRadius, Frame, Margin, Pos2, Response, RichText, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

use crate::shared_kernel::WebUrl;
use crate::ui_kit::{Palette, semibold};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Drafted,
    Active,
    Interview,
    Offer,
    Hired,
    Closed,
}

impl Tone {
    pub fn color(self, p: &Palette) -> Color32 {
        match self {
            Self::Plain => p.muted,
            Self::Drafted => p.drafted,
            Self::Active => p.active,
            Self::Interview => p.interview,
            Self::Offer => p.offer,
            Self::Hired => p.hired,
            Self::Closed => p.closed,
        }
    }
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let color = Palette::of(ui).muted;
    ui.label(RichText::new(text).color(color))
}

pub fn faint_mono(ui: &mut Ui, text: impl Into<String>) -> Response {
    let color = Palette::of(ui).faint;
    ui.label(RichText::new(text).monospace().color(color))
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) -> Response {
    ui.label(RichText::new(text).family(semibold()))
}

/// Painted at its own size, so table cells and wrapped rows cannot stretch it.
pub fn tag(ui: &mut Ui, text: impl Into<String>, tone: Tone) -> Response {
    let text = text.into();
    let color = tone.color(&Palette::of(ui));
    let galley = ui.painter().layout_no_wrap(text.clone(), egui::TextStyle::Small.resolve(ui.style()), color);
    let size = galley.size() + vec2(12.0, 6.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &text));
    response
}

/// The double-ruled verdict on a posting.
pub fn stamp(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let color = tone.color(&Palette::of(ui));
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::new(26.0, semibold()), color);
    let size = vec2(galley.size().x.max(46.0) + 16.0, 46.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    for inset in [0.0, 3.0] {
        ui.painter().rect_stroke(rect.shrink(inset), 1.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    response
}

/// One card's edge showing under the stack, `inset` narrower on each side.
pub fn stack_edge(ui: &mut Ui, inset: f32) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
    let edge = rect.shrink2(vec2(inset, 0.0));
    ui.painter().rect(edge, CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, p.raised, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
}

pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(8)).show(ui, add).inner
}

pub fn panel_header(ui: &mut Ui, title: &str, note: Option<&str>) {
    Frame::new().inner_margin(Margin::symmetric(14, 9)).show(ui, |ui| {
        ui.horizontal(|ui| {
            strong(ui, title);
            if let Some(note) = note {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| faint_mono(ui, note));
            }
        });
    });
    divider(ui);
}

pub fn divider(ui: &mut Ui) {
    let color = Palette::of(ui).line;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
}

pub fn panel_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new().inner_margin(Margin::symmetric(14, 10)).show(ui, add).inner
}

pub fn bar(ui: &mut Ui, fraction: f32, color: Color32) {
    let track = Palette::of(ui).raised;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, track);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 3.0, color);
}

/// A labelled measure: name and value above a bar.
pub fn measure(ui: &mut Ui, label: &str, value: usize, of: usize, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.monospace(value.to_string()));
    });
    bar(ui, if of == 0 { 0.0 } else { value as f32 / of as f32 }, color);
    ui.add_space(4.0);
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of(ui);
    ui.add(Button::new(RichText::new(text).color(p.on_primary).family(semibold())).fill(p.primary).stroke(Stroke::NONE))
}

fn command_request_id() -> egui::Id {
    egui::Id::NULL.with("requested_command")
}

/// A command button on any screen. The shell picks the request up and opens the launcher.
pub fn command_button(ui: &mut Ui, command: &str) {
    let clicked = ui.add(Button::new(RichText::new(command).monospace())).on_hover_text("Open in command launcher").clicked();
    if clicked {
        ui.ctx().data_mut(|data| data.insert_temp(command_request_id(), command.to_owned()));
    }
}

pub fn take_command_request(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|data| data.remove_temp(command_request_id()))
}

pub fn open_link(ui: &mut Ui, label: &str, url: Option<&WebUrl>) {
    let Some(url) = url else { return };
    let response = ui.button(label).on_hover_text(url.as_str());
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url.as_str()));
    }
}

/// Segmented control. Returns the option picked this frame.
pub fn segmented<T: Copy + PartialEq>(ui: &mut Ui, current: T, options: &[(T, String)]) -> Option<T> {
    let p = Palette::of(ui);
    let mut picked = None;
    Frame::new().fill(p.panel).stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(6)).inner_margin(Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal_wrapped(|ui| {
            for (value, label) in options {
                let on = *value == current;
                let text = RichText::new(label).color(if on { p.text } else { p.muted });
                let button = Button::new(text).fill(if on { p.raised } else { Color32::TRANSPARENT }).stroke(Stroke::NONE);
                if ui.add(button).clicked() {
                    picked = Some(*value);
                }
            }
        });
    });
    picked
}

/// Ring chart. Segments are `(value, colour)`; arcs keep their share of the circle.
pub fn doughnut(ui: &mut Ui, size: f32, segments: &[(usize, Color32)]) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let total: usize = segments.iter().map(|(value, _)| value).sum();
    let centre = rect.center();
    let radius = size / 2.0 - 8.0;
    if total == 0 {
        ui.painter().circle_stroke(centre, radius, Stroke::new(12.0, Palette::of(ui).raised));
        return response;
    }
    let mut start = -std::f32::consts::FRAC_PI_2;
    for (value, color) in segments.iter().filter(|(value, _)| *value > 0) {
        let sweep = std::f32::consts::TAU * *value as f32 / total as f32;
        let steps = (sweep * 32.0).ceil() as usize + 1;
        let points: Vec<Pos2> = (0..=steps)
            .map(|step| {
                let angle = start + sweep * step as f32 / steps as f32;
                pos2(centre.x + radius * angle.cos(), centre.y + radius * angle.sin())
            })
            .collect();
        ui.painter().add(Shape::line(points, Stroke::new(12.0, *color)));
        start += sweep;
    }
    response
}
