use egui::{Align, Key, Label, Layout, Ui, vec2};

use crate::settings::application::{ComingUp, LaterLasts, Page, ReasonRefusal, Settings, SkipReasons, index};
use crate::ui_kit::tokens::SETTING_CONTROL_GAP;
use crate::ui_kit::{Ink, Type, divider, enabled_when, rich, setting_row, setting_section, small_button, text_field, toggle_group};

/// How tall a reason's row is, and the room above and under the field that adds one.
const REASON_ROW: f32 = 44.0;
const ADD_PADDING: f32 = 9.0;
const ADD: &str = "Add reason";
/// The room Add reason takes at the right of the field. The field goes in first, for the Tab order, so it cannot measure it.
const ADD_WIDTH: f32 = 96.0;
const HINT: &str = "A new reason";
/// What a screen reader hears for the field, which has no label of its own on screen.
const FIELD_NAME: &str = "New skip reason";

/// How far Coming up looks and how long Later lasts, then the skip reasons, each with Remove, and a field that adds one.
/// `typed` holds the reason being typed. Every change reaches Today on its next frame.
pub fn body(ui: &mut Ui, settings: &mut Settings, typed: &mut String, query: &str) {
    let &[dealing, coming, coming_help, later, later_help, reasons, reasons_note] = index(Page::Queue) else { return };
    setting_section(ui, dealing, None, query);
    setting_row(ui, coming, coming_help, query, |ui| {
        let options = ComingUp::ALL.map(|span| (span, span.label()));
        if let Some(span) = toggle_group(ui, Some(settings.preferences().queue.coming_up), &options) {
            settings.set_coming_up(span);
        }
    });
    setting_row(ui, later, later_help, query, |ui| {
        let options = LaterLasts::ALL.map(|lasts| (lasts, lasts.label()));
        if let Some(lasts) = toggle_group(ui, Some(settings.preferences().queue.later), &options) {
            settings.set_later(lasts);
        }
    });
    setting_section(ui, reasons, Some(reasons_note), query);
    let list = settings.preferences().queue.skip_reasons.list().to_vec();
    let mut removed = None;
    for (at, reason) in list.iter().enumerate() {
        // A row keeps its Remove's id when a row above it goes.
        ui.push_id(("skip reason", reason), |ui| {
            if at > 0 {
                divider(ui);
            }
            if reason_row(ui, reason) {
                removed = Some(reason.clone());
            }
        });
    }
    if let Some(reason) = removed {
        settings.remove_skip_reason(&reason);
    }
    if !list.is_empty() {
        divider(ui);
    }
    add_row(ui, settings, typed);
}

/// A reason with Remove at the right. Returns whether Remove was pressed. The reason was cleaned as it was typed or
/// read, and shows as plain text.
fn reason_row(ui: &mut Ui, reason: &str) -> bool {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), REASON_ROW), Layout::right_to_left(Align::Center), |ui| {
        ui.set_min_height(REASON_ROW);
        let remove = small_button(ui, "Remove");
        let name = format!("Remove {reason}");
        ui.ctx().accesskit_node_builder(remove.id, |node| node.set_label(name));
        ui.add_space(SETTING_CONTROL_GAP);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.add(Label::new(rich(ui, reason, Type::BODY, Ink::Text)).truncate());
        });
        remove.clicked()
    })
    .inner
}

/// The field for a new reason and Add reason, which waits while the field is empty or the list would refuse what it
/// holds, and says why. Enter adds it too. A full list says so under them, and the field waits too.
/// The field comes first, so Tab goes from it to Add reason.
fn add_row(ui: &mut Ui, settings: &mut Settings, typed: &mut String) {
    let reasons = &settings.preferences().queue.skip_reasons;
    // A full list refuses any reason, so it says that before it asks for one.
    let full = reasons.list().len() >= SkipReasons::MOST_REASONS;
    let refused = if full { Some(ReasonRefusal::Full) } else { reasons.check(typed).err() };
    let why = refused.map(|refusal| refusal.why()).unwrap_or_default();
    ui.add_space(ADD_PADDING);
    let (added, entered) = ui
        .allocate_ui_with_layout(vec2(ui.available_width(), 0.0), Layout::left_to_right(Align::Center), |ui| {
            let width = (ui.available_width() - ADD_WIDTH - SETTING_CONTROL_GAP).max(0.0);
            let field = ui.add_enabled_ui(!full, |ui| text_field(ui, typed, width, HINT, SkipReasons::REASON_CHARS)).inner.on_disabled_hover_text(why);
            ui.ctx().accesskit_node_builder(field.id, |node| node.set_label(FIELD_NAME));
            let entered = field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            if entered {
                // The field keeps the focus, so the next reason can follow.
                field.request_focus();
            }
            let added = ui.with_layout(Layout::right_to_left(Align::Center), |ui| enabled_when(ui, refused.is_none(), why, |ui| small_button(ui, ADD))).inner;
            (added.clicked(), entered)
        })
        .inner;
    if full {
        ui.add_space(ADD_PADDING);
        ui.label(rich(ui, why, Type::NOTE, Ink::Muted));
    }
    ui.add_space(ADD_PADDING);
    if (added || entered) && settings.add_skip_reason(typed).is_ok() {
        typed.clear();
    }
}
