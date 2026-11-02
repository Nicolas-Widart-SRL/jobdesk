use chrono::NaiveDateTime;
use egui::{Align, Frame, Label, Layout, Margin, Rect, Ui, UiBuilder, pos2};
use egui_commonmark::CommonMarkCache;

use crate::profile::application::{Fill, Mark, ProfileView, SectionView, TOPICS, TopicCounts, TopicId, TopicView, UpdatedMarks, topic_counts};
use crate::profile::ui::rail::{UPDATED_HOVER, loading_rail, rail};
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::{BUTTON_GAP, DEAL_PADDING, GUTTER, ITEM_GAP, ROLE_DATES, RUN_SIDE, SCREEN_GAP, SKELETON_GAP, SKELETON_LINE};
use crate::ui_kit::{
    Ink, RoleEntry, ScreenState, Subtitle, Title, Tone, Type, button, desk_label, divider, empty_note, empty_state, enabled_when, faded, faint_mono,
    headed_paper, page_body, page_head, paper, primary_command_action, profile_document, rich, role_entry, skeleton, skeleton_lines, small_button,
    small_command_action, tag,
};

/// What the head says under the title once the profile is read.
const SUBTITLE: &str = "Claude reads this before it scores a job or writes a CV or a letter.";
const CLAUDE: &str = "Update with Claude…";
/// Why Update with Claude… waits while a run works.
const BUSY: &str = "Another run is active. One run at a time keeps your files safe.";
/// Why Update with Claude… waits while a file of the topic cannot be read.
const UNREADABLE: &str = "Fix the file first, so Claude does not write over it.";
/// Why every edit button waits, until editing is built.
const EDIT_LATER: &str = "Editing comes in the next step.";

/// What stays between frames: the topic on paper, and the rendered Markdown.
#[derive(Default)]
pub struct ProfileScreen {
    picked: Option<TopicId>,
    markdown: CommonMarkCache,
}

/// The rail of topics beside the one picked, on paper. Until a topic is picked, the first with something left to fill opens.
/// A picked topic is open, so its rail mark clears, and picking another leaves it, so its head line goes.
/// `busy` says a run works, so Update with Claude… waits. `now` ages the head line.
/// Every action only opens the launcher: nothing here starts a run.
pub fn show(ui: &mut Ui, screen: &mut ProfileScreen, state: &ScreenState, view: &ProfileView, marks: &mut UpdatedMarks, busy: bool, now: NaiveDateTime) {
    let reading = state.reading();
    let subtitle = if reading { "Reading your profile…" } else { SUBTITLE };
    page_head(ui, Title::Slab("Profile"), Some(Subtitle::Text(subtitle)), |_| {});
    page_body(ui, |ui| {
        (state.alerts)(ui);
        if reading {
            columns(ui, |ui| loading_rail(ui, TOPICS.len()), loading_paper);
            return;
        }
        let counts = topic_counts(view);
        if !counts.any_found && counts.hidden == 0 {
            no_profile(ui);
            return;
        }
        let Some(picked) = screen.picked.or_else(|| first_to_fill(view)) else { return };
        screen.picked = Some(picked);
        marks.open(picked);
        let Some(topic) = view.topics.iter().find(|topic| topic.id == picked) else { return };
        let mut chosen = None;
        columns(
            ui,
            |ui| chosen = rail(ui, view, counts, picked, marks),
            |ui| {
                ui.push_id(("profile topic", picked), |ui| topic_paper(ui, &mut screen.markdown, topic, counts, marks.head_line(picked), busy, now));
            },
        );
        if let Some(next) = chosen.filter(|&next| next != picked) {
            marks.leave(picked);
            marks.open(next);
            screen.picked = Some(next);
        }
    });
}

/// The topic a fresh screen opens on: the first with something left to fill, or else the first.
fn first_to_fill(view: &ProfileView) -> Option<TopicId> {
    let open = view.topics.iter().find(|topic| matches!(topic.fill, Fill::ToFill(_) | Fill::NotStarted));
    open.or(view.topics.first()).map(|topic| topic.id)
}

/// The rail at the left in `RUN_SIDE`, and the paper beside it, both as tall as the room left.
fn columns(ui: &mut Ui, side: impl FnOnce(&mut Ui), main: impl FnOnce(&mut Ui)) {
    let area = ui.available_rect_before_wrap();
    let rail_rect = Rect::from_min_max(area.min, pos2((area.left() + RUN_SIDE).min(area.right()), area.bottom()));
    let page_rect = Rect::from_min_max(pos2((rail_rect.right() + f32::from(GUTTER)).min(area.right()), area.top()), area.max);
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut rail_ui = ui.new_child(column(rail_rect, "profile_rail_column"));
    rail_ui.set_width(rail_rect.width());
    side(&mut rail_ui);
    let mut page_ui = ui.new_child(column(page_rect, "profile_page_column"));
    page_ui.set_width(page_rect.width());
    main(&mut page_ui);
    ui.advance_cursor_after_rect(area);
}

