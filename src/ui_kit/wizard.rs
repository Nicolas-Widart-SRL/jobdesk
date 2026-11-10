//! The first-run wizard's parts: one centred card per step under the step dots, the setup stamp in its head, the lines a
//! step works through, the ways a welcome offers to go, and the alert when a step fails.

use egui::{Align, Color32, Frame, Label, Layout, Margin, Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::controls::{SweepForm, live_sweep, paint_pick_edge, pick_look, pointer_over, pressed};
use crate::ui_kit::marks::{KnotState, day_stamp, knot, paint_tag, slip_bar, tag, tag_galley};
use crate::ui_kit::palette::{Ink, Tone};
use crate::ui_kit::surfaces::{Choice, divider, folder_fill, footed, paint_choice, paper, wizard_scope};
use crate::ui_kit::text::{add_text, copyable, cut_hover, line, paint_text, rich, wrapped, wrapped_height};
use crate::ui_kit::tokens::{
    BAR_HEIGHT, BODY_PADDING, BUTTON_GAP, BUTTON_HEIGHT, DEAL_FOOT_PADDING, DEAL_PADDING, FOCUS_RING, GUTTER, HAIRLINE, HEAD_GAP, INSET, ITEM_GAP, KNOT_SIZE,
    MARK_RULE, NOTE_PADDING, RADIO, RADIUS_PANEL, ROW_GAP, ROW_ITEM_GAP, ROW_PADDING, SETTING_CHECK_GAP, STATUS_DOT, TAG_HEIGHT, TAG_PADDING, Type,
};

/// The room between a choice card's title and its sentence.
const CHOICE_LINE_GAP: f32 = 2.0;

/// The setup stamp in a wizard card's head: the day stamp's rings with one tick per step, lit as each is done, and
/// "Step 2 of 4" inside. `at` is the open step, counting from 0. The welcome and the recap show the date in `day_stamp`.
pub fn setup_stamp(ui: &mut Ui, at: usize, steps: usize) -> Response {
    let step = (at + 1).to_string();
    day_stamp(ui, "Step", &step, &format!("of {steps}"), steps, at, &format!("Step {step} of {steps}"))
}

/// Where a wizard stands: one `STATUS_DOT` square per step with its name under it, joined by a hairline. The steps before
/// `at` are done, in `text`. The open one is ringed in `text`, its name in `text` too. The rest are unlit, in `off`, and
/// joined in `line`. A screen reader hears the open step, such as "Step 2 of 4, Computer".
pub fn step_dots(ui: &mut Ui, steps: &[&str], at: usize) -> Response {
    let p = Palette::of(ui);
    let names: Vec<_> = steps
        .iter()
        .enumerate()
        .map(|(index, name)| if index == at { line(ui, name, Type::TAG, p.text, f32::INFINITY) } else { line(ui, name, Type::SMALL, p.muted, f32::INFINITY) })
        .collect();
    // Each step is as wide as the longest name and a gutter either side, so the dots sit evenly apart.
    let pitch = names.iter().map(|name| name.size().x).fold(0.0, f32::max) + f32::from(GUTTER) * 2.0;
    let ring = STATUS_DOT + (FOCUS_RING + HAIRLINE) * 2.0;
    let name_height = names.iter().map(|name| name.size().y).fold(0.0, f32::max);
    let size = vec2(pitch * steps.len() as f32, ring + ITEM_GAP / 2.0 + name_height);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let y = ui.painter().round_to_pixel_center(rect.top() + ring / 2.0);
    let centre = |index: usize| rect.left() + pitch * (index as f32 + 0.5);
    // A join stops at the open step's ring, so no line runs between the ring and its dot.
    let half = |index: usize| if index == at { ring / 2.0 } else { STATUS_DOT / 2.0 };
    for index in 1..steps.len() {
        let ink = if index <= at { p.text } else { p.line };
        ui.painter().hline((centre(index - 1) + half(index - 1))..=(centre(index) - half(index)), y, Stroke::new(HAIRLINE, ink));
    }
    for (index, name) in names.into_iter().enumerate() {
        let dot = Rect::from_center_size(pos2(centre(index), y), Vec2::splat(STATUS_DOT));
        ui.painter().rect_filled(dot, 0.0, if index <= at { p.text } else { p.off });
        if index == at {
            ui.painter().rect_stroke(dot.expand(FOCUS_RING), 0.0, Stroke::new(HAIRLINE, p.text), StrokeKind::Outside);
        }
        let ink = if index == at { p.text } else { p.muted };
        let place = pos2(centre(index) - name.size().x / 2.0, rect.top() + ring + ITEM_GAP / 2.0);
        paint_text(ui, place, name, ink, Color32::TRANSPARENT);
    }
    let spoken = match steps.get(at) {
        Some(name) => format!("Step {} of {}, {name}", at + 1, steps.len()),
        None => format!("{} steps", steps.len()),
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, &spoken));
    response
}

