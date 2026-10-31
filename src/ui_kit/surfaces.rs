use egui::{Align, CornerRadius, Frame, Layout, Margin, Panel, Rect, Response, Sense, Shadow, Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::text::{line, paint_centred, rich};
use crate::ui_kit::tokens::{
    BODY_PADDING, CARD_PADDING, CLEAR_PADDING, CODE_PADDING, GUTTER, HAIRLINE, HEADER_PADDING, HEAD_GAP, HEAD_PADDING, INSET, ITEM_GAP, LANE_GAP, LANE_PADDING, NODE_ARROW, NODE_GAP, NODE_PADDING, NODE_TALLY_HEIGHT,
    NODE_TALLY_MARGIN, NOTE_PADDING, OFFSET_SHADOW, PICK_PADDING, PROMPT_PADDING, RADIUS_CARD, RADIUS_PANEL, ROW_GAP, ROW_PADDING, SCREEN_PADDING, SECTION_RULE, SHELL_PADDING,
    STATUS_PADDING, STEP_GAP, STEP_HEIGHT, STEP_PADDING, Type,
};

fn margin((x, y): (i8, i8)) -> Margin {
    Margin::symmetric(x, y)
}

/// The desk plane, under everything.
pub fn desk_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).bg)
}

/// The sheet plane, where a screen's content sits.
pub fn sheet_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).sheet)
}

/// The desk strip above the sheet, where the brand and the folder tabs stand.
pub fn shell_top_frame(ui: &Ui) -> Frame {
    let (x, top, bottom) = SHELL_PADDING;
    Frame::new().fill(Palette::of(ui).bg).inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 })
}

/// The desk strip under the sheet, for the status line.
pub fn status_frame(ui: &Ui) -> Frame {
    Frame::new().fill(Palette::of(ui).bg).inner_margin(margin((STATUS_PADDING.0 as i8, STATUS_PADDING.1 as i8)))
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

/// A screen's head: title and subtitle on the left, `actions` on the right, level with the last line.
/// Actions are added right to left. They share the head's width with the title, so keep both short.
pub fn page_head(ui: &mut Ui, title: Title, subtitle: Option<Subtitle>, actions: impl FnOnce(&mut Ui)) {
    let (x, top, bottom) = HEAD_PADDING;
    Frame::new().inner_margin(Margin { left: x as i8, right: x as i8, top: top as i8, bottom: bottom as i8 }).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let right = ui.max_rect().right();
        let text = ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
                let (text, style) = match title {
                    Title::Slab(text) => (text, Type::H1),
                    Title::Lead(text) => (text, Type::LEAD),
                    Title::Typed(text) => (text, Type::PATH),
                };
                let text = rich(ui, text, style, Ink::Text);
                ui.label(text);
                if let Some(subtitle) = subtitle {
                    let (text, style) = match subtitle {
                        Subtitle::Text(text) => (text, Type::BODY),
                        Subtitle::Typed(text) => (text, Type::MONO),
                    };
                    let text = rich(ui, text, style, Ink::Muted);
                    ui.label(text);
                }
            })
            .response
            .rect;
        let row = Rect::from_min_max(text.min, pos2(right, text.bottom()));
        let mut side = ui.new_child(UiBuilder::new().max_rect(row).layout(Layout::right_to_left(Align::BOTTOM)));
        side.spacing_mut().item_spacing.x = HEAD_GAP.0;
        actions(&mut side);
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

/// A fixed column on the left of a screen's content, such as a list to pick from.
pub fn list_column(id: &'static str, width: f32) -> Panel {
    Panel::left(id).exact_size(width).resizable(false).show_separator_line(false).frame(Frame::new().inner_margin(Margin { right: GUTTER, ..Default::default() }))
}

/// A strip along the bottom of a column, for input.
pub fn input_strip(id: &'static str) -> Panel {
    Panel::bottom(id).show_separator_line(false).frame(Frame::new().inner_margin(Margin { top: ROW_GAP as i8, ..Default::default() }))
}

/// Stacked sections set their own padding, so nothing adds a gap between them.
fn stacked<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.spacing_mut().item_spacing.y = 0.0;
    add(ui)
}

