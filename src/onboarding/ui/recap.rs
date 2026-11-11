//! The recap once the Profile chapter is done, in the app: what setup made, the time it took, and Open Jobdesk. It shows
//! no spend. The facts come from the snapshot and the run, which the shell reads.

use chrono::NaiveDate;
use egui::{Align, Frame, Layout, Margin, Rect, Response, Ui, UiBuilder, pos2, vec2};

use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::{
    BUTTON_GAP, DEAL_FOOT_PADDING, DEAL_PADDING, GUTTER, HEAD_GAP, INSET, ITEM_GAP, KNOT_SIZE, ROW_PADDING, WELCOME_TOP, WELCOME_WIDTH,
};
use crate::ui_kit::{
    Ink, KnotState, Tone, Type, day_stamp, desk_label, divider, fat_tally, knot, link, paper, path_label, primary_button, tag, wrapped_height,
};

/// What the recap says, as plain facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recap {
    /// The job search folder as the screens show it, such as `Documents/Job search`.
    pub folder: String,
    /// LaTeX is not installed, so no PDF can be made yet.
    pub without_pdfs: bool,
    /// The profile's topics filled, of all of them.
    pub topics: (usize, usize),
    /// The CV PDF's file name, once there is one.
    pub cv: Option<String>,
    /// The names of the job sites switched on.
    pub sites: Vec<String>,
    /// The user's country, as the profile says it.
    pub country: Option<String>,
    /// How long Claude worked in setup's run, in seconds, as Tasks counts its Elapsed: the time spent waiting on the
    /// user is not in it.
    pub worked: u64,
    pub today: NaiveDate,
}

/// What the user asked for on the recap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecapRequest {
    ShowFolder,
    OpenPdf,
    /// Add a job site you use…: the launcher on `/add-portal`. Nothing runs until the user starts it there.
    AddPortal,
    OpenJobdesk,
}

const SHOW_FOLDER: &str = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" };

/// The recap, centred on the desk under the menu.
pub fn recap(ui: &mut Ui, facts: &Recap) -> Option<RecapRequest> {
    let mut asked = None;
    ui.add_space(WELCOME_TOP);
    let width = WELCOME_WIDTH.min(ui.available_width());
    let rect = Rect::from_min_size(pos2(ui.max_rect().center().x - width / 2.0, ui.cursor().top()), vec2(width, ui.available_height()));
    ui.scope_builder(UiBuilder::new().id_salt("setup_recap").max_rect(rect).layout(Layout::top_down(Align::Min)), |ui| {
        paper(ui, true, |ui| {
            let (x, y) = DEAL_PADDING;
            Frame::new().inner_margin(Margin { left: x, right: x, top: y, bottom: GUTTER }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                head(ui, facts);
                ui.add_space(HEAD_GAP.1);
                paper(ui, false, |ui| lines(ui, facts, &mut asked));
                ui.add_space(ITEM_GAP);
                desk_label(ui, worked_line(facts.worked), Type::NOTE, Ink::Text);
            });
            divider(ui);
            let (x, y) = DEAL_FOOT_PADDING;
            Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = BUTTON_GAP;
                    if primary_button(ui, "Open Jobdesk").clicked() {
                        asked = Some(RecapRequest::OpenJobdesk);
                    }
                    let why = if facts.without_pdfs { "Setup stays in the menu until LaTeX is installed." } else { "Today suggests your first search." };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| desk_label(ui, why, Type::NOTE, Ink::Muted));
                });
            });
        });
    });
    asked
}

/// The date stamp beside the title over the lead.
fn head(ui: &mut Ui, facts: &Recap) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = HEAD_GAP.0;
        let date = |pattern| facts.today.format(pattern).to_string();
        day_stamp(ui, &date("%a"), &date("%-d"), &date("%b"), 0, 0, &date("%A %-d %B"));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            desk_label(ui, "Your job search is ready", Type::CLEAR, Ink::Text);
            let lead = if facts.without_pdfs {
                "Here is what Jobdesk set up. PDFs come once LaTeX is installed."
            } else {
                "Here is what Jobdesk set up. You can change any of it later."
            };
            desk_label(ui, lead, Type::BODY, Ink::Muted);
        });
    });
}