/// A pickable card for one of a few ways to go, such as the welcome's Set up from scratch: a radio button, then `title`
/// with an optional tag beside it, over `sentence` in `muted`. Picked, it takes the pick look: the `raised` fill in a
/// `text` edge. The card is one control: a click anywhere on it counts, Tab stops on it once with the focus ring round
/// the radio, and a screen reader hears one radio button named `title`, with the tag and `sentence` as its description.
/// The caller picks the value when the response is clicked, and the dot shows on from that frame.
pub fn choice_card(ui: &mut Ui, on: bool, title: &str, tag: Option<(&str, Tone)>, sentence: &str) -> Response {
    let p = Palette::of(ui);
    let full = ui.available_width();
    let (x, y) = (f32::from(INSET), f32::from(BODY_PADDING));
    let lead = RADIO + SETTING_CHECK_GAP;
    let room = (full - x * 2.0 - lead).max(0.0);
    let tagged = tag.map(|(text, tone)| (tone, tag_galley(ui, text, tone)));
    let tag_width = tagged.as_ref().map_or(0.0, |(_, galley)| galley.size().x + TAG_PADDING * 2.0);
    let titled = line(ui, title, Type::STRONG, p.text, (room - tag_width - ROW_GAP).max(0.0));
    let said = wrapped(ui, sentence, Type::NOTE, p.muted, room);
    // The radio and the tag sit on the title's line, which a tag makes as tall as the tag.
    let first_line = titled.size().y.max(if tagged.is_some() { TAG_HEIGHT } else { 0.0 });
    let height = y * 2.0 + first_line + CHOICE_LINE_GAP + said.size().y;
    let (rect, response) = ui.allocate_exact_size(vec2(full, height), Sense::click());
    let pick = pick_look(&p, on, pointer_over(&response), pressed(&response));
    let fill = if pick.fill == Color32::TRANSPARENT { p.card } else { pick.fill };
    ui.painter().rect(rect, RADIUS_PANEL, fill, Stroke::new(HAIRLINE, pick.edge.unwrap_or(p.line)), StrokeKind::Inside);
    let (left, top) = (rect.left() + x + lead, rect.top() + y);
    let title_at = pos2(left, top + (first_line - titled.size().y) / 2.0);
    let title_right = title_at.x + titled.size().x;
    let mut response = cut_hover(response, &titled, title);
    paint_text(ui, title_at, titled, p.text, fill);
    if let Some((tone, galley)) = tagged {
        // The tag keeps the paper's fill, so it reads the same on a picked card.
        let place = Rect::from_min_size(pos2(title_right + ROW_GAP, top + (first_line - TAG_HEIGHT) / 2.0), vec2(tag_width, TAG_HEIGHT));
        paint_tag(ui, place, galley, tone, p.card);
    }
    paint_text(ui, pos2(left, top + first_line + CHOICE_LINE_GAP), said, p.muted, fill);
    if response.clicked() && !on {
        response.mark_changed();
    }
    let on = on || response.clicked();
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, ui.is_enabled(), on, title));
    let description = match tag {
        Some((text, _)) => format!("{text}. {sentence}"),
        None => sentence.to_owned(),
    };
    ui.ctx().accesskit_node_builder(response.id, |node| node.set_description(description));
    let radio = Rect::from_center_size(pos2(rect.left() + x + RADIO / 2.0, top + first_line / 2.0), Vec2::splat(RADIO));
    paint_choice(ui, &response, Choice::Radio, radio, on);
    response
}