/// Why Update with Claude… waits, if it does.
fn claude_wait(topic: &TopicView, busy: bool) -> Option<&'static str> {
    if busy {
        Some(BUSY)
    } else if !topic.readable {
        Some(UNREADABLE)
    } else {
        None
    }
}

/// Update with Claude… opens the launcher on the topic's `/setup` line. A topic `/setup` has no section for has none.
fn update_with_claude(ui: &mut Ui, topic: &TopicView, busy: bool, primary: bool) {
    let Some(line) = topic.setup_line.as_deref() else { return };
    let wait = claude_wait(topic, busy);
    enabled_when(ui, wait.is_none(), wait.unwrap_or_default(), |ui| {
        if primary { primary_command_action(ui, CLAUDE, line) } else { small_command_action(ui, CLAUDE, line) }
    });
}

fn topic_paper(ui: &mut Ui, cache: &mut CommonMarkCache, topic: &TopicView, counts: TopicCounts, updated: Option<&Mark>, busy: bool, now: NaiveDateTime) {
    headed_paper(
        ui,
        |ui| head(ui, topic, updated, busy, now),
        |ui| match topic.fill {
            Fill::Hidden => hidden(ui, topic, counts),
            Fill::NotStarted => not_started(ui, cache, topic, busy),
            Fill::Filled | Fill::ToFill(_) => sections(ui, cache, topic),
        },
        |ui| foot(ui, topic),
    );
}

/// The title, what the topic is for, and the Updated line, with the actions at the right.
fn head(ui: &mut Ui, topic: &TopicView, updated: Option<&Mark>, busy: bool, now: NaiveDateTime) {
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        update_with_claude(ui, topic, busy, false);
        if topic.readable && topic.fill != Fill::NotStarted && !topic.sections.is_empty() {
            enabled_when(ui, false, EDIT_LATER, |ui| small_button(ui, "Edit section"));
        }
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            ui.label(rich(ui, topic.title, Type::DEAL, Ink::Text));
            ui.add(Label::new(rich(ui, lead(topic), Type::BODY, Ink::Muted)).wrap());
            if let Some(mark) = updated {
                ui.add_space(ITEM_GAP);
                ui.horizontal(|ui| {
                    tag(ui, "Updated", Tone::Active).on_hover_text(UPDATED_HOVER);
                    ui.add(Label::new(rich(ui, updated_words(mark, now), Type::NOTE, Ink::Muted)).wrap());
                });
            }
        });
    });
}

/// One line on what the topic is for, in the voice of the rail.
fn lead(topic: &TopicView) -> String {
    match topic.id {
        TopicId::Identity => "Who you are and the languages you speak. Every CV and letter opens with it.".to_owned(),
        TopicId::Experience => {
            let roles = topic.sections.iter().flat_map(|section| &section.children).filter(|child| child.role.is_some()).count();
            match roles {
                _ if !topic.readable => "Roles, newest first. Claude picks from these for each CV.".to_owned(),
                1 => "1 role. Claude picks from it for each CV.".to_owned(),
                roles => format!("{roles} roles, newest first. Claude picks from these for each CV."),
            }
        }
        TopicId::Education => "Your degrees and courses. Claude lists them on each CV.".to_owned(),
        TopicId::Skills => "What you know and how well. Claude matches it against what each job asks for.".to_owned(),
        TopicId::Publications => "Side projects, papers and awards. Claude adds the ones a job cares about.".to_owned(),
        TopicId::References => "People who can vouch for you. Claude names them when an application asks.".to_owned(),
        TopicId::Statements => "The short pitch at the top of a CV. Claude tailors one for each job.".to_owned(),
        TopicId::HowIWork => "How you like to work. Claude weighs it when it scores a team and its pace.".to_owned(),
        TopicId::Goals => "What you want, and what rules a job out. Claude scores every job against it.".to_owned(),
        TopicId::Stories => "Your STAR stories. Claude picks from these when it prepares you for an interview.".to_owned(),
        TopicId::JobSearch => "What to search for, where and in which languages. Every job search runs on it.".to_owned(),
    }
}

/// "by the last task, 4 minutes ago. It changed Lead Engineer - Calder (2021 – now)." Headings show as plain text.
fn updated_words(mark: &Mark, now: NaiveDateTime) -> String {
    let blocks: Vec<String> = mark.blocks.iter().map(|block| visible_chars(block).collect()).collect();
    format!("by the last task, {}. It changed {}.", age(mark.at, now), and_list(&blocks))
}

