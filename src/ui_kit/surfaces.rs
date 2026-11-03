use std::sync::Arc;

use egui::{
    Align, Color32, CornerRadius, EventFilter, Frame, Galley, Key, Layout, Margin, Modifiers, Painter, Panel, Popup, PopupCloseBehavior, Pos2, Rect, RectAlign,
    Response, Role, Sense, Shadow, Stroke, StrokeKind, TextWrapMode, Ui, UiBuilder, UiStackInfo, Vec2, WidgetInfo, pos2, vec2,
};
use egui_commonmark::CommonMarkCache;

use crate::shared_kernel::Source;
use crate::shared_kernel::placeholders::Piece;
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::Palette;
use crate::ui_kit::controls::{
    danger_button, dialog_open, enabled_when, field_in, interactive, interactive_part, interactive_row, link, paint_pick_edge, pick_look,
    plain_shortcut_button, pointer_over, pressed, primary_button, reveal_command,
};
use crate::ui_kit::marks::{FillMarks, KnotState};
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::text::{
    add_cut_text, add_galley, add_job, add_marked, add_text, blank_line, copyable, cut_hover, line, paint_centred, paint_marked, paint_text, profile_markdown,
    release_markdown, rich, strong, table_head, wrapped,
};
use crate::ui_kit::tokens::{
    BAND_COLUMNS, BAND_COMPACT_PADDING, BAND_GAP, BAND_LABEL_HEIGHT, BAND_PADDING, BAND_ROW_GAP, BODY_PADDING, BUTTON_GAP, BUTTON_HEIGHT, CARD_PADDING,
    CHECK_BOX, CODE_PADDING, DEAL_FOOT_PADDING, DEAL_PADDING, DISABLED_OPACITY, DOC_MEASURE, EDITOR_GAP, EDITOR_PADDING, EDITOR_ROWS, EDITOR_ROWS_CHANGED,
    EMPTY_GAP, EMPTY_PADDING, EMPTY_WIDTH, FADED_OPACITY, FIELD_INLINE, FIELD_INLINE_MARGIN, FIELD_INLINE_NUMBER, FIELD_PADDING, FIGURE_BAR_MARGIN,
    FIGURE_SKELETON, FIGURE_SKELETON_MARGIN, FIGURE_SKELETON_NOTES, FOCUS_OFFSET, FOCUS_RING, GRID_PITCH, GUTTER, HAIRLINE, HEAD_GAP, HEAD_PADDING,
    HEAD_TITLE_MIN, HEADER_PADDING, INSET, ITEM_GAP, KNOT_SIZE_SMALL, LANE_GAP, LANE_PADDING, LATER_OPACITY, NODE_ARROW, NODE_GAP, NODE_PADDING,
    NODE_TALLY_HEIGHT, NODE_TALLY_MARGIN, NOTE_PADDING, NOTES_BODY_PADDING, NOTES_GAP, NOTES_HEAD_PADDING, NOTES_HEIGHT, NOTES_INLINE, NOTES_SLIP_PADDING,
    NOTES_WIDTH, OFFSET_SHADOW, PICK_EDGE, PICK_PADDING, PROMPT_PADDING, RADIO, RADIO_DOT, RADIUS_BUTTON, RADIUS_CARD, RADIUS_PANEL, RAIL_TITLE_PADDING,
    ROLE_DATES, ROLE_PADDING, ROW_GAP, ROW_ITEM_GAP, ROW_PADDING, SCREEN_PADDING, SETTING_CHECK_GAP, SETTING_CONTROL_GAP, SETTING_HELP_GAP, SETTING_NOTE_GAP,
    SETTING_ROW_MIN, SETTING_ROW_PADDING, SETTING_SECTION_GAP, SETTINGS_ACTIONS_LIFT, SETTINGS_BODY_PADDING, SETTINGS_HEAD_PADDING, SHELL_PADDING,
    SKELETON_GAP, SKELETON_LINE, SPLIT_GAP, STATUS_DOT, STATUS_PADDING, STATUS_PROGRESS_GAP, STEP_LINE_GAP, STEP_LINE_HEIGHT, STEP_NAME_WIDTH,
    TABLE_CELL_PADDING, TABLE_HEAD_HEIGHT, TALLY_BAND, Type, WAITS_EDGE, WINDOW_SHADOW,
};

fn margin((x, y): (i8, i8)) -> Margin {
    Margin::symmetric(x, y)
}

/// Marks the `Ui` that stands for the desk, so a widget can tell whether it sits straight on it.
const DESK: &str = "jobdesk_desk";

/// The gridded desk under `ui`'s whole rect, with everything `add` lays out standing on it.
/// Widgets that stand straight on it, in no filled frame, see `on_desk` and keep the grid out of their small text.
pub fn desk<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    paint_grid(ui.painter(), ui.max_rect());
    ui.scope_builder(UiBuilder::new().ui_stack_info(UiStackInfo::default().with_tag(DESK)), add).inner
}

/// Marks a `Ui` that takes a click as a whole, such as a row or a card. `copyable` text must never sit inside one.
pub(in crate::ui_kit) const CLICKABLE: &str = "jobdesk_clickable";

/// A child `Ui` that takes a click anywhere on it, tagged `CLICKABLE`.
fn clickable() -> UiBuilder {
    UiBuilder::new().sense(Sense::click()).ui_stack_info(UiStackInfo::default().with_tag(CLICKABLE))
}

/// Marks a `Ui` whose owner paints its own fill under it with the painter, not with a `Frame`, such as a row that lights up.
const SURFACE: &str = "jobdesk_surface";

/// A child `Ui` that counts as a surface for `on_desk`, for a widget that paints its own fill under what it holds.
pub(in crate::ui_kit) fn surface() -> UiBuilder {
    UiBuilder::new().ui_stack_info(UiStackInfo::default().with_tag(SURFACE))
}

/// Whether `ui` stands straight on the desk: inside `desk`, with no filled frame, such as paper or a slip, in between,
/// and no `surface` either. Popups and windows stand on their own fill, so they are never on the desk.
pub(in crate::ui_kit) fn on_desk(ui: &Ui) -> bool {
    for node in ui.stack().iter() {
        if node.tags().contains(DESK) {
            return true;
        }
        if node.frame().fill != Color32::TRANSPARENT || node.tags().contains(SURFACE) {
            return false;
        }
    }
    false
}

/// `add` on a patch of the desk's base, the size of what it lays out, so no grid line runs through it.
pub(in crate::ui_kit) fn backed<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).sheet;
    Frame::new().fill(fill).show(ui, add).inner
}

/// The desk's base, ruled every `GRID_PITCH` in `grid`.
/// The lines start at the window's origin, so they hold still while content scrolls over them.
fn paint_grid(painter: &Painter, rect: Rect) {
    let p = Palette::for_theme(painter.ctx().theme());
    painter.rect_filled(rect, 0.0, p.sheet);
    let stroke = Stroke::new(HAIRLINE, p.grid);
    // On pixel centres, so each line stays one crisp row of pixels.
    let first = |from: f32| (from / GRID_PITCH).ceil() * GRID_PITCH;
    let mut x = first(rect.left());
    while x <= rect.right() {
        painter.vline(painter.round_to_pixel_center(x), rect.y_range(), stroke);
        x += GRID_PITCH;
    }
    let mut y = first(rect.top());
    while y <= rect.bottom() {
        painter.hline(rect.x_range(), painter.round_to_pixel_center(y), stroke);
        y += GRID_PITCH;
    }
}

/// Where a screen's content sits: straight on the desk, with no fill of its own.
pub fn sheet_frame() -> Frame {
    Frame::new()
}

/// The menu on the desk: brand, tabs and New task in one row.
pub fn shell_top_frame() -> Frame {
    let (x, top, bottom) = SHELL_PADDING;
    Frame::new().inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 })
}

/// The strip at the desk's bottom edge, for the status line. The shell draws its top hairline as the panel's separator.
pub fn status_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).sheet).inner_margin(margin((STATUS_PADDING.0 as i8, STATUS_PADDING.1 as i8)))
}

/// The big line at the top of a screen. The voice says what the line is.
#[derive(Debug, Clone, Copy)]
pub enum Title<'a> {
    /// A screen's name.
    Slab(&'a str),
    /// The one line a screen leads with.
    Lead(&'a str),
    /// A command or a path.
    Typed(&'a str),
}

#[derive(Debug, Clone, Copy)]
pub enum Subtitle<'a> {
    Text(&'a str),
    Typed(&'a str),
}

/// A screen's head: title and subtitle on the left, `actions` on the right, their bottom edges level, as `align-items: flex-end` sets them.
/// With no subtitle, the actions sit on the title's centre line instead. Actions are added right to left. Title and subtitle are cut to the width the actions leave, with the full text on hover.
/// When the actions leave less than `HEAD_TITLE_MIN`, they move to a row of their own under the title.
///
/// `actions` runs twice a frame: first in an unseen, disabled sizing pass to measure it, then for real.
/// So it must change nothing except inside `clicked()`, and must not give its widgets explicit ids,
/// because the sizing pass would claim the same ids first.
pub fn page_head(ui: &mut Ui, title: Title, subtitle: Option<Subtitle>, mut actions: impl FnMut(&mut Ui)) {
    let (x, top, bottom) = HEAD_PADDING;
    Frame::new().inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let full = ui.available_width();
        let right = ui.max_rect().right();
        let measure = Rect::from_min_size(ui.cursor().min, vec2(full, BUTTON_HEIGHT));
        let mut sizing =
            ui.new_child(UiBuilder::new().id_salt("page_head_sizing").max_rect(measure).layout(Layout::right_to_left(Align::Center)).sizing_pass().invisible());
        sizing.spacing_mut().item_spacing.x = HEAD_GAP.0;
        actions(&mut sizing);
        let used = sizing.min_rect().size();
        let beside = if used.x > 0.0 { full - used.x - HEAD_GAP.0 } else { full };
        let inline = beside >= HEAD_TITLE_MIN;
        let room = if inline { beside } else { full };
        let (title, style) = match title {
            Title::Slab(text) => (text, Type::H1),
            Title::Lead(text) => (text, Type::LEAD),
            Title::Typed(text) => (text, Type::HEAD_COMMAND),
        };
        let title = line(ui, title, style, Palette::of(ui).text, room);
        let subtitle = subtitle.map(|subtitle| {
            let (text, style) = match subtitle {
                Subtitle::Text(text) => (text, Type::BODY),
                Subtitle::Typed(text) => (text, Type::MONO),
            };
            line(ui, text, style, Palette::of(ui).muted, room)
        });
        let line_gap = ITEM_GAP - 2.0;
        // A one-line head centres its actions on the title. A two-line one ends both on one line.
        let centred = subtitle.is_none();
        let text_height = title.size().y + subtitle.as_ref().map_or(0.0, |subtitle| line_gap + subtitle.size().y);
        let overhang = if inline { (used.y - text_height).max(0.0) } else { 0.0 };
        if overhang > 0.0 {
            ui.add_space(if centred { overhang / 2.0 } else { overhang });
        }
        let text = ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = line_gap;
                add_galley(ui, title);
                if let Some(subtitle) = subtitle {
                    add_galley(ui, subtitle);
                }
            })
            .response
            .rect;
        if inline && centred {
            let row = Rect::from_center_size(pos2((text.left() + right) / 2.0, text.center().y), vec2(right - text.left(), used.y.max(text.height())));
            let mut side = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::Center)));
            side.spacing_mut().item_spacing.x = HEAD_GAP.0;
            actions(&mut side);
            // Room for the half of the actions that hangs below the title.
            ui.expand_to_include_rect(side.min_rect());
        } else if inline {
            let row = Rect::from_min_max(pos2(text.left(), text.bottom() - used.y.max(text.height())), pos2(right, text.bottom()));
            let mut side = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::BOTTOM)));
            side.spacing_mut().item_spacing.x = HEAD_GAP.0;
            actions(&mut side);
        } else {
            ui.add_space(HEAD_GAP.1);
            ui.allocate_ui_with_layout(vec2(full, BUTTON_HEIGHT), Layout::right_to_left(Align::Center).with_main_wrap(true), |ui| {
                ui.spacing_mut().item_spacing = HEAD_GAP.into();
                actions(ui);
            });
        }
    });
}

/// Where a screen's source stands. A failed source loads as nothing, so the screen must not read it as empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// Not read yet.
    Reading,
    /// Could not be read: the screen hides what it would show and says so.
    Failed,
    Ready,
}

/// What the shell tells a screen beside its data: whether its source loaded, which sources could not be read,
/// and the alert to draw under its head.
/// Every screen draws `page_head`, then `alerts`, then its body, so the alert sits in the same place on each.
#[derive(Clone, Copy)]
pub struct ScreenState<'a> {
    pub load: Load,
    /// Every source that could not be read. A screen fed by more than one hides only the parts a failed one feeds.
    pub unread: &'a [Source],
    pub alerts: &'a dyn Fn(&mut Ui),
}

impl ScreenState<'_> {
    pub fn reading(&self) -> bool {
        self.load == Load::Reading
    }

    pub fn cannot_read(&self, source: Source) -> bool {
        self.unread.contains(&source)
    }
}

/// Why a part's figures are hidden, said in their place, so no zero reads as a real count.
/// Left aligned even in a justified column, which would stretch the wrapped words apart.
pub fn hidden_note(ui: &mut Ui, why: &str) -> Response {
    ui.with_layout(Layout::top_down(Align::Min), |ui| add_text(ui, why, Type::BODY, Ink::Muted)).inner
}

/// The content of a screen, under its head.
pub fn page_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (x, top, bottom) = SCREEN_PADDING;
    Frame::new()
        .inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A fixed column beside a screen's content. Planes and gutters separate it, never a line.
pub fn side_column(id: &'static str, width: f32) -> Panel {
    Panel::right(id)
        .exact_size(width)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().inner_margin(Margin { left: GUTTER, ..Default::default() }))
}

