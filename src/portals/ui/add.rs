use egui::{Align, Frame, Label, Layout, Margin, Response, ScrollArea, Ui, vec2};

use crate::portals::application::{AddError, AddField, add_portal_line};
use crate::ui_kit::tokens::{ITEM_GAP, KNOT_SIZE, ROW_ITEM_GAP, SCREEN_GAP, Type};
use crate::ui_kit::{
    Ink, KnotState, Tone, desk_label, divider, enabled_when, knot, panel_body, panel_header, paper, primary_command_action, reveal_command, rich, slip,
    text_field,
};

/// The form's widest, and the slip beside it, from the canvas.
const FORM_WIDTH: f32 = 640.0;
const SLIP_WIDTH: f32 = 320.0;
/// The room inside the form's paper.
const FORM_PADDING: (i8, i8) = (22, 18);
/// The room between the form's fields.
const FIELD_GAP: f32 = 12.0;
/// The most characters each field takes.
const WEBSITE_CHARS: usize = 300;
const ANSWER_CHARS: usize = 100;
/// What the website's field says when its address holds a space.
const SPACE_IN_ADDRESS: &str = "A web address has no spaces. Type it as it shows in your browser, such as https://jobs.example.be/jobs.";
/// What the website's field says when it holds no address.
const NOT_AN_ADDRESS: &str = "This is not a web address. Type it as it shows in your browser, such as https://jobs.example.be.";

/// What the user typed into Add a portal, kept while the screen is away.
#[derive(Debug, Default)]
pub(super) struct AddForm {
    website: String,
    country: String,
    language: String,
    title: String,
    /// The user hid the command under Build it with Claude…. It shows until then.
    command_hidden: bool,
}

/// The form on paper beside what happens next on a slip. Build it with Claude… only opens the launcher on
/// `/add-portal` with the answers on its line: nothing runs until the user starts it there.
pub(super) fn add(ui: &mut Ui, form: &mut AddForm) {
    // A short window scrolls the form rather than cut it.
    ScrollArea::vertical().id_salt("add_portal").auto_shrink([false, false]).show(ui, |ui| form_and_steps(ui, form));
}

fn form_and_steps(ui: &mut Ui, form: &mut AddForm) {
    let room = ui.available_width();
    let width = FORM_WIDTH.min(room - SLIP_WIDTH - SCREEN_GAP).max(FORM_WIDTH.min(room) / 2.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = SCREEN_GAP;
        ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            paper(ui, false, |ui| {
                Frame::new().inner_margin(Margin::symmetric(FORM_PADDING.0, FORM_PADDING.1)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = FIELD_GAP;
                    fields(ui, form);
                });
            });
        });
        ui.allocate_ui_with_layout(vec2(SLIP_WIDTH.min(ui.available_width()).max(0.0), 0.0), Layout::top_down(Align::Min), next_steps);
    });
}

fn fields(ui: &mut Ui, form: &mut AddForm) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        desk_label(ui, "A job board Claude should search", Type::SLIP_TITLE, Ink::Text);
        wrapped(ui, "Claude builds a small search tool for it and tests it with your job title. What you fill in here, it does not ask again.", Type::BODY);
    });
    let typed = add_portal_line(&form.website, &form.country, &form.language, &form.title);
    let line = typed.as_deref().map_err(|error| *error);
    let full = ui.available_width();
    field(ui, "The job board’s website", false, |ui, name| {
        let input = text_field(ui, &mut form.website, full, "Such as https://jobs.example.be", WEBSITE_CHARS);
        name.tie(&input);
        match line {
            Err(AddError::NotWebAddress) => {
                desk_label(ui, NOT_AN_ADDRESS, Type::TABLE_HEAD, Tone::Closed);
            }
            Err(AddError::SpaceInWebsite) => {
                desk_label(ui, SPACE_IN_ADDRESS, Type::TABLE_HEAD, Tone::Closed);
            }
            Err(AddError::Hidden(AddField::Website)) => {
                desk_label(ui, hidden_words(AddField::Website), Type::TABLE_HEAD, Tone::Closed);
            }
            _ => wrapped(ui, "The page where the board lists its jobs.", Type::NOTE),
        }
    });
    let half = (full - FIELD_GAP) / 2.0;
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = FIELD_GAP;
        ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
            field(ui, "Country", true, |ui, name| answer(ui, name, &mut form.country, half, "Such as Belgium", AddField::Country, line))
        });
        ui.allocate_ui_with_layout(vec2(half, 0.0), Layout::top_down(Align::Min), |ui| {
            field(ui, "Language of its job ads", true, |ui, name| answer(ui, name, &mut form.language, half, "Such as French", AddField::Language, line))
        });
    });
    field(ui, "One job title to test with", true, |ui, name| {
        answer(ui, name, &mut form.title, full, "Such as Lead Backend Engineer", AddField::TestTitle, line);
        wrapped(
            ui,
            "Claude searches the board for it, to check the new portal finds real jobs. Leave a field empty and Claude asks you in the task.",
            Type::NOTE,
        );
    });
    divider(ui);
    // Read again, so what was typed on this frame reaches the button on this frame.
    let line = add_portal_line(&form.website, &form.country, &form.language, &form.title);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = ITEM_GAP;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            let why = line.as_ref().err().map(|error| waiting_words(*error));
            enabled_when(ui, line.is_ok(), why.as_deref().unwrap_or_default(), |ui| {
                primary_command_action(ui, "Build it with Claude…", line.as_deref().unwrap_or_default())
            });
            if let Some(why) = &why {
                desk_label(ui, why.as_str(), Type::NOTE, Ink::Muted);
            }
        });
        if let Ok(line) = &line {
            let mut open = !form.command_hidden;
            reveal_command(ui, line, &mut open);
            form.command_hidden = !open;
        }
    });
    wrapped(ui, "This is a task, so it costs like any other and shows in Usage. Nothing runs until you start it.", Type::NOTE);
}