/// A section: a heavy rule on the sheet, with no fill of its own.
pub fn panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let color = Palette::of(ui).text;
    let shown = Frame::new().inner_margin(Margin { top: SECTION_RULE as i8, ..Default::default() }).show(ui, |ui| stacked(ui, add));
    let mut rule = shown.response.rect;
    rule.set_height(SECTION_RULE);
    ui.painter().rect_filled(rule, 0.0, color);
    shown.inner
}

/// A side slip: the only kind of section with a fill.
pub fn slip<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new().fill(fill).corner_radius(RADIUS_PANEL).show(ui, |ui| stacked(ui, add)).inner
}

fn header(ui: &mut Ui, title: &str, note: Option<&str>, ink: Ink) {
    Frame::new().inner_margin(Margin::symmetric(INSET, HEADER_PADDING)).show(ui, |ui| {
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

pub fn divider(ui: &mut Ui) {
    let color = Palette::of(ui).line;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HAIRLINE), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, color);
}

pub fn panel_body<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .inner_margin(Margin::symmetric(INSET, BODY_PADDING))
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
        .inner_margin(Margin::symmetric(INSET, ROW_PADDING))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP;
            ui.set_width(ui.available_width());
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

/// The empty state of a whole screen.
pub fn clear_panel<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let fill = Palette::of(ui).panel;
    Frame::new()
        .fill(fill)
        .corner_radius(RADIUS_PANEL)
        .inner_margin(margin(CLEAR_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
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
pub(in crate::ui_kit) fn offset_shadow(ui: &Ui) -> Shadow {
    Shadow { offset: [OFFSET_SHADOW, OFFSET_SHADOW], blur: 0, spread: 0, color: Palette::of(ui).interview }
}

/// A run's question or permission request, set apart from the sheet.
pub fn prompt_box<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = Palette::of(ui);
    Frame::new()
        .fill(p.sheet)
        .stroke(Stroke::new(HAIRLINE, p.interview))
        .shadow(offset_shadow(ui))
        .corner_radius(RADIUS_CARD)
        .inner_margin(margin(PROMPT_PADDING))
        .outer_margin(Margin { right: OFFSET_SHADOW, bottom: OFFSET_SHADOW, ..Default::default() })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
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

/// One application on a board. Clicking anywhere on it counts.
pub fn index_card(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    ui.scope_builder(UiBuilder::new().sense(Sense::click()), |ui| {
        let fill = if on || ui.response().hovered() { p.raised } else { p.sheet };
        Frame::new().fill(fill).corner_radius(RADIUS_CARD).inner_margin(margin(CARD_PADDING)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 2.0;
            add(ui);
        });
    })
    .response
}

/// Where a run has got to. Steps before `current` are done; `live` lights the current one.
pub fn step_rail(ui: &mut Ui, labels: &[&str], current: usize, live: bool) {
    let p = Palette::of(ui);
    ui.spacing_mut().item_spacing = vec2(STEP_GAP, STEP_GAP);
    ui.horizontal_wrapped(|ui| {
        for (index, label) in labels.iter().enumerate() {
            let now = index == current && live;
            let done = index < current;
            let (style, ink) = if now {
                (Type::STEP_NOW, p.on_primary)
            } else if done {
                (Type::STEP_DONE, p.text)
            } else {
                (Type::STEP, p.muted)
            };
            let galley = line(ui, &format!("{index} {label}"), style, ink, f32::INFINITY);
            let size = vec2(galley.size().x + STEP_PADDING * 2.0, STEP_HEIGHT);
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let corners = CornerRadius {
                nw: if index == 0 { RADIUS_PANEL } else { 0 },
                sw: if index == 0 { RADIUS_PANEL } else { 0 },
                ne: if index + 1 == labels.len() { RADIUS_PANEL } else { 0 },
                se: if index + 1 == labels.len() { RADIUS_PANEL } else { 0 },
            };
            ui.painter().rect_filled(rect, corners, if now { p.primary } else { p.panel });
            let at = Rect::from_center_size(rect.center(), galley.size()).min;
            ui.painter().galley(at, galley, ink);
        }
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
