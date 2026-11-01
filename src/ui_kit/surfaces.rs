use std::sync::Arc;

use egui::{
    Align, Color32, CornerRadius, Frame, Galley, Layout, Margin, Painter, Panel, Pos2, Rect, Response, RichText, Role, Sense, Shadow, Shape, Stroke, StrokeKind, TextWrapMode, Ui, UiBuilder, Vec2, WidgetInfo, WidgetText, pos2,
    vec2,
};

use crate::ui_kit::Palette;
use crate::ui_kit::marks::KnotState;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::text::{line, paint_centred, rich};
use crate::ui_kit::tokens::{
    BAND_COLUMNS, BAND_COMPACT_PADDING, BAND_GAP, BAND_LABEL_HEIGHT, BAND_PADDING, BAND_ROW_GAP, BODY_PADDING, BUTTON_HEIGHT, CARD_PADDING, CODE_PADDING, FADED_OPACITY, GRID_PITCH, GUTTER, HAIRLINE, HEADER_PADDING, HEAD_GAP, HEAD_PADDING,
    HEAD_TITLE_MIN, INSET, ITEM_GAP, KNOT_SIZE_SMALL, LANE_GAP, LANE_PADDING, NODE_ARROW, NODE_GAP, NODE_PADDING, NODE_TALLY_HEIGHT, NODE_TALLY_MARGIN, NOTE_PADDING, OFFSET_SHADOW, PICK_PADDING, PROMPT_PADDING, RADIUS_CARD, RADIUS_PANEL,
    ROW_PADDING, SCREEN_PADDING, SHELL_PADDING, SPLIT_GAP, STATUS_PADDING, STEP_LINE_GAP, STEP_LINE_HEIGHT, STEP_NAME_WIDTH, TABLE_CELL_PADDING, TALLY_BAND, Type, WAITS_EDGE, WINDOW_SHADOW,
};

fn margin((x, y): (i8, i8)) -> Margin {
    Margin::symmetric(x, y)
}

/// The desk plane, under everything.
pub fn desk_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).bg)
}

/// The gridded desk under the whole window: `sheet`, ruled every `GRID_PITCH` in `grid`.
/// The lines start at the window's origin, so they hold still while content scrolls over them.
pub fn paint_grid(painter: &Painter, rect: Rect) {
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
pub fn sheet_frame(_ui: &Ui) -> Frame {
    Frame::new()
}

/// The menu on the desk: brand, tabs and New task in one row.
pub fn shell_top_frame(_ui: &Ui) -> Frame {
    let (x, top, bottom) = SHELL_PADDING;
    Frame::new().inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 })
}

/// The strip at the desk's bottom edge, for the status line. The shell draws its top hairline as the panel's separator.
pub fn status_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).sheet).inner_margin(margin((STATUS_PADDING.0 as i8, STATUS_PADDING.1 as i8)))
}

/// Backs small text that sits straight on the desk with the desk's base, so no grid line runs through it.
/// The backing is the size of what `add` lays out, and is painted under it.
pub fn on_desk<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let backing = ui.painter().add(Shape::Noop);
    let shown = ui.scope(add);
    let fill = Palette::of(ui).sheet;
    ui.painter().set(backing, Shape::rect_filled(shown.response.rect, 0.0, fill));
    shown.inner
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
        // A typed subtitle is a path in thin mono type, so it gets the desk's base behind it.
        let typed = matches!(subtitle, Some(Subtitle::Typed(_)));
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
                ui.add(egui::Label::new(title));
                if let Some(subtitle) = subtitle {
                    if typed {
                        on_desk(ui, |ui| ui.add(egui::Label::new(subtitle)));
                    } else {
                        ui.add(egui::Label::new(subtitle));
                    }
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
            let heading = rich(ui, title, Type::SECTION, ink);
            ui.label(heading);
            if let Some(note) = note {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let note = rich(ui, note, Type::NOTE, Ink::Muted);
                    ui.add(egui::Label::new(note).truncate());
                });
            }
        });
    });
    divider(ui);
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
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing = vec2(ITEM_GAP, 0.0);
        add(ui)
    })
    .inner
}