/// A screen's content with a fixed column `width` wide at its right, laid out as `side_column` lays one out beside a
/// central panel. The content is built first, so Tab reaches it before the side column, where a panel would come first.
pub fn with_side_column(ui: &mut Ui, width: f32, main: impl FnOnce(&mut Ui), side: impl FnOnce(&mut Ui)) {
    let area = ui.available_rect_before_wrap();
    let side_rect = Rect::from_min_max(pos2(area.right() - width.min(area.width()), area.top()), area.max);
    let main_rect = Rect::from_min_max(area.min, pos2(side_rect.left(), area.bottom()));
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut main_ui = ui.new_child(column(main_rect, "with_side_main"));
    main_ui.shrink_clip_rect(main_rect);
    main(&mut main_ui);
    let mut side_ui = ui.new_child(column(Rect::from_min_max(pos2(side_rect.left() + f32::from(GUTTER), side_rect.top()), side_rect.max), "with_side_side"));
    side_ui.shrink_clip_rect(side_rect);
    side(&mut side_ui);
    ui.advance_cursor_after_rect(area);
}

/// A main column and a side column of slips, `SPLIT_GAP` apart, both starting at the top.
/// The side column is `side_width` wide, but never wider than the main one.
/// Each column gets the height left in `ui`: a column that should scroll on its own puts a scroll area in it.
/// `add` gets the main column, then the side one, so both can use the same state.
pub fn split<R>(ui: &mut Ui, side_width: f32, add: impl FnOnce(&mut Ui, &mut Ui) -> R) -> R {
    let area = ui.available_rect_before_wrap();
    let side_width = side_width.min((area.width() - SPLIT_GAP) / 2.0);
    let main_rect = Rect::from_min_max(area.min, pos2(area.right() - side_width - SPLIT_GAP, area.bottom()));
    let side_rect = Rect::from_min_max(pos2(area.right() - side_width, area.top()), area.max);
    let id = ui.next_auto_id();
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(id.with(salt)).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut main = ui.new_child(column(main_rect, "split_main"));
    main.set_width(main_rect.width());
    let mut side = ui.new_child(column(side_rect, "split_side"));
    side.set_width(side_rect.width());
    let inner = add(&mut main, &mut side);
    ui.allocate_rect(main.min_rect().union(side.min_rect()), Sense::hover());
    inner
}

/// How far a section's heading, body and rows sit in from its edge.
/// A panel's sit flush with the column; a slip's sit `INSET` in, as app.css pads only `.split > .b > .panel`.
fn inset_id() -> egui::Id {
    egui::Id::NULL.with("section_inset")
}

pub(in crate::ui_kit) fn inset(ui: &Ui) -> i8 {
    ui.data(|data| data.get_temp(inset_id())).unwrap_or(INSET)
}

/// Stacked sections set their own padding, so nothing adds a gap between them.
/// `inset` holds for the section's own content, and the one around it comes back after.
fn stacked<R>(ui: &mut Ui, inset: i8, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.spacing_mut().item_spacing.y = 0.0;
    let outer: Option<i8> = ui.data_mut(|data| data.get_temp(inset_id()));
    ui.data_mut(|data| data.insert_temp(inset_id(), inset));
    let inner = add(ui);
    ui.data_mut(|data| match outer {
        Some(outer) => {
            data.insert_temp(inset_id(), outer);
        }
        None => {
            data.remove_temp::<i8>(inset_id());
        }
    });
    inner
}

/// A section of rows, on paper so no grid line runs between them. Its content sits `INSET` in.
pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    paper(ui, false, |ui| stacked(ui, INSET, add))
}

/// A side slip: a section in the folder plane. Its content sits `INSET` in.
pub fn slip<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new().fill(fill).corner_radius(RADIUS_PANEL).show(ui, |ui| stacked(ui, INSET, add)).inner
}

fn header(ui: &mut Ui, title: &str, note: Option<&str>, ink: Ink) {
    Frame::new().inner_margin(Margin::symmetric(inset(ui), HEADER_PADDING)).show(ui, |ui| {
        ui.horizontal(|ui| {
            add_text(ui, title, Type::SECTION, ink);
            if let Some(note) = note {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_cut_text(ui, note, Type::NOTE, Ink::Muted);
                });
            }
        });
    });
    divider(ui);
}

/// The title of a group of rows inside a section, set in as the section's header is, with no rule of its own.
pub fn group_title(ui: &mut Ui, title: &str) {
    Frame::new().inner_margin(Margin::symmetric(inset(ui), HEADER_PADDING)).show(ui, |ui| {
        add_text(ui, title, Type::SECTION, Ink::Text);
    });
}

/// The title of a group of pick rows in a rail, such as Settings' Personal: small `Type::TABLE_HEAD` in `muted`,
/// so the rows under it lead. It sits in as the rail's other words do.
pub fn rail_title(ui: &mut Ui, title: &str) {
    let (top, bottom) = RAIL_TITLE_PADDING;
    Frame::new().inner_margin(Margin { left: inset(ui), right: inset(ui), top, bottom }).show(ui, |ui| {
        add_text(ui, title, Type::TABLE_HEAD, Ink::Muted);
    });
}

pub fn panel_header(ui: &mut Ui, title: &str, note: Option<&str>) {
    header(ui, title, note, Ink::Text);
}

/// A section heading in a status colour, such as the lane a card sits in.
pub fn panel_header_tone(ui: &mut Ui, title: &str, note: Option<&str>, tone: Tone) {
    header(ui, title, note, tone.into());
}

/// A hairline across the whole width, in `color`.
pub(in crate::ui_kit) fn hairline(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HAIRLINE), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
}

pub fn divider(ui: &mut Ui) {
    let color = Palette::of(ui).line;
    hairline(ui, color);
}

pub fn panel_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .inner_margin(Margin::symmetric(inset(ui), BODY_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            add(ui)
        })
        .inner
}

/// One row of a list inside a panel, ruled off from the row above unless `first`.
pub fn list_row<R>(ui: &mut Ui, first: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    if !first {
        divider(ui);
    }
    Frame::new()
        .inner_margin(Margin::symmetric(inset(ui), ROW_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// How a table cell is ruled underneath: the head in ink, body rows in a hairline, the last row not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellRule {
    Head,
    Row,
    Last,
}

/// One cell of a table, padded inline and ruled underneath. Lay the table out with no item spacing, so rules meet.
/// A table is read in rows, so it sits on paper, and egui_extras paints a picked row's fill under its cells: each cell is a `surface`.
pub fn table_cell<R>(ui: &mut Ui, rule: CellRule, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let rect = ui.max_rect();
    let color = match rule {
        CellRule::Head => Some(p.text),
        CellRule::Row => Some(p.line),
        CellRule::Last => None,
    };
    if let Some(color) = color {
        ui.painter().rect_filled(Rect::from_min_max(pos2(rect.left(), rect.bottom() - HAIRLINE), rect.right_bottom()), 0.0, color);
    }
    let inner = rect.shrink2(vec2(TABLE_CELL_PADDING.0, 0.0));
    ui.scope_builder(surface().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing = vec2(ITEM_GAP, 0.0);
        add(ui)
    })
    .inner
}

/// A whole row that picks something, such as a run in a list. Picked, it takes an ink edge at its leading side.
/// Disabled, it fades to `FADED_OPACITY`, takes no hover and keeps the arrow, so `enabled_when` can hold it.
pub fn pick_row(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let enabled = ui.is_enabled();
    let response = ui
        .scope_builder(clickable(), |ui| {
            if !enabled {
                ui.multiply_opacity(FADED_OPACITY);
            }
            let row = ui.response();
            let look = pick_look(&p, on, enabled && pointer_over(&row), enabled && pressed(&row));
            let frame = Frame::new().fill(look.fill).corner_radius(RADIUS_PANEL).inner_margin(margin(PICK_PADDING));
            let shown = frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                add(ui);
            });
            if let Some(edge) = look.edge {
                paint_pick_edge(ui, shown.response.rect, RADIUS_PANEL, edge);
            }
        })
        .response;
    interactive_row(ui, &response, RADIUS_PANEL);
    response
}

/// `pick_row`, heard as `spoken`, so what a mark in the row says on hover reaches focus too. `focus_tip`, if any, shows
/// as a tooltip while the row has keyboard focus, so a sighted keyboard user reads what the pointer would.
pub fn spoken_pick_row(ui: &mut Ui, on: bool, spoken: &str, focus_tip: Option<&str>, add: impl FnOnce(&mut Ui)) -> Response {
    let response = pick_row(ui, on, add);
    response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), on, spoken));
    if let Some(tip) = focus_tip
        && response.has_focus()
    {
        response.show_tooltip_text(tip);
    }
    response
}

/// A pick row for a choice that is not available yet, such as Cloud providers: disabled, faded to `LATER_OPACITY`,
/// heard as `spoken`, and saying `why` on hover and to a screen reader as its description.
pub fn later_pick_row(ui: &mut Ui, spoken: &str, why: &str, add: impl FnOnce(&mut Ui)) -> Response {
    ui.scope(|ui| {
        // A disabled `pick_row` fades itself to `FADED_OPACITY`, so this takes it the rest of the way.
        ui.multiply_opacity(LATER_OPACITY / FADED_OPACITY);
        let response = enabled_when(ui, false, why, |ui| spoken_pick_row(ui, false, spoken, None, add));
        ui.ctx().accesskit_node_builder(response.id, |node| node.set_description(why));
        response
    })
    .inner
}

/// A page of Settings on paper: `title` in `Type::H1` over one `lead` line in `Type::TAKE` `muted`, `actions` at the right,
/// their bottom edge level with the lead's, a hairline, then `body`, which scrolls with its bar shown at rest.
/// The page takes all the height it has. Unlike `page_head`'s, `actions` runs once, so it may change state and give ids.
pub fn settings_page<A, B>(ui: &mut Ui, title: &str, lead: &str, actions: impl FnOnce(&mut Ui) -> A, body: impl FnOnce(&mut Ui) -> B) -> (A, B) {
    paper(ui, false, |ui| {
        let acted = settings_head(ui, title, lead, actions);
        divider(ui);
        let shown = egui::ScrollArea::vertical().id_salt("settings_page_body").auto_shrink([false, false]).show(ui, |ui| settings_body(ui, body));
        (acted, shown.inner)
    })
}

/// A `settings_page` whose values come from the job search folder: under its body, a one-line strip pinned at the bottom
/// under a hairline names the files, as Profile's foot does. `foot` lays out left to right with no gap of its own, so
/// its words carry their own spaces.
pub fn settings_page_with_foot<A, B>(
    ui: &mut Ui,
    title: &str,
    lead: &str,
    actions: impl FnOnce(&mut Ui) -> A,
    body: impl FnOnce(&mut Ui) -> B,
    foot: impl FnOnce(&mut Ui),
) -> (A, B) {
    paper(ui, false, |ui| {
        let acted = settings_head(ui, title, lead, actions);
        divider(ui);
        let shown = footed(
            ui,
            true,
            false,
            |ui| settings_body(ui, body),
            |ui| {
                divider(ui);
                Frame::new().inner_margin(margin(DEAL_FOOT_PADDING)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        foot(ui);
                    });
                });
            },
        );
        (acted, shown)
    })
}

/// A settings page's head: the title over the lead, and the actions at the right.
fn settings_head<A>(ui: &mut Ui, title: &str, lead: &str, actions: impl FnOnce(&mut Ui) -> A) -> A {
    let (x, top, bottom) = SETTINGS_HEAD_PADDING;
    Frame::new()
        .inner_margin(Margin { left: x, right: x, top, bottom })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let full = ui.available_width();
            let p = Palette::of(ui);
            // The actions go in first, at the right, so the words take the room they leave.
            // They end on the bottom of the words as measured on the frame before.
            let kept = KeptHeight::read(ui, ui.scope_id().with("settings_page_head"), BUTTON_HEIGHT);
            let row = Rect::from_min_size(ui.cursor().min, vec2(full, (kept.height - SETTINGS_ACTIONS_LIFT).max(0.0)));
            let mut side = ui.new_child(UiBuilder::new().id_salt("settings_page_actions").max_rect(row).layout(Layout::right_to_left(Align::Max)));
            side.spacing_mut().item_spacing.x = BUTTON_GAP;
            let acted = actions(&mut side);
            let used = side.min_rect();
            let room = if used.width() > 0.0 { full - used.width() - HEAD_GAP.0 } else { full };
            let title_line = line(ui, title, Type::H1, p.text, room);
            let lead_lines = wrapped(ui, lead, Type::TAKE, p.muted, room.min(DOC_MEASURE));
            let words = ui
                .vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
                    let shown = add_galley(ui, title_line.clone());
                    cut_hover(shown, &title_line, title);
                    add_galley(ui, lead_lines);
                })
                .response
                .rect;
            kept.keep(ui, words.height().max(used.height() + SETTINGS_ACTIONS_LIFT), "a settings head changed height");
            ui.expand_to_include_rect(used);
            acted
        })
        .inner
}

/// A settings page's body, padded, inside its scroll area.
fn settings_body<B>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> B) -> B {
    let (x, top, bottom) = SETTINGS_BODY_PADDING;
    Frame::new()
        .inner_margin(Margin { left: x, right: x, top, bottom })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            body(ui)
        })
        .inner
}

/// A note at the top of a Settings page, in a `ruled_panel`: `text`, with "Show the command" at its right, which opens
/// `line` under the words in a code box, as `reveal_command` does. Hits of the search `query` are highlighted. The caller
/// holds `open`. Returns the link's response.
pub fn ruled_note_with_command(ui: &mut Ui, text: &str, line: &str, open: &mut bool, query: &str) -> Response {
    let (x, y) = NOTE_PADDING;
    ruled_panel(ui, (x, y, y), |ui| {
        ui.spacing_mut().item_spacing.y = ITEM_GAP;
        let shown = ui
            .horizontal(|ui| {
                // The link goes in first, at the right, so the words wrap in the room it leaves.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if *open { "Hide the command" } else { "Show the command" };
                    let shown = link(ui, label);
                    ui.add_space(ROW_ITEM_GAP);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.set_max_width(ui.available_width().min(DOC_MEASURE));
                        let galley = wrapped(ui, text, Type::BODY, Palette::of(ui).text, ui.available_width());
                        add_marked(ui, galley, text, query);
                    });
                    shown
                })
                .inner
            })
            .inner;
        if shown.clicked() {
            *open = !*open;
        }
        if *open {
            code_box(ui, |ui| {
                add_text(ui, line, Type::MONO, Ink::Text);
            });
        }
        shown
    })
}

