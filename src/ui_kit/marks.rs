use std::sync::Arc;

use egui::emath::TSTransform;
use egui::text::{LayoutJob, TextFormat};
use egui::{
    Color32, CornerRadius, Galley, Pos2, Rect, Response, Role, Sense, Shape, Stroke, StrokeKind, TextWrapMode, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2,
};

use crate::shared_kernel::CheckState;
use crate::ui_kit::Palette;
use crate::ui_kit::controls::motion_time;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::on_desk;
use crate::ui_kit::text::{add_galley, add_job, add_text, line, paint_centred, paint_text};
use crate::ui_kit::tokens::{
    BAR_HEIGHT, BRAND_GAP, BRAND_MARK_WIDTH, CHIP_HEIGHT, CHIP_PADDING, DAY_STAMP_EMPTY_TICKS, DAY_STAMP_MAX_TICKS, DAY_STAMP_RINGS, DAY_STAMP_SETTLE,
    DAY_STAMP_SIZE, DAY_STAMP_TICK, FADED_OPACITY, HAIRLINE, HEAT_LABELS, HEAT_SIDE, ITEM_GAP, KEY_PADDING, KEY_SIZE, KNOT_PADDING, KNOT_SIZE, RADIUS_CARD,
    RADIUS_STAMP, ROW_GAP, ROW_TAG_HEIGHT, ROW_TAG_PADDING, STAMP_HEIGHT, STAMP_MIN_WIDTH, STAMP_PADDING, STAMP_RULE_GAP, STATUS_DOT, TAG_HEIGHT, TAG_PADDING,
    TALLY_FAT, TALLY_GAP_Y, TALLY_THIN, TallySize, Type,
};

/// Painted at its own size, so table cells and wrapped rows cannot stretch it.
/// Straight on the desk it fills with the desk's base, so no grid line runs through its small type.
pub fn tag(ui: &mut Ui, text: impl Into<String>, tone: Tone) -> Response {
    sized_tag(ui, text.into(), tone, TAG_PADDING, TAG_HEIGHT)
}

/// A `tag` a step smaller, for a mark inside a pick row beside a title that should not be cut.
pub fn row_tag(ui: &mut Ui, text: impl Into<String>, tone: Tone) -> Response {
    sized_tag(ui, text.into(), tone, ROW_TAG_PADDING, ROW_TAG_HEIGHT)
}

fn sized_tag(ui: &mut Ui, text: String, tone: Tone, padding: f32, height: f32) -> Response {
    let p = Palette::of(ui);
    let color = tone.color(&p);
    let galley = line(ui, &text, Type::TAG, color, f32::INFINITY);
    let size = vec2(galley.size().x + padding * 2.0, height);
    let fill = if on_desk(ui) { p.sheet } else { Color32::TRANSPARENT };
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(rect, RADIUS_CARD, fill, Stroke::new(HAIRLINE, color), StrokeKind::Inside);
    paint_centred(ui, rect, galley, color, fill);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
    response
}

/// A tag filled with its colour, for what needs you now. The same size as `tag`.
pub fn solid_tag(ui: &mut Ui, text: impl Into<String>, tone: Tone) -> Response {
    let text = text.into();
    let p = Palette::of(ui);
    let fill = tone.color(&p);
    let ink = if tone == Tone::Turn { p.on_turn } else { p.sheet };
    let galley = line(ui, &text, Type::TAG, ink, f32::INFINITY);
    let size = vec2(galley.size().x + TAG_PADDING * 2.0, TAG_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, RADIUS_CARD, fill);
    paint_centred(ui, rect, galley, ink, fill);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &text));
    response
}

/// A lane's heading: a square in the tag's colour, then the name.
pub fn lane_title(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let color = tone.color(&Palette::of(ui));
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, color);
        add_text(ui, text, Type::SECTION, Ink::Text)
    })
    .inner
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
    paint_text(ui, rect.center() - galley.mesh_bounds.center().to_vec2(), galley, color, Color32::TRANSPARENT);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
    response
}

/// A key on the keyboard, in the colour of whatever it sits on. `under` is the fill of the widget it sits in, if any.
pub(in crate::ui_kit) fn paint_key(ui: &Ui, rect: Rect, key: &str, color: Color32, under: Color32) {
    ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, color), StrokeKind::Inside);
    let galley = line(ui, key, Type::KEY, color, f32::INFINITY);
    paint_centred(ui, rect, galley, color, under);
}

