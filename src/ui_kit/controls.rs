use std::sync::Arc;
use std::time::Duration;

use egui::{Color32, CornerRadius, CursorIcon, Galley, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, WidgetInfo, vec2};

use crate::shared_kernel::WebUrl;
use crate::ui_kit::Palette;
use crate::ui_kit::marks::{CountChip, paint_key};
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{code_box, hairline, on_desk};
use crate::ui_kit::text::{add_text, cut_hover, line, paint_centred, paint_text};
use crate::ui_kit::tokens::{
    BUTTON_GAP, BUTTON_HEIGHT, BUTTON_PADDING, CHIP_HEIGHT, COMMAND_NAME_WIDTH, DISABLED_OPACITY, FIELD_HEIGHT, FIELD_PADDING, FOCUS_OFFSET, FOCUS_RING,
    HAIRLINE, KEY_SIZE, KEYS_GAP, MARK_HEIGHT, MARK_LINE, MARK_OVERHANG, MARK_RULE, MIN_HIT, NAV_TAB_HEIGHT, NAV_TAB_PADDING, PICK_EDGE, PICK_ROW_HEIGHT,
    PICK_ROW_INLINE, RADIUS_BUTTON, ROW_ITEM_GAP, SEGMENT_GAP, SEGMENT_HEIGHT, SEGMENT_PADDING, SEGMENT_TOP, SMALL_BUTTON_HEIGHT, SMALL_BUTTON_PADDING,
    STATUS_DOT, STATUS_SEGMENT_HEIGHT, STATUS_SEGMENT_PADDING, TOGGLE_GAP, Type,
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

/// What a button shows after its text: nothing, one key in a box, or a chord spelled out in plain glyphs.
#[derive(Clone, Copy)]
enum Hint<'a> {
    None,
    Key(&'a str),
    Keys(&'a [&'a str]),
}

const NORMAL: Fit = Fit { height: BUTTON_HEIGHT, padding: BUTTON_PADDING, font: Type::BUTTON };
const SMALL: Fit = Fit { height: SMALL_BUTTON_HEIGHT, padding: SMALL_BUTTON_PADDING, font: Type::SMALL_BUTTON };
const COMMAND: Fit = Fit { height: SMALL_BUTTON_HEIGHT, padding: SMALL_BUTTON_PADDING, font: Type::MONO };

/// Whether the pointer is over `response` now. Every hover fill and the hand ask this, so they light and go out together.
/// A disabled widget never reports hover. `hovered` alone also holds on a key press and while a press is dragged off
/// the widget, so the pointer must be over it too.
pub(in crate::ui_kit) fn pointer_over(response: &Response) -> bool {
    response.hovered() && response.contains_pointer()
}

/// Whether a press on `response` is held with the pointer still over it. A press dragged off lets go of the pressed look,
/// as it lets go of the hover. Each widget paints it as its fill, under its text, one step deeper than its hover.
pub(in crate::ui_kit) fn pressed(response: &Response) -> bool {
    response.is_pointer_button_down_on() && pointer_over(response)
}

/// What every clickable kit widget calls last: the pointing hand while the pointer is over it, then the focus ring around it.
pub(in crate::ui_kit) fn interactive(ui: &Ui, response: &Response, radius: impl Into<CornerRadius>) {
    hand_and_ring(ui, response, radius, response.rect.expand(FOCUS_OFFSET), StrokeKind::Outside);
}

/// The same for a control whose hit area is wider than the part that shows it, such as a checkbox with its words:
/// the ring goes round `shown`.
pub(in crate::ui_kit) fn interactive_part(ui: &Ui, response: &Response, shown: Rect, radius: impl Into<CornerRadius>) {
    hand_and_ring(ui, response, radius, shown.expand(FOCUS_OFFSET), StrokeKind::Outside);
}

/// The same for a row or a card that touches its neighbours or the edge of its column: the focus ring sits inside it,
/// so nothing beside it covers or clips the ring.
pub(in crate::ui_kit) fn interactive_row(ui: &Ui, response: &Response, radius: impl Into<CornerRadius>) {
    hand_and_ring(ui, response, radius, response.rect, StrokeKind::Inside);
}

