use egui::{Align, Frame, Label, Layout, Margin, Ui, vec2};

use crate::portals::application::{Origin, Phrase, PortalView, Priority, Quotes, SEARCH_TERMS_LINE, SearchPlan, site_lines_for, term_as_sent};
use crate::portals::ui::rail::{badge_word, day, origin_word, portal_switch};
use crate::portals::ui::screen::{BUILT_IN, BUSY, TRASH};
use crate::ui_kit::tokens::{ITEM_GAP, ROW_ITEM_GAP, SKELETON_LINE, Type};
use crate::ui_kit::{
    Confirm, Ink, Tone, alert, code_box, confirm_popover, desk_label, divider, enabled_when, headed_paper, hidden_note, lead_line, muted, path_label, rich,
    row_tag, skeleton, skeleton_lines, small_command_action, small_danger_button, tag,
};

/// The width of How it searches' left column.
const KEY_WIDTH: f32 = 170.0;
/// The room between the detail's sections, and above the first.
const SECTION_GAP: f32 = 18.0;
/// The room between the head's lines.
const HEAD_GAP: f32 = 10.0;
/// Remove…'s ask, as wide as its words need.
const CONFIRM_WIDTH: f32 = 330.0;
/// What a placeholder term says on hover.
const TO_FILL: &str = "This term still holds a word from the setup, in square brackets. Change search terms… fills it in.";

/// What the detail asked of the screen this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailAsk {
    Switch(bool),
    Remove,
}

/// What the detail shows: the portal, the search terms every portal shares and whether they could be read, whether a
/// run works, and why its last change failed, if it did.
pub(super) struct Detail<'a> {
    pub portal: &'a PortalView,
    pub plan: &'a SearchPlan,
    pub plan_read: bool,
    pub busy: bool,
    pub failure: Option<&'a str>,
}

/// The picked portal on paper: its head with the switch, its facts and actions, then how it searches and the searches
/// it will run, over the folder and the search file in the foot. `confirm` holds Remove…'s ask.
pub(super) fn detail(ui: &mut Ui, shown: &Detail, confirm: &mut Option<bool>) -> Option<DetailAsk> {
    let portal = shown.portal;
    let mut ask = None;
    headed_paper(
        ui,
        |ui| ask = head(ui, shown, confirm),
        |ui| {
            ui.add_space(SECTION_GAP);
            how(ui, portal.quotes);
            ui.add_space(SECTION_GAP);
            divider(ui);
            ui.add_space(SECTION_GAP);
            searches(ui, shown);
        },
        |ui| {
            desk_label(ui, "Kept in ", Type::NOTE, Ink::Faint);
            path_label(ui, ".agents/skills/", &portal.folder);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                path_label(ui, "job-scraper/", "search-queries.md");
                // A label's trailing space takes no room at the right of a right-to-left row.
                ui.add_space(ITEM_GAP / 2.0);
                desk_label(ui, "Search terms kept in", Type::NOTE, Ink::Faint);
            });
        },
    );
    ask
}

/// The name and the switch, the country and the sentence, the facts and tags, then the actions, and a failed change.
fn head(ui: &mut Ui, shown: &Detail, confirm: &mut Option<bool>) -> Option<DetailAsk> {
    let portal = shown.portal;
    let mut ask = None;
    ui.spacing_mut().item_spacing.y = HEAD_GAP;
    let width = ui.available_width();
    ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::right_to_left(Align::Min), |ui| {
        if portal_switch(ui, portal, shown.busy).clicked() {
            ask = Some(DetailAsk::Switch(!portal.on));
        }
        ui.add_space(ROW_ITEM_GAP);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add(Label::new(rich(ui, &portal.name, Type::DEAL, Ink::Text)).truncate());
            let about = match (&portal.country, &portal.sentence) {
                (Some(country), Some(sentence)) => Some(format!("{country}. {sentence}")),
                (Some(country), None) => Some(country.clone()),
                (None, sentence) => sentence.clone(),
            };
            if let Some(about) = about {
                ui.add(Label::new(rich(ui, about, Type::BODY, Ink::Muted)).wrap());
            }
        });
    });
    facts(ui, portal);
    if actions(ui, shown, confirm) {
        ask = Some(DetailAsk::Remove);
    }
    if let Some(failure) = shown.failure {
        alert(ui, failure);
    }
    ask
}