fn section_rule_id() -> egui::Id {
    egui::Id::NULL.with("setting_section_rule")
}

/// A setting's ink: `ink`, or at the disabled opacity while `ui` is disabled.
fn setting_ink(ui: &Ui, ink: Color32) -> Color32 {
    if ui.is_enabled() { ink } else { ink.gamma_multiply(DISABLED_OPACITY) }
}

/// A part of a Settings page: its heading in `Type::SECTION`, an optional `note` in `Type::NOTE` `muted`, then a hairline.
/// Hits of the search `query` are highlighted. Inside paper there is no heavier section rule.
pub fn setting_section(ui: &mut Ui, title: &str, note: Option<&str>, query: &str) {
    if ui.min_rect().height() > 0.0 {
        ui.add_space(SETTING_SECTION_GAP.0);
    }
    let (text, muted) = (setting_ink(ui, Palette::of(ui).text), setting_ink(ui, Palette::of(ui).muted));
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let width = ui.available_width();
        add_marked(ui, line(ui, title, Type::SECTION, text, width), title, query);
        if let Some(note) = note {
            ui.add_space(SETTING_NOTE_GAP);
            add_marked(ui, wrapped(ui, note, Type::NOTE, muted, width.min(DOC_MEASURE)), note, query);
        }
        ui.add_space(SETTING_SECTION_GAP.1);
        divider(ui);
    });
    // The first row under it takes this rule as its own.
    let at = (ui.ctx().cumulative_pass_nr(), ui.cursor().min);
    ui.ctx().data_mut(|data| data.insert_temp(section_rule_id(), at));
}

/// The hairline above a setting row, unless the row sits straight under a section's rule.
fn setting_rule(ui: &mut Ui) {
    let pass = ui.ctx().cumulative_pass_nr();
    let ruled: Option<(u64, Pos2)> = ui.ctx().data(|data| data.get_temp(section_rule_id()));
    if !ruled.is_some_and(|(at_pass, at)| at_pass == pass && at.distance(ui.cursor().min) < 0.5) {
        divider(ui);
    }
}

/// One setting: `label` in `Type::STRONG` over its `help` in `Type::NOTE` `muted`, held to `DOC_MEASURE`, and the
/// `control` at the right, centred on them. A hairline sits above. Hits of the search `query` are highlighted.
pub fn setting_row(ui: &mut Ui, label: &str, help: &str, query: &str, control: impl FnOnce(&mut Ui)) -> Response {
    let (text, muted) = (setting_ink(ui, Palette::of(ui).text), setting_ink(ui, Palette::of(ui).muted));
    // Each row keeps its own measure, so it is keyed on its place among its parent's widgets.
    let id = ui.next_auto_id();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        setting_rule(ui);
        let full = ui.available_width();
        let origin = ui.cursor().min;
        let top = origin.y + SETTING_ROW_PADDING;
        let least = SETTING_ROW_MIN - SETTING_ROW_PADDING * 2.0;
        // The control goes in first, at the right, so the words take the room it leaves.
        // It is centred on the height the row measured on the frame before.
        let kept = KeptHeight::read(ui, id.with("setting_row"), least);
        let band = Rect::from_min_size(pos2(origin.x, top), vec2(full, kept.height));
        let mut side = ui.new_child(UiBuilder::new().id_salt("setting_control").max_rect(band).layout(Layout::right_to_left(Align::Center)));
        side.spacing_mut().item_spacing.x = BUTTON_GAP;
        control(&mut side);
        let used = side.min_rect();
        let beside = if used.width() > 0.0 { used.width() + SETTING_CONTROL_GAP } else { 0.0 };
        let room = (full - beside).clamp(0.0, DOC_MEASURE);
        let label_lines = wrapped(ui, label, Type::STRONG, text, room);
        let help_lines = (!help.is_empty()).then(|| wrapped(ui, help, Type::NOTE, muted, room));
        let words = label_lines.size().y + help_lines.as_ref().map_or(0.0, |help| SETTING_HELP_GAP + help.size().y);
        let height = words.max(used.height()).max(least);
        let place = Rect::from_min_size(pos2(origin.x, top + (height - words) / 2.0), vec2(room, words));
        let mut column = ui.new_child(UiBuilder::new().id_salt("setting_words").max_rect(place).layout(Layout::top_down(Align::Min)));
        column.spacing_mut().item_spacing.y = SETTING_HELP_GAP;
        add_marked(&mut column, label_lines, label, query);
        if let Some(help_lines) = help_lines {
            add_marked(&mut column, help_lines, help, query);
        }
        kept.keep(ui, height, "a setting row changed height");
        ui.allocate_rect(Rect::from_min_size(origin, vec2(full, height + SETTING_ROW_PADDING * 2.0)), Sense::hover())
    })
    .inner
}

/// What marks a setting that is picked: a checkbox, or a radio button's dot.
#[derive(Clone, Copy)]
enum Choice {
    Check,
    Radio,
}

impl Choice {
    fn size(self) -> f32 {
        match self {
            Self::Check => CHECK_BOX,
            Self::Radio => RADIO,
        }
    }
}

/// Paints `choice` in `boxed` as egui paints its own checkbox and radio button, lit by `response`'s hover, press and
/// focus, then the hand and the focus ring round it.
fn paint_choice(ui: &Ui, response: &Response, choice: Choice, boxed: Rect, on: bool) {
    let visuals = *ui.style().interact(response);
    let mut painter = ui.painter().clone();
    if !ui.is_enabled() {
        painter.multiply_opacity(DISABLED_OPACITY);
    }
    match choice {
        Choice::Check => {
            let radius = visuals.corner_radius.at_most((0.3 * CHECK_BOX).round() as u8);
            painter.rect(boxed, radius, visuals.bg_fill, visuals.bg_stroke, StrokeKind::Inside);
            if on {
                let tick = Rect::from_center_size(boxed.center(), Vec2::splat(CHECK_BOX * 2.0 / 3.0));
                painter.line(vec![pos2(tick.left(), tick.center().y), pos2(tick.center().x, tick.bottom()), pos2(tick.right(), tick.top())], visuals.fg_stroke);
            }
            interactive_part(ui, response, boxed, radius);
        }
        Choice::Radio => {
            // The ring's hairline sits inside its circle, as a checkbox's border sits inside its box.
            painter.circle(boxed.center(), (RADIO - HAIRLINE) / 2.0, visuals.bg_fill, visuals.bg_stroke);
            if on {
                painter.circle_filled(boxed.center(), RADIO_DOT / 2.0, visuals.fg_stroke.color);
            }
            // The focus ring round a circle is a circle too.
            interactive_part(ui, response, boxed, CornerRadius::same((RADIO / 2.0 + FOCUS_OFFSET).ceil() as u8));
        }
    }
}

/// A setting picked by a mark at the start of its row: `choice`'s mark, then `label` over its `help`. The mark and its
/// words are one control: a click on either takes it, Tab stops on it once with the focus ring round the mark, and a
/// screen reader hears one control named `label`, with `help` as its description. `picked` reads the click and says
/// whether the mark shows on. A hairline sits above. Hits of the search `query` are highlighted.
fn setting_choice(ui: &mut Ui, choice: Choice, label: &str, help: &str, query: &str, picked: impl FnOnce(&mut Response) -> bool) -> Response {
    let enabled = ui.is_enabled();
    let (text, muted) = (setting_ink(ui, Palette::of(ui).text), setting_ink(ui, Palette::of(ui).muted));
    let id = ui.next_auto_id();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        setting_rule(ui);
        let full = ui.available_width();
        let origin = ui.cursor().min;
        let top = origin.y + SETTING_ROW_PADDING;
        let mark = choice.size();
        let lead = mark + SETTING_CHECK_GAP;
        let room = (full - lead).clamp(0.0, DOC_MEASURE);
        let label_lines = wrapped(ui, label, Type::STRONG, text, room);
        let help_lines = (!help.is_empty()).then(|| wrapped(ui, help, Type::NOTE, muted, room));
        let words = label_lines.size().y + help_lines.as_ref().map_or(0.0, |help| SETTING_HELP_GAP + help.size().y);
        let wide = help_lines.as_ref().map_or(label_lines.size().x, |help| label_lines.size().x.max(help.size().x));
        // The mark sits on the label's first line.
        let first_line = label_lines.rows.first().map_or(mark, |row| row.rect().height());
        let boxed = Rect::from_min_size(pos2(origin.x, top + ((first_line - mark) / 2.0).max(0.0)), vec2(mark, mark));
        let place = Rect::from_min_size(pos2(origin.x + lead, top), vec2(wide, words));
        let (salt, role) = match choice {
            Choice::Check => ("setting_check", Role::CheckBox),
            Choice::Radio => ("setting_radio", Role::RadioButton),
        };
        let mut response = ui.interact(boxed.union(place), id.with(salt), Sense::click());
        let on = picked(&mut response);
        response.widget_info(|| WidgetInfo::selected(role, enabled, on, label));
        if !help.is_empty() {
            ui.ctx().accesskit_node_builder(response.id, |node| node.set_description(help));
        }
        paint_choice(ui, &response, choice, boxed, on);
        let label_height = label_lines.size().y;
        paint_marked(ui, place.min, label_lines, label, query);
        if let Some(help_lines) = help_lines {
            paint_marked(ui, pos2(place.left(), place.top() + label_height + SETTING_HELP_GAP), help_lines, help, query);
        }
        let height = words.max(mark).max(SETTING_ROW_MIN - SETTING_ROW_PADDING * 2.0);
        ui.allocate_rect(Rect::from_min_size(origin, vec2(full, height + SETTING_ROW_PADDING * 2.0)), Sense::hover());
        response
    })
    .inner
}

/// A setting that is on or off: a checkbox at `CHECK_BOX` first, painted as egui paints its own, then `label` over its
/// `help`. The box and its words are one control: a click on either flips it, Tab stops on it once with the focus ring
/// round the box, and a screen reader hears one checkbox named `label`, with `help` as its description.
/// A hairline sits above. Hits of the search `query` are highlighted.
pub fn setting_check(ui: &mut Ui, on: &mut bool, label: &str, help: &str, query: &str) -> Response {
    setting_choice(ui, Choice::Check, label, help, query, |response| {
        if response.clicked() {
            *on = !*on;
            response.mark_changed();
        }
        *on
    })
}

/// One of a few settings that exclude each other, such as how tool permissions go: a radio button at `RADIO` first,
/// then `label` over its `help`, laid out as `setting_check` lays out a checkbox. The dot and its words are one control,
/// which a screen reader hears as one radio button named `label`. The caller picks the value when the response is
/// clicked, and the dot shows on from that frame.
pub fn setting_radio(ui: &mut Ui, on: bool, label: &str, help: &str, query: &str) -> Response {
    setting_choice(ui, Choice::Radio, label, help, query, |response| {
        if response.clicked() && !on {
            response.mark_changed();
        }
        on || response.clicked()
    })
}

/// A setting that is on or off with an amount in its words, such as "Stop at [$ 60.00] a month": a checkbox, then
/// `before`, an amount field holding `text`, `after`, and the `help` under them. The box and the words are one control,
/// as in `setting_check`, and the field is a control of its own. A screen reader hears the checkbox named by the whole
/// sentence, and the field named by `before`. A hairline sits above. Hits of the search `query` are highlighted.
/// Returns the checkbox's response, then the field's.
pub fn setting_check_amount(ui: &mut Ui, on: &mut bool, text: &mut String, before: &str, after: &str, help: &str, query: &str) -> (Response, Response) {
    let field = InlineField { unit: Some("$"), size: FIELD_INLINE, limit: None };
    setting_check_field(ui, on, text, [before, after, help], field, query)
}

/// `setting_check_amount` with a whole number in place of an amount, such as "Hide jobs scored under [60]": no unit
/// before it, a narrower field, and at most `digits` characters. A screen reader hears the checkbox named by the whole
/// sentence, the number included, and the field named by `before`. A blank `after` leaves the field last on the line.
#[allow(clippy::too_many_arguments)]
pub fn setting_check_number(
    ui: &mut Ui,
    on: &mut bool,
    text: &mut String,
    before: &str,
    after: &str,
    help: &str,
    digits: usize,
    query: &str,
) -> (Response, Response) {
    let field = InlineField { unit: None, size: FIELD_INLINE_NUMBER, limit: Some(digits) };
    setting_check_field(ui, on, text, [before, after, help], field, query)
}

/// The field set in a checkbox's sentence: the unit before what it holds, its size, and how many characters it takes.
struct InlineField<'a> {
    unit: Option<&'a str>,
    size: (f32, f32),
    limit: Option<usize>,
}