fn hand_and_ring(ui: &Ui, response: &Response, radius: impl Into<CornerRadius>, ring: Rect, kind: StrokeKind) {
    if pointer_over(response) {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.has_focus() {
        let color = Palette::of(ui).active;
        ui.painter().rect_stroke(ring, radius, Stroke::new(FOCUS_RING, color), kind);
    }
}

/// The pointing hand on a control egui paints, such as a checkbox or a combo box. The kit's own widgets set it themselves.
pub fn hand(response: Response) -> Response {
    if pointer_over(&response) {
        response.ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    response
}

/// How a pickable row or card looks. `fill` is clear at rest, so the caller keeps its own rest fill.
/// `edge` is the ink that says it is picked: a bar at a row's leading side, or a border round a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui_kit) struct PickLook {
    pub fill: Color32,
    pub edge: Option<Color32>,
}

/// One look for every pickable thing. Hover is `raised`, and a press is one step deeper, `line`.
/// Picked keeps the hover fill and adds a `text` edge, so a hovered row never looks picked.
pub(in crate::ui_kit) fn pick_look(p: &Palette, on: bool, hovered: bool, pressed: bool) -> PickLook {
    let fill = if pressed {
        p.line
    } else if on || hovered {
        p.raised
    } else {
        Color32::TRANSPARENT
    };
    PickLook { fill, edge: on.then_some(p.text) }
}

/// The ink bar at the leading side of a picked row, rounded as the row's own left corners are.
pub(in crate::ui_kit) fn paint_pick_edge(ui: &Ui, rect: Rect, radius: u8, color: Color32) {
    let edge = Rect::from_min_max(rect.min, egui::pos2(rect.left() + PICK_EDGE, rect.bottom()));
    ui.painter().rect_filled(edge, CornerRadius { nw: radius, sw: radius, ne: 0, se: 0 }, color);
}

/// A plain button at rest is clear on a surface, and fills with paper on the desk, so the grid never runs behind its label.
/// A press goes one step deeper than the hover: `line` under a plain or danger button, `muted` under a filled one.
fn look_colors(p: &Palette, look: Look, hovered: bool, pressed: bool, desk: bool) -> (Color32, Color32) {
    let rest = if desk { p.card } else { Color32::TRANSPARENT };
    match look {
        Look::Primary | Look::On if pressed => (p.muted, p.muted),
        Look::Primary => (p.primary, p.primary),
        Look::On => (p.text, p.text),
        Look::Plain if pressed => (p.line, p.line),
        Look::Danger if pressed => (p.line, p.closed),
        Look::Plain if hovered => (p.raised, p.line),
        Look::Danger if hovered => (p.raised, p.closed),
        Look::Plain => (rest, p.line),
        Look::Danger => (rest, p.closed),
    }
}

/// `color` at the disabled opacity.
fn dimmed(color: Color32) -> Color32 {
    color.gamma_multiply(DISABLED_OPACITY)
}

/// A disabled plain or danger button keeps its opaque rest fill and shows its border at half.
/// A filled one paints its fill at half over the surface, with no border and its ink kept, so the label still reads on the paler fill.
fn disabled_colors(p: &Palette, look: Look, desk: bool) -> (Color32, Color32) {
    let rest = if desk { p.card } else { Color32::TRANSPARENT };
    match look {
        Look::Primary => (dimmed(p.primary), Color32::TRANSPARENT),
        Look::On => (dimmed(p.text), Color32::TRANSPARENT),
        Look::Plain => (rest, dimmed(p.line)),
        Look::Danger => (rest, dimmed(p.closed)),
    }
}

/// Runs `add` enabled or disabled, and says `why` when the pointer rests on it while it is disabled.
/// `add` must be a kit widget, which paints its own disabled look: egui's own widgets no longer fade.
pub fn enabled_when(ui: &mut Ui, enabled: bool, why: &str, add: impl FnOnce(&mut Ui) -> Response) -> Response {
    ui.add_enabled_ui(enabled, add).inner.on_disabled_hover_text(why)
}

fn press(ui: &mut Ui, text: &str, look: Look, fit: Fit, key: Option<&str>) -> Response {
    press_as(ui, text, look, fit, key.map_or(Hint::None, Hint::Key), None)
}

/// `selected` makes a screen reader hear the button as a toggle in `role` that is on or off.
fn press_as(ui: &mut Ui, text: &str, look: Look, fit: Fit, hint: Hint, selected: Option<(Role, bool)>) -> Response {
    let p = Palette::of(ui);
    let ink = match look {
        Look::Plain => p.text,
        Look::Primary => p.on_primary,
        Look::Danger => p.closed,
        Look::On => p.sheet,
    };
    let enabled = ui.is_enabled();
    // A filled button keeps its ink: its fill is what goes pale.
    let ink = if enabled || matches!(look, Look::Primary | Look::On) { ink } else { dimmed(ink) };
    // A chord with no keys takes no room and paints nothing.
    let chord = Chord::new(ui, if let Hint::Keys(keys) = hint { keys } else { &[] }, ink);
    let keys = if let Hint::Key(_) = hint { KEY_SIZE + BUTTON_GAP } else { chord.width };
    let room = ui.available_width() - fit.padding * 2.0 - keys;
    let galley = line(ui, text, fit.font, ink, room);
    let size = vec2(galley.size().x + fit.padding * 2.0 + keys, fit.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, border) =
        if enabled { look_colors(&p, look, pointer_over(&response), pressed(&response), on_desk(ui)) } else { disabled_colors(&p, look, on_desk(ui)) };
    ui.painter().rect(rect, RADIUS_BUTTON, fill, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(fit.padding, (fit.height - galley.size().y) / 2.0), galley.size());
    if matches!(look, Look::Primary) && pointer_over(&response) && !pressed(&response) {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    paint_text(ui, text_rect.min, galley, ink, fill);
    if let Hint::Key(key) = hint {
        let hint = Rect::from_center_size(egui::pos2(text_rect.right() + BUTTON_GAP + KEY_SIZE / 2.0, rect.center().y), vec2(KEY_SIZE, KEY_SIZE));
        paint_key(ui, hint, key, ink, fill);
    }
    chord.paint(ui, text_rect.right(), rect.center().y, ink, fill);
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| match selected {
        Some((role, on)) => WidgetInfo::selected(role, ui.is_enabled(), on, text),
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
    press_as(ui, text, if on { Look::On } else { Look::Plain }, SMALL, Hint::None, Some((Role::Button, on)))
}

/// A choice of two to five values: a row of small toggles, one on, that a screen reader hears as one radio group.
/// `current` is `None` when the value matches no option, so none is on. Returns the option picked this frame.
pub fn toggle_group<T: Copy + PartialEq>(ui: &mut Ui, current: Option<T>, options: &[(T, &str)]) -> Option<T> {
    let mut picked = None;
    // Measured first, so the row takes only its own width and reads left to right in any layout, such as a setting's
    // control at the right of its row.
    let ink = Palette::of(ui).text;
    let labels: f32 = options.iter().map(|(_, label)| line(ui, label, SMALL.font, ink, f32::INFINITY).size().x + SMALL.padding * 2.0).sum();
    let width = labels + TOGGLE_GAP * options.len().saturating_sub(1) as f32;
    ui.allocate_ui_with_layout(vec2(width, SMALL.height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.scope_builder(egui::UiBuilder::new().layout(egui::Layout::left_to_right(egui::Align::Center)).accessibility_role(Role::RadioGroup), |ui| {
            ui.spacing_mut().item_spacing.x = TOGGLE_GAP;
            for &(value, label) in options {
                let on = current == Some(value);
                if press_as(ui, label, if on { Look::On } else { Look::Plain }, SMALL, Hint::None, Some((Role::RadioButton, on))).clicked() {
                    picked = Some(value);
                }
            }
        });
    });
    picked
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

/// A primary button that names the chord it also answers to, such as Cmd Enter.
pub fn primary_shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    press_as(ui, text, Look::Primary, NORMAL, Hint::Keys(keys), None)
}

/// A danger button that names the chord it also answers to, such as Save mine over it with Cmd Enter.
pub fn danger_shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    press_as(ui, text, Look::Danger, NORMAL, Hint::Keys(keys), None)
}

/// A plain button that names the chord it also answers to, such as Cancel with Esc. Unlike `shortcut_button` it is
/// clear on a surface, as every plain button is.
pub(in crate::ui_kit) fn plain_shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    press_as(ui, text, Look::Plain, NORMAL, Hint::Keys(keys), None)
}

/// The command key and `key`, as a shortcut button names them. The bundled fonts have no command key glyph, so macOS spells it out.
pub const fn command_chord(key: &'static str) -> [&'static str; 2] {
    if cfg!(target_os = "macos") { ["Cmd", key] } else { ["Ctrl", key] }
}

/// A plain button in the menu that names its keyboard shortcut. The keys are plain glyphs, with no box.
pub fn shortcut_button(ui: &mut Ui, text: &str, keys: &[&str]) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let (ink, key_ink, border) = if enabled { (p.text, p.muted, p.line) } else { (dimmed(p.text), dimmed(p.muted), dimmed(p.line)) };
    let label = line(ui, text, NORMAL.font, ink, f32::INFINITY);
    let chord = Chord::new(ui, keys, key_ink);
    let size = vec2(NORMAL.padding * 2.0 + label.size().x + chord.width, NORMAL.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if pressed(&response) { p.line } else { p.card };
    ui.painter().rect(rect, RADIUS_BUTTON, fill, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    let text_rect = Rect::from_min_size(rect.min + vec2(NORMAL.padding, (NORMAL.height - label.size().y) / 2.0), label.size());
    if pointer_over(&response) && !pressed(&response) {
        ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(HAIRLINE, ink));
    }
    paint_text(ui, text_rect.min, label, ink, fill);
    chord.paint(ui, text_rect.right(), rect.center().y, key_ink, fill);
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, text));
    response
}