/// A whole row that picks something, such as a run in a list.
pub fn pick_row(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        let lit = on || ui.response().hovered();
        let frame = Frame::new().corner_radius(RADIUS_PANEL).inner_margin(margin(PICK_PADDING));
        let frame = if lit { frame.fill(p.panel) } else { frame };
        frame.show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    })
    .response
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
            add(ui)
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
                ui.add(egui::Label::new(rich(ui, title, Type::LEAD, Ink::Text)).truncate());
                ui.add(egui::Label::new(rich(ui, subtitle, Type::TAKE, Ink::Muted)).truncate());
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
pub fn index_card(ui: &mut Ui, look: CardLook, spoken: &str, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let response = ui
        .scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
            if look.faded {
                ui.multiply_opacity(FADED_OPACITY);
            }
            let fill = if look.on || ui.response().hovered() { p.raised } else { p.card };
            let frame = Frame::new().fill(fill).stroke(Stroke::new(HAIRLINE, p.line)).corner_radius(RADIUS_CARD).inner_margin(margin(CARD_PADDING)).show(ui, |ui| {
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
    let spoken = if look.waits { format!("{spoken}, waits for you in Today") } else { spoken.to_owned() };
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &spoken));
    response
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
                    ui.label(rich(ui, node.label, Type::TAB, Ink::Muted));
                    ui.label(rich(ui, node.figure.as_str(), Type::FIGURE, Ink::Text));
                    ui.add_space(NODE_TALLY_MARGIN.0);
                    let tally = crate::ui_kit::tally_strip(ui, &node.marks, false);
                    ui.add_space((NODE_TALLY_HEIGHT - tally.rect.height()).max(0.0) + NODE_TALLY_MARGIN.1);
                    ui.label(rich(ui, node.note.as_str(), Type::BODY, Ink::Muted));
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
        paint_centred(ui, patch, arrow, p.faint);
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

/// Paints `galley` at `at` and names it for screen readers and tests.
fn painted(ui: &Ui, id: egui::Id, at: Pos2, galley: Arc<Galley>, color: Color32, text: &str) {
    let rect = Rect::from_min_size(at, galley.size());
    ui.painter().galley(at, galley, color);
    ui.interact(rect, id, Sense::hover()).widget_info(|| WidgetInfo::labeled(Role::Label, true, text));
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
        let at = pos2(cell_rect.left() + x, rect.top() + top);
        // The label's line: centred in its row, or on the value's baseline when compact.
        let label_top = if compact { at.y + baseline(&cell.value) - baseline(&cell.label) } else { at.y + (BAND_LABEL_HEIGHT - cell.label.size().y) / 2.0 };
        let label_width = cell.label.size().x;
        let label_centre = label_top + cell.label.size().y / 2.0;
        painted(ui, id.with("label"), pos2(at.x, label_top), cell.label, p.text, cell_text.label);
        let mut after = at.x + label_width + BAND_GAP;
        if !cell.marks.is_empty() {
            let tally = crate::ui_kit::marks::paint_mark_row(ui, pos2(after, label_centre - TALLY_BAND.height / 2.0), &cell.marks, TALLY_BAND);
            ui.interact(tally, id.with("tally"), Sense::hover()).widget_info(|| WidgetInfo::labeled(Role::Label, true, tally_label));
            after = tally.right() + BAND_GAP;
        }
        if compact {
            painted(ui, id.with("value"), pos2(after, at.y), cell.value, p.text, cell_text.value);
        } else {
            let value_top = at.y + BAND_LABEL_HEIGHT + BAND_ROW_GAP;
            let value_bottom = value_top + cell.value.size().y;
            painted(ui, id.with("value"), pos2(at.x, value_top), cell.value, p.text, cell_text.value);
            if let Some(note) = cell.note {
                painted(ui, id.with("note"), pos2(at.x, value_bottom + BAND_ROW_GAP), note, p.text, cell_text.note);
            }
        }
        if index > 0 {
            ui.painter().rect_filled(Rect::from_min_size(cell_rect.left_top(), vec2(HAIRLINE, rect.height())), 0.0, p.line);
        }
    }
    response
}

/// One plain line for a step: its knot, its name, one sentence and an optional toggle on the right, such as "Show 3 actions".
/// With no knot, its place stays empty, so names line up. Returns the toggle's response.
pub fn step_line(ui: &mut Ui, knot: Option<(&str, KnotState)>, name: &str, sentence: &str, toggle: Option<(&str, bool)>) -> Option<Response> {
    let width = ui.available_width();
    let name_ink = if knot.is_some_and(|(_, state)| state == KnotState::Waiting) { Ink::Status(Tone::Turn) } else { Ink::Text };
    ui.allocate_ui_with_layout(vec2(width, STEP_LINE_HEIGHT), Layout::right_to_left(Align::Center), |ui| {
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
                ui.add(egui::Label::new(rich(ui, name, Type::STRONG, name_ink)).truncate());
            });
            ui.add(egui::Label::new(rich(ui, sentence, Type::BODY, Ink::Text)).truncate());
        });
        toggled
    })
    .inner
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
        let clicked = std::rc::Rc::new(std::cell::Cell::new(false));
        let seen = std::rc::Rc::clone(&clicked);
        let mut harness = Harness::builder().with_size([700.0, 100.0]).with_context_setup(setup).build_ui(move |ui| {
            if step_line(ui, Some(("2", KnotState::Working)), "Draft", "Wrote the CV.", Some(("Show 3 actions", false))).is_some_and(|toggle| toggle.clicked()) {
                seen.set(true);
            }
        });
        harness.step();
        harness.get_by_label("2, working");
        harness.get_by_label("Draft");
        harness.get_by_label("Show 3 actions").click();
        harness.run();
        assert!(clicked.get());
    }

    #[test]
    fn actions_take_their_own_row_when_the_title_would_be_too_narrow() {
        let (title, action) = first_frame(380.0);
        assert!(action.top() >= title.bottom(), "{title:?} shares a row with {action:?}");
        assert!(title.width() >= HEAD_TITLE_MIN);
    }
}