fn setting_check_field(
    ui: &mut Ui,
    on: &mut bool,
    text: &mut String,
    [before, after, help]: [&str; 3],
    inline: InlineField,
    query: &str,
) -> (Response, Response) {
    let enabled = ui.is_enabled();
    let (ink, muted) = (setting_ink(ui, Palette::of(ui).text), setting_ink(ui, Palette::of(ui).muted));
    let id = ui.next_auto_id();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        setting_rule(ui);
        let full = ui.available_width();
        let origin = ui.cursor().min;
        let top = origin.y + SETTING_ROW_PADDING;
        let lead = CHECK_BOX + SETTING_CHECK_GAP;
        let room = (full - lead).clamp(0.0, DOC_MEASURE);
        let (field_width, field_height) = inline.size;
        let before_line = line(ui, before, Type::STRONG, ink, room);
        let after_line = line(ui, after, Type::STRONG, ink, room);
        let help_lines = (!help.is_empty()).then(|| wrapped(ui, help, Type::NOTE, muted, room));
        // The words and the field share one line, centred on it, and the box sits on that line too.
        let first_line = field_height.max(before_line.size().y).max(after_line.size().y);
        let centre = top + first_line / 2.0;
        let left = origin.x + lead;
        let before_at = pos2(left, centre - before_line.size().y / 2.0);
        let field = Rect::from_min_size(pos2(left + before_line.size().x + FIELD_INLINE_MARGIN, centre - field_height / 2.0), vec2(field_width, field_height));
        let after_at = pos2(field.right() + FIELD_INLINE_MARGIN, centre - after_line.size().y / 2.0);
        let words = first_line + help_lines.as_ref().map_or(0.0, |help| SETTING_HELP_GAP + help.size().y);
        let sentence = after_at.x + after_line.size().x - left;
        let wide = help_lines.as_ref().map_or(sentence, |help| sentence.max(help.size().x));
        let boxed = Rect::from_min_size(pos2(origin.x, centre - CHECK_BOX / 2.0), vec2(CHECK_BOX, CHECK_BOX));
        let place = Rect::from_min_size(pos2(left, top), vec2(wide, words));
        // The box and the words before the field are the control a screen reader and Tab find, so its rect stays off
        // the field. The rest of the words take a click too, through a part Tab skips. The field is added after both,
        // so it lies over them and takes its own clicks.
        let named = boxed.union(Rect::from_min_size(pos2(left, top), vec2(before_line.size().x, first_line)));
        let named = ui.interact(named, id.with("setting_check_amount"), Sense::click());
        let rest = ui.interact(place, id.with("setting_check_amount_words"), Sense::CLICK);
        let clicked = named.clicked() || rest.clicked();
        if clicked {
            *on = !*on;
        }
        let checked = *on;
        let amount = text.trim();
        let amount = if amount.is_empty() { String::new() } else { format!("{}{amount}", inline.unit.unwrap_or_default()) };
        let spoken = [before, amount.as_str(), after].into_iter().filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ");
        // Told to the named part alone, so a screen reader's bounds stay off the field.
        named.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, checked, &spoken));
        if !help.is_empty() {
            ui.ctx().accesskit_node_builder(named.id, |node| node.set_description(help));
        }
        let mut check = named.union(rest);
        if clicked {
            check.mark_changed();
        }
        paint_choice(ui, &check, Choice::Check, boxed, checked);
        paint_marked(ui, before_at, before_line, before, query);
        paint_marked(ui, after_at, after_line, after, query);
        if let Some(help_lines) = help_lines {
            paint_marked(ui, pos2(left, top + first_line + SETTING_HELP_GAP), help_lines, help, query);
        }
        let field = field_in(ui, id.with("setting_amount"), field, text, "", inline.unit, inline.limit);
        ui.ctx().accesskit_node_builder(field.id, |node| node.set_label(before));
        let height = words.max(CHECK_BOX).max(SETTING_ROW_MIN - SETTING_ROW_PADDING * 2.0);
        ui.allocate_rect(Rect::from_min_size(origin, vec2(full, height + SETTING_ROW_PADDING * 2.0)), Sense::hover());
        (check, field)
    })
    .inner
}

/// What a `confirm_popover` says.
#[derive(Debug, Clone, Copy)]
pub struct Confirm<'a> {
    /// The question, in strong type, such as "End this session?".
    pub question: &'a str,
    /// One `muted` line on what acting does.
    pub note: &'a str,
    /// The safe primary button, such as "Keep it open".
    pub keep: &'a str,
    /// The danger button that acts, such as "End session".
    pub act: &'a str,
    /// The `faint` line on what Esc does, such as "Esc keeps it open."
    pub esc_note: &'a str,
}

/// Asks before something that cannot be taken back, in a popover under `button`, `width` wide: the question, its note,
/// the safe primary and the danger button that acts, then the Esc note.
/// `state` is `None` while it is closed. The caller opens it with `Some(true)` when `button` is clicked. While it is
/// open, it holds whether the safe button still has to take the focus, which it does once, so Enter keeps too.
/// The safe button, a click outside and Esc close it and keep. Returns true on the frame the danger button is clicked,
/// which closes it too.
pub fn confirm_popover(button: &Response, state: &mut Option<bool>, words: &Confirm, width: f32) -> bool {
    let mut open = state.is_some();
    let mut focus = state.unwrap_or(false);
    let (mut keep, mut act) = (false, false);
    Popup::from_response(button)
        .open_bool(&mut open)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .align(RectAlign::BOTTOM_END)
        .gap(ITEM_GAP)
        .width(width)
        .accessible_name(words.question)
        .show(|ui| {
            ui.set_max_width(width);
            strong(ui, words.question);
            ui.add(egui::Label::new(rich(ui, words.note, Type::BODY, Ink::Muted)).wrap());
            ui.add_space(ITEM_GAP);
            ui.horizontal(|ui| {
                let kept = primary_button(ui, words.keep);
                if focus && !ui.is_sizing_pass() {
                    kept.request_focus();
                    focus = false;
                }
                keep = kept.clicked();
                act = danger_button(ui, words.act).clicked();
            });
            ui.label(rich(ui, words.esc_note, Type::NOTE, Ink::Faint));
        });
    *state = (open && !keep && !act).then_some(focus);
    act
}

/// What a `release_notes_popover` shows. Every word but the notes is the caller's own.
#[derive(Debug, Clone, Copy)]
pub struct ReleaseNotes<'a> {
    /// The title, such as "Jobdesk 0.2.0".
    pub title: &'a str,
    /// One `muted` line under it, such as "Released Fri 30 Oct. You have 0.1.0.".
    pub released: &'a str,
    /// The release's notes: untrusted Markdown from the network, shown through `release_markdown`.
    pub notes: &'a str,
    /// The `faint` line under the notes on where they come from and what they cannot do.
    pub trust: &'a str,
    /// What the primary action does, on a slip above the actions, once the update is ready. `None` before.
    pub slip: Option<&'a str>,
    /// The primary action, such as "Download 0.2.0" or "Restart to update".
    pub primary: &'a str,
    /// Why the primary action waits, said on hover while it is disabled. `None` while it can go.
    pub primary_wait: Option<&'a str>,
    /// The size beside the primary action, such as "14.2 MB".
    pub size: Option<&'a str>,
    /// The link to the release page, such as "Open the release page".
    pub open_page: &'a str,
    /// The `faint` words on Esc: under the actions, or at the end of their row beside a slip.
    pub esc_note: &'a str,
}

/// What the user picked in a `release_notes_popover`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesAction {
    /// The primary action. It closes the popover too.
    Primary,
    /// The release page link. The caller opens the address it builds itself, never one from the notes.
    OpenPage,
}

/// A release's notes in a popover above `anchor`, their right edges level, built as `confirm_popover` is: the title and
/// its line over a hairline, the notes on paper at a fixed height with the scroll bar shown at rest, the trust line, the
/// slip once there is one, then the primary action, the size and the release page link, and the Esc note.
/// The caller flips `open` when `anchor` is clicked. Each time it opens, the primary action takes the focus, so Enter
/// acts, unless it waits: then it is disabled and says why on hover. Esc, a click outside and the primary action close
/// it. Returns what was picked this frame.
pub fn release_notes_popover(anchor: &Response, open: &mut bool, notes: &ReleaseNotes, cache: &mut CommonMarkCache) -> Option<NotesAction> {
    let popup = Popup::from_response(anchor);
    let id = popup.get_id();
    let focus_id = id.with("release_notes_focus");
    // A popover not shown last frame opens now, with the focus to give. Its first frame is a sizing pass, which cannot.
    let opening = anchor.ctx.read_response(id).is_none();
    let mut focus = opening || anchor.ctx.data(|data| data.get_temp::<bool>(focus_id)).unwrap_or(false);
    let mut picked = None;
    let frame = Frame::popup(&anchor.ctx.global_style()).inner_margin(0);
    popup
        .open_bool(open)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .align(RectAlign::TOP_END)
        .gap(ITEM_GAP)
        .width(NOTES_WIDTH)
        .frame(frame)
        .accessible_name(notes.title)
        .show(|ui| {
            let p = Palette::of(ui);
            ui.set_width(NOTES_WIDTH - 2.0 * HAIRLINE);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (top, bottom) = NOTES_HEAD_PADDING;
            Frame::new().inner_margin(Margin { left: NOTES_INLINE, right: NOTES_INLINE, top, bottom }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                add_text(ui, notes.title, Type::CALL_TITLE, Ink::Text);
                ui.add_space(SETTING_HELP_GAP + 1.0);
                ui.add(egui::Label::new(rich(ui, notes.released, Type::NOTE, Ink::Muted)).wrap());
            });
            divider(ui);
            Frame::new().inner_margin(Margin { left: NOTES_INLINE, right: NOTES_INLINE, top: NOTES_GAP as i8, bottom: 0 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                Frame::new().fill(p.card).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_PANEL).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let height = NOTES_HEIGHT - 2.0 * HAIRLINE;
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("release_notes"))
                        .auto_shrink([false, false])
                        .min_scrolled_height(height)
                        .max_height(height)
                        .show(ui, |ui| {
                            let (left, top, right, bottom) = NOTES_BODY_PADDING;
                            Frame::new().inner_margin(Margin { left, right, top, bottom }).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = ITEM_GAP;
                                release_markdown(ui, cache, notes.notes);
                            });
                        });
                });
                ui.add_space(ROW_GAP);
                ui.add(egui::Label::new(rich(ui, notes.trust, Type::SMALL, Ink::Faint)).wrap());
                if let Some(slip) = notes.slip {
                    ui.add_space(NOTES_GAP);
                    Frame::new().fill(p.panel).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_PANEL).inner_margin(margin(NOTES_SLIP_PADDING)).show(
                        ui,
                        |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(rich(ui, slip, Type::NOTE, Ink::Text)).wrap());
                        },
                    );
                }
            });
            let bottom = if notes.slip.is_some() { NOTES_HEAD_PADDING.0 } else { NOTES_GAP as i8 };
            Frame::new().inner_margin(Margin { left: NOTES_INLINE, right: NOTES_INLINE, top: NOTES_GAP as i8, bottom }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = STATUS_PROGRESS_GAP;
                    let wait = notes.primary_wait;
                    let primary = enabled_when(ui, wait.is_none(), wait.unwrap_or_default(), |ui| primary_button(ui, notes.primary));
                    if focus && !ui.is_sizing_pass() {
                        if wait.is_none() {
                            primary.request_focus();
                        }
                        focus = false;
                    }
                    if primary.clicked() {
                        picked = Some(NotesAction::Primary);
                    }
                    if notes.slip.is_some() {
                        if link(ui, notes.open_page).clicked() {
                            picked = Some(NotesAction::OpenPage);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| add_text(ui, notes.esc_note, Type::SMALL, Ink::Faint));
                    } else {
                        if let Some(size) = notes.size {
                            add_text(ui, size, Type::NOTE, Ink::Muted);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if link(ui, notes.open_page).clicked() {
                                picked = Some(NotesAction::OpenPage);
                            }
                        });
                    }
                });
                if notes.slip.is_none() {
                    ui.add_space(ROW_GAP);
                    ui.add(egui::Label::new(rich(ui, notes.esc_note, Type::SMALL, Ink::Faint)).wrap());
                }
            });
        });
    if picked == Some(NotesAction::Primary) {
        *open = false;
    }
    anchor.ctx.data_mut(|data| {
        if *open {
            data.insert_temp(focus_id, focus);
        } else {
            data.remove::<bool>(focus_id);
        }
    });
    picked
}

/// How wide a column of a `pick_table` is: fixed, or a share of the room the fixed ones leave, never under `least`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Exact(f32),
    Rest { least: f32 },
}

/// A column of a `pick_table`: its head and its width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableColumn<'a> {
    pub head: &'a str,
    pub width: ColumnWidth,
}

/// What a `pick_table` lays out: its columns, the height of a row, how many rows and which one is picked.
pub struct PickTable<'a> {
    pub id_salt: &'a str,
    pub columns: &'a [TableColumn<'a>],
    pub row_height: f32,
    pub rows: usize,
    pub picked: Option<usize>,
}

/// A table whose every row picks something, such as a job. A row is one target under its cells: one Tab stop, named
/// `spoken(row)` for screen readers, lit as every pickable thing is and ringed inside when it has focus. Its text takes
/// no click, so a click on it picks the row, and a cut title or a mark beneath the pointer still shows its tooltip.
/// Up and Down move the pick while a row or nothing has the focus. `cell(ui, row, column)` fills one cell.
/// The head stays put while the rows scroll. Returns the row picked this frame.
pub fn pick_table(ui: &mut Ui, table: &PickTable, spoken: impl Fn(usize) -> String, mut cell: impl FnMut(&mut Ui, usize, usize)) -> Option<usize> {
    let p = Palette::of(ui);
    let id = ui.make_persistent_id(table.id_salt);
    let row_id = |row: usize| id.with(("pick_table_row", row));
    let widths = column_widths(table.columns, ui.available_width());
    let width: f32 = widths.iter().sum();
    let is_row = |focused: egui::Id| (0..table.rows).any(|row| row_id(row) == focused);
    let moved = arrow_pick(ui, table, is_row);
    if let Some(row) = moved
        && ui.memory(|mem| mem.focused()).is_some_and(is_row)
    {
        ui.memory_mut(|mem| mem.request_focus(row_id(row)));
    }
    let shown = moved.or(table.picked);
    let mut clicked = None;
    table_heads(ui, table.columns, &widths);
    egui::ScrollArea::vertical().id_salt(id.with("rows")).auto_shrink([false, true]).show_rows(ui, table.row_height, table.rows, |ui, range| {
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        let top = ui.max_rect().top() - range.start as f32 * table.row_height;
        if let Some(row) = moved {
            ui.scroll_to_rect(Rect::from_min_size(pos2(ui.max_rect().left(), top + row as f32 * table.row_height), vec2(width, table.row_height)), None);
        }
        for row in range {
            let rect = Rect::from_min_size(ui.cursor().min, vec2(width, table.row_height));
            let rule = if row + 1 == table.rows { CellRule::Last } else { CellRule::Row };
            let response = ui
                .scope_builder(clickable().scope_id(row_id(row)).max_rect(rect), |ui| {
                    let hit = ui.response();
                    let look = pick_look(&p, shown == Some(row), pointer_over(&hit), pressed(&hit));
                    ui.painter().rect_filled(rect, 0.0, look.fill);
                    for (column, cell_rect) in column_rects(rect, &widths).enumerate() {
                        table_cell_in(ui, cell_rect, rule, |ui| cell(ui, row, column));
                    }
                    if let Some(edge) = look.edge {
                        paint_pick_edge(ui, rect, 0, edge);
                    }
                    ui.expand_to_include_rect(rect);
                })
                .response;
            interactive_row(ui, &response, 0);
            if response.has_focus() {
                // Up and Down move the pick, and the focus with it, so egui must not move the focus on its own.
                ui.memory_mut(|mem| mem.set_focus_lock_filter(response.id, EventFilter { vertical_arrows: true, ..EventFilter::default() }));
            }
            response.widget_info(|| WidgetInfo::selected(Role::Button, ui.is_enabled(), shown == Some(row), spoken(row)));
            if response.clicked() {
                clicked = Some(row);
            }
        }
    });
    clicked.or(moved)
}