/// What closes a recap line at its right.
enum End<'a> {
    Nothing,
    Link(&'a str),
    Tag(&'a str, Tone),
    Tally(usize, usize),
}

/// The five done lines, then the optional one.
fn lines(ui: &mut Ui, facts: &Recap, asked: &mut Option<RecapRequest>) {
    let (dir, name) = match facts.folder.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_owned()),
        None => (String::new(), facts.folder.clone()),
    };
    let folder = line(ui, "1", KnotState::Done, "Folder saved", Under::Path(&dir, &name), End::Link(SHOW_FOLDER), true);
    if folder.is_some_and(|link| link.clicked()) {
        *asked = Some(RecapRequest::ShowFolder);
    }
    if facts.without_pdfs {
        let under = Under::Text("Python and Bun are installed. LaTeX is not, so PDFs are not ready yet.");
        line(ui, "2", KnotState::Failed, "This computer is almost ready", under, End::Tag("Not yet", Tone::Plain), false);
    } else {
        line(ui, "2", KnotState::Done, "This computer is ready", Under::Text("Python, Bun and LaTeX are installed."), End::Nothing, false);
    }
    let (filled, total) = facts.topics;
    let left = total.saturating_sub(filled);
    let title = format!("Profile written: {filled} of {total} topics");
    let under = match left {
        0 => "Every topic is filled.".to_owned(),
        1 => "Finish the other one in Profile when you like.".to_owned(),
        left => format!("Finish the other {left} in Profile when you like."),
    };
    line(ui, "3", KnotState::Done, &title, Under::Text(&under), End::Tally(filled, total), false);
    match (&facts.cv, facts.without_pdfs) {
        (_, true) => {
            line(ui, "4", KnotState::Ahead, "Your CV is written", Under::Text("Its PDF comes once LaTeX is installed."), End::Nothing, false);
        }
        (Some(cv), false) => {
            let under = plain(cv);
            if line(ui, "4", KnotState::Done, "Your CV is made", Under::Text(&under), End::Link("Open PDF"), false).is_some_and(|link| link.clicked()) {
                *asked = Some(RecapRequest::OpenPdf);
            }
        }
        (None, false) => {
            line(ui, "4", KnotState::Ahead, "Your CV is not built yet", Under::Text("Profile › CV documents shows it once it is."), End::Nothing, false);
        }
    }
    let names: Vec<String> = facts.sites.iter().map(|site| plain(site)).collect();
    if names.is_empty() {
        line(ui, "5", KnotState::Ahead, "No job site is on yet", Under::Text("Portals lists the ones you can switch on."), End::Nothing, false);
    } else {
        let listed = english_list(&names);
        let under = match &facts.country {
            Some(country) => format!("{listed}, for {}.", plain(country)),
            None => format!("{listed}."),
        };
        line(ui, "5", KnotState::Done, "Job sites chosen", Under::Text(&under), End::Nothing, false);
    }
    let add = line(
        ui,
        "+",
        KnotState::Ahead,
        "Use a job site that is not listed?",
        Under::Text("Claude can add it as a new search, from the site's address."),
        End::Link("Add a job site you use…"),
        false,
    );
    if add.is_some_and(|link| link.clicked()) {
        *asked = Some(RecapRequest::AddPortal);
    }
}

/// What a recap line says under its title.
enum Under<'a> {
    Text(&'a str),
    /// A folder, in `mono`.
    Path(&'a str, &'a str),
}

/// One recap line: its knot, its title over what it says, and its end at the right. Ruled off from the line above
/// unless `first`. Gives the end's link, if it has one.
fn line(ui: &mut Ui, mark: &str, state: KnotState, title: &str, under: Under, end: End, first: bool) -> Option<Response> {
    if !first {
        divider(ui);
    }
    Frame::new()
        .inner_margin(Margin::symmetric(INSET, ROW_PADDING))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let height = KNOT_SIZE.max(wrapped_height(ui, title, Type::STRONG, f32::INFINITY) + wrapped_height(ui, "Ag", Type::NOTE, f32::INFINITY));
            ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = f32::from(GUTTER);
                let linked = match end {
                    End::Nothing => None,
                    End::Link(text) => Some(link(ui, text)),
                    End::Tag(text, tone) => {
                        tag(ui, text, tone);
                        None
                    }
                    End::Tally(marked, total) => {
                        fat_tally(ui, marked, total, Ink::Text);
                        None
                    }
                };
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    knot(ui, mark, state, KNOT_SIZE);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        desk_label(ui, title, Type::STRONG, Ink::Text);
                        match under {
                            Under::Text(text) => {
                                desk_label(ui, text, Type::NOTE, Ink::Muted);
                            }
                            Under::Path(dir, name) => {
                                path_label(ui, dir, name);
                            }
                        }
                    });
                });
                linked
            })
            .inner
        })
        .inner
}

/// How long Claude worked, in whole minutes: "for 41 minutes", "for 1 minute", or "for under a minute".
fn worked_line(seconds: u64) -> String {
    let lead = "Claude worked on your profile for";
    match seconds / 60 {
        0 => format!("{lead} under a minute."),
        1 => format!("{lead} 1 minute."),
        minutes => format!("{lead} {minutes} minutes."),
    }
}

/// "LinkedIn", "LinkedIn and Freehire", "A, B and C".
fn english_list(names: &[String]) -> String {
    match names.split_last() {
        None => String::new(),
        Some((only, [])) => only.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// `text` as plain text, without hidden characters.
fn plain(text: &str) -> String {
    visible_chars(text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_claude_worked_reads_in_whole_minutes() {
        assert_eq!(worked_line(41 * 60 + 20), "Claude worked on your profile for 41 minutes.");
        assert_eq!(worked_line(89), "Claude worked on your profile for 1 minute.");
        assert_eq!(worked_line(59), "Claude worked on your profile for under a minute.");
        assert_eq!(worked_line(0), "Claude worked on your profile for under a minute.");
    }

    #[test]
    fn site_names_read_as_a_sentence() {
        let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        assert_eq!(english_list(&names(&["LinkedIn"])), "LinkedIn");
        assert_eq!(english_list(&names(&["LinkedIn", "Freehire"])), "LinkedIn and Freehire");
        assert_eq!(english_list(&names(&["A", "B", "C"])), "A, B and C");
    }
}