pub fn key_hint(ui: &mut Ui, key: &str) -> Response {
    let color = Palette::of(ui).muted;
    let width = line(ui, key, Type::KEY, color, f32::INFINITY).size().x + KEY_PADDING * 2.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width.max(KEY_SIZE), KEY_SIZE), Sense::hover());
    paint_key(ui, rect, key, color, Color32::TRANSPARENT);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, key));
    response
}

fn tally_marks(ui: &mut Ui, colors: &[Color32], mark: TallySize) -> Response {
    let p = Palette::of(ui);
    let marked = colors.iter().filter(|color| **color != p.off).count();
    tally_spoken(ui, colors, mark, format!("{marked} of {}", colors.len()))
}

/// Marks that wrap to the width they have, spoken as `spoken`.
fn tally_spoken(ui: &mut Ui, colors: &[Color32], mark: TallySize, spoken: String) -> Response {
    let total = colors.len();
    let pitch = mark.width + mark.gap;
    let per_row = ((ui.available_width() + mark.gap) / pitch).floor().max(1.0) as usize;
    let rows = total.div_ceil(per_row).max(1);
    let size = vec2(per_row.min(total) as f32 * pitch, rows as f32 * (mark.height + TALLY_GAP_Y) - TALLY_GAP_Y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    for (index, color) in colors.iter().enumerate() {
        let at = rect.min + vec2((index % per_row) as f32 * pitch, (index / per_row) as f32 * (mark.height + TALLY_GAP_Y));
        ui.painter().rect_filled(Rect::from_min_size(at, vec2(mark.width, mark.height)), 0.0, *color);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &spoken));
    response
}

fn lit_marks(ui: &Ui, marked: usize, total: usize, ink: Ink) -> Vec<Color32> {
    let p = Palette::of(ui);
    (0..total).map(|index| if index < marked { ink.color(&p) } else { p.off }).collect()
}

/// One mark per item, like the edges of a stack of files. `total` past `marked` show unlit.
pub fn tally(ui: &mut Ui, marked: usize, total: usize, ink: impl Into<Ink>) -> Response {
    let colors = lit_marks(ui, marked, total, ink.into());
    tally_marks(ui, &colors, TALLY_THIN)
}

pub fn fat_tally(ui: &mut Ui, marked: usize, total: usize, ink: impl Into<Ink>) -> Response {
    let colors = lit_marks(ui, marked, total, ink.into());
    tally_marks(ui, &colors, TALLY_FAT)
}

/// One colour per mark, from runs of `(count, ink)`. `None` is an unlit run.
pub(in crate::ui_kit) fn strip_colors(p: &Palette, segments: &[(usize, Option<Ink>)]) -> Vec<Color32> {
    segments.iter().flat_map(|(count, ink)| std::iter::repeat_n(ink.map_or(p.off, |ink| ink.color(p)), *count)).collect()
}

/// Marks on one line from `at`, for a widget that paints its own tally. Returns where they went.
pub(in crate::ui_kit) fn paint_mark_row(ui: &Ui, at: Pos2, colors: &[Color32], mark: TallySize) -> Rect {
    let pitch = mark.width + mark.gap;
    for (index, color) in colors.iter().enumerate() {
        ui.painter().rect_filled(Rect::from_min_size(at + vec2(index as f32 * pitch, 0.0), vec2(mark.width, mark.height)), 0.0, *color);
    }
    Rect::from_min_size(at, vec2((colors.len() as f32 * pitch - mark.gap).max(0.0), mark.height))
}

/// One wrapping tally in runs of marks: `(count, ink)`. `None` is an unlit run.
pub fn tally_strip(ui: &mut Ui, segments: &[(usize, Option<Ink>)], fat: bool) -> Response {
    let colors = strip_colors(&Palette::of(ui), segments);
    tally_marks(ui, &colors, if fat { TALLY_FAT } else { TALLY_THIN })
}

/// A tally of things to fill in, such as profile topics, in that order: `filled` ones in `text`, `open` ones in
/// `open_ink`, or unlit with `None`, then `hidden` ones in `closed`, whose state is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillMarks {
    pub filled: usize,
    pub open: usize,
    pub hidden: usize,
    pub open_ink: Option<Ink>,
}

impl FillMarks {
    fn runs(self) -> [(usize, Option<Ink>); 3] {
        [(self.filled, Some(Ink::Text)), (self.open, self.open_ink), (self.hidden, Some(Ink::Status(Tone::Closed)))]
    }

    /// "6 of 11 filled, 1 hidden": a hidden mark is lit, but it is never counted as filled.
    pub(in crate::ui_kit) fn spoken(self) -> String {
        let total = self.filled + self.open + self.hidden;
        let filled = format!("{} of {total} filled", self.filled);
        if self.hidden == 0 { filled } else { format!("{filled}, {} hidden", self.hidden) }
    }
}