/// The shape of a `pick_table` before its rows are read: the same head and column widths over `rows` grey rows, each
/// cell holding the lines `lines(column)` gives, as wide as it says. Nothing in it takes a click.
pub fn skeleton_table<'a>(ui: &mut Ui, columns: &[TableColumn], row_height: f32, rows: usize, lines: impl Fn(usize) -> &'a [f32]) {
    let widths = column_widths(columns, ui.available_width());
    let width: f32 = widths.iter().sum();
    table_heads(ui, columns, &widths);
    for row in 0..rows {
        let rect = Rect::from_min_size(ui.cursor().min, vec2(width, row_height));
        let rule = if row + 1 == rows { CellRule::Last } else { CellRule::Row };
        for (column, cell_rect) in column_rects(rect, &widths).enumerate() {
            let widths = lines(column);
            let height = widths.len() as f32 * (SKELETON_LINE + SKELETON_GAP) - SKELETON_GAP;
            // A box as tall as the lines, so the row centres them as it centres text.
            table_cell_in(ui, cell_rect, rule, |ui| {
                ui.allocate_ui_with_layout(vec2(ui.available_width(), height.max(0.0)), Layout::top_down(Align::Min), |ui| {
                    skeleton_lines(ui, widths, skeleton)
                });
            });
        }
        ui.advance_cursor_after_rect(rect);
    }
}

/// A table's head row, each head over its column. From here on cells pad themselves, so rules and a picked row run unbroken.
fn table_heads(ui: &mut Ui, columns: &[TableColumn], widths: &[f32]) {
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let head = Rect::from_min_size(ui.cursor().min, vec2(widths.iter().sum(), TABLE_HEAD_HEIGHT));
    for (column, rect) in column_rects(head, widths).enumerate() {
        table_cell_in(ui, rect, CellRule::Head, |ui| {
            table_head(ui, columns[column].head);
        });
    }
    ui.advance_cursor_after_rect(head);
}

/// The row Up or Down picks this frame, if either was pressed and the pick moves. The arrows are the table's only while
/// one of its rows or nothing has the focus, so a field or a button keeps them, and never while a dialog asks.
fn arrow_pick(ui: &mut Ui, table: &PickTable, is_row: impl Fn(egui::Id) -> bool) -> Option<usize> {
    let focused = ui.memory(|mem| mem.focused());
    if table.rows == 0 || focused.is_some_and(|focused| !is_row(focused)) || dialog_open(ui.ctx()) {
        return None;
    }
    let (up, down) = ui.input_mut(|input| (input.consume_key(Modifiers::NONE, Key::ArrowUp), input.consume_key(Modifiers::NONE, Key::ArrowDown)));
    let row = match (table.picked, up, down) {
        (Some(row), true, false) => row.saturating_sub(1),
        (Some(row), false, true) => (row + 1).min(table.rows - 1),
        (None, true, _) | (None, _, true) => 0,
        _ => return None,
    };
    (Some(row) != table.picked).then_some(row)
}

/// Each column's width: fixed ones as given, and the others sharing what is left of `room`.
fn column_widths(columns: &[TableColumn], room: f32) -> Vec<f32> {
    let fixed: f32 = columns.iter().filter_map(|column| if let ColumnWidth::Exact(width) = column.width { Some(width) } else { None }).sum();
    let rest = columns.iter().filter(|column| matches!(column.width, ColumnWidth::Rest { .. })).count().max(1);
    let share = (room - fixed) / rest as f32;
    columns
        .iter()
        .map(|column| match column.width {
            ColumnWidth::Exact(width) => width,
            ColumnWidth::Rest { least } => share.max(least),
        })
        .collect()
}

fn column_rects(row: Rect, widths: &[f32]) -> impl Iterator<Item = Rect> + '_ {
    widths.iter().scan(row.left(), move |left, width| {
        let rect = Rect::from_min_size(pos2(*left, row.top()), vec2(*width, row.height()));
        *left += width;
        Some(rect)
    })
}

/// A `table_cell` at `rect`, clipped to it, so a long value never runs into the next column.
fn table_cell_in(ui: &mut Ui, rect: Rect, rule: CellRule, add: impl FnOnce(&mut Ui)) {
    let mut cell = ui.new_child(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)));
    cell.shrink_clip_rect(rect);
    table_cell(&mut cell, rule, add);
}

/// A filled line across a screen, for a short message and its actions.
pub fn note<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(margin(NOTE_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A panel inside paper, such as a note at the top of a Settings page or the detail beside a list: the panel fill in a
/// `line` hairline, which keeps it apart in dark, where panel and paper are one colour. `padding` is the left and right,
/// the top and the bottom room inside it.
pub fn ruled_panel<R>(ui: &mut Ui, padding: (i8, i8, i8), add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let (x, top, bottom) = padding;
    Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(HAIRLINE, p.line))
        .corner_radius(RADIUS_PANEL)
        .inner_margin(Margin { left: x, right: x, top, bottom })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The height of a part laid out after what must leave room for it, such as a footer under a scroll area,
/// as it measured on the frame before.
#[derive(Debug, Clone, Copy)]
pub struct KeptHeight {
    id: egui::Id,
    pub height: f32,
}

impl KeptHeight {
    /// The height kept under `id`, or `fallback` before the first measure.
    pub fn read(ui: &Ui, id: egui::Id, fallback: f32) -> Self {
        Self { id, height: ui.data(|data| data.get_temp::<f32>(id)).unwrap_or(fallback) }
    }

    /// Keeps `laid` when it differs from the height read. A discard redraws within this frame when a pass is left.
    /// The repaint covers the frames where none is, so an idle window still settles on the new measure.
    pub fn keep(self, ui: &Ui, laid: f32, reason: &'static str) {
        if (laid - self.height).abs() > 0.5 {
            ui.data_mut(|data| data.insert_temp(self.id, laid));
            ui.ctx().request_discard(reason);
            ui.ctx().request_repaint();
        }
    }
}

/// Commands and untrusted text: plain, in a recessed box.
pub fn code_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    code_frame(ui, Stroke::NONE, add)
}

/// `code_box` in a `line` hairline, for text quoted inside a box of its own, such as what Claude Code said in a failed
/// test, so the quote still stands apart from the box around it.
pub fn ruled_code_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let line = Palette::of(ui).line;
    code_frame(ui, Stroke::new(HAIRLINE, line), add)
}

fn code_frame<R>(ui: &mut Ui, stroke: Stroke, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).field;
    Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(RADIUS_CARD)
        .inner_margin(margin(CODE_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            copyable(ui, add)
        })
        .inner
}

/// A hard offset shadow in the status colour: this needs reading before anything else.
pub(in crate::ui_kit) fn offset_shadow(_ui: &Ui, color: Color32) -> Shadow {
    Shadow { offset: [OFFSET_SHADOW, OFFSET_SHADOW], blur: 0, spread: 0, color }
}

/// A run's question or permission request, set apart from the sheet.
pub fn prompt_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    prompt_box_tone(ui, Tone::Interview, add)
}

/// A prompt box framed in the colour of what it asks for, such as `Closed` for a source that could not be read.
pub fn prompt_box_tone<R>(ui: &mut Ui, tone: Tone, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let color = tone.color(&p);
    Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(HAIRLINE, color))
        .shadow(offset_shadow(ui, color))
        .corner_radius(RADIUS_CARD)
        .inner_margin(margin(PROMPT_PADDING))
        .outer_margin(Margin { right: OFFSET_SHADOW, bottom: OFFSET_SHADOW, ..Default::default() })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A sheet of paper on the desk: the card fill in a hairline. `shadow` lifts it with a hard offset in the line colour.
/// It has no padding, so a body and a footer can each set their own.
pub fn paper<R>(ui: &mut Ui, shadow: bool, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    let frame = Frame::new().fill(p.card).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_PANEL);
    let frame = if shadow {
        frame.shadow(Shadow { offset: [WINDOW_SHADOW, WINDOW_SHADOW], blur: 0, spread: 0, color: p.line }).outer_margin(Margin {
            right: WINDOW_SHADOW,
            bottom: WINDOW_SHADOW,
            ..Default::default()
        })
    } else {
        frame
    };
    frame
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui)
        })
        .inner
}

/// A sheet of paper whose body scrolls over its actions, pinned under a hairline. It takes only the height its body needs,
/// up to what is left in `ui`, so a long body gives way first and the actions stay in view.
pub fn paper_with_footer(ui: &mut Ui, body: impl FnOnce(&mut Ui), footer: impl FnOnce(&mut Ui)) {
    paper(ui, false, |ui| {
        footed(ui, false, false, body, |ui| {
            let padding = (inset(ui), BODY_PADDING);
            footer_row(ui, padding, footer);
        })
    });
}

/// A sheet of paper that takes all the height it has: `head`, padded as a deal is, a hairline, then `body`, which scrolls
/// with its bar shown at rest, over `foot`, a one-line strip pinned at the bottom under a hairline. `foot` lays out left to
/// right with no gap of its own, so its words carry their own spaces.
pub fn headed_paper<R>(ui: &mut Ui, head: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R, foot: impl FnOnce(&mut Ui)) -> R {
    paper(ui, false, |ui| {
        let (x, y) = DEAL_PADDING;
        Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            head(ui);
        });
        divider(ui);
        footed(
            ui,
            true,
            false,
            |ui| {
                Frame::new()
                    .inner_margin(Margin { left: x, right: x, top: 0, bottom: y })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        body(ui)
                    })
                    .inner
            },
            |ui| {
                divider(ui);
                Frame::new().inner_margin(margin(DEAL_FOOT_PADDING)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        foot(ui);
                    });
                });
            },
        )
    })
}

/// What `add` lays out, faded to `FADED_OPACITY`, such as a template's outline shown for what it holds.
pub fn faded<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope(|ui| {
        ui.multiply_opacity(FADED_OPACITY);
        add(ui)
    })
    .inner
}

/// A scrolling body over a footer pinned at its foot, inside a frame's content `ui`. `fill` takes all the height left,
/// where otherwise the two take what the body needs, up to it. `fresh` opens the body at the top.
pub(in crate::ui_kit) fn footed<R>(ui: &mut Ui, fill: bool, fresh: bool, body: impl FnOnce(&mut Ui) -> R, footer: impl FnOnce(&mut Ui)) -> R {
    let room = ui.available_size();
    let kept = KeptHeight::read(ui, ui.scope_id().with("footed_footer"), 0.0);
    let needed = KeptHeight::read(ui, ui.scope_id().with("footed_body"), room.y);
    let height = if fill { room.y } else { (needed.height + kept.height).min(room.y) };
    let inner = Rect::from_min_size(ui.cursor().min, vec2(room.x, height));
    ui.advance_cursor_after_rect(inner);
    // The footer goes in first, at the foot, so the body takes what it leaves.
    // One height is kept for whatever the body shows, so only a footer of a new height costs a pass.
    let foot_rect = Rect::from_min_max(pos2(inner.left(), (inner.bottom() - kept.height).max(inner.top())), inner.max);
    let mut foot = ui.new_child(UiBuilder::new().id_salt("footed_footer").max_rect(foot_rect).layout(Layout::top_down(Align::Min)));
    foot.shrink_clip_rect(foot_rect);
    footer(&mut foot);
    kept.keep(ui, foot.min_rect().height().min(room.y), "a footer changed height");
    let body_rect = Rect::from_min_max(inner.min, pos2(inner.right(), foot_rect.top()));
    let mut body_ui = ui.new_child(UiBuilder::new().id_salt("footed_body").max_rect(body_rect).layout(Layout::top_down(Align::Min)));
    body_ui.shrink_clip_rect(body_rect);
    let scroll = egui::ScrollArea::vertical().id_salt("footed_body").auto_shrink([false, false]);
    let scroll = if fresh { scroll.vertical_scroll_offset(0.0) } else { scroll };
    let shown = scroll.show(&mut body_ui, body);
    if !fill {
        needed.keep(ui, shown.content_size.y, "a body changed height");
    }
    shown.inner
}

/// The row under a body that holds its actions, ruled off on top and padded `(x, y)`.
pub(in crate::ui_kit) fn footer_row(ui: &mut Ui, (x, y): (i8, i8), add: impl FnOnce(&mut Ui)) {
    divider(ui);
    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(BUTTON_GAP, BUTTON_GAP);
            add(ui);
        });
    });
}

/// What a screen shows when it has nothing yet: a title, a sentence on what to do, then the actions, all on paper.
/// The actions only open the launcher: the user starts the run there.
pub fn empty_state(ui: &mut Ui, title: &str, body: &str, actions: impl FnOnce(&mut Ui)) {
    empty_sheet(ui, title, body, Some(actions), None::<fn(&mut Ui)>);
}

/// `empty_state` with "Show the command" under its actions, which shows `line` as `reveal_command` does. The caller holds `open`.
pub fn empty_state_with_command(ui: &mut Ui, title: &str, body: &str, actions: impl FnOnce(&mut Ui), line: &str, open: &mut bool) {
    empty_sheet(
        ui,
        title,
        body,
        Some(actions),
        Some(|ui: &mut Ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = ITEM_GAP;
                reveal_command(ui, line, open);
            });
        }),
    );
}

/// The same with no actions, such as a lane that is empty with nothing to offer. A blank `body` leaves the sentence out.
pub fn empty_note(ui: &mut Ui, title: &str, body: &str) {
    empty_sheet(ui, title, body, None::<fn(&mut Ui)>, None::<fn(&mut Ui)>);
}

