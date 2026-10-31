use egui::{Color32, CornerRadius, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::text::{line, paint_centred};
use crate::ui_kit::tokens::{
    BAR_HEIGHT, BRAND_MARK_WIDTH, HAIRLINE, ITEM_GAP, KEY_PADDING, KEY_SIZE, RADIUS_CARD, RADIUS_STAMP, STAMP_HEIGHT, STAMP_MIN_WIDTH, STAMP_PADDING, STAMP_RULE_GAP, STATUS_DOT, TAG_HEIGHT, TAG_PADDING,
    TALLY_FAT, TALLY_GAP_Y, TALLY_THIN, TallySize, Type,
};

/// Painted at its own size, so table cells and wrapped rows cannot stretch it.
pub fn tag(ui: &mut Ui, text: impl Into<String>, tone: Tone) -> Response {
    let text = text.into();
    let color = tone.color(&Palette::of(ui));
    let galley = line(ui, &text, Type::TAG, color, f32::INFINITY);
    let size = vec2(galley.size().x + TAG_PADDING * 2.0, TAG_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, color), StrokeKind::Inside);
    paint_centred(ui, rect, galley, color);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
    response
}

/// A lane's heading: the tag's colour without its box.
pub fn lane_title(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let text = crate::ui_kit::rich(ui, text, Type::SECTION, tone);
    ui.label(text)
}

/// The double-ruled verdict on a posting.
pub fn stamp(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let color = tone.color(&Palette::of(ui));
    let galley = line(ui, text, Type::STAMP, color, f32::INFINITY);
    let ruled = STAMP_RULE_GAP * 2.0;
    let size = vec2((galley.size().x + STAMP_PADDING * 2.0).max(STAMP_MIN_WIDTH) + ruled, STAMP_HEIGHT + ruled);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    for inset in [0.0, STAMP_RULE_GAP] {
        ui.painter().rect_stroke(rect.shrink(inset), RADIUS_STAMP, Stroke::new(HAIRLINE, color), StrokeKind::Inside);
    }
    // Centred on the ink: a score has no descenders to balance the line box.
    ui.painter().galley(rect.center() - galley.mesh_bounds.center().to_vec2(), galley, color);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

/// A key on the keyboard, in the colour of whatever it sits on.
pub(in crate::ui_kit) fn paint_key(ui: &Ui, rect: Rect, key: &str, color: Color32) {
    ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, color), StrokeKind::Inside);
    let galley = line(ui, key, Type::KEY, color, f32::INFINITY);
    paint_centred(ui, rect, galley, color);
}

pub fn key_hint(ui: &mut Ui, key: &str) -> Response {
    let color = Palette::of(ui).muted;
    let width = line(ui, key, Type::KEY, color, f32::INFINITY).size().x + KEY_PADDING * 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width.max(KEY_SIZE), KEY_SIZE), Sense::hover());
    paint_key(ui, rect, key, color);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, key));
    response
}