/// One wrapping tally of what is filled in, spoken as filled of all, with the hidden ones apart.
pub fn fill_tally(ui: &mut Ui, marks: FillMarks, fat: bool) -> Response {
    let colors = strip_colors(&Palette::of(ui), &marks.runs());
    tally_spoken(ui, &colors, if fat { TALLY_FAT } else { TALLY_THIN }, marks.spoken())
}

/// Where a step stands, as its knot shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnotState {
    Done,
    Working,
    Waiting,
    Failed,
    /// The step to do next, outside a run, such as the first-run welcome's.
    Next,
    /// A step still to come.
    Ahead,
}

/// A step's numbered square, `size` tall: `KNOT_SIZE`, or `KNOT_SIZE_SMALL` in a line of text.
/// Done and failed are drawn in a hairline, the others are filled.
pub fn knot(ui: &mut Ui, text: &str, state: KnotState, size: f32) -> Response {
    let p = Palette::of(ui);
    let (fill, ink, edge) = match state {
        KnotState::Done => (p.sheet, p.text, Some(p.text)),
        KnotState::Working => (p.primary, p.on_primary, None),
        KnotState::Waiting => (p.turn, p.on_turn, None),
        KnotState::Failed => (p.sheet, p.closed, Some(p.closed)),
        KnotState::Next => (p.primary, p.on_primary, None),
        KnotState::Ahead => (p.panel, p.muted, None),
    };
    let style = if size < KNOT_SIZE { Type::KEY } else { Type::TAG };
    let galley = line(ui, text, style, ink, f32::INFINITY);
    let (rect, response) = ui.allocate_exact_size(vec2((galley.size().x + KNOT_PADDING * 2.0).max(size), size), Sense::hover());
    ui.painter().rect_filled(rect, RADIUS_CARD, fill);
    if let Some(edge) = edge {
        ui.painter().rect_stroke(rect, RADIUS_CARD, Stroke::new(HAIRLINE, edge), StrokeKind::Inside);
    }
    // Centred on the ink, as a stamp is: digits have no descenders to balance the line box.
    paint_text(ui, rect.center() - galley.mesh_bounds.center().to_vec2(), galley, ink, fill);
    let said = match state {
        KnotState::Done => "done",
        KnotState::Working => "working",
        KnotState::Waiting => "waiting for you",
        KnotState::Failed => "failed",
        KnotState::Next => "next",
        KnotState::Ahead => "to do",
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{text}, {said}")));
    response
}

/// A path on one line: the folder in Faint, the file in Text, cut at the right with the whole path on hover.
/// `dir` keeps its trailing separator.
pub fn path_label(ui: &mut Ui, dir: &str, file: &str) -> Response {
    let p = Palette::of(ui);
    let mut job = LayoutJob::default();
    job.append(dir, 0.0, TextFormat::simple(Type::MONO.font(), p.faint));
    job.append(file, 0.0, TextFormat::simple(Type::MONO.font(), p.text));
    add_job(ui, job, TextWrapMode::Truncate)
}

/// The centres of the weekday, day and month lines, below the stamp's centre, from the canvas's 80px view box.
const DAY_STAMP_LINES: (f32, f32, f32) = (-17.0, 0.0, 17.5);

/// The ticks lit on a ring of `ticks` for `done` of `total`. Past the ring's room they light in proportion.
fn lit_ticks(total: usize, done: usize, ticks: usize) -> usize {
    let done = done.min(total);
    if total <= ticks { done } else { (done * ticks + total / 2) / total }
}

/// The day's postmark: two rings, one tick per decision filled as it is made, and the date inside.
/// With nothing to decide, the ring shows `DAY_STAMP_EMPTY_TICKS` unlit ticks in quieter rings.
/// A new tick fills over `motion_time`, and when the last one fills the stamp settles from `DAY_STAMP_SETTLE`, once.
/// Each animation is keyed on the tick count, so a ring that first shows or changes size never moves.
pub fn day_stamp(ui: &mut Ui, weekday: &str, day: &str, month: &str, total: usize, done: usize, spoken: &str) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(DAY_STAMP_SIZE), Sense::hover());
    let centre = rect.center();
    let empty = total == 0;
    let ticks = if empty { DAY_STAMP_EMPTY_TICKS } else { total.min(DAY_STAMP_MAX_TICKS) };
    let lit = if empty { 0 } else { lit_ticks(total, done, ticks) };
    let motion = motion_time(ui.ctx());
    let filled = ui.ctx().animate_value_with_time(response.id.with(("lit", ticks)), lit as f32, motion);
    let full = !empty && lit == ticks;
    let settled = ui.ctx().animate_bool_with_time(response.id.with(("full", ticks)), full, motion);
    // A disc of the desk's base under the outer ring, so no grid line crosses the date.
    let mut shapes = vec![Shape::circle_filled(centre, DAY_STAMP_RINGS.0, p.sheet)];
    let (inner, outer, width) = DAY_STAMP_TICK;
    for index in 0..ticks {
        let angle = -std::f32::consts::FRAC_PI_2 + (index as f32 + 0.5) * std::f32::consts::TAU / ticks as f32;
        let along = Vec2::angled(angle);
        let color = match (filled - index as f32).clamp(0.0, 1.0) {
            1.0 => p.text,
            0.0 => p.off,
            part => p.off.lerp_to_gamma(p.text, part),
        };
        shapes.push(Shape::line_segment([centre + along * inner, centre + along * outer], Stroke::new(width, color)));
    }
    let edge = if empty { p.line } else { p.text };
    for radius in [DAY_STAMP_RINGS.0, DAY_STAMP_RINGS.1] {
        shapes.push(Shape::circle_stroke(centre, radius, Stroke::new(HAIRLINE, edge)));
    }
    let (top, middle, bottom) = DAY_STAMP_LINES;
    for (text, style, color, below) in [(weekday, Type::SMALL, p.muted, top), (day, Type::CLOCK, p.text, middle), (month, Type::SMALL, p.muted, bottom)] {
        let galley = line(ui, text, style, color, DAY_STAMP_RINGS.1 * 2.0);
        // Centred on the ink, as the stamp is: dates have few descenders.
        shapes.push(Shape::galley(centre + vec2(0.0, below) - galley.mesh_bounds.center().to_vec2(), galley, color));
    }
    let scale = if full && settled < 1.0 { DAY_STAMP_SETTLE + (1.0 - DAY_STAMP_SETTLE) * settled } else { 1.0 };
    for mut shape in shapes {
        if scale != 1.0 {
            shape.transform(
                TSTransform::from_translation(centre.to_vec2()) * TSTransform::from_scaling(scale) * TSTransform::from_translation(-centre.to_vec2()),
            );
        }
        ui.painter().add(shape);
    }
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, spoken));
    response
}