fn empty_sheet(ui: &mut Ui, title: &str, body: &str, actions: Option<impl FnOnce(&mut Ui)>, below: Option<impl FnOnce(&mut Ui)>) {
    paper(ui, false, |ui| {
        Frame::new().inner_margin(Margin::same(EMPTY_PADDING)).show(ui, |ui| {
            ui.set_max_width(EMPTY_WIDTH);
            ui.spacing_mut().item_spacing.y = EMPTY_GAP;
            add_text(ui, title, Type::SLIP_TITLE, Ink::Text);
            if !body.is_empty() {
                add_text(ui, body, Type::BODY, Ink::Muted);
            }
            if let Some(actions) = actions {
                ui.horizontal(actions);
            }
            if let Some(below) = below {
                below(ui);
            }
        });
    });
}

/// A grey block where content will be, while it loads. In a slip or a paper section it takes the raised plane.
pub fn skeleton(ui: &mut Ui, width: f32, height: f32) {
    let p = Palette::of(ui);
    // Only a section's content sits `INSET` in, so that inset says the block is on a filled surface.
    let on_slip = ui.data(|data| data.get_temp::<i8>(inset_id())) == Some(INSET);
    block(ui, width, height, if on_slip { p.raised } else { p.panel });
}

/// The same block on a card. Where the card already has the panel's fill, as in dark, it takes the raised plane.
pub fn card_skeleton(ui: &mut Ui, width: f32, height: f32) {
    let p = Palette::of(ui);
    block(ui, width, height, if p.card == p.panel { p.raised } else { p.panel });
}

/// Grey lines in a column, `SKELETON_GAP` apart, drawn with `block`: `skeleton`, or `card_skeleton` on a card.
pub fn skeleton_lines(ui: &mut Ui, widths: &[f32], block: fn(&mut Ui, f32, f32)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = SKELETON_GAP;
        for &width in widths {
            block(ui, width, SKELETON_LINE);
        }
    });
}

fn block(ui: &mut Ui, width: f32, height: f32, fill: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width.min(ui.available_width()), height), Sense::hover());
    ui.painter().rect_filled(rect, RADIUS_CARD, fill);
}

/// A screen's head led by a stamp: the stamp, then the lead title over a muted subtitle, centred beside it.
pub fn stamped_head(ui: &mut Ui, stamp: impl FnOnce(&mut Ui), title: &str, subtitle: &str) {
    let (x, top, bottom) = HEAD_PADDING;
    Frame::new().inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = HEAD_GAP.0;
            stamp(ui);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
                add_cut_text(ui, title, Type::LEAD, Ink::Text);
                add_cut_text(ui, subtitle, Type::TAKE, Ink::Muted);
            });
        });
    });
}

/// The hairline around a rendered document page.
pub fn page_frame<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let border = Palette::of(ui).line;
    Frame::new().stroke(Stroke::new(HAIRLINE, border)).corner_radius(1).show(ui, add).inner
}

/// A folder of index cards on a board.
pub fn lane<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let (x, top, bottom) = LANE_PADDING;
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(Margin { left: x, right: x, top, bottom })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = LANE_GAP;
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// How an index card looks: picked, waiting on the user in Today, or decided and set back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CardLook {
    pub on: bool,
    pub waits: bool,
    pub faded: bool,
}

/// One application on a board. Clicking anywhere on it counts. `spoken` names the card for screen readers.
/// Picked, its border takes ink, so its left edge stays free for the `turn` mark.
pub fn index_card(ui: &mut Ui, look: CardLook, spoken: &str, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let response = ui
        .scope_builder(clickable(), |ui| {
            if look.faded {
                ui.multiply_opacity(FADED_OPACITY);
            }
            let card = ui.response();
            let pick = pick_look(&p, look.on, pointer_over(&card), pressed(&card));
            let fill = if pick.fill == Color32::TRANSPARENT { p.card } else { pick.fill };
            let frame = Frame::new()
                .fill(fill)
                .stroke(Stroke::new(HAIRLINE, pick.edge.unwrap_or(p.line)))
                .corner_radius(RADIUS_CARD)
                .inner_margin(margin(CARD_PADDING))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    add(ui);
                });
            if look.waits {
                let rect = frame.response.rect;
                let edge = Rect::from_min_max(rect.min, pos2(rect.left() + WAITS_EDGE, rect.bottom()));
                let corners = CornerRadius { nw: RADIUS_CARD, sw: RADIUS_CARD, ne: 0, se: 0 };
                ui.painter().rect_filled(edge, corners, p.turn);
            }
        })
        .response;
    interactive_row(ui, &response, RADIUS_CARD);
    let spoken = if look.waits { format!("{spoken}, waits for you in Today") } else { spoken.to_owned() };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &spoken));
    response
}

/// The shape of an index card while its board loads: the card's face, with nothing to pick.
pub fn blank_card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    Frame::new().fill(p.card).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_CARD).inner_margin(margin(CARD_PADDING)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        add(ui);
    });
}

pub(in crate::ui_kit) fn paint_outlined(ui: &Ui, rect: Rect, radius: CornerRadius) {
    let p = Palette::of(ui);
    ui.painter().rect(rect, radius, p.raised, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
}

/// One stage in a band of counts.
/// A stage's marks: runs of `(count, ink)`, spoken as the lit ones of all, or a fill, spoken as the filled ones of all.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeMarks {
    Runs(Vec<(usize, Option<Ink>)>),
    Fill(FillMarks),
}

pub struct FlowNode<'a> {
    pub label: &'a str,
    pub figure: String,
    pub marks: NodeMarks,
    pub note: String,
    /// Why the stage's counts are hidden. It shows in place of the figure, tally and note, so no zero reads as a real count.
    pub hidden: Option<&'a str>,
}

/// A folder strip of counts, read left to right. Columns are equal, with a hairline between them.
pub fn flow_band(ui: &mut Ui, nodes: &[FlowNode]) {
    if nodes.is_empty() {
        return;
    }
    let p = Palette::of(ui);
    let (x, top, bottom) = NODE_PADDING;
    let padding = Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 };
    let band = Frame::new().fill(p.panel).corner_radius(RADIUS_PANEL).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.columns(nodes.len(), |columns| {
            for (column, node) in columns.iter_mut().zip(nodes) {
                Frame::new().inner_margin(padding).show(column, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = NODE_GAP;
                    add_text(ui, node.label, Type::TAB, Ink::Muted);
                    if let Some(hidden) = node.hidden {
                        hidden_note(ui, hidden);
                        return;
                    }
                    add_text(ui, node.figure.as_str(), Type::FIGURE, Ink::Text);
                    ui.add_space(NODE_TALLY_MARGIN.0);
                    let tally = match &node.marks {
                        NodeMarks::Runs(runs) => crate::ui_kit::tally_strip(ui, runs, false),
                        NodeMarks::Fill(fill) => crate::ui_kit::fill_tally(ui, *fill, false),
                    };
                    ui.add_space((NODE_TALLY_HEIGHT - tally.rect.height()).max(0.0) + NODE_TALLY_MARGIN.1);
                    add_text(ui, node.note.as_str(), Type::BODY, Ink::Muted);
                });
            }
        });
    });
    let rect = band.response.rect;
    let pitch = rect.width() / nodes.len() as f32;
    for index in 1..nodes.len() {
        let at = rect.left() + pitch * index as f32;
        ui.painter().rect_filled(Rect::from_min_max(pos2(at, rect.top()), pos2(at + HAIRLINE, rect.bottom())), 0.0, p.line);
    }
    // The arrow sits on the line between two nodes, on a patch of the band's fill.
    for index in 1..nodes.len() {
        let centre = pos2(rect.left() + pitch * index as f32, rect.top() + NODE_ARROW.0 + NODE_ARROW.1);
        let patch = Rect::from_center_size(centre, Vec2::splat(NODE_ARROW.1 * 2.0));
        ui.painter().rect_filled(patch, 0.0, p.panel);
        let arrow = line(ui, "\u{2192}", Type::BODY, p.faint, f32::INFINITY);
        paint_centred(ui, patch, arrow, p.faint, p.panel);
    }
}

/// One cell of a figure band: a label, a figure, then a note. `bar` puts a `slip_bar` under the figure, filled to its share
/// in its ink, such as spend against a cap.
pub struct Figure<'a> {
    pub label: &'a str,
    pub figure: String,
    pub note: String,
    pub bar: Option<(f32, Ink)>,
}

/// Figures read side by side, such as what a period cost: `flow_band` without arrows or tallies. The cells are equal,
/// on the folder plane, with a hairline around and between them, because on paper in dark the plane alone would not show.
pub fn figure_band(ui: &mut Ui, figures: &[Figure]) {
    figure_cells(ui, figures.len(), |ui, index| {
        let figure = &figures[index];
        add_text(ui, figure.label, Type::TAB, Ink::Muted);
        add_text(ui, figure.figure.as_str(), Type::FIGURE, Ink::Text);
        if let Some((fraction, ink)) = figure.bar {
            ui.add_space(FIGURE_BAR_MARGIN.0);
            crate::ui_kit::slip_bar(ui, fraction, ink);
            ui.add_space(FIGURE_BAR_MARGIN.1);
        }
        add_text(ui, figure.note.as_str(), Type::NOTE, Ink::Muted);
    });
}

/// The same band of `cells` while its figures load: grey blocks for the label, the figure and the note.
pub fn figure_band_loading(ui: &mut Ui, cells: usize) {
    let fill = Palette::of(ui).raised;
    let (label, figure, height) = FIGURE_SKELETON;
    figure_cells(ui, cells, |ui, index| {
        block(ui, label, SKELETON_LINE, fill);
        ui.add_space(FIGURE_SKELETON_MARGIN.0);
        block(ui, figure, height, fill);
        ui.add_space(FIGURE_SKELETON_MARGIN.1);
        block(ui, FIGURE_SKELETON_NOTES[index % FIGURE_SKELETON_NOTES.len()], SKELETON_LINE, fill);
    });
}

/// A figure band's frame around `count` equal cells, each laid out by `cell` with its index.
fn figure_cells(ui: &mut Ui, count: usize, mut cell: impl FnMut(&mut Ui, usize)) {
    if count == 0 {
        return;
    }
    let p = Palette::of(ui);
    let (x, top, bottom) = NODE_PADDING;
    let padding = Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 };
    let band = Frame::new().fill(p.panel).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_PANEL).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.columns(count, |columns| {
            for (index, column) in columns.iter_mut().enumerate() {
                Frame::new().inner_margin(padding).show(column, |ui| {
                    ui.set_width(ui.available_width());
                    // A column is justified, which would spread a wrapped note's words across the cell.
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        ui.spacing_mut().item_spacing.y = NODE_GAP;
                        cell(ui, index);
                    });
                });
            }
        });
    });
    let rect = band.response.rect.shrink(HAIRLINE);
    let pitch = rect.width() / count as f32;
    for index in 1..count {
        let at = rect.left() + pitch * index as f32;
        ui.painter().rect_filled(Rect::from_min_max(pos2(at, rect.top()), pos2(at + HAIRLINE, rect.bottom())), 0.0, p.line);
    }
}

/// How a band cell reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellLook {
    Plain,
    /// The value in a status colour, such as Claude's turn in `Active`.
    Ink(Tone),
    /// Filled in `turn`: the move is the user's.
    Yours,
}

/// One cell of a turn band: a label, a value and a note. `marks` is a tally beside the label, in runs of `(count, ink)`.
pub struct BandCell<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub note: &'a str,
    pub look: CellLook,
    pub marks: &'a [(usize, Option<Ink>)],
}

/// How far the first line's baseline sits below the galley's top.
fn baseline(galley: &Galley) -> f32 {
    galley.rows.first().map_or(galley.size().y, |row| row.pos.y + row.glyphs.first().map_or(row.size.y, |glyph| glyph.pos.y))
}

/// Paints `galley` at `at`, over the `under` fill, and names it for screen readers and tests.
fn painted(ui: &Ui, id: egui::Id, at: Pos2, galley: Arc<Galley>, under: Color32, text: &str) {
    let rect = Rect::from_min_size(at, galley.size());
    let response = cut_hover(ui.interact(rect, id, Sense::hover()), &galley, text);
    paint_text(ui, at, galley, Palette::of(ui).text, under);
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
}