fn tally_marks(ui: &mut Ui, colors: &[Color32], mark: TallySize) -> Response {
    let p = Palette::of(ui);
    let total = colors.len();
    let marked = colors.iter().filter(|color| **color != p.off).count();
    let pitch = mark.width + mark.gap;
    let per_row = ((ui.available_width() + mark.gap) / pitch).floor().max(1.0) as usize;
    let rows = total.div_ceil(per_row).max(1);
    let size = vec2(per_row.min(total) as f32 * pitch, rows as f32 * (mark.height + TALLY_GAP_Y) - TALLY_GAP_Y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    for (index, color) in colors.iter().enumerate() {
        let at = rect.min + vec2((index % per_row) as f32 * pitch, (index / per_row) as f32 * (mark.height + TALLY_GAP_Y));
        ui.painter().rect_filled(Rect::from_min_size(at, vec2(mark.width, mark.height)), 0.0, *color);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{marked} of {total}")));
    response
}

fn lit_marks(ui: &Ui, marked: usize, total: usize, tone: Tone) -> Vec<Color32> {
    let p = Palette::of(ui);
    (0..total).map(|index| if index < marked { tone.color(&p) } else { p.off }).collect()
}

/// One mark per item, like the edges of a stack of files. `total` past `marked` show unlit.
pub fn tally(ui: &mut Ui, marked: usize, total: usize, tone: Tone) -> Response {
    let colors = lit_marks(ui, marked, total, tone);
    tally_marks(ui, &colors, TALLY_THIN)
}

pub fn fat_tally(ui: &mut Ui, marked: usize, total: usize, tone: Tone) -> Response {
    let colors = lit_marks(ui, marked, total, tone);
    tally_marks(ui, &colors, TALLY_FAT)
}

/// One wrapping tally in runs of marks: `(count, ink)`. `None` is an unlit run.
pub fn tally_strip(ui: &mut Ui, segments: &[(usize, Option<Ink>)], fat: bool) -> Response {
    let p = Palette::of(ui);
    let colors: Vec<Color32> = segments.iter().flat_map(|(count, ink)| std::iter::repeat_n(ink.map_or(p.off, |ink| ink.color(&p)), *count)).collect();
    tally_marks(ui, &colors, if fat { TALLY_FAT } else { TALLY_THIN })
}

/// The Jobdesk mark: a folder with a J hooked into it, drawn from the logo's 652 by 600 view box.
pub fn brand_mark(ui: &mut Ui) -> Response {
    const ORIGIN: (f32, f32) = (186.0, 216.0);
    const VIEW: (f32, f32) = (652.0, 600.0);
    const ARC_SEGMENTS: usize = 12;
    let p = Palette::of(ui);
    let scale = BRAND_MARK_WIDTH / VIEW.0;
    let (rect, response) = ui.allocate_exact_size(vec2(BRAND_MARK_WIDTH, VIEW.1 * scale), Sense::hover());
    let at = |x: f32, y: f32| pos2(rect.left() + (x - ORIGIN.0) * scale, rect.top() + (y - ORIGIN.1) * scale);
    let radius = (36.0 * scale).round().max(1.0) as u8;
    let tab = CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 };
    let body = CornerRadius { nw: 0, ne: radius, sw: radius, se: radius };
    ui.painter().rect_filled(Rect::from_min_max(at(206.0, 236.0), at(506.0, 340.0)), tab, p.text);
    ui.painter().rect_filled(Rect::from_min_max(at(206.0, 300.0), at(818.0, 796.0)), body, p.text);
    let mut hook = vec![at(596.0, 408.0)];
    for step in 0..=ARC_SEGMENTS {
        let angle = std::f32::consts::FRAC_PI_2 * step as f32 / ARC_SEGMENTS as f32;
        hook.push(at(500.0 + 96.0 * angle.cos(), 596.0 + 96.0 * angle.sin()));
    }
    hook.push(at(392.0, 692.0));
    ui.painter().add(Shape::line(hook, Stroke::new(86.0 * scale, p.bg)));
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, "Jobdesk mark"));
    response
}

/// A square of the status colour, such as the one that says the CLI is found.
pub fn status_dot(ui: &mut Ui, tone: Tone) -> Response {
    let color = tone.color(&Palette::of(ui));
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
    response
}

fn draw_bar(ui: &mut Ui, fraction: f32, ink: Ink, track: Color32) {
    let color = ink.color(&Palette::of(ui));
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, track);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 0.0, color);
}

pub fn bar(ui: &mut Ui, fraction: f32, ink: impl Into<Ink>) {
    let track = Palette::of(ui).raised;
    draw_bar(ui, fraction, ink.into(), track);
}

/// The same bar on a filled slip, where the sheet's track would vanish.
pub fn slip_bar(ui: &mut Ui, fraction: f32, ink: impl Into<Ink>) {
    let track = Palette::of(ui).off;
    draw_bar(ui, fraction, ink.into(), track);
}

/// A labelled measure: name and value above a bar.
pub fn measure(ui: &mut Ui, label: &str, value: usize, of: usize, ink: impl Into<Ink>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.monospace(value.to_string()));
    });
    bar(ui, if of == 0 { 0.0 } else { value as f32 / of as f32 }, ink);
    ui.add_space(ITEM_GAP - 2.0);
}

/// Ring chart. Segments are `(value, status)`; arcs keep their share of the circle.
pub fn doughnut(ui: &mut Ui, size: f32, segments: &[(usize, Tone)]) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let total: usize = segments.iter().map(|(value, _)| value).sum();
    let centre = rect.center();
    let radius = size / 2.0 - 8.0;
    if total == 0 {
        ui.painter().circle_stroke(centre, radius, Stroke::new(12.0, p.raised));
        return response;
    }
    let mut start = -std::f32::consts::FRAC_PI_2;
    for (value, tone) in segments.iter().filter(|(value, _)| *value > 0) {
        let sweep = std::f32::consts::TAU * *value as f32 / total as f32;
        let steps = (sweep * 32.0).ceil() as usize + 1;
        let points: Vec<Pos2> = (0..=steps)
            .map(|step| {
                let angle = start + sweep * step as f32 / steps as f32;
                pos2(centre.x + radius * angle.cos(), centre.y + radius * angle.sin())
            })
            .collect();
        ui.painter().add(Shape::line(points, Stroke::new(12.0, tone.color(&p))));
        start += sweep;
    }
    response
}