/// What closes a setup line at its right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineEnd<'a> {
    Nothing,
    /// A word in `muted`, such as Next on a line not reached yet.
    Note(&'a str),
    Tag(&'a str, Tone),
}

/// One line of a setup step, such as a helper that installs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetupLine<'a> {
    pub knot: &'a str,
    pub state: KnotState,
    pub title: &'a str,
    /// The line under the title, in `muted`, or in `closed` on a failed line, where it says why. Empty leaves it out.
    pub under: &'a str,
    pub end: LineEnd<'a>,
    /// A row under the words: a bar at the share done, then a muted note, such as the time left.
    pub bar: Option<(f32, &'a str)>,
    /// The live sweep along the line's bottom edge, on the line that works now: its form and the seconds into it.
    pub live: Option<(SweepForm, f64)>,
}

/// A line of a setup step on paper: its knot, its title over the line under it, and a tag or a note at the right, with an
/// optional bar row under the words. A line still ahead reads in `muted`. Ruled off from the line above unless `first`.
/// Returns whether its live sweep moves: the screen asks for frames only then.
pub fn setup_line(ui: &mut Ui, setup: &SetupLine, first: bool) -> bool {
    if !first {
        divider(ui);
    }
    let p = Palette::of(ui);
    let shown = Frame::new().inner_margin(Margin::symmetric(INSET, ROW_PADDING)).show(ui, |ui| {
        let full = ui.available_width();
        let origin = ui.cursor().min;
        let end = match setup.end {
            LineEnd::Nothing => None,
            LineEnd::Note(text) => Some(line(ui, text, Type::NOTE, p.muted, f32::INFINITY).size()),
            LineEnd::Tag(text, _) => Some(vec2(line(ui, text, Type::TAG, p.text, f32::INFINITY).size().x + TAG_PADDING * 2.0, TAG_HEIGHT)),
        };
        let lead = KNOT_SIZE + f32::from(GUTTER);
        let room = (full - lead - end.map_or(0.0, |size| size.x + f32::from(GUTTER))).max(0.0);
        let id = ui.next_auto_id();
        let depth = ui.available_height().max(KNOT_SIZE);
        let child = |ui: &mut Ui, rect: Rect, salt: &str, layout: Layout| ui.new_child(UiBuilder::new().id_salt(id.with(salt)).max_rect(rect).layout(layout));

        let mut words = child(ui, Rect::from_min_size(origin + vec2(lead, 0.0), vec2(room, depth)), "setup_words", Layout::top_down(Align::Min));
        words.spacing_mut().item_spacing.y = 0.0;
        if setup.state == KnotState::Ahead {
            add_text(&mut words, setup.title, Type::BODY, Ink::Muted);
        } else {
            add_text(&mut words, setup.title, Type::STRONG, Ink::Text);
        }
        if !setup.under.is_empty() {
            let ink = if setup.state == KnotState::Failed { Ink::Status(Tone::Closed) } else { Ink::Muted };
            add_text(&mut words, setup.under, Type::NOTE, ink);
        }
        let height = words.min_rect().height().max(KNOT_SIZE);

        let mut knotted = child(ui, Rect::from_min_size(origin, vec2(lead, KNOT_SIZE)), "setup_knot", Layout::left_to_right(Align::Min));
        knot(&mut knotted, setup.knot, setup.state, KNOT_SIZE);
        if let Some(size) = end {
            let rect = Rect::from_min_size(pos2(origin.x + full - size.x, origin.y + (height - size.y) / 2.0), size);
            let mut ended = child(ui, rect, "setup_end", Layout::left_to_right(Align::Min));
            match setup.end {
                LineEnd::Note(text) => {
                    add_text(&mut ended, text, Type::NOTE, Ink::Muted);
                }
                LineEnd::Tag(text, tone) => {
                    tag(&mut ended, text, tone);
                }
                LineEnd::Nothing => {}
            }
        }
        let mut bottom = origin.y + height;
        if let Some((fraction, note)) = setup.bar {
            let top = bottom + ITEM_GAP;
            let tall = line(ui, note, Type::NOTE, p.muted, f32::INFINITY).size().y.max(BAR_HEIGHT);
            let rect = Rect::from_min_max(pos2(origin.x + lead, top), pos2(origin.x + full, top + tall));
            let mut barred = child(ui, rect, "setup_bar", Layout::right_to_left(Align::Center));
            barred.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            add_text(&mut barred, note, Type::NOTE, Ink::Muted);
            slip_bar(&mut barred, fraction, Ink::Text);
            bottom = rect.bottom();
        }
        ui.allocate_rect(Rect::from_min_max(origin, pos2(origin.x + full, bottom)), Sense::hover());
    });
    setup.live.is_some_and(|(form, seconds)| {
        let rect = shown.response.rect;
        live_sweep(ui, Rect::from_min_max(pos2(rect.left(), rect.bottom() - MARK_RULE), rect.max), form, seconds)
    })
}

/// Something in a step went wrong, on a slip: `title` in `closed` over `detail`, with a `closed` edge at its leading side.
/// Its words can be selected and copied, as an alert's can. On a wizard card it takes `folder_fill`.
pub fn alert_slip(ui: &mut Ui, title: &str, detail: &str) -> Response {
    let p = Palette::of(ui);
    let (x, y) = NOTE_PADDING;
    let shown = Frame::new().fill(folder_fill(ui)).corner_radius(RADIUS_PANEL).inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 2.0;
        copyable(ui, |ui| {
            add_text(ui, title, Type::STRONG, Tone::Closed);
            if !detail.is_empty() {
                add_text(ui, detail, Type::BODY, Ink::Text);
            }
        });
    });
    paint_pick_edge(ui, shown.response.rect, RADIUS_PANEL, p.closed);
    shown.response
}