/// A chord such as Cmd K, spelled out in plain glyphs after a button's text.
struct Chord {
    glyphs: Vec<Arc<Galley>>,
    /// The room it takes after the text, with its lead. No keys take none.
    width: f32,
}

impl Chord {
    /// The keys sit 2px further out than a button's own gap.
    const LEAD: f32 = BUTTON_GAP + 2.0;

    fn new(ui: &Ui, keys: &[&str], ink: Color32) -> Self {
        let glyphs: Vec<_> = keys.iter().map(|key| line(ui, key, Type::KEYS, ink, f32::INFINITY)).collect();
        let width =
            if glyphs.is_empty() { 0.0 } else { Self::LEAD + glyphs.iter().map(|glyph| glyph.size().x).sum::<f32>() + KEYS_GAP * (glyphs.len() - 1) as f32 };
        Self { glyphs, width }
    }

    /// Paints the keys after the text that ends at `text_right`, centred on `centre_y`.
    fn paint(self, ui: &Ui, text_right: f32, centre_y: f32, ink: Color32, under: Color32) {
        let mut x = text_right + Self::LEAD;
        for glyph in self.glyphs {
            let at = egui::pos2(x, centre_y - glyph.size().y / 2.0);
            x += glyph.size().x + KEYS_GAP;
            paint_text(ui, at, glyph, ink, under);
        }
    }
}