/// Where a run is, whose turn it is and what comes next: three cells on the folder plane, with a hairline between them.
/// Labels, values and notes line up across the cells. When `compact`, each cell is one line, label then value, with no note.
/// `group_label` names the band and `tally_label` its tally for screen readers.
pub fn turn_band(ui: &mut Ui, cells: &[BandCell; 3], compact: bool, group_label: &str, tally_label: &str) -> Response {
    let p = Palette::of(ui);
    let width = ui.available_width();
    let parts: f32 = BAND_COLUMNS.iter().sum();
    let (x, top, bottom) = if compact { BAND_COMPACT_PADDING } else { BAND_PADDING };
    let value_style = if compact { Type::BAND_LINE } else { Type::SCORE };
    struct Laid {
        left: f32,
        width: f32,
        fill: Option<Color32>,
        label: Arc<Galley>,
        value: Arc<Galley>,
        note: Option<Arc<Galley>>,
        marks: Vec<Color32>,
    }
    let mut left = 0.0;
    let laid: Vec<Laid> = cells
        .iter()
        .zip(BAND_COLUMNS)
        .map(|(cell, part)| {
            let cell_width = width * part / parts;
            let room = (cell_width - x * 2.0).max(0.0);
            let (ink, value_ink, fill) = match cell.look {
                CellLook::Plain => (p.muted, p.text, None),
                CellLook::Ink(tone) => (p.muted, tone.color(&p), None),
                CellLook::Yours => (p.on_turn, p.on_turn, Some(p.turn)),
            };
            let marks = crate::ui_kit::marks::strip_colors(&p, cell.marks);
            let label = line(ui, cell.label, Type::NOTE, ink, room);
            let tally = if marks.is_empty() { 0.0 } else { marks.len() as f32 * (TALLY_BAND.width + TALLY_BAND.gap) - TALLY_BAND.gap + BAND_GAP };
            let value_room = if compact { (room - label.size().x - BAND_GAP - tally).max(0.0) } else { room };
            let value = line(ui, cell.value, value_style, value_ink, value_room);
            let note = (!compact && !cell.note.is_empty()).then(|| wrapped(ui, cell.note, Type::BODY, ink, room));
            let laid = Laid { left, width: cell_width, fill, label, value, note, marks };
            left += cell_width;
            laid
        })
        .collect();
    let content = laid
        .iter()
        .map(|cell| {
            if compact {
                cell.value.size().y.max(cell.label.size().y)
            } else {
                BAND_LABEL_HEIGHT + BAND_ROW_GAP + cell.value.size().y + cell.note.as_ref().map_or(0.0, |note| BAND_ROW_GAP + note.size().y)
            }
        })
        .fold(0.0, f32::max);
    let (rect, response) = ui.allocate_exact_size(vec2(width, top + content + bottom), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Group, true, group_label));
    ui.painter().rect_filled(rect, RADIUS_PANEL, p.panel);
    for (index, cell) in laid.into_iter().enumerate() {
        let cell_rect = Rect::from_min_size(pos2(rect.left() + cell.left, rect.top()), vec2(cell.width, rect.height()));
        if let Some(fill) = cell.fill {
            let first = if index == 0 { RADIUS_PANEL } else { 0 };
            let last = if index + 1 == cells.len() { RADIUS_PANEL } else { 0 };
            ui.painter().rect_filled(cell_rect, CornerRadius { nw: first, sw: first, ne: last, se: last }, fill);
        }
        let id = response.id.with(index);
        let cell_text = &cells[index];
        let under = cell.fill.unwrap_or(p.panel);
        let at = pos2(cell_rect.left() + x, rect.top() + top);
        // The label's line: centred in its row, or on the value's baseline when compact.
        let label_top = if compact { at.y + baseline(&cell.value) - baseline(&cell.label) } else { at.y + (BAND_LABEL_HEIGHT - cell.label.size().y) / 2.0 };
        let label_width = cell.label.size().x;
        let label_centre = label_top + cell.label.size().y / 2.0;
        painted(ui, id.with("label"), pos2(at.x, label_top), cell.label, under, cell_text.label);
        let mut after = at.x + label_width + BAND_GAP;
        if !cell.marks.is_empty() {
            let tally = crate::ui_kit::marks::paint_mark_row(ui, pos2(after, label_centre - TALLY_BAND.height / 2.0), &cell.marks, TALLY_BAND);
            ui.interact(tally, id.with("tally"), Sense::hover()).widget_info(|| WidgetInfo::labeled(Role::Label, true, tally_label));
            after = tally.right() + BAND_GAP;
        }
        if compact {
            painted(ui, id.with("value"), pos2(after, at.y), cell.value, under, cell_text.value);
        } else {
            let value_top = at.y + BAND_LABEL_HEIGHT + BAND_ROW_GAP;
            let value_bottom = value_top + cell.value.size().y;
            painted(ui, id.with("value"), pos2(at.x, value_top), cell.value, under, cell_text.value);
            if let Some(note) = cell.note {
                painted(ui, id.with("note"), pos2(at.x, value_bottom + BAND_ROW_GAP), note, under, cell_text.note);
            }
        }
        if index > 0 {
            ui.painter().rect_filled(Rect::from_min_size(cell_rect.left_top(), vec2(HAIRLINE, rect.height())), 0.0, p.line);
        }
    }
    response
}

/// One plain line for a step: its knot, its name, one sentence and an optional toggle on the right, such as "Show 3 actions".
/// With no knot, its place stays empty, so names line up.
/// With a toggle, the whole line is its hit area: a click anywhere on it counts, and the line lights up under the pointer.
/// The toggle stays the line's one keyboard stop and shows its state. Returns a response that reports a click on either.
pub fn step_line(ui: &mut Ui, knot: Option<(&str, KnotState)>, name: &str, sentence: &str, toggle: Option<(&str, bool)>) -> Option<Response> {
    let width = ui.available_width();
    let name_ink = if knot.is_some_and(|(_, state)| state == KnotState::Waiting) { Ink::Status(Tone::Turn) } else { Ink::Text };
    let rect = Rect::from_min_size(ui.cursor().min, vec2(width, STEP_LINE_HEIGHT));
    // Sensed before the toggle is added, so the toggle sits on top and takes its own clicks.
    // It takes no focus, because the toggle already gives the line its keyboard stop.
    let line = toggle.map(|_| ui.interact(rect, ui.next_auto_id().with("step_line"), Sense::CLICK));
    let fill = ui.painter().add(egui::Shape::Noop);
    // The line paints its own fill under what it holds, so its toggle and text stand on a surface.
    let shown = ui.scope_builder(surface().max_rect(rect).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.set_min_size(vec2(width, STEP_LINE_HEIGHT));
        ui.spacing_mut().item_spacing.x = STEP_LINE_GAP;
        let toggled = toggle.map(|(text, on)| crate::ui_kit::small_toggle(ui, text, on));
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            match knot {
                Some((text, state)) => {
                    crate::ui_kit::knot(ui, text, state, KNOT_SIZE_SMALL);
                }
                None => {
                    ui.allocate_exact_size(Vec2::splat(KNOT_SIZE_SMALL), Sense::hover());
                }
            }
            ui.allocate_ui_with_layout(vec2(STEP_NAME_WIDTH, KNOT_SIZE_SMALL), Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_width(STEP_NAME_WIDTH);
                add_cut_text(ui, name, Type::STRONG, name_ink);
            });
            add_cut_text(ui, sentence, Type::BODY, Ink::Text);
        });
        toggled
    });
    let (line, toggled) = (line?, shown.inner?);
    let p = Palette::of(ui);
    // Over the toggle, the line stays lit: the toggle is part of it.
    let look = pick_look(&p, false, pointer_over(&line) || pointer_over(&toggled), pressed(&line));
    ui.painter().set(fill, egui::epaint::RectShape::filled(rect, RADIUS_CARD, look.fill));
    interactive(ui, &line, RADIUS_CARD);
    Some(line.union(toggled))
}

/// One role from a profile, as a CV line. Every part is the profile's own text, so it is untrusted.
#[derive(Debug, Clone, Copy)]
pub struct RoleEntry<'a> {
    /// The parts of the role's heading as `placeholders::shown_pieces` gives them, such as "2021 – present".
    pub dates: &'a [Piece],
    pub title: &'a [Piece],
    pub company: &'a [Piece],
    /// Markdown under the role's heading, such as its bullets.
    pub body: &'a str,
    /// The first role on the paper takes no rule above it.
    pub first: bool,
}

/// A role as a CV line: the dates in a fixed column, then the title over the company and the body, held to the measure,
/// and a small Edit button at the right edge, outside the copyable text. A placeholder in the heading reads as a blank,
/// and one in the body shows as written. `blocked` says why
/// Edit is off, such as another open edit. Returns the Edit button's response.
pub fn role_entry(ui: &mut Ui, cache: &mut CommonMarkCache, role: &RoleEntry, blocked: Option<&str>) -> Response {
    if !role.first {
        divider(ui);
    }
    ui.add_space(ROLE_PADDING.0);
    let edit = ui
        .with_layout(Layout::right_to_left(Align::Min), |ui| {
            let edit = enabled_when(ui, blocked.is_none(), blocked.unwrap_or_default(), |ui| crate::ui_kit::small_button(ui, "Edit"));
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.allocate_ui_with_layout(vec2(ROLE_DATES, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(ROLE_DATES);
                    let dates = blank_line(ui, role.dates, Type::NOTE, Ink::Muted);
                    copyable(ui, |ui| add_job(ui, dates, TextWrapMode::Wrap));
                });
                let width = (ui.available_width() - ITEM_GAP).clamp(0.0, DOC_MEASURE);
                ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let (title, company) = (blank_line(ui, role.title, Type::SECTION, Ink::Text), blank_line(ui, role.company, Type::BODY, Ink::Muted));
                    copyable(ui, |ui| {
                        add_job(ui, title, TextWrapMode::Wrap);
                        add_job(ui, company, TextWrapMode::Wrap);
                    });
                    ui.add_space(ITEM_GAP);
                    ui.spacing_mut().item_spacing.y = ITEM_GAP;
                    ui.style_mut().text_styles.insert(egui::TextStyle::Body, Type::TAKE.font());
                    profile_markdown(ui, cache, role.body);
                });
            });
            edit
        })
        .inner;
    ui.add_space(ROLE_PADDING.1);
    edit
}

/// A run works: Save is off and says `why` on hover, and `line` says the same beside Cancel.
#[derive(Debug, Clone, Copy)]
pub struct EditorBusy<'a> {
    pub why: &'a str,
    pub line: &'a str,
}

/// The warning box at the top of a block editor whose block changed on disk.
#[derive(Debug, Clone, Copy)]
pub struct ChangedOnDisk<'a> {
    /// In `interview` ink, then `detail` after it.
    pub title: &'a str,
    pub detail: &'a str,
    /// What the lines are, such as "Now in the file, the end of this role".
    pub caption: &'a str,
    /// The block's lines in the file now, each with whether it is new. `None` when the block is gone from the file.
    pub lines: Option<&'a [(&'a str, bool)]>,
    /// The button that takes the file's text, and the note beside it.
    pub take: &'a str,
    pub take_note: &'a str,
}

/// What a `block_editor` holds besides its draft. The words are the screen's.
#[derive(Debug, Clone, Copy)]
pub struct BlockEditor<'a> {
    /// Such as "Edit this role", with `hint` beside it.
    pub title: &'a str,
    pub hint: &'a str,
    /// Save's label: "Save", or what saving does over a change on disk.
    pub save: &'a str,
    /// The note beside Cancel.
    pub note: &'a str,
    /// Shown at the right of the footer, such as "Unsaved changes".
    pub unsaved: Option<&'a str>,
    /// While a run works. It holds over a change on disk too: no Save of either kind writes while a run works.
    pub busy: Option<EditorBusy<'a>>,
    /// The block changed on disk since the editor opened.
    pub changed: Option<&'a ChangedOnDisk<'a>>,
    /// The field takes the focus this frame, such as on the frame the editor opens.
    pub focus: bool,
    /// The footer scrolls into view this frame, such as when the warning box opens and pushes Save down.
    pub reveal: bool,
}

/// What the user asked of a `block_editor` this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    Save,
    Cancel,
    /// Replace the draft with the block as it is in the file now.
    TakeTheirs,
}

/// A slip in place of a block, to edit its raw Markdown: a title row with Preview, then the draft in a field in the body
/// face, then Save (the command key and Enter) and Cancel (Esc). The field renders nothing. With Preview on, `preview`
/// shows the draft as the view would. Over a change on disk, Save is a danger button. While busy, Save of either kind is
/// off and the command key and Enter do nothing, and a change on disk still shows. A held chord saves once: its repeats
/// do nothing. The caller names the field with an id of its own around the call, one per block opened, so no two blocks
/// share an undo history.
pub fn block_editor(
    ui: &mut Ui,
    editor: &BlockEditor,
    draft: &mut String,
    preview: &mut bool,
    show_preview: impl FnOnce(&mut Ui, &str),
) -> Option<EditorAction> {
    let p = Palette::of(ui);
    let busy = editor.busy.is_some();
    // Taken before the field sees it, so the chord never types a line break.
    let mut action = None;
    if !busy && fresh_chord(ui, Key::Enter) {
        action = Some(EditorAction::Save);
    }
    if crate::ui_kit::back_pressed(ui.ctx()) {
        action = Some(EditorAction::Cancel);
    }
    let (left, top, bottom) = EDITOR_PADDING;
    let frame =
        Frame::new().fill(p.panel).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_PANEL).inner_margin(Margin { left, right: left, top, bottom });
    frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = EDITOR_GAP;
        let title = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                let title = add_text(ui, editor.title, Type::STRONG, Ink::Text);
                add_text(ui, editor.hint, Type::NOTE, Ink::Muted);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::ui_kit::small_toggle(ui, "Preview", *preview).clicked() {
                        *preview = !*preview;
                    }
                });
                title.id
            })
            .inner;
        if let Some(changed) = editor.changed
            && changed_box(ui, changed)
        {
            action = Some(EditorAction::TakeTheirs);
        }
        if *preview {
            show_preview(ui, draft);
        } else {
            // A named field keeps its focus and cursor when the warning box opens above it.
            let field = egui::TextEdit::multiline(draft)
                .id_salt("block_editor_draft")
                .font(Type::BODY.font())
                .margin(Margin::same(FIELD_PADDING as i8))
                .desired_width(f32::INFINITY)
                // Under the warning box the field takes only the draft's lines, so Save stays near.
                .desired_rows(if editor.changed.is_some() { EDITOR_ROWS_CHANGED } else { EDITOR_ROWS });
            let field = ui.add(field).labelled_by(title);
            if editor.focus {
                field.request_focus();
            }
            if field.has_focus() {
                ui.painter().rect_stroke(field.rect.expand(FOCUS_OFFSET), RADIUS_BUTTON, Stroke::new(FOCUS_RING, p.active), StrokeKind::Outside);
            }
        }
        let footer = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = BUTTON_GAP;
            let chord = crate::ui_kit::command_chord("Enter");
            let why = editor.busy.map_or("", |busy| busy.why);
            let save = enabled_when(ui, !busy, why, |ui| {
                if editor.changed.is_some() {
                    crate::ui_kit::danger_shortcut_button(ui, editor.save, &chord)
                } else {
                    crate::ui_kit::primary_shortcut_button(ui, editor.save, &chord)
                }
            });
            if save.clicked() {
                action = Some(EditorAction::Save);
            }
            if plain_shortcut_button(ui, "Cancel", &["Esc"]).clicked() {
                action = Some(EditorAction::Cancel);
            }
            ui.add_space(ROW_ITEM_GAP);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(unsaved) = editor.unsaved {
                    add_text(ui, unsaved, Type::NOTE, Ink::Muted);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| match editor.busy {
                    Some(busy) => {
                        ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                        let (square, _) = ui.allocate_exact_size(Vec2::splat(STATUS_DOT), Sense::hover());
                        ui.painter().rect_filled(square, 0.0, p.active);
                        add_text(ui, busy.line, Type::NOTE, Ink::Text);
                    }
                    None => {
                        add_text(ui, editor.note, Type::NOTE, Ink::Muted);
                    }
                });
            });
        });
        if editor.reveal {
            // With the slip's own bottom padding, so its edge shows under Save.
            let rect = footer.response.rect;
            ui.scroll_to_rect(rect.with_max_y(rect.bottom() + f32::from(bottom)), Some(Align::Max));
        }
    });
    action
}