/// How long ago `at` was, in whole minutes, or hours past the first one.
fn age(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let minutes = (now - at).num_minutes();
    match minutes {
        ..1 => "just now".to_owned(),
        1 => "1 minute ago".to_owned(),
        2..60 => format!("{minutes} minutes ago"),
        60..120 => "1 hour ago".to_owned(),
        _ => format!("{} hours ago", minutes / 60),
    }
}

/// "A", "A and B", "A, B and C".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The topic's blocks, as written. Each section shows under its heading when the topic has more than one.
/// A role reads as a CV line, and the rest goes through `profile_document`, as ARCHITECTURE.md#trust says.
fn sections(ui: &mut Ui, cache: &mut CommonMarkCache, topic: &TopicView) {
    let headed = topic.sections.len() > 1;
    for (index, section) in topic.sections.iter().enumerate() {
        ui.push_id(("profile section", index), |ui| {
            if index > 0 {
                ui.add_space(SCREEN_GAP / 2.0);
                divider(ui);
            }
            ui.add_space(SCREEN_GAP / 2.0);
            section_blocks(ui, cache, section, headed);
        });
    }
}

fn section_blocks(ui: &mut Ui, cache: &mut CommonMarkCache, section: &SectionView, headed: bool) {
    ui.spacing_mut().item_spacing.y = ITEM_GAP;
    if headed {
        heading(ui, &section.block.heading, Type::SLIP_TITLE);
    }
    let own = !section.block.body.is_empty();
    if own {
        profile_document(ui, cache, &section.block.body);
    }
    // A heading needs no rule under it, so only words of the section's own put one over its first role.
    let mut first = !own;
    for child in &section.children {
        match &child.role {
            Some(role) => {
                let entry = RoleEntry { dates: &role.dates, title: &role.title, company: &role.company, body: &child.body, first };
                role_entry(ui, cache, &entry, Some(EDIT_LATER));
            }
            None => {
                ui.add_space(ITEM_GAP);
                heading(ui, &child.heading, Type::SECTION);
                profile_document(ui, cache, &child.body);
            }
        }
        first = false;
    }
}

/// A block's heading, as plain text with hidden characters removed.
fn heading(ui: &mut Ui, text: &str, style: Type) {
    desk_label(ui, visible_chars(text).collect::<String>(), style, Ink::Text);
}

/// A topic whose blocks hold nothing yet: what Claude will ask, the ways to fill it, then its outline, faded.
fn not_started(ui: &mut Ui, cache: &mut CommonMarkCache, topic: &TopicView, busy: bool) {
    ui.add_space(SCREEN_GAP / 2.0);
    let body = match asks(topic.id) {
        Some(asks) if topic.setup_line.is_some() => format!("Claude asks {asks}, then writes it here. You can change any of it afterwards."),
        _ => "Write it in your own words. Claude reads it as you leave it.".to_owned(),
    };
    empty_state(ui, "Not filled in yet", &body, |ui| {
        update_with_claude(ui, topic, busy, true);
        enabled_when(ui, false, EDIT_LATER, |ui| button(ui, "Write it myself"));
    });
    if topic.sections.is_empty() {
        return;
    }
    ui.add_space(SCREEN_GAP / 2.0);
    divider(ui);
    ui.add_space(SCREEN_GAP / 2.0);
    desk_label(ui, "What this topic holds", Type::NOTE, Ink::Muted);
    faded(ui, |ui| sections(ui, cache, topic));
}

/// What `/setup` asks for a topic, in the empty state's sentence.
fn asks(topic: TopicId) -> Option<&'static str> {
    match topic {
        TopicId::Identity => Some("your name, where you live and the languages you speak"),
        TopicId::Experience => Some("about each role you held, newest first"),
        TopicId::Education => Some("about your degrees and courses"),
        TopicId::Skills => Some("what you know and how well"),
        TopicId::Publications => Some("about your side projects, papers and awards"),
        TopicId::References => Some("who can vouch for you"),
        TopicId::HowIWork => Some("how you like to work and where you do your best work"),
        TopicId::Goals => Some("what you want next, what you will not accept and how far you will travel"),
        TopicId::JobSearch => Some("what to search for, where and in which languages"),
        TopicId::Statements | TopicId::Stories => None,
    }
}