/// A small square of the stamp ink beside something that waits on the user.
pub fn stamp_square(ui: &mut Ui, spoken: &str) -> Response {
    let color = Palette::of(ui).turn;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, spoken));
    response.on_hover_text(spoken)
}

/// How one check on a posting came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMark {
    Ok,
    Unclear,
    Fails,
}

impl CheckMark {
    /// The tag for a check's state. A check the ad never went through has none.
    pub fn of(state: CheckState) -> Option<Self> {
        match state {
            CheckState::Pass => Some(Self::Ok),
            CheckState::Unclear => Some(Self::Unclear),
            CheckState::Fail => Some(Self::Fails),
            CheckState::Unchecked => None,
        }
    }
}

/// A check's result as a tag in plain words.
pub fn check_tag(ui: &mut Ui, check: CheckMark) -> Response {
    let (text, tone) = match check {
        CheckMark::Ok => ("OK", Tone::Hired),
        CheckMark::Unclear => ("Unclear", Tone::Interview),
        CheckMark::Fails => ("Fails", Tone::Closed),
    };
    tag(ui, text, tone)
}

/// The Jobdesk mark: a folder with a J hooked into it, drawn from the logo's 652 by 600 view box.
/// It is drawn in `text`, and the J is stroked in `ground`, the colour the mark stands on.
fn paint_brand_mark(ui: &mut Ui, ground: Color32) -> Response {
    let (ink, cut) = (Palette::of(ui).text, ground);
    const ORIGIN: (f32, f32) = (186.0, 216.0);
    const VIEW: (f32, f32) = (652.0, 600.0);
    const ARC_SEGMENTS: usize = 12;
    let scale = BRAND_MARK_WIDTH / VIEW.0;
    let (rect, response) = ui.allocate_exact_size(vec2(BRAND_MARK_WIDTH, VIEW.1 * scale), Sense::hover());
    let at = |x: f32, y: f32| pos2(rect.left() + (x - ORIGIN.0) * scale, rect.top() + (y - ORIGIN.1) * scale);
    let radius = (36.0 * scale).round().max(1.0) as u8;
    let tab = CornerRadius { nw: radius, ne: radius, sw: 0, se: 0 };
    let body = CornerRadius { nw: 0, ne: radius, sw: radius, se: radius };
    ui.painter().rect_filled(Rect::from_min_max(at(206.0, 236.0), at(506.0, 340.0)), tab, ink);
    ui.painter().rect_filled(Rect::from_min_max(at(206.0, 300.0), at(818.0, 796.0)), body, ink);
    let mut hook = vec![at(596.0, 408.0)];
    for step in 0..=ARC_SEGMENTS {
        let angle = std::f32::consts::FRAC_PI_2 * step as f32 / ARC_SEGMENTS as f32;
        hook.push(at(500.0 + 96.0 * angle.cos(), 596.0 + 96.0 * angle.sin()));
    }
    hook.push(at(392.0, 692.0));
    ui.painter().add(Shape::line(hook, Stroke::new(86.0 * scale, cut)));
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, "Jobdesk mark"));
    response
}