fn focus_note_id() -> egui::Id {
    egui::Id::NULL.with("focused_widget")
}

/// Notes, at the end of each frame, which widget has the focus. egui drops the focus on Esc before the next frame's
/// widgets run, so `back_pressed` needs the note to tell leaving a field from going back.
pub fn note_focus(ctx: &egui::Context) {
    let focused = ctx.memory(|memory| memory.focused());
    ctx.data_mut(|data| data.insert_temp(focus_note_id(), focused));
}

/// Whether Esc in this frame leaves a text field. The note counts only if egui agrees the widget held the focus as
/// this frame began, so a note left by a field that has since gone, as when Cmd K leaves the screen, never counts.
fn leaves_a_field(ctx: &egui::Context) -> bool {
    let noted: Option<egui::Id> = ctx.data(|data| data.get_temp(focus_note_id())).flatten();
    noted.is_some_and(|id| ctx.memory(|memory| memory.had_focus_last_frame(id)) && egui::text_edit::TextEditState::load(ctx, id).is_some())
}

/// Esc as back, for the launcher and the application details. An open popup takes Esc first and closes, and in a
/// text field Esc only leaves the field: a second Esc goes back. The key is taken only when it goes back.
pub fn back_pressed(ctx: &egui::Context) -> bool {
    let esc = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    esc && !egui::Popup::is_any_open(ctx) && !leaves_a_field(ctx) && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

fn dialog_note_id() -> egui::Id {
    egui::Id::NULL.with("dialog_open")
}

/// Notes that a dialog asks this frame. Called before any widget runs, so the keys behind it hold still.
pub fn note_dialog(ctx: &egui::Context) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| data.insert_temp(dialog_note_id(), frame));
}

/// Whether a dialog asks this frame, as `note_dialog` noted.
pub(in crate::ui_kit) fn dialog_open(ctx: &egui::Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|data| data.get_temp::<u64>(dialog_note_id())) == Some(frame)
}

/// How long a decision moves for, in seconds, while motion is on.
const MOTION_SECONDS: f32 = 0.15;

fn reduced_motion_id() -> egui::Id {
    egui::Id::NULL.with("reduced_motion")
}

/// Turns motion off or back on for the whole window. Motion is decoration only: off, every change shows at once.
/// Kept apart from egui's own animation time, which the test harness zeroes.
pub fn set_reduced_motion(ctx: &egui::Context, reduced: bool) {
    ctx.data_mut(|data| data.insert_temp(reduced_motion_id(), reduced));
}

/// How long a decision moves for, in seconds: none while motion is reduced.
pub fn motion_time(ctx: &egui::Context) -> f32 {
    let reduced = ctx.data(|data| data.get_temp::<bool>(reduced_motion_id())).unwrap_or(false);
    if reduced { 0.0 } else { MOTION_SECONDS }
}

fn command_request_id() -> egui::Id {
    egui::Id::NULL.with("requested_command")
}

fn request_command(ui: &Ui, command: &str) {
    ui.ctx().data_mut(|data| data.insert_temp(command_request_id(), command.to_owned()));
}

/// A button showing `label` that opens `line` in the launcher. The shell picks the request up.
fn command_press(ui: &mut Ui, label: &str, line: &str, hint: &str, look: Look, fit: Fit) -> Response {
    let response = press(ui, label, look, fit, None).on_hover_text(hint);
    if response.clicked() {
        request_command(ui, line);
    }
    response
}

/// A command action: it opens the launcher before anything runs, so its label ends in "…".
fn command_action_with(ui: &mut Ui, label: &str, line: &str, hint: &str, look: Look, fit: Fit) -> Response {
    if label.ends_with('…') { command_press(ui, label, line, hint, look, fit) } else { command_press(ui, &format!("{label}…"), line, hint, look, fit) }
}

/// A command button on any screen, labelled with the command itself.
pub fn command_button(ui: &mut Ui, command: &str) {
    command_press(ui, command, command, "Open in command launcher", Look::Plain, COMMAND);
}