/// Jobs found, the last new job, and the tags. The jobs hide while the saved jobs cannot be read.
fn facts(ui: &mut Ui, portal: &PortalView) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        match portal.jobs {
            Some((0, _)) => {
                desk_label(ui, "No jobs yet", Type::NOTE, Ink::Muted);
            }
            Some((count, last)) => {
                desk_label(ui, count.to_string(), Type::STRONG, Ink::Text);
                desk_label(ui, if count == 1 { "job found" } else { "jobs found" }, Type::NOTE, Ink::Muted);
                if let Some(last) = last {
                    ui.add_space(ROW_ITEM_GAP);
                    desk_label(ui, "Last new job", Type::NOTE, Ink::Muted);
                    desk_label(ui, day(last), Type::STRONG, Ink::Text);
                }
            }
            None => {
                desk_label(ui, "Jobs found are hidden while your saved jobs cannot be read.", Type::NOTE, Ink::Muted);
            }
        }
        ui.add_space(ROW_ITEM_GAP);
        tag(ui, origin_word(portal.origin), Tone::Plain);
        for badge in &portal.badges {
            tag(ui, badge_word(*badge), Tone::Interview);
        }
    });
}

/// Check this portal… for a folder name safe on a command line, Change search terms…, and Remove…, which asks first.
/// A built-in portal has no Remove…, only its switch. Returns true on the frame the user answers Remove.
fn actions(ui: &mut Ui, shown: &Detail, confirm: &mut Option<bool>) -> bool {
    let portal = shown.portal;
    let busy = shown.busy;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = ITEM_GAP;
        if let Some(line) = portal.check_line() {
            enabled_when(ui, !busy, BUSY, |ui| small_command_action(ui, "Check this portal…", &line));
        }
        enabled_when(ui, !busy, BUSY, |ui| small_command_action(ui, "Change search terms…", SEARCH_TERMS_LINE));
        if portal.origin == Origin::BuiltIn {
            ui.add_space(ITEM_GAP);
            desk_label(ui, BUILT_IN, Type::NOTE, Ink::Muted);
            return false;
        }
        let remove = enabled_when(ui, !busy, BUSY, |ui| small_danger_button(ui, "Remove…"));
        if remove.clicked() {
            *confirm = Some(true);
        }
        let question = format!("Remove {}?", portal.name);
        let note = remove_note(portal);
        let words = Confirm { question: &question, note: &note, keep: "Keep it", act: "Remove", esc_note: "Esc keeps it." };
        // A run that starts while the ask is open closes it, as Remove waits too.
        if busy {
            *confirm = None;
        }
        confirm_popover(&remove, confirm, &words, CONFIRM_WIDTH)
    })
    .inner
}

/// What Remove… says it does: the folder goes to the Trash, or the Recycle Bin on Windows, and the jobs it found stay.
fn remove_note(portal: &PortalView) -> String {
    let jobs = match portal.jobs {
        Some((0, _)) => String::new(),
        Some((1, _)) => " The job it found stays in Jobs.".to_owned(),
        Some((count, _)) => format!(" The {count} jobs it found stay in Jobs."),
        None => " The jobs it found stay in Jobs.".to_owned(),
    };
    format!("Jobdesk moves its folder, .agents/skills/{}, to {TRASH}.{jobs} To use it again, put the folder back, or add it again.", portal.folder)
}

/// A section's title, with a note at its right.
fn section_title(ui: &mut Ui, title: &str, note: Option<&str>) {
    ui.horizontal(|ui| {
        desk_label(ui, title, Type::SECTION, Ink::Text);
        if let Some(note) = note {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| desk_label(ui, note, Type::NOTE, Ink::Muted));
        }
    });
}