/// The words above a field: its label, and Optional when it may stay empty.
struct FieldName {
    words: Vec<egui::Id>,
}

impl FieldName {
    /// Ties `input` to the words above it, so a screen reader names it as the screen does, such as "Country Optional".
    fn tie(&self, input: &Response) {
        for words in &self.words {
            let _ = input.clone().labelled_by(*words);
        }
    }
}

/// A field's label, with Optional after it when it may stay empty, over what `add` puts under it. `add` takes the
/// field's name, to tie its input to the label.
fn field(ui: &mut Ui, label: &str, optional: bool, add: impl FnOnce(&mut Ui, &FieldName)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let words = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ITEM_GAP;
            let mut words = vec![desk_label(ui, label, Type::TABLE_HEAD, Ink::Text).id];
            if optional {
                words.push(desk_label(ui, "Optional", Type::SMALL, Ink::Muted).id);
            }
            words
        });
        add(ui, &FieldName { words: words.inner });
    });
}

/// An optional answer's field, and why it waits when it holds a character that shows nothing.
fn answer(ui: &mut Ui, name: &FieldName, text: &mut String, width: f32, hint: &str, which: AddField, line: Result<&str, AddError>) {
    let input = text_field(ui, text, width, hint, ANSWER_CHARS);
    name.tie(&input);
    if line == Err(AddError::Hidden(which)) {
        desk_label(ui, hidden_words(which), Type::TABLE_HEAD, Tone::Closed);
    }
}

/// Why Build it with Claude… waits.
fn waiting_words(error: AddError) -> String {
    match error {
        AddError::NoWebsite => "Type the board’s website first.".to_owned(),
        AddError::NotWebAddress => "Type a web address first.".to_owned(),
        AddError::SpaceInWebsite => "Take the spaces out of the address first.".to_owned(),
        AddError::Hidden(which) => hidden_words(which),
    }
}

/// A field that holds a character that shows nothing, which Claude would read though the user never saw it.
fn hidden_words(which: AddField) -> String {
    let field = match which {
        AddField::Website => "website",
        AddField::Country => "country",
        AddField::Language => "language",
        AddField::TestTitle => "job title",
    };
    format!("The {field} holds characters that do not show. Type it again by hand.")
}

fn wrapped(ui: &mut Ui, text: &str, style: Type) {
    ui.add(Label::new(rich(ui, text, style, Ink::Muted)).wrap());
}

/// What happens once the user builds it: three numbered steps, then a line on boards behind a sign-in.
fn next_steps(ui: &mut Ui) {
    slip(ui, |ui| {
        panel_header(ui, "What happens next", None);
        panel_body(ui, |ui| {
            let steps = [
                ("1", "New task opens", "With the board’s address and your answers filled in. You start it there."),
                (
                    "2",
                    "Claude builds and tests",
                    "It reads the board, writes the portal, and searches it for your job title. It asks you in Tasks only for what you left empty, or when it needs you.",
                ),
                ("3", "It joins Your portals", "Tagged Made by you. Turn it off there at any time."),
            ];
            for (number, title, sentence) in steps {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                    knot(ui, number, KnotState::Ahead, KNOT_SIZE);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        desk_label(ui, title, Type::STRONG, Ink::Text);
                        wrapped(ui, sentence, Type::NOTE);
                    });
                });
                divider(ui);
            }
            wrapped(ui, "Boards that need a sign-in or a paid key work too. Claude tells you how to set them up.", Type::NOTE);
        });
    });
}