pub fn danger_command_button(ui: &mut Ui, command: &str) {
    command_press(ui, command, command, "Open in command launcher", Look::Danger, COMMAND);
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
    let response = link(ui, text);
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
/// A press shows the `mark` without its rule, a preview of the open tab.
/// `line` is the palest neutral that stands off both the desk's base and its grid lines.
fn tab_swipe(ui: &Ui, response: &Response, on: bool) -> Option<(Color32, Option<Color32>)> {
    let p = Palette::of(ui);
    match (on, pointer_over(response), pressed(response)) {
        (true, _, _) => Some((p.mark, Some(p.turn))),
        (false, _, true) => Some((p.mark, None)),
        (false, true, false) => Some((p.line, None)),
        (false, false, false) => None,
    }
}

fn tab_info(label: &str, mark: TabMark, on: bool, enabled: bool) -> WidgetInfo {
    let title = match mark {
        TabMark::Count(n) if n > 0 => format!("{label}, {n} need you"),
        TabMark::Failed => format!("{label}, could not be read"),
        _ => label.to_owned(),
    };
    WidgetInfo::selected(Role::Tab, enabled, on, title)
}

/// A tab in the menu, on the desk. The open tab's label is highlighted: a pale swipe over a stamp-ink rule.
/// Its mark sits after the label, outside the swipe.
pub fn folder_tab(ui: &mut Ui, label: &str, mark: TabMark, on: bool) -> Response {
    let p = Palette::of(ui);
    let font = if on { Type::TAB_ON } else { Type::TAB };
    let enabled = ui.is_enabled();
    let ink = if on { p.text } else { p.muted };
    let ink = if enabled { ink } else { dimmed(ink) };
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
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| tab_info(label, mark, on, enabled));
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

/// A segment's colours: fill and ink when on, ink when off, the fill under the pointer and the one under a press.
#[derive(Clone, Copy)]
struct SegmentInks {
    on: (Color32, Color32),
    off: Color32,
    hover: Color32,
    pressed: Color32,
}

/// A segment is as wide as its label in the heavier face, on or off, so picking one never moves the others.
fn segment(ui: &mut Ui, label: &str, on: bool, look: SegmentLook, inks: SegmentInks) -> Response {
    let font = if on { look.font_on } else { look.font };
    let enabled = ui.is_enabled();
    // The picked segment keeps its ink, as a filled button does: its fill is what goes pale.
    let ink = if on {
        inks.on.1
    } else if enabled {
        inks.off
    } else {
        dimmed(inks.off)
    };
    let room = ui.available_width() - look.padding * 2.0;
    let galley = line(ui, label, font, ink, room);
    let widest = if on { galley.size().x } else { line(ui, label, look.font_on, ink, room).size().x.max(galley.size().x) };
    let size = vec2(widest + look.padding * 2.0, look.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = match (on, pointer_over(&response), pressed(&response)) {
        (true, _, _) if !enabled => dimmed(inks.on.0),
        (true, _, _) => inks.on.0,
        (false, _, true) => inks.pressed,
        (false, true, false) => inks.hover,
        (false, false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, look.radius, fill);
    let response = cut_hover(response, &galley, label);
    paint_centred(ui, rect, galley, ink, fill);
    interactive(ui, &response, look.radius);
    response.widget_info(|| WidgetInfo::selected(Role::Tab, enabled, on, label));
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
                if segment(ui, label, *value == current, look, SegmentInks { on: (p.text, p.sheet), off: p.muted, hover: p.panel, pressed: p.raised }).clicked()
                {
                    picked = Some(*value);
                }
            }
        });
        hairline(ui, p.text);
    });
    picked
}

/// A button in the status strip that opens a screen, such as Settings, and names its chord after the text.
/// Clear at rest, `raised` with `text` ink while `on`. It is as wide as its text in the heavier face, so it never moves.
/// A screen reader hears the text alone, selected while `on`.
pub fn status_button(ui: &mut Ui, text: &str, keys: &[&str], on: bool) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let (font, ink) = if on { (Type::TAG, p.text) } else { (Type::SMALL, p.muted) };
    let ink = if enabled { ink } else { dimmed(ink) };
    let key_ink = if enabled { p.muted } else { dimmed(p.muted) };
    let label = line(ui, text, font, ink, f32::INFINITY);
    let widest = line(ui, text, Type::TAG, ink, f32::INFINITY).size().x.max(label.size().x);
    let glyphs: Vec<_> = keys.iter().map(|key| line(ui, key, Type::SMALL, key_ink, f32::INFINITY)).collect();
    let chord = glyphs.iter().map(|glyph| glyph.size().x).sum::<f32>() + KEYS_GAP * glyphs.len().saturating_sub(1) as f32;
    let lead = if glyphs.is_empty() { 0.0 } else { Chord::LEAD };
    let size = vec2(STATUS_SEGMENT_PADDING * 2.0 + widest + lead + chord, STATUS_SEGMENT_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = match (on, pointer_over(&response), pressed(&response)) {
        (_, _, true) => p.line,
        (true, _, false) => p.raised,
        (false, true, false) => p.panel,
        (false, false, false) => Color32::TRANSPARENT,
    };
    ui.painter().rect_filled(rect, RADIUS_BUTTON, fill);
    let centre_y = rect.center().y;
    paint_text(ui, egui::pos2(rect.left() + STATUS_SEGMENT_PADDING, centre_y - label.size().y / 2.0), label, ink, fill);
    let mut x = rect.left() + STATUS_SEGMENT_PADDING + widest + lead;
    for glyph in glyphs {
        let at = egui::pos2(x, centre_y - glyph.size().y / 2.0);
        x += glyph.size().x + KEYS_GAP;
        paint_text(ui, at, glyph, key_ink, fill);
    }
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| WidgetInfo::selected(Role::Button, enabled, on, text));
    response
}

