use std::sync::Arc;

use egui::{
    Align, Color32, CornerRadius, EventFilter, Frame, Galley, Key, Layout, Margin, Modifiers, Painter, Panel, Pos2, Rect, Response, RichText, Role, Sense, Shadow, Stroke, StrokeKind, TextWrapMode, Ui, UiBuilder, UiStackInfo, Vec2,
    WidgetInfo, WidgetText, pos2, vec2,
};

use crate::shared_kernel::Source;
use crate::ui_kit::Palette;
use crate::ui_kit::controls::{interactive, interactive_row, paint_pick_edge, pick_look, pointer_over, pressed};
use crate::ui_kit::marks::KnotState;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::text::{add_cut_text, add_galley, add_text, copyable, cut_hover, line, paint_centred, paint_text, table_head};
use crate::ui_kit::tokens::{
    BAND_COLUMNS, BAND_COMPACT_PADDING, BAND_GAP, BAND_LABEL_HEIGHT, BAND_PADDING, BAND_ROW_GAP, BODY_PADDING, BUTTON_GAP, BUTTON_HEIGHT, CARD_PADDING, CODE_PADDING, EMPTY_GAP, EMPTY_PADDING, EMPTY_WIDTH, FADED_OPACITY, GRID_PITCH, GUTTER, HAIRLINE, HEADER_PADDING, HEAD_GAP, HEAD_PADDING,
    HEAD_TITLE_MIN, INSET, ITEM_GAP, KNOT_SIZE_SMALL, LANE_GAP, LANE_PADDING, NODE_ARROW, NODE_GAP, NODE_PADDING, NODE_TALLY_HEIGHT, NODE_TALLY_MARGIN, NOTE_PADDING, OFFSET_SHADOW, PICK_PADDING, PROMPT_PADDING, RADIUS_CARD, RADIUS_PANEL,
    ROW_PADDING, SCREEN_PADDING, SHELL_PADDING, SKELETON_GAP, SKELETON_LINE, SPLIT_GAP, STATUS_PADDING, STEP_LINE_GAP, STEP_LINE_HEIGHT, STEP_NAME_WIDTH, TABLE_CELL_PADDING, TABLE_HEAD_HEIGHT, TALLY_BAND, Type, WAITS_EDGE, WINDOW_SHADOW,
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
        let mut sizing = ui.new_child(UiBuilder::new().id_salt("page_head_sizing").max_rect(measure).layout(Layout::right_to_left(Align::Center)).sizing_pass().invisible());
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

    pub fn failed(&self) -> bool {
        self.load == Load::Failed
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
    Panel::right(id).exact_size(width).resizable(false).show_separator_line(false).frame(Frame::new().inner_margin(Margin { left: GUTTER, ..Default::default() }))
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
pub fn pick_row(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let response = ui
        .scope_builder(clickable(), |ui| {
            let row = ui.response();
            let look = pick_look(&p, on, pointer_over(&row), pressed(&row));
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
/// Up and Down move the pick while no text field has focus. `cell(ui, row, column)` fills one cell.
/// The head stays put while the rows scroll. Returns the row picked this frame.
pub fn pick_table(ui: &mut Ui, table: &PickTable, spoken: impl Fn(usize) -> String, mut cell: impl FnMut(&mut Ui, usize, usize)) -> Option<usize> {
    let p = Palette::of(ui);
    let id = ui.make_persistent_id(table.id_salt);
    let row_id = |row: usize| id.with(("pick_table_row", row));
    let widths = column_widths(table.columns, ui.available_width());
    let width: f32 = widths.iter().sum();
    let moved = arrow_pick(ui, table);
    if let Some(row) = moved {
        let focused = ui.memory(|mem| mem.focused());
        if focused.is_some_and(|focused| (0..table.rows).any(|row| row_id(row) == focused)) {
            ui.memory_mut(|mem| mem.request_focus(row_id(row)));
        }
    }
    let shown = moved.or(table.picked);
    let mut clicked = None;
    // Cells pad themselves, so rules and the picked row run unbroken.
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    let head = Rect::from_min_size(ui.cursor().min, vec2(width, TABLE_HEAD_HEIGHT));
    for (column, rect) in column_rects(head, &widths).enumerate() {
        table_cell_in(ui, rect, CellRule::Head, |ui| {
            table_head(ui, table.columns[column].head);
        });
    }
    ui.advance_cursor_after_rect(head);
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

/// The row Up or Down picks this frame, if either was pressed and the pick moves. Typing in a field keeps its arrows.
fn arrow_pick(ui: &mut Ui, table: &PickTable) -> Option<usize> {
    if table.rows == 0 || ui.ctx().text_edit_focused() {
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
    let fill = Palette::of(ui).field;
    Frame::new()
        .fill(fill)
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
        frame
            .shadow(Shadow { offset: [WINDOW_SHADOW, WINDOW_SHADOW], blur: 0, spread: 0, color: p.line })
            .outer_margin(Margin { right: WINDOW_SHADOW, bottom: WINDOW_SHADOW, ..Default::default() })
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
    empty_sheet(ui, title, body, Some(actions));
}

/// The same with no actions, such as a lane that is empty with nothing to offer. A blank `body` leaves the sentence out.
pub fn empty_note(ui: &mut Ui, title: &str, body: &str) {
    empty_sheet(ui, title, body, None::<fn(&mut Ui)>);
}

fn empty_sheet(ui: &mut Ui, title: &str, body: &str, actions: Option<impl FnOnce(&mut Ui)>) {
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
            let frame = Frame::new().fill(fill).stroke(Stroke::new(HAIRLINE, pick.edge.unwrap_or(p.line))).corner_radius(RADIUS_CARD).inner_margin(margin(CARD_PADDING)).show(ui, |ui| {
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
pub struct FlowNode<'a> {
    pub label: &'a str,
    pub figure: String,
    pub marks: Vec<(usize, Option<Ink>)>,
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
                    let tally = crate::ui_kit::tally_strip(ui, &node.marks, false);
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

/// Text wrapped to `width`, for widgets that paint their own.
fn wrapped(ui: &Ui, text: &str, style: Type, color: Color32, width: f32) -> Arc<Galley> {
    WidgetText::from(RichText::new(text).font(style.font()).color(color)).into_galley(ui, Some(TextWrapMode::Wrap), width, style.font())
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

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

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
}