/// What a wizard card shows of its step: the steps' names and the open one, counting from 0, then its title and lead.
#[derive(Debug, Clone, Copy)]
pub struct WizardStep<'a> {
    pub steps: &'a [&'a str],
    pub at: usize,
    pub title: &'a str,
    pub lead: &'a str,
}

/// One step of the first-run wizard: `step_dots`, then a card centred under them, at most `width` wide, that takes the
/// height left. Its head holds the setup stamp beside the title and the lead. `body` scrolls over the foot, pinned at
/// the bottom under a hairline. In the foot, `actions` lay out from the left: the one primary, then a plain button or a
/// `link`. `why` is the foot's one muted line at the right, such as when a disabled primary opens; empty leaves it out.
/// On the card, the folder plane takes `folder_fill`: slips, alerts and knots still ahead stay apart from it in dark.
pub fn wizard_card<R>(ui: &mut Ui, step: &WizardStep, width: f32, body: impl FnOnce(&mut Ui) -> R, actions: impl FnOnce(&mut Ui), why: &str) -> R {
    ui.vertical_centered(|ui| step_dots(ui, step.steps, step.at));
    let room = ui.available_rect_before_wrap();
    let card_width = width.min(room.width());
    let rect = Rect::from_min_size(pos2(room.center().x - card_width / 2.0, room.top()), vec2(card_width, room.height()));
    // Keyed on the title, so each step's card keeps its own scroll, and two cards on one sheet never share an id.
    ui.scope_builder(wizard_scope().id_salt(("wizard_card", step.title)).max_rect(rect).layout(Layout::top_down(Align::Min)), |ui| {
        paper(ui, true, |ui| {
            let (x, y) = DEAL_PADDING;
            Frame::new().inner_margin(Margin { left: x, right: x, top: y, bottom: GUTTER }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                wizard_head(ui, step);
            });
            let body = |ui: &mut Ui| {
                Frame::new()
                    .inner_margin(Margin { left: x, right: x, top: 0, bottom: BODY_PADDING })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = f32::from(GUTTER);
                        body(ui)
                    })
                    .inner
            };
            footed(ui, true, false, body, |ui| wizard_foot(ui, actions, why))
        })
    })
    .inner
}

/// The setup stamp, then the step's title over its lead.
fn wizard_head(ui: &mut Ui, step: &WizardStep) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = HEAD_GAP.0;
        setup_stamp(ui, step.at, step.steps.len());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            add_text(ui, step.title, Type::CALL_TITLE, Ink::Text);
            add_text(ui, step.lead, Type::BODY, Ink::Muted);
        });
    });
}

