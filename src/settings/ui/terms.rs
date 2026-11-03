use egui::{Label, Ui};

use crate::settings::application::{
    CONTRACT_TERM, Commute, PROFILE_UNREADABLE, Page, TERMS_LINE, TermPart, TermsState, YourTerms, floor_words, index, plain, terms_failed_line, tier_title,
};
use crate::settings::ui::SettingsRequest;
use crate::ui_kit::tokens::SETTING_SECTION_GAP;
use crate::ui_kit::{
    Ink, Tone, Type, copyable, desk_label, empty_note, enabled_when, faded, faint_mono, prompt_box_tone, rich, ruled_note_with_command, setting_row,
    setting_section, small_button, small_command_action, tag,
};

const EDIT: &str = "Edit in Profile";
const UPDATE: &str = "Update with Claude…";
const NO_FOLDER: &str = "Choose a folder first.";
const READING_WAIT: &str = "Your profile is still being read.";
/// The lead while the profile is read, in place of the page's own.
pub const READING: &str = "Reading your profile…";
const NOTHING_YET: &str = "Your terms show here once you choose your job search folder.";
const FLOOR_HELP: &str = "A job that states a lower day rate fails this check.";
const NO_FLOOR: &str = "Your profile does not state one, so every job's day-rate check reads Unclear instead of guessing.";
const TIERS: &str = "Commute tiers";
/// Without tiers a fully remote job still passes the commute check, and a place the location check ruled out still fails it.
const NO_TIERS: &str =
    "Your profile does not state them, so the commute check reads Unclear for a job that is not fully remote, unless the location check ruled it out.";
const NOT_STATED: &str = "Not stated";
const COULD_NOT: &str = "Your terms could not be read.";
const HIDDEN_TITLE: &str = "Your terms are hidden, not gone";
const HIDDEN_BODY: &str = "Jobdesk changed nothing in the file. This page fills in again as soon as the file is fixed, because Jobdesk watches it.";
const ANOTHER_FILE: &str = "This part comes from another file, so it still shows.";
/// The files the terms are read from, as the foot names them.
const PROFILE_FILE: &str = "01-candidate-profile.md";
const EVALUATION_FILE: &str = "04-job-evaluation.md";
/// The most a value takes at the right of its row, so a long one never squeezes its label. It is cut, whole on hover.
const VALUE_WIDTH: f32 = 360.0;

/// Edit in Profile and Update with Claude…, which only opens the launcher. Both wait for a folder that is read, and
/// Update with Claude… waits too while a file it would write cannot be read. Returns what the user asked of the shell.
pub fn actions(ui: &mut Ui, state: TermsState) -> Option<SettingsRequest> {
    let folder_wait = match state {
        TermsState::NoFolder => Some(NO_FOLDER),
        TermsState::Reading => Some(READING_WAIT),
        TermsState::Read(_) => None,
    };
    let claude_wait = match state {
        TermsState::Read(terms) if !terms.failures().is_empty() => Some(PROFILE_UNREADABLE),
        _ => folder_wait,
    };
    // Laid out from the right, so Update with Claude… goes in first.
    enabled_when(ui, claude_wait.is_none(), claude_wait.unwrap_or_default(), |ui| small_command_action(ui, UPDATE, TERMS_LINE));
    let edit = enabled_when(ui, folder_wait.is_none(), folder_wait.unwrap_or_default(), |ui| small_button(ui, EDIT));
    edit.clicked().then_some(SettingsRequest::OpenProfile)
}