/// The mark and the name, in the menu on the desk.
pub fn brand(ui: &mut Ui) -> Response {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = BRAND_GAP;
        paint_brand_mark(ui, p.sheet);
        add_galley(ui, line(ui, "Jobdesk", Type::BRAND, p.text, f32::INFINITY));
    })
    .response
}

/// A square of the status colour, such as the one that says the CLI is found.
pub fn status_dot(ui: &mut Ui, tone: Tone) -> Response {
    let color = tone.color(&Palette::of(ui));
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
    response
}

/// The status line in the status strip: a square that says whether all is well, then the words.
pub fn status_line(ui: &mut Ui, ok: bool, text: &str) -> Response {
    let p = Palette::of(ui);
    squared_status(ui, if ok { p.hired } else { p.closed }, text)
}

/// The status line while something waits on the user, such as a question in a running task: its square in stamp ink.
pub fn status_waits(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of(ui);
    squared_status(ui, p.turn, text)
}

fn squared_status(ui: &mut Ui, square: Color32, text: &str) -> Response {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = ROW_GAP;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
        ui.painter().rect_filled(rect, 0.0, square);
        add_galley(ui, line(ui, text, Type::NOTE, p.muted, f32::INFINITY));
    })
    .response
}

/// A note in the status strip. `alert` says something could not be read.
pub fn status_note(ui: &mut Ui, text: &str, alert: bool) -> Response {
    let p = Palette::of(ui);
    add_galley(ui, line(ui, text, Type::NOTE, if alert { p.closed } else { p.muted }, f32::INFINITY))
}

/// The stamp-ink chip that counts what waits on the user, laid out but not yet placed.
pub(in crate::ui_kit) struct CountChip {
    galley: Arc<Galley>,
    fill: Color32,
    ink: Color32,
}

impl CountChip {
    pub(in crate::ui_kit) fn layout(ui: &Ui, n: usize) -> Self {
        let p = Palette::of(ui);
        let (fill, ink) = (p.turn, p.on_turn);
        Self { galley: line(ui, &n.to_string(), Type::TAG, ink, f32::INFINITY), fill, ink }
    }

    pub(in crate::ui_kit) fn size(&self) -> Vec2 {
        vec2((self.galley.size().x + CHIP_PADDING * 2.0).max(CHIP_HEIGHT), CHIP_HEIGHT)
    }

    pub(in crate::ui_kit) fn paint(self, ui: &Ui, rect: Rect) {
        ui.painter().rect_filled(rect, RADIUS_CARD, self.fill);
        paint_centred(ui, rect, self.galley, self.ink, self.fill);
    }
}

/// How many things wait on the user, in the stamp ink.
pub fn attention_count(ui: &mut Ui, n: usize) -> Response {
    let chip = CountChip::layout(ui, n);
    let (rect, response) = ui.allocate_exact_size(chip.size(), Sense::hover());
    chip.paint(ui, rect);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, format!("{n} need you")));
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
        add_text(ui, label, Type::BODY, Ink::Text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| add_text(ui, value.to_string(), Type::MONO, Ink::Text));
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

/// One day in a `day_heatmap`: its step, from 0 for nothing to 3 for the most, and what its tooltip says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeatDay {
    pub step: u8,
    pub tip: String,
}

/// A year of days: one column of seven per week, Monday on top. A `None` day is outside the range, such as one after
/// today, and paints nothing. `today` is a (week, day) place. `months` puts each month's name over its first week.
/// `notes` and the `legend`, one word per step, sit at the right. `faded` draws it all at `FADED_OPACITY`.
pub struct DayHeatmap<'a> {
    pub weeks: &'a [[Option<HeatDay>; 7]],
    pub today: Option<(usize, usize)>,
    pub months: &'a [(usize, &'a str)],
    pub notes: &'a [&'a str],
    pub legend: [&'a str; 4],
    pub faded: bool,
}