/// The card's foot, as the deal footer: the actions from the left, then the muted line at the right, centred on them.
fn wizard_foot(ui: &mut Ui, actions: impl FnOnce(&mut Ui), why: &str) {
    divider(ui);
    let (x, y) = DEAL_FOOT_PADDING;
    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = BUTTON_GAP;
            actions(ui);
            if why.is_empty() {
                return;
            }
            ui.add_space(f32::from(GUTTER));
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                let height = wrapped_height(ui, why, Type::NOTE, ui.available_width());
                ui.add_space(((BUTTON_HEIGHT - height) / 2.0).max(0.0));
                ui.add(Label::new(rich(ui, why, Type::NOTE, Ink::Muted)).halign(Align::Max).wrap());
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use egui::Theme;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};

    use super::*;

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    const WAYS: [(&str, &str); 2] = [
        ("Set up from scratch", "Jobdesk makes your job search folder and builds your profile."),
        ("I already have an ai-job-search folder", "Choose it, and Jobdesk opens it as it is."),
    ];

    /// Two choice cards, the first picked, that pick whichever is clicked.
    fn ways() -> (Harness<'static>, Rc<Cell<usize>>) {
        let picked = Rc::new(Cell::new(0));
        let seen = Rc::clone(&picked);
        let harness = Harness::builder().with_size([600.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            for (index, (title, sentence)) in WAYS.into_iter().enumerate() {
                let tag = (index == 0).then_some(("Recommended", Tone::Plain));
                if choice_card(ui, seen.get() == index, title, tag, sentence).clicked() {
                    seen.set(index);
                }
            }
        });
        (harness, picked)
    }

    /// The card is one radio button named by its title, with its sentence as the description and no node of its own.
    #[test]
    fn a_choice_card_is_one_radio_named_by_its_title() {
        let (mut harness, _) = ways();
        harness.run();
        let radios: Vec<_> = harness.query_all_by_role(egui::accesskit::Role::RadioButton).collect();
        assert_eq!(radios.len(), 2);
        let first = harness.get_by_label(WAYS[0].0);
        assert_eq!(first.accesskit_node().role(), egui::accesskit::Role::RadioButton);
        assert_eq!(first.accesskit_node().description().as_deref(), Some(format!("Recommended. {}", WAYS[0].1).as_str()), "the tag leads");
        let second = harness.get_by_label(WAYS[1].0);
        assert_eq!(second.accesskit_node().description().as_deref(), Some(WAYS[1].1));
        assert!(harness.query_by_label_contains(WAYS[1].1).is_none(), "the sentence is no node of its own");
        assert!(harness.query_by_label("Recommended").is_none(), "nor is the tag");
    }

    /// A click on a card's words picks it, and Space picks a card that has the focus.
    #[test]
    fn a_click_anywhere_on_a_choice_card_picks_it() {
        let (mut harness, picked) = ways();
        harness.run();
        // On the sentence, near the card's bottom left.
        let words = harness.get_by_label(WAYS[1].0).rect().left_bottom() + egui::vec2(60.0, -16.0);
        for pressed in [true, false] {
            harness.input_mut().events.push(egui::Event::PointerButton {
                pos: words,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
        assert_eq!(picked.get(), 1, "the second card is picked");
        let toggled = |harness: &Harness, title: &str| harness.get_by_label(title).accesskit_node().toggled() == Some(egui::accesskit::Toggled::True);
        assert!(toggled(&harness, WAYS[1].0) && !toggled(&harness, WAYS[0].0), "it reads as picked, and the other not");

        harness.get_by_label(WAYS[0].0).focus();
        harness.run();
        harness.key_press(egui::Key::Space);
        harness.run();
        assert_eq!(picked.get(), 0, "Space picks the focused card");
    }

    #[test]
    fn the_step_dots_and_the_stamp_say_the_open_step() {
        let mut harness = Harness::builder().with_size([600.0, 300.0]).with_context_setup(setup).build_ui(|ui| {
            step_dots(ui, &["Folder", "Computer", "Profile", "Done"], 1);
            setup_stamp(ui, 1, 4);
        });
        harness.run();
        assert!(harness.query_by_label("Step 2 of 4, Computer").is_some());
        assert!(harness.query_by_label("Step 2 of 4").is_some());
        assert!(harness.query_by_label("Folder").is_none(), "the names are painted, under the one label");
    }

    /// In dark, where the folder plane and the card are one colour, a slip on a wizard card takes the desk's base.
    /// Anywhere else, and in light, it keeps the folder plane.
    #[test]
    fn the_folder_plane_on_a_wizard_card_takes_the_sheet_in_dark() {
        for theme in [Theme::Dark, Theme::Light] {
            let fills = Rc::new(Cell::new((Color32::TRANSPARENT, Color32::TRANSPARENT)));
            let seen = Rc::clone(&fills);
            let mut harness = Harness::builder().with_size([600.0, 500.0]).with_theme(theme).with_context_setup(setup).build_ui(move |ui| {
                let outside = folder_fill(ui);
                let step = WizardStep { steps: &["Folder", "Computer"], at: 0, title: "Where should your job search live?", lead: "" };
                let inside = wizard_card(ui, &step, 500.0, |ui| folder_fill(ui), |_| {}, "");
                seen.set((outside, inside));
            });
            harness.run();
            let p = Palette::for_theme(theme);
            let (outside, inside) = fills.get();
            assert_eq!(outside, p.panel, "{theme:?}: off the card");
            assert_eq!(inside, if theme == Theme::Dark { p.sheet } else { p.panel }, "{theme:?}: on the card");
        }
    }
}