/// A pickable row in a list. `text` is cut to one line.
pub fn select_row(ui: &mut Ui, text: &str, style: Type, on: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), Sense::click());
    let look = pick_look(&p, on, pointer_over(&response), pressed(&response));
    let fill = look.fill;
    ui.painter().rect_filled(rect, 0.0, fill);
    if let Some(edge) = look.edge {
        paint_pick_edge(ui, rect, 0, edge);
    }
    let ink = if on { p.text } else { p.muted };
    let galley = line(ui, text, style, ink, rect.width() - crate::ui_kit::tokens::INSET as f32 * 2.0);
    let at = egui::pos2(rect.left() + crate::ui_kit::tokens::INSET as f32, rect.center().y - galley.size().y / 2.0);
    paint_text(ui, at, galley, ink, fill);
    interactive_row(ui, &response, 0);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, text));
    response
}

/// A command in the catalogue: its name, what it does and what it writes. Painted whole, so a test finds one label.
pub fn command_row(ui: &mut Ui, name: &str, summary: &str, writes: &str, on: bool) -> Response {
    let p = Palette::of(ui);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), PICK_ROW_HEIGHT), Sense::click());
    let look = pick_look(&p, on, pointer_over(&response), pressed(&response));
    let fill = look.fill;
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter().rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), HAIRLINE)), 0.0, p.line);
    if let Some(edge) = look.edge {
        paint_pick_edge(ui, rect, 0, edge);
    }
    let left = rect.left() + PICK_ROW_INLINE;
    let right = rect.right() - PICK_ROW_INLINE;
    let summary_left = left + COMMAND_NAME_WIDTH;
    let written = line(ui, writes, Type::MONO, p.faint, rect.width() / 2.0);
    let named = line(ui, name, Type::MONO, p.text, COMMAND_NAME_WIDTH - ROW_ITEM_GAP);
    let room = (right - written.size().x - ROW_ITEM_GAP - summary_left).max(0.0);
    let summed = line(ui, summary, Type::BODY, p.text, room);
    let centred = |galley: &Arc<Galley>, x: f32| egui::pos2(x, rect.center().y - galley.size().y / 2.0);
    let (named_at, summed_at, written_at) = (centred(&named, left), centred(&summed, summary_left), centred(&written, right - written.size().x));
    let response = cut_hover(cut_hover(cut_hover(response, &named, name), &summed, summary), &written, writes);
    paint_text(ui, named_at, named, p.text, fill);
    paint_text(ui, summed_at, summed, p.text, fill);
    paint_text(ui, written_at, written, p.faint, fill);
    interactive_row(ui, &response, 0);
    response.widget_info(|| WidgetInfo::selected(Role::Button, true, on, format!("{name} {summary}")));
    response
}

/// A link in a sentence or under an action: `active` ink with a `HAIRLINE` underline at rest, a `MARK_RULE` underline
/// under the pointer, and `text` ink while pressed. Its hit rect is at least `MIN_HIT` tall, so the line of text is not the whole target.
pub fn link(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of(ui);
    // A wrapping row moves a link that does not fit to its next line, so the text may take the row's whole width.
    let room = ui.max_rect().width();
    let size = line(ui, text, Type::NOTE, p.active, room).size();
    let (rect, response) = ui.allocate_exact_size(vec2(size.x, size.y.max(MIN_HIT)), Sense::click());
    let (over, down) = (pointer_over(&response), pressed(&response));
    let ink = if down { p.text } else { p.active };
    let galley = line(ui, text, Type::NOTE, ink, room);
    let text_rect = Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - size.y / 2.0), size);
    let rule = if over && !down { MARK_RULE } else { HAIRLINE };
    ui.painter().hline(text_rect.x_range(), text_rect.bottom() - 1.0, Stroke::new(rule, ink));
    paint_text(ui, text_rect.min, galley, ink, Color32::TRANSPARENT);
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| WidgetInfo::labeled(Role::Link, ui.is_enabled(), text));
    response
}

/// A field that filters what is listed under it: the `hint` in `faint` while it is empty, and a Clear link inside its right
/// edge while it holds text. Esc in it clears it too. The response is the field's, changed when the text changed either way.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &str) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    // The field's id is its place in `ui`, taken before anything is added, so Clear coming and going never moves it
    // and the field keeps the focus, and two fields in one `ui` never share it.
    let id = ui.next_auto_id().with("search_field");
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), FIELD_HEIGHT), Sense::hover());
    // The field's box is painted last, when its focus is known, under what it holds.
    let under = ui.painter().add(egui::Shape::Noop);
    let inner = rect.shrink2(vec2(FIELD_PADDING, 0.0));
    let mut room = inner;
    let mut cleared = false;
    if !text.is_empty() {
        let mut side = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if link(&mut side, "Clear").clicked() {
            text.clear();
            cleared = true;
        }
        room.max.x = side.min_rect().left() - ROW_ITEM_GAP;
    }
    let ink = if enabled { p.text } else { dimmed(p.text) };
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .font(Type::BODY.font())
        .text_color(ink)
        .hint_text(egui::RichText::new(hint).font(Type::BODY.font()).color(if enabled { p.faint } else { dimmed(p.faint) }))
        .desired_width(room.width().max(0.0))
        .align(egui::Align2::LEFT_CENTER);
    let mut response = ui.put(room, edit);
    if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) && !text.is_empty() {
        text.clear();
        cleared = true;
    }
    if cleared {
        response.mark_changed();
    }
    let border = match (enabled, response.has_focus()) {
        (false, _) => dimmed(p.line),
        (true, true) => p.text,
        (true, false) => p.line,
    };
    ui.painter().set(under, egui::Shape::rect_filled(rect, RADIUS_BUTTON, p.field));
    ui.painter().rect_stroke(rect, RADIUS_BUTTON, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    response
}