/// A heatmap as shown: its response, and each day's square by week, for a test to point at.
pub struct ShownHeatmap {
    pub response: Response,
    pub cells: Vec<[Rect; 7]>,
}

/// What a screen reader hears for the whole heatmap. Each day with a task is a node inside it, named by its tip.
const HEATMAP_LABEL: &str = "Tasks per day over the last 12 months";
const HEAT_WEEKDAYS: [(usize, &str); 3] = [(0, "Mon"), (2, "Wed"), (4, "Fri")];

/// A step's fill. Neutral inks only: the heatmap shows activity, not a status, so it never takes stamp ink.
fn heat_fill(p: &Palette, step: u8) -> Color32 {
    match step {
        0 => p.raised,
        1 => p.line,
        2 => p.faint,
        _ => p.text,
    }
}

/// The width of `weeks` columns with the weekday names before them.
fn heat_width(weeks: usize) -> f32 {
    HEAT_LABELS.0 + weeks as f32 * (STATUS_DOT + TALLY_GAP_Y)
}

/// A year of days as `STATUS_DOT` squares, `TALLY_GAP_Y` apart, in four neutral steps, with a `text` ring around today.
/// Hovering a day shows its tip. Each day with a task takes the keyboard's focus, which shows its tip too, and a screen reader reads it.
/// It takes no click.
/// Its notes and legend sit at its right.
pub fn day_heatmap(ui: &mut Ui, map: &DayHeatmap) -> ShownHeatmap {
    ui.scope(|ui| {
        if map.faded {
            ui.multiply_opacity(FADED_OPACITY);
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = HEAT_SIDE.0;
            let shown = heat_grid(ui, map);
            heat_side(ui, map);
            shown
        })
        .inner
    })
    .inner
}

fn heat_grid(ui: &mut Ui, map: &DayHeatmap) -> ShownHeatmap {
    let p = Palette::of(ui);
    let pitch = STATUS_DOT + TALLY_GAP_Y;
    let size = vec2(heat_width(map.weeks.len()), HEAT_LABELS.1 + 7.0 * pitch);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    // A group, not an image, so a screen reader goes on into the days inside it.
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, HEATMAP_LABEL));
    let origin = rect.min + vec2(HEAT_LABELS.0 + TALLY_GAP_Y, HEAT_LABELS.1 + TALLY_GAP_Y);
    let cells: Vec<[Rect; 7]> = (0..map.weeks.len())
        .map(|week| std::array::from_fn(|day| Rect::from_min_size(origin + vec2(week as f32, day as f32) * pitch, Vec2::splat(STATUS_DOT))))
        .collect();
    for &(week, name) in map.months {
        if let Some(column) = cells.get(week) {
            let galley = line(ui, name, Type::SMALL, p.muted, f32::INFINITY);
            paint_text(ui, pos2(column[0].left(), rect.top()), galley, p.muted, Color32::TRANSPARENT);
        }
    }
    for (day, name) in HEAT_WEEKDAYS {
        let galley = line(ui, name, Type::SMALL, p.muted, f32::INFINITY);
        let centre = origin.y + day as f32 * pitch + STATUS_DOT / 2.0;
        paint_text(ui, pos2(rect.left(), centre - galley.size().y / 2.0), galley, p.muted, Color32::TRANSPARENT);
    }
    for (week, days) in map.weeks.iter().enumerate() {
        for (day, heat) in days.iter().enumerate() {
            let Some(heat) = heat else { continue };
            let cell = cells[week][day];
            if map.today == Some((week, day)) {
                // A `text` ring outside a gap in the paper's fill, so it reads beside the darkest step too.
                ui.painter().rect_filled(cell.expand(HAIRLINE * 2.0), 0.0, p.text);
                ui.painter().rect_filled(cell.expand(HAIRLINE), 0.0, p.card);
            }
            ui.painter().rect_filled(cell, 0.0, heat_fill(&p, heat.step));
        }
    }
    // Pointer-only tips leave the days out of the keyboard's and a screen reader's reach, so each day with a task gets a node under the map.
    // They take focus but no click or drag, so they take nothing from the map's own hover.
    let spoken = ui.new_child(UiBuilder::new().max_rect(rect).accessibility_parent(response.id));
    let mut focused = None;
    for (week, days) in map.weeks.iter().enumerate() {
        for (day, heat) in days.iter().enumerate() {
            let Some(heat) = heat.as_ref().filter(|heat| heat.step > 0) else { continue };
            let node = spoken.interact(cells[week][day], response.id.with((week, day)), Sense::focusable_noninteractive());
            node.widget_info(|| WidgetInfo::labeled(Role::Label, true, heat.tip.as_str()));
            if node.has_focus() {
                focused = Some((node, cells[week][day], heat));
            }
        }
    }
    let hovered = response.hover_pos().and_then(|at| {
        cells.iter().enumerate().find_map(|(week, days)| {
            let day = days.iter().position(|cell| cell.expand(TALLY_GAP_Y / 2.0).contains(at))?;
            Some((days[day], map.weeks[week][day].as_ref()?))
        })
    });
    // The pointer's day wins over the focused one, so one tip shows at a time.
    let tip = match (hovered, focused) {
        (Some((cell, heat)), _) => Some((egui::Tooltip::for_enabled(&response), cell, heat)),
        (None, Some((node, cell, heat))) => Some((egui::Tooltip::for_widget(&node), cell, heat)),
        (None, None) => None,
    };
    if let Some((mut tooltip, cell, heat)) = tip {
        // Under the day and ending at it, so the tip covers past days, not the notes at the right.
        tooltip.popup = tooltip.popup.anchor(cell).align(egui::RectAlign::BOTTOM_END);
        tooltip.show(|ui| add_text(ui, heat.tip.as_str(), Type::BODY, Ink::Text));
    }
    ShownHeatmap { response, cells }
}