/// Whether the command key and `key` were pressed this frame and not only held: the press is taken either way, so a
/// held chord neither acts again nor reaches a field.
fn fresh_chord(ui: &Ui, key: Key) -> bool {
    ui.input_mut(|input| {
        let fresh = input.events.iter().any(|event| {
            matches!(event, egui::Event::Key { key: pressed, pressed: true, repeat: false, modifiers, .. } if *pressed == key && modifiers.matches_logically(Modifiers::COMMAND))
        });
        input.consume_key(Modifiers::COMMAND, key) && fresh
    })
}

/// The warning box over a block that changed on disk: what happened, the block's lines as they are now, plain and
/// copyable with an `interview` edge on each new line, and the button that takes them. Returns whether it was clicked.
fn changed_box(ui: &mut Ui, changed: &ChangedOnDisk) -> bool {
    let p = Palette::of(ui);
    prompt_box_tone(ui, Tone::Interview, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        let mut job = egui::text::LayoutJob::default();
        job.append(changed.title, 0.0, egui::TextFormat::simple(Type::STRONG.font(), p.interview));
        job.append(&format!(" {}", changed.detail), 0.0, egui::TextFormat::simple(Type::BODY.font(), p.text));
        copyable(ui, |ui| add_job(ui, job, TextWrapMode::Wrap));
        let Some(lines) = changed.lines else { return false };
        code_box(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let edge = ui.max_rect().left() - f32::from(CODE_PADDING.0);
            add_text(ui, changed.caption, Type::SMALL, Ink::Muted);
            for (text, added) in lines {
                // The file's own text: shown, and copied, without hidden characters, as the paper shows it.
                let shown = add_text(ui, visible_chars(text).collect::<String>(), Type::BODY, Ink::Text);
                if *added {
                    let rect = Rect::from_min_max(pos2(edge, shown.rect.top()), pos2(edge + PICK_EDGE, shown.rect.bottom()));
                    ui.painter().rect_filled(rect, 0.0, p.interview);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            let take = crate::ui_kit::small_button(ui, changed.take).clicked();
            add_text(ui, changed.take_note, Type::NOTE, Ink::Muted);
            take
        })
        .inner
    })
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    const LONG: &str = "Tessel Group Holdings International, Principal Solution Architect for Payments";

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    /// The title and the action on the very first frame, at `width`.
    fn first_frame(width: f32) -> (Rect, Rect) {
        let mut harness = Harness::builder().with_size([width, 300.0]).with_context_setup(setup).build_ui(|ui| {
            page_head(ui, Title::Slab(LONG), Some(Subtitle::Typed("documents/applications/tessel/")), |ui| {
                crate::ui_kit::button(ui, "Record a result");
            });
        });
        harness.step();
        (harness.get_by_label(LONG).rect(), harness.get_by_label("Record a result").rect())
    }

    #[test]
    fn a_long_title_stops_short_of_the_actions_from_the_first_frame() {
        let (title, action) = first_frame(900.0);
        assert!(title.right() <= action.left(), "{title:?} runs under {action:?}");
        assert!(title.width() >= HEAD_TITLE_MIN);
    }

    #[test]
    fn a_one_line_head_centres_its_actions() {
        const LINE: &str = "/apply jobs.marlowe.example/postings/tech-lead-2291";
        for (title, small) in [(Title::Typed(LINE), true), (Title::Typed(LINE), false), (Title::Slab(LINE), true)] {
            let mut harness = Harness::builder().with_size([1000.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
                page_head(ui, title, None, |ui| {
                    if small {
                        crate::ui_kit::small_button(ui, "End session");
                    } else {
                        crate::ui_kit::button(ui, "End session");
                    }
                });
            });
            harness.step();
            let (title, action) = (harness.get_by_label(LINE).rect(), harness.get_by_label("End session").rect());
            assert!((title.center().y - action.center().y).abs() <= 1.0, "{title:?} and {action:?} are off centre");
        }
    }

    #[test]
    fn the_band_names_its_text_and_tally() {
        let mut harness = Harness::builder().with_size([900.0, 200.0]).with_context_setup(setup).build_ui(|ui| {
            let marks = [(1, Some(Ink::Text)), (1, Some(Ink::Status(Tone::Turn))), (5, None)];
            let cells = [
                BandCell { label: "Where", value: "1 Evaluate, done", note: "Draft waits for your go", look: CellLook::Plain, marks: &marks },
                BandCell { label: "Whose turn", value: "Yours", note: "Claude stopped 2 min ago", look: CellLook::Yours, marks: &[] },
                BandCell { label: "What next", value: "Answer Claude", note: "Its question is right below.", look: CellLook::Plain, marks: &[] },
            ];
            turn_band(ui, &cells, false, "Where the run is", "1 of 7 steps done");
        });
        harness.step();
        for label in ["Where the run is", "1 of 7 steps done", "Yours", "Answer Claude", "Its question is right below."] {
            harness.get_by_label(label);
        }
    }

    #[test]
    fn a_compact_band_drops_its_notes() {
        let mut harness = Harness::builder().with_size([900.0, 200.0]).with_context_setup(setup).build_ui(|ui| {
            let cell = |label, value| BandCell { label, value, note: "A note", look: CellLook::Plain, marks: &[] };
            turn_band(ui, &[cell("Where", "2 Draft"), cell("Whose turn", "Claude's"), cell("What next", "Nothing for now")], true, "Where the run is", "");
        });
        harness.step();
        harness.get_by_label("Claude's");
        assert!(harness.query_by_label("A note").is_none());
    }

    #[test]
    fn a_step_line_reports_its_toggle() {
        let clicked = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = std::rc::Rc::clone(&clicked);
        let mut harness = Harness::builder().with_size([700.0, 100.0]).with_context_setup(setup).build_ui(move |ui| {
            if step_line(ui, Some(("2", KnotState::Working)), "Draft", "Wrote the CV.", Some(("Show 3 actions", false))).is_some_and(|line| line.clicked()) {
                seen.set(seen.get() + 1);
            }
        });
        harness.step();
        harness.get_by_label("2, working");
        harness.get_by_label("Show 3 actions").click();
        harness.run();
        assert_eq!(clicked.get(), 1, "a click on the toggle");
        harness.get_by_label("Draft").click();
        harness.run();
        assert_eq!(clicked.get(), 2, "a click on the step's name");
        harness.get_by_label("Wrote the CV.").click();
        harness.run();
        assert_eq!(clicked.get(), 3, "a click on its sentence");
    }

    #[test]
    fn a_step_line_without_a_toggle_takes_no_click() {
        let mut harness = Harness::builder().with_size([700.0, 100.0]).with_context_setup(setup).build_ui(|ui| {
            assert!(step_line(ui, None, "You", "Yes, go ahead.", None).is_none());
        });
        harness.step();
        harness.get_by_label("You").click();
        harness.run();
    }

    #[test]
    fn actions_take_their_own_row_when_the_title_would_be_too_narrow() {
        let (title, action) = first_frame(380.0);
        assert!(action.top() >= title.bottom(), "{title:?} shares a row with {action:?}");
        assert!(title.width() >= HEAD_TITLE_MIN);
    }

    /// The box and its words are one control: Tab stops on it once, and it is one checkbox named by its label.
    #[test]
    fn a_setting_check_is_one_tab_stop_named_by_its_label() {
        use egui_kittest::kittest::NodeT;

        let on = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = std::rc::Rc::clone(&on);
        let mut harness = Harness::builder().with_size([700.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            let _ = ui.button("Before");
            let mut value = seen.get();
            if setting_check(ui, &mut value, "Reduce motion", "Cards change at once, with no slide.", "").changed() {
                seen.set(value);
            }
            let _ = ui.button("After");
        });
        harness.run();
        let named: Vec<_> =
            harness.query_all_by(|node| node.label().as_deref() == Some("Reduce motion") || node.value().as_deref() == Some("Reduce motion")).collect();
        assert_eq!(named.len(), 1, "one node is named by the label");
        assert_eq!(named[0].accesskit_node().role(), egui::accesskit::Role::CheckBox);
        assert_eq!(named[0].accesskit_node().description().as_deref(), Some("Cards change at once, with no slide."));
        assert!(harness.query_by_label("Cards change at once, with no slide.").is_none(), "the help is no node of its own");

        harness.get_by_label("Before").focus();
        harness.run();
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_role(egui::accesskit::Role::CheckBox).is_focused(), "Tab reaches the checkbox");
        harness.key_press(egui::Key::Space);
        harness.run();
        assert!(on.get(), "Space flips it");
        harness.key_press(egui::Key::Tab);
        harness.run();
        assert!(harness.get_by_label("After").is_focused(), "the next Tab leaves the row");
    }

    const CHANGED_LINES: [(&str, bool); 2] = [("- Budget you owned: [PLACEHOLDER]", false), ("- Introduced a weekly\u{202e} planning review.", true)];

    const CHANGED: ChangedOnDisk = ChangedOnDisk {
        title: "This role changed while you were editing.",
        detail: "A task added a line to it.",
        caption: "Now in the file",
        lines: Some(&CHANGED_LINES),
        take: "Use the new version",
        take_note: "Replaces your text with the file's.",
    };

    /// An editor on the bench: whether a run works, whether the block changed on disk, and what it asked for so far.
    struct Bench {
        busy: bool,
        changed: bool,
        draft: String,
        actions: Vec<EditorAction>,
    }

    fn bench(busy: bool, changed: bool) -> Harness<'static, Bench> {
        let state = Bench { busy, changed, draft: "### Lead Engineer - Calder Freight (2021 – present)".to_owned(), actions: Vec::new() };
        Harness::builder().with_size([800.0, 600.0]).with_context_setup(setup).build_ui_state(
            |ui, bench: &mut Bench| {
                let editor = BlockEditor {
                    title: "Edit this role",
                    hint: "Markdown.",
                    save: if bench.changed { "Save mine over it" } else { "Save" },
                    note: "Only this role changes.",
                    unsaved: None,
                    busy: bench.busy.then_some(EditorBusy { why: "A task is running.", line: "Save waits until it ends." }),
                    changed: bench.changed.then_some(&CHANGED),
                    focus: false,
                    reveal: false,
                };
                if let Some(action) = block_editor(ui, &editor, &mut bench.draft, &mut false, |_, _| {}) {
                    bench.actions.push(action);
                }
                crate::ui_kit::note_focus(ui.ctx());
            },
            state,
        )
    }

    fn save_button<'h>(harness: &'h Harness<'static, Bench>) -> egui_kittest::Node<'h> {
        let label = if harness.state().changed { "Save mine over it" } else { "Save" };
        harness.get_by_role_and_label(egui::accesskit::Role::Button, label)
    }

    #[test]
    fn the_command_key_and_enter_saves_only_while_no_run_works() {
        for changed in [false, true] {
            let mut harness = bench(false, changed);
            harness.run();
            harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
            harness.run();
            assert_eq!(harness.state().actions, vec![EditorAction::Save], "changed: {changed}");

            let mut harness = bench(true, changed);
            harness.run();
            harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
            harness.run();
            save_button(&harness).click();
            harness.run();
            assert_eq!(harness.state().actions, Vec::new(), "changed: {changed}");
            assert!(save_button(&harness).accesskit_node().is_disabled());
        }
    }

    /// A held chord saves once, and its repeats do nothing.
    #[test]
    fn a_held_chord_saves_once() {
        let mut harness = bench(false, true);
        harness.run();
        harness.key_down_modifiers(Modifiers::COMMAND, Key::Enter);
        harness.run();
        assert_eq!(harness.state().actions, vec![EditorAction::Save], "the press");
        // The key is still down, so egui reads each further press as a repeat.
        for _ in 0..3 {
            harness.key_down_modifiers(Modifiers::COMMAND, Key::Enter);
            harness.run();
        }
        assert_eq!(harness.state().actions, vec![EditorAction::Save], "the repeats");
    }

    /// A line of the file shows as the paper shows it, with no hidden characters to make it read as another line.
    #[test]
    fn the_lines_on_disk_show_without_hidden_characters() {
        let mut harness = bench(false, true);
        harness.state_mut().changed = true;
        harness.run();
        assert!(harness.query_by_label("- Introduced a weekly planning review.").is_some());
        assert!(harness.query_by_label_contains("\u{202e}").is_none());
    }

    #[test]
    fn a_change_on_disk_still_shows_while_a_run_works() {
        let mut harness = bench(true, true);
        harness.run();
        assert!(harness.query_by_label_contains("This role changed").is_some());
        harness.get_by_label("Use the new version").click();
        harness.run();
        assert_eq!(harness.state().actions, vec![EditorAction::TakeTheirs]);
    }

    #[test]
    fn the_first_esc_leaves_the_field_and_the_second_cancels() {
        let mut harness = bench(false, false);
        harness.run();
        harness.query_all_by_role(egui::accesskit::Role::MultilineTextInput).next().expect("the field").focus();
        harness.run();
        harness.key_press(Key::Escape);
        harness.run();
        assert_eq!(harness.state().actions, Vec::new());
        harness.key_press(Key::Escape);
        harness.run();
        assert_eq!(harness.state().actions, vec![EditorAction::Cancel]);
    }

    #[test]
    fn a_blocked_role_offers_no_edit() {
        for (blocked, clicks) in [(None, 1), (Some("Save or cancel the open edit first."), 0)] {
            let mut cache = CommonMarkCache::default();
            let text = |text: &str| vec![Piece::Text(text.to_owned())];
            let (dates, title, company) = (text("2021 – present"), text("Lead Engineer"), text("Calder Freight"));
            let mut harness = Harness::builder().with_size([800.0, 300.0]).with_context_setup(setup).build_ui_state(
                move |ui, clicked: &mut usize| {
                    let role = RoleEntry { dates: &dates, title: &title, company: &company, body: "- Leads a team of six.", first: true };
                    if role_entry(ui, &mut cache, &role, blocked).clicked() {
                        *clicked += 1;
                    }
                },
                0,
            );
            harness.run();
            assert_eq!(harness.get_by_label("Edit").accesskit_node().is_disabled(), blocked.is_some());
            harness.get_by_label("Edit").click();
            harness.run();
            assert_eq!(*harness.state(), clicks, "blocked: {blocked:?}");
        }
    }
}