/// How the portal reads a term of several words, from its `Query syntax:` line: the bold answer, then one sentence.
fn exact_phrases(quotes: Quotes) -> (&'static str, &'static str) {
    match quotes {
        Quotes::Phrase => ("Yes.", "A term of two or more words goes in quotes, so the words stay together."),
        Quotes::AllWords => ("No.", "A term of two or more words goes in quotes, so the board finds every word, in any order."),
        Quotes::Ignored => ("No.", "The board ignores quotes, so Claude leaves them out."),
        Quotes::Unknown => ("Not stated.", "The portal does not say how it reads quotes, so each term shows here as written."),
    }
}

/// How it searches, in plain words.
fn how(ui: &mut Ui, quotes: Quotes) {
    section_title(ui, "How it searches", None);
    ui.add_space(ITEM_GAP);
    let (answer, why) = exact_phrases(quotes);
    for (at, (key, answer, why)) in [("Exact phrases", answer, why), ("Combine with OR", "No.", "Each term is its own search.")].into_iter().enumerate() {
        if at > 0 {
            divider(ui);
        }
        Frame::new().inner_margin(Margin::symmetric(0, 8)).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(KEY_WIDTH, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(KEY_WIDTH);
                    desk_label(ui, key, Type::NOTE, Ink::Muted);
                });
                ui.vertical(|ui| lead_line(ui, answer, why));
            });
        });
    }
}

/// The words for how a term goes out.
fn phrase_word(phrase: Phrase) -> &'static str {
    match phrase {
        Phrase::OneWord => "One word",
        Phrase::Together => "These words together",
        Phrase::AnyOrder => "These words, in any order",
    }
}

/// A term as the portal receives it, and how it reads it. A portal that does not say how it reads quotes gets each term
/// as written, with no word on how it reads it.
fn shown_term(text: &str, quotes: Quotes) -> (String, Option<&'static str>) {
    if quotes == Quotes::Unknown {
        return (text.trim().to_owned(), None);
    }
    let (sent, phrase) = term_as_sent(text, quotes);
    (sent, Some(phrase_word(phrase)))
}

/// The shared search terms by priority, each as this portal receives it, then the Google lines that name its site.
fn searches(ui: &mut Ui, shown: &Detail) {
    let (portal, plan) = (shown.portal, shown.plan);
    let count: usize = plan.priorities.iter().map(|priority| priority.terms.len()).sum();
    let note = (shown.plan_read && count > 0).then(|| if count == 1 { "1 search".to_owned() } else { format!("{count} searches") });
    section_title(ui, "Searches it will run", note.as_deref());
    ui.add_space(ITEM_GAP);
    if !shown.plan_read {
        hidden_note(ui, "Your search terms are hidden while the search file cannot be read.");
        return;
    }
    if plan.priorities.is_empty() {
        muted(ui, "You have no search terms yet. Change search terms… sets them up.");
        return;
    }
    let quoted = matches!(portal.quotes, Quotes::Phrase | Quotes::AllWords);
    let lead = format!(
        "Your search terms are shared by every portal. This is how {} receives them: one search per term{}.",
        portal.name,
        if quoted { ", with a term of several words in quotes" } else { "" }
    );
    ui.add(Label::new(rich(ui, lead, Type::NOTE, Ink::Muted)).wrap());
    for priority in &plan.priorities {
        ui.add_space(ITEM_GAP * 2.0);
        priority_terms(ui, priority, portal.quotes);
    }
    ui.add_space(SECTION_GAP);
    if_it_fails(ui, portal, plan);
}

/// "Priority 1" and its name, then each term as sent with how the portal reads it.
fn priority_terms(ui: &mut Ui, priority: &Priority, quotes: Quotes) {
    ui.horizontal(|ui| {
        row_tag(ui, format!("Priority {}", priority.number), Tone::Plain);
        ui.add(Label::new(rich(ui, &priority.name, Type::TABLE_HEAD, Ink::Text)).truncate());
    });
    if priority.terms.is_empty() {
        ui.add_space(ITEM_GAP);
        desk_label(ui, "No plain terms here, so Claude takes this priority's terms from its Google lines.", Type::NOTE, Ink::Muted);
        return;
    }
    for term in &priority.terms {
        let (sent, word) = shown_term(&term.text, quotes);
        Frame::new().inner_margin(Margin::symmetric(0, 5)).show(ui, |ui| {
            let width = ui.available_width();
            ui.allocate_ui_with_layout(vec2(width, 0.0), Layout::right_to_left(Align::Center), |ui| {
                if let Some(word) = word {
                    desk_label(ui, word, Type::NOTE, Ink::Muted);
                }
                if term.placeholder {
                    row_tag(ui, "To fill", Tone::Interview).on_hover_text(TO_FILL);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(Label::new(rich(ui, sent, Type::BODY, Ink::Text)).truncate());
                });
            });
        });
        divider(ui);
    }
}