/// The notes beside the heatmap, then one square and word per step.
fn heat_side(ui: &mut Ui, map: &DayHeatmap) {
    let p = Palette::of(ui);
    ui.vertical(|ui| {
        ui.add_space(HEAT_SIDE.1);
        ui.spacing_mut().item_spacing.y = HEAT_SIDE.2;
        for note in map.notes {
            add_text(ui, *note, Type::NOTE, Ink::Muted);
        }
        ui.add_space(ITEM_GAP);
        // A legend row is as tall as its words, not as a control.
        ui.spacing_mut().interact_size.y = 0.0;
        for (step, word) in map.legend.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ITEM_GAP;
                let (square, _) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
                ui.painter().rect_filled(square, 0.0, heat_fill(&p, step as u8));
                add_text(ui, *word, Type::NOTE, Ink::Muted);
            });
        }
    });
}

/// The heatmap of `weeks` while it loads: seven grey rows at its width, on the paper of a page.
pub fn day_heatmap_loading(ui: &mut Ui, weeks: usize) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = TALLY_GAP_Y;
        for _ in 0..7 {
            crate::ui_kit::card_skeleton(ui, heat_width(weeks), STATUS_DOT);
        }
    });
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    fn harness(add: impl FnMut(&mut Ui) + 'static) -> Harness<'static> {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(add)
    }

    #[test]
    fn the_day_stamp_reads_its_spoken_label() {
        let spoken = "Saturday 31 October. 3 of 11 decided today.";
        let mut harness = harness(move |ui| {
            day_stamp(ui, "Sat", "31", "Oct", 11, 3, spoken);
        });
        harness.step();
        assert_eq!(harness.get_by_label(spoken).rect().width(), DAY_STAMP_SIZE);
    }

    #[test]
    fn a_long_day_lights_its_ticks_in_proportion() {
        assert_eq!(lit_ticks(11, 3, 11), 3);
        assert_eq!(lit_ticks(80, 40, DAY_STAMP_MAX_TICKS), 20);
        assert_eq!(lit_ticks(80, 80, DAY_STAMP_MAX_TICKS), DAY_STAMP_MAX_TICKS);
        assert_eq!(lit_ticks(5, 9, 5), 5);
    }

    /// Three weeks, every day with a tip, except the last two days, which come after today.
    fn three_weeks() -> Vec<[Option<HeatDay>; 7]> {
        (0..3)
            .map(|week| std::array::from_fn(|day| (week * 7 + day < 19).then(|| HeatDay { step: (day % 4) as u8, tip: format!("Tip {week} {day}") })))
            .collect()
    }

    /// A heatmap of `three_weeks` and the squares it drew.
    fn heatmap(faded: bool) -> (Harness<'static>, std::rc::Rc<std::cell::RefCell<Vec<[Rect; 7]>>>) {
        let cells: std::rc::Rc<std::cell::RefCell<Vec<[Rect; 7]>>> = std::rc::Rc::default();
        let seen = std::rc::Rc::clone(&cells);
        let weeks = three_weeks();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let harness = Harness::builder().with_size([700.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            let map = DayHeatmap {
                weeks: &weeks,
                today: Some((2, 4)),
                months: &[(0, "Oct")],
                notes: &["Last 3 weeks"],
                legend: ["None", "1 task", "2 or 3 tasks", "4 or more"],
                faded,
            };
            *seen.borrow_mut() = day_heatmap(ui, &map).cells;
        });
        (harness, cells)
    }

    /// The days of `three_weeks` with a task, each read out by its tip.
    fn spoken_days() -> usize {
        three_weeks().iter().flatten().flatten().filter(|heat| heat.step > 0).count()
    }

    #[test]
    fn hovering_a_day_shows_its_tip() {
        let (mut harness, cells) = heatmap(false);
        harness.run();
        harness.hover_at(cells.borrow()[1][2].center());
        harness.run_steps(4);
        assert_eq!(harness.query_all_by_label("Tip 1 2").count(), 2, "its node and its tip");
        assert_eq!(harness.query_all_by_label_contains("Tip ").count(), spoken_days() + 1, "one tip at a time");
    }

    #[test]
    fn tabbing_to_a_day_shows_its_tip() {
        let (mut harness, _) = heatmap(false);
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run_steps(4);
        assert!(harness.query_all_by_label("Tip 0 1").any(|node| node.is_focused()), "the first day with a task takes the focus");
        assert_eq!(harness.query_all_by_label("Tip 0 1").count(), 2, "its node and its tip");
        harness.key_press(egui::Key::Tab);
        harness.run_steps(4);
        assert_eq!(harness.query_all_by_label("Tip 0 2").count(), 2, "the next day's node and tip");
        assert_eq!(harness.query_all_by_label_contains("Tip ").count(), spoken_days() + 1, "one tip at a time");
    }

    #[test]
    fn a_screen_reader_reads_each_day_with_a_task() {
        let (mut harness, cells) = heatmap(false);
        harness.run();
        assert_eq!(spoken_days(), 13);
        assert_eq!(harness.query_all_by_label_contains("Tip ").count(), spoken_days(), "no tip shows, yet each day is read");
        assert_eq!(harness.get_by_label("Tip 1 2").rect(), cells.borrow()[1][2], "the node sits on its square");
        assert!(harness.query_by_label("Tip 1 0").is_none(), "a day with no task adds none");
        assert!(harness.query_by_label("Tip 2 5").is_none(), "nor does a day after today");
    }

    #[test]
    fn a_day_outside_the_range_paints_nothing_and_has_no_tip() {
        let (mut harness, cells) = heatmap(false);
        harness.run();
        let (outside, inside) = (cells.borrow()[2][6], cells.borrow()[2][3]);
        harness.hover_at(outside.center());
        harness.run_steps(4);
        assert_eq!(harness.query_all_by_label_contains("Tip ").count(), spoken_days(), "no tip, only the days' nodes");
        let painted = |rect: Rect| harness.output().shapes.iter().any(|clipped| matches!(&clipped.shape, Shape::Rect(shape) if shape.rect == rect));
        assert!(painted(inside), "a day in the range is painted");
        assert!(!painted(outside), "a day outside it is not");
    }

    #[test]
    fn a_faded_heatmap_keeps_its_label() {
        let (mut harness, _) = heatmap(true);
        harness.run();
        harness.get_by_label(HEATMAP_LABEL);
        harness.get_by_label("4 or more");
    }

    /// A hidden mark is lit in `closed`, yet the label counts only the filled ones, as the figure beside it does.
    #[test]
    fn a_fill_tally_says_filled_of_all_and_hidden_apart() {
        let mut harness = harness(|ui| {
            fill_tally(ui, FillMarks { filled: 6, open: 4, hidden: 1, open_ink: None }, true);
            fill_tally(ui, FillMarks { filled: 7, open: 4, hidden: 0, open_ink: Some(Tone::Interview.into()) }, false);
        });
        harness.step();
        harness.get_by_label("6 of 11 filled, 1 hidden");
        harness.get_by_label("7 of 11 filled");
        assert!(harness.query_by_label("7 of 11").is_none(), "the hidden mark is not counted as filled");
    }

    #[test]
    fn check_tags_use_plain_words() {
        let mut harness = harness(|ui| {
            ui.horizontal(|ui| {
                check_tag(ui, CheckMark::Ok);
                check_tag(ui, CheckMark::Unclear);
                check_tag(ui, CheckMark::Fails);
            });
        });
        harness.step();
        for word in ["OK", "Unclear", "Fails"] {
            harness.get_by_label(word);
        }
    }
}
