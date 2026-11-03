use egui::{Label, Ui};

use crate::settings::application::{Page, Settings, TermsState, floor_filter_help, index, jobs_count_note};
use crate::settings::ui::defaults::typed;
use crate::ui_kit::tokens::NOTE_PADDING;
use crate::ui_kit::{Ink, Type, rich, ruled_panel, setting_check, setting_check_number, setting_section};

/// How many characters the score's field takes: 100 at most.
const SCORE_DIGITS: usize = 3;

/// What the Jobs list leaves out: how many jobs it hides, the checks a job can fail, the score under which it hides
/// jobs, and the safety check. They hide rows only, so no job changes its list. `jobs` is `(hidden, total)`, None until
/// the saved jobs are read, and `score` holds the score as typed while its field has the focus.
pub fn body(ui: &mut Ui, settings: &mut Settings, terms: TermsState, jobs: Option<(usize, usize)>, score: &mut Option<String>, query: &str) {
    let &[fail, floor, far, far_help, language, language_help, score_title, under, under_help, flagged, flagged_help] = index(Page::JobsView) else {
        return;
    };
    let filters = settings.preferences().jobs_view;
    let (x, y) = NOTE_PADDING;
    ruled_panel(ui, (x, y, y), |ui| {
        ui.add(Label::new(rich(ui, jobs_count_note(&filters, jobs), Type::BODY, Ink::Text)).wrap());
    });
    setting_section(ui, fail, None, query);
    let mut on = filters.below_floor;
    if setting_check(ui, &mut on, floor, &floor_filter_help(terms), query).changed() {
        settings.set_hide_below_floor(on);
    }
    let mut on = filters.too_far;
    if setting_check(ui, &mut on, far, far_help, query).changed() {
        settings.set_hide_too_far(on);
    }
    let mut on = filters.language;
    if setting_check(ui, &mut on, language, language_help, query).changed() {
        settings.set_hide_language(on);
    }
    setting_section(ui, score_title, None, query);
    let mut on = filters.under_score;
    let mut text = score.clone().unwrap_or_else(|| filters.score.to_string());
    let (check, field) = setting_check_number(ui, &mut on, &mut text, under, "", under_help, SCORE_DIGITS, query);
    if check.changed() {
        settings.set_hide_under_score(on);
    }
    // A score saves once it reads as one from 0 to 100. Anything else stays in the field while it has the focus.
    if let Some(number) = typed(score, text, &field).and_then(|text| text.trim().parse::<u8>().ok()).filter(|&number| number <= 100) {
        settings.set_score_floor(number);
    }
    let mut on = filters.flagged;
    if setting_check(ui, &mut on, flagged, flagged_help, query).changed() {
        settings.set_hide_flagged(on);
    }
}