/// Your terms, read only: the contract, the floor and the commute tiers. A part that failed hides the page behind an
/// alert, and the parts still read show faded under it. Every word from the profile shows as plain text.
pub fn body(ui: &mut Ui, state: TermsState, show_command: &mut bool, query: &str) {
    let TermsState::Read(terms) = state else {
        if matches!(state, TermsState::NoFolder) {
            desk_label(ui, NOTHING_YET, Type::BODY, Ink::Muted);
        }
        return;
    };
    let &[note, contract_rate, contract, contract_help, floor, commute] = index(Page::Terms) else { return };
    let failures = terms.failures();
    if failures.is_empty() {
        ruled_note_with_command(ui, note, TERMS_LINE, show_command, query);
        contract_and_rate(ui, terms, [contract_rate, contract, contract_help, floor], query);
        commute_section(ui, terms, commute, None, query);
        return;
    }
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(rich(ui, COULD_NOT, Type::STRONG, Tone::Closed));
        // A reason quotes the system, so it shows as plain text with its hidden characters gone.
        for failure in &failures {
            let line = format!("{}: {}", plain(&failure.source), plain(&failure.reason));
            copyable(ui, |ui| ui.add(Label::new(rich(ui, line, Type::BODY, Ink::Text)).wrap()));
        }
        ui.add(Label::new(rich(ui, terms_failed_line(terms), Type::BODY, Ink::Muted)).wrap());
    });
    ui.add_space(SETTING_SECTION_GAP.0);
    empty_note(ui, HIDDEN_TITLE, HIDDEN_BODY);
    // The faded parts lay out in a scope of their own, where a section sees nothing above it, so the gap is added here.
    ui.add_space(SETTING_SECTION_GAP.0);
    faded(ui, |ui| {
        if matches!(terms.floor, TermPart::Read(_)) {
            contract_and_rate(ui, terms, [contract_rate, contract, contract_help, floor], query);
        }
        if matches!(terms.commute, TermPart::Read(_)) {
            commute_section(ui, terms, commute, Some(ANOTHER_FILE), query);
        }
    });
}

/// The contract, fixed in Jobdesk, and the floor from the profile, or Not stated.
fn contract_and_rate(ui: &mut Ui, terms: &YourTerms, [section, contract, contract_help, floor]: [&str; 4], query: &str) {
    setting_section(ui, section, None, query);
    setting_row(ui, contract, contract_help, query, |ui| value(ui, CONTRACT_TERM));
    match terms.floor {
        TermPart::Read(Some(amount)) => {
            setting_row(ui, floor, FLOOR_HELP, query, |ui| value(ui, &floor_words(amount)));
        }
        TermPart::Read(None) => {
            setting_row(ui, floor, NO_FLOOR, query, |ui| {
                tag(ui, NOT_STATED, Tone::Interview);
            });
        }
        TermPart::Failed(_) => {}
    }
}

/// Where the tiers are measured from, as the section's note, then one row per tier: its name, its note as the help, and
/// its places as the profile writes them. `more` follows the note, such as why a part shows while another failed.
fn commute_section(ui: &mut Ui, terms: &YourTerms, title: &str, more: Option<&str>, query: &str) {
    let TermPart::Read(Commute { tiers, .. }) = &terms.commute else { return };
    let origin = terms.origin_line().map(|line| plain(&line));
    let note = match (origin, more) {
        (Some(origin), Some(more)) => Some(format!("{origin} {more}")),
        (origin, more) => origin.or(more.map(str::to_owned)),
    };
    setting_section(ui, title, note.as_deref(), query);
    if tiers.is_empty() {
        setting_row(ui, TIERS, NO_TIERS, query, |ui| {
            tag(ui, NOT_STATED, Tone::Interview);
        });
        return;
    }
    for (at, tier) in tiers.iter().enumerate() {
        ui.push_id(("commute tier", at), |ui| {
            let places = tier.places.iter().map(|place| plain(place)).collect::<Vec<_>>().join(", ");
            setting_row(ui, &tier_title(&tier.name), &plain(&tier.note), query, |ui| value(ui, &places));
        });
    }
}

/// A value at the right of its row, in `Type::STRONG` with no field round it.
fn value(ui: &mut Ui, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(VALUE_WIDTH);
        ui.add(Label::new(rich(ui, text, Type::STRONG, Ink::Text)).truncate());
    });
}

/// Where the terms come from: the two files, in the job search folder.
pub fn foot(ui: &mut Ui) {
    desk_label(ui, "Read from ", Type::NOTE, Ink::Muted);
    faint_mono(ui, PROFILE_FILE);
    desk_label(ui, " and ", Type::NOTE, Ink::Muted);
    faint_mono(ui, EVALUATION_FILE);
    desk_label(ui, " in your job search folder", Type::NOTE, Ink::Muted);
}