/// A topic whose file could not be read: hidden, not gone, and back once the file is fixed.
fn hidden(ui: &mut Ui, topic: &TopicView, counts: TopicCounts) {
    ui.add_space(SCREEN_GAP / 2.0);
    let others = counts.hidden.saturating_sub(1);
    let shown = match others {
        0 => "so this topic is not shown".to_owned(),
        1 => "so this topic and 1 other are not shown".to_owned(),
        others => format!("so this topic and {others} others are not shown"),
    };
    let body =
        format!("Jobdesk could not read a file that holds it, {shown}. Nothing in the file was changed. Jobdesk reads it again as soon as the file changes.");
    empty_note(ui, hidden_title(topic.id), &body);
}

fn hidden_title(topic: TopicId) -> &'static str {
    match topic {
        TopicId::Identity => "Your identity and languages are hidden, not gone",
        TopicId::Experience => "Your experience is hidden, not gone",
        TopicId::Education => "Your education is hidden, not gone",
        TopicId::Skills => "Your skills are hidden, not gone",
        TopicId::Publications => "Your projects and publications are hidden, not gone",
        TopicId::References => "Your references are hidden, not gone",
        TopicId::Statements => "Your profile statements are hidden, not gone",
        TopicId::HowIWork => "How you work is hidden, not gone",
        TopicId::Goals => "Your goals and deal-breakers are hidden, not gone",
        TopicId::Stories => "Your interview stories are hidden, not gone",
        TopicId::JobSearch => "Your job search is hidden, not gone",
    }
}

/// Where the topic lives: the only place a file name shows on this screen. The count to fill sits at the right.
/// The headings are cut to the line, with the whole on hover.
fn foot(ui: &mut Ui, topic: &TopicView) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if let Fill::ToFill(left) = topic.fill {
            desk_label(ui, format!("{left} to fill in this topic"), Type::NOTE, Ink::Muted);
            ui.add_space(ITEM_GAP * 2.0);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            desk_label(ui, "Kept in ", Type::NOTE, Ink::Muted);
            for (index, file) in topic.files.iter().enumerate() {
                if index > 0 {
                    desk_label(ui, ", ", Type::NOTE, Ink::Muted);
                }
                faint_mono(ui, *file);
            }
            if !topic.headings.is_empty() {
                ui.add(Label::new(rich(ui, format!(", under {}", and_list(&topic.headings)), Type::NOTE, Ink::Muted)).truncate());
            }
        });
    });
}

/// No profile file in the folder: one way in, through `/setup`.
fn no_profile(ui: &mut Ui) {
    let body = "Your job search folder holds none of the profile files yet. Claude asks about you, your work and what you want, then writes them.";
    empty_state(ui, "No profile yet", body, |ui| {
        primary_command_action(ui, "Set up your profile…", "/setup");
    });
}

/// The paper before the first read: grey lines where the head and three roles go.
fn loading_paper(ui: &mut Ui) {
    paper(ui, false, |ui| {
        ui.set_min_height(ui.available_height());
        let (x, y) = DEAL_PADDING;
        Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            skeleton_lines(ui, &[220.0, 380.0], skeleton);
        });
        divider(ui);
        Frame::new().inner_margin(Margin::symmetric(x, 0)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            for index in 0..3 {
                if index > 0 {
                    divider(ui);
                }
                ui.add_space(f32::from(y));
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(ROLE_DATES, SKELETON_LINE), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(ROLE_DATES);
                        skeleton(ui, 80.0, SKELETON_LINE);
                    });
                    skeleton_lines(ui, &[240.0, 180.0, 520.0, 460.0, 380.0], skeleton);
                });
                ui.add_space(f32::from(y) - SKELETON_GAP);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn the_head_line_says_how_long_ago_and_what_changed() {
        let mark = Mark { at: at(9, 30), blocks: vec!["Lead Engineer - Calder (2021 – now)".to_owned(), "the section".to_owned()] };
        assert_eq!(updated_words(&mark, at(9, 30)), "by the last task, just now. It changed Lead Engineer - Calder (2021 – now) and the section.");
        assert_eq!(age(at(9, 30), at(9, 31)), "1 minute ago");
        assert_eq!(age(at(9, 30), at(9, 34)), "4 minutes ago");
        assert_eq!(age(at(9, 30), at(10, 45)), "1 hour ago");
        assert_eq!(age(at(9, 30), at(12, 0)), "2 hours ago");
        let hidden = Mark { at: at(9, 30), blocks: vec!["Lead\u{202e} Engineer".to_owned()] };
        assert_eq!(updated_words(&hidden, at(9, 30)), "by the last task, just now. It changed Lead Engineer.");
    }

    #[test]
    fn lists_read_as_a_sentence() {
        let words = |items: &[&str]| and_list(&items.iter().map(|item| item.to_string()).collect::<Vec<_>>());
        assert_eq!(words(&["A"]), "A");
        assert_eq!(words(&["A", "B"]), "A and B");
        assert_eq!(words(&["A", "B", "C"]), "A, B and C");
    }
}