/// How long a Copy button reads Copied, in seconds.
const COPIED_FOR: f64 = 2.0;

/// A small button that copies `text` to the clipboard and reads Copied for two seconds, so the user sees it worked.
/// It is as wide as the longer of the two words, so nothing beside it moves when the word changes.
pub fn copy_button(ui: &mut Ui, text: &str) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let ink = if enabled { p.text } else { dimmed(p.text) };
    let copy = line(ui, "Copy", SMALL.font, ink, f32::INFINITY);
    let copied = line(ui, "Copied", SMALL.font, ink, f32::INFINITY);
    let size = vec2(copy.size().x.max(copied.size().x) + SMALL.padding * 2.0, SMALL.height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let ctx = ui.ctx().clone();
    let now = ctx.input(|input| input.time);
    if response.clicked() {
        ctx.copy_text(text.to_owned());
        ctx.data_mut(|data| data.insert_temp(response.id, now));
    }
    let left = ctx.data(|data| data.get_temp::<f64>(response.id)).map_or(0.0, |at| COPIED_FOR - (now - at));
    if left > 0.0 {
        ctx.request_repaint_after(Duration::from_secs_f64(left));
    }
    let (label, galley) = if left > 0.0 { ("Copied", copied) } else { ("Copy", copy) };
    let (fill, border) = if enabled {
        look_colors(&p, Look::Plain, pointer_over(&response), pressed(&response), on_desk(ui))
    } else {
        disabled_colors(&p, Look::Plain, on_desk(ui))
    };
    ui.painter().rect(rect, RADIUS_BUTTON, fill, Stroke::new(HAIRLINE, border), StrokeKind::Inside);
    paint_centred(ui, rect, galley, ink, fill);
    interactive(ui, &response, RADIUS_BUTTON);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    response
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    const LINE: &str = "/apply https://tessel.example/jobs/1";

    #[test]
    fn pick_look_never_matches_its_surface() {
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            let p = Palette::for_theme(theme);
            let hover = pick_look(&p, false, true, false);
            for (name, container) in [("panel", p.panel), ("card", p.card), ("sheet", p.sheet)] {
                assert_ne!(hover.fill, container, "{theme:?}: a hovered row on {name} shows no fill");
            }
            assert_eq!(hover.edge, None, "{theme:?}: a hovered row looks picked");
            for (hovered, pressed) in [(false, false), (true, false), (true, true)] {
                assert!(pick_look(&p, true, hovered, pressed).edge.is_some(), "{theme:?}: a picked row lost its edge");
            }
            assert_ne!(pick_look(&p, false, true, true).fill, hover.fill, "{theme:?}: a pressed row looks hovered");
            for look in [Look::Plain, Look::Primary, Look::Danger, Look::On] {
                for desk in [false, true] {
                    assert_ne!(
                        look_colors(&p, look, true, true, desk).0,
                        look_colors(&p, look, true, false, desk).0,
                        "{theme:?}: a pressed button looks hovered"
                    );
                }
            }
        }
    }

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
    fn copy_reads_copied_then_copy() {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([300.0, 80.0]).with_context_setup(setup).build_ui(|ui| {
            copy_button(ui, "a draft");
        });
        harness.run();
        let width = harness.get_by_label("Copy").rect().width();
        harness.get_by_label("Copy").click();
        harness.run();
        assert_eq!(harness.get_by_label("Copied").rect().width(), width, "the button moved when it changed its word");
        assert!(harness.query_by_label("Copy").is_none());
        let later = harness.ctx.input(|input| input.time) + 2.1;
        harness.input_mut().time = Some(later);
        harness.run();
        harness.get_by_label("Copy");
        assert!(harness.query_by_label("Copied").is_none());
    }

    #[test]
    fn command_actions_end_in_an_ellipsis() {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([800.0, 120.0]).with_context_setup(setup).build_ui(|ui| {
            ui.horizontal(|ui| {
                command_action(ui, "Tune scoring", "/setup");
                primary_command_action(ui, "Find skill gaps", "/upskill");
                small_command_action(ui, "Add portal", "/add-portal");
                command_action(ui, "Find new jobs…", "/scrape");
                command_button(ui, "/reset");
            });
        });
        harness.run();
        for label in ["Tune scoring…", "Find skill gaps…", "Add portal…", "Find new jobs…", "/reset"] {
            harness.get_by_label(label);
        }
        for label in ["Tune scoring", "Find new jobs……", "/reset…"] {
            assert!(harness.query_by_label(label).is_none(), "{label} is shown");
        }
        harness.get_by_label("Add portal…").click();
        harness.run();
        assert_eq!(take_command_request(&harness.ctx).as_deref(), Some("/add-portal"));
    }

    fn kit_setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    #[test]
    fn a_toggle_group_is_one_radio_group_with_one_on() {
        let mut current = Some(1);
        let mut harness = Harness::builder().with_size([400.0, 80.0]).with_context_setup(kit_setup).build_ui(move |ui| {
            if let Some(picked) = toggle_group(ui, current, &[(0, "System"), (1, "Light"), (2, "Dark")]) {
                current = Some(picked);
            }
        });
        harness.run();
        let on = |harness: &Harness| -> Vec<String> {
            harness
                .query_all_by_role(egui::accesskit::Role::RadioButton)
                .filter(|node| node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
                .filter_map(|node| node.accesskit_node().label())
                .collect()
        };
        assert_eq!(harness.query_all_by_role(egui::accesskit::Role::RadioGroup).count(), 1);
        assert_eq!(harness.query_all_by_role(egui::accesskit::Role::RadioButton).count(), 3);
        assert_eq!(on(&harness), ["Light"]);
        harness.get_by_label("Dark").click();
        harness.run();
        assert_eq!(on(&harness), ["Dark"]);
    }

    #[test]
    fn an_off_step_value_turns_no_toggle_on() {
        let mut harness = Harness::builder().with_size([400.0, 80.0]).with_context_setup(kit_setup).build_ui(|ui| {
            toggle_group(ui, None::<u8>, &[(90, "90%"), (100, "100%")]);
        });
        harness.run();
        assert!(
            harness.query_all_by_role(egui::accesskit::Role::RadioButton).all(|node| node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::False))
        );
    }

    #[test]
    fn a_status_button_is_named_by_its_text_and_clicks() {
        let clicked = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = std::rc::Rc::clone(&clicked);
        let mut harness = Harness::builder().with_size([300.0, 60.0]).with_context_setup(kit_setup).build_ui(move |ui| {
            if status_button(ui, "Settings", &command_chord(","), true).clicked() {
                seen.set(true);
            }
        });
        harness.run();
        let button = harness.get_by_label("Settings");
        assert_eq!(button.accesskit_node().toggled(), Some(egui::accesskit::Toggled::True));
        button.click();
        harness.run();
        assert!(clicked.get());
    }

    #[test]
    fn clear_shows_only_with_text_and_empties_the_search() {
        let text = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let field = std::rc::Rc::clone(&text);
        let mut harness = Harness::builder().with_size([300.0, 60.0]).with_context_setup(kit_setup).build_ui(move |ui| {
            search_field(ui, &mut field.borrow_mut(), "Search settings");
        });
        harness.run();
        assert!(harness.query_by_label("Clear").is_none(), "an empty search offers Clear");
        text.borrow_mut().push_str("spend");
        harness.run();
        harness.get_by_label("Clear").click();
        harness.run();
        assert_eq!(text.borrow().as_str(), "");
        harness.run();
        assert!(harness.query_by_label("Clear").is_none());
    }

    #[test]
    fn two_fields_with_one_hint_keep_their_own_text_and_the_focus() {
        use egui_kittest::kittest::NodeT;

        let texts = std::rc::Rc::new(std::cell::RefCell::new([String::new(), String::new()]));
        let fields = std::rc::Rc::clone(&texts);
        let mut harness = Harness::builder().with_size([300.0, 120.0]).with_context_setup(kit_setup).build_ui(move |ui| {
            let [first, second] = &mut *fields.borrow_mut();
            search_field(ui, first, "Search settings");
            search_field(ui, second, "Search settings");
        });
        harness.run();
        let field = |harness: &Harness, at: usize| harness.get_all_by_role(egui::accesskit::Role::TextInput).nth(at).unwrap().accesskit_node().id();
        let first = field(&harness, 0);
        harness.get_by(|node| node.id() == first).focus();
        harness.run();
        harness.get_by(|node| node.id() == first).type_text("spend");
        harness.run();
        // Clear now shows in the first field, and the next keys still reach it.
        harness.get_by(|node| node.id() == first).type_text("ing");
        harness.run();
        let second = field(&harness, 1);
        harness.get_by(|node| node.id() == second).focus();
        harness.run();
        harness.get_by(|node| node.id() == second).type_text("motion");
        harness.run();
        assert_eq!(*texts.borrow(), ["spending".to_owned(), "motion".to_owned()]);
    }

    #[test]
    fn reduced_motion_takes_no_time() {
        let ctx = egui::Context::default();
        assert!(motion_time(&ctx) > 0.0);
        set_reduced_motion(&ctx, true);
        assert_eq!(motion_time(&ctx), 0.0);
        set_reduced_motion(&ctx, false);
        assert!(motion_time(&ctx) > 0.0);
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