/// The `site:` lines that name this portal's site, which Claude searches Google with when the portal fails.
fn if_it_fails(ui: &mut Ui, portal: &PortalView, plan: &SearchPlan) {
    let lines = site_lines_for(plan, &portal.folder);
    if lines.is_empty() {
        desk_label(ui, format!("If {} fails", portal.name), Type::TABLE_HEAD, Ink::Text);
        ui.add_space(ITEM_GAP);
        ui.add(Label::new(rich(ui, "None of your Google lines name this board, so Claude only notes that it failed.", Type::NOTE, Ink::Muted)).wrap());
        return;
    }
    desk_label(ui, format!("If {} fails, Claude searches Google with", portal.name), Type::TABLE_HEAD, Ink::Text);
    ui.add_space(ITEM_GAP);
    code_box(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for line in lines {
            ui.add(Label::new(rich(ui, line, Type::MONO, Ink::Text)).wrap());
        }
    });
    ui.add_space(ITEM_GAP);
    desk_label(ui, "These are the Google lines in your search terms that name this board's site.", Type::NOTE, Ink::Muted);
}

/// The detail before the first read: a head and a section in grey lines.
pub(super) fn loading_detail(ui: &mut Ui) {
    headed_paper(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.y = HEAD_GAP;
            skeleton(ui, 180.0, SKELETON_LINE * 2.0);
            skeleton(ui, 360.0, SKELETON_LINE);
            skeleton(ui, 260.0, SKELETON_LINE);
        },
        |ui| {
            ui.add_space(SECTION_GAP);
            skeleton(ui, 160.0, SKELETON_LINE + 4.0);
            ui.add_space(ITEM_GAP);
            skeleton_lines(ui, &[520.0, 480.0, 500.0, 440.0], skeleton);
        },
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portal(jobs: Option<(usize, Option<chrono::NaiveDate>)>) -> PortalView {
        PortalView {
            folder: "northfield-search".into(),
            name: "Northfield Jobs".into(),
            country: None,
            sentence: None,
            on: true,
            origin: Origin::MadeByYou,
            badges: vec![],
            jobs,
            quotes: Quotes::Ignored,
            safe_name: true,
        }
    }

    #[test]
    fn a_portal_that_does_not_say_how_it_reads_quotes_shows_each_term_as_written() {
        assert_eq!(shown_term("Platform Lead", Quotes::Unknown), ("Platform Lead".to_owned(), None));
        assert_eq!(shown_term("\"Platform Lead\"", Quotes::Unknown), ("\"Platform Lead\"".to_owned(), None));
        assert_eq!(shown_term("Platform Lead", Quotes::Ignored), ("Platform Lead".to_owned(), Some("These words, in any order")));
        assert_eq!(shown_term("Platform Lead", Quotes::Phrase), ("\"Platform Lead\"".to_owned(), Some("These words together")));
        assert_eq!(shown_term("Rust", Quotes::Unknown).1, None, "one word gets no word either");
    }

    #[test]
    fn remove_says_where_the_folder_goes_and_that_the_jobs_stay() {
        assert_eq!(
            remove_note(&portal(Some((7, None)))),
            format!(
                "Jobdesk moves its folder, .agents/skills/northfield-search, to {TRASH}. The 7 jobs it found stay in Jobs. To use it again, put the folder back, or add it again."
            )
        );
        assert!(remove_note(&portal(Some((1, None)))).contains(" The job it found stays in Jobs. "));
        assert!(!remove_note(&portal(Some((0, None)))).contains("Jobs."), "no job to keep");
        assert!(remove_note(&portal(None)).contains(" The jobs it found stay in Jobs. "), "the count is unknown");
    }
}
