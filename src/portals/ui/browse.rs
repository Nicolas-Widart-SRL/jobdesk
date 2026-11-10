use chrono::{DateTime, NaiveDateTime, Utc};
use egui::{Align, Label, Layout, Rect, ScrollArea, Ui, UiBuilder, vec2};

use crate::portals::application::{Community, CommunityFailure, CommunityPortalView, CommunityState, CommunityView, CountryGroup, list_url};
use crate::shared_kernel::visible_text::all_visible;
use crate::ui_kit::tokens::{FIELD_HEIGHT, INSET, ITEM_GAP, ROW_ITEM_GAP, SKELETON_LINE, TABLE_CELL_PADDING, Type};
use crate::ui_kit::{
    CellRule, Ink, Tone, card_skeleton, desk_label, empty_state, enabled_when, fitted_row_tag, lead_line, link, list_row, note, paper, primary_button,
    prompt_box_tone, rich, search_field, small_button, small_command_action_hinted, status_dot, table_cell, tag,
};

/// What Install… says on hover.
const INSTALL_HINT: &str =
    "Opens a new task. Claude copies this portal, explains what its code does, then asks before it installs. Nothing runs until you start it.";
/// Why Install… waits when the list gave Jobdesk a task it would not show in full.
const INSTALL_REFUSED: &str = "Jobdesk could not write this task in plain words. Use View code ↗ to read the portal on GitHub.";
/// The search field's width, from the canvas.
const SEARCH_WIDTH: f32 = 360.0;
/// The widths of an entry's owners, tag and actions, from the canvas. The name and sentence take what is left.
const OWNERS_WIDTH: f32 = 190.0;
const TAG_WIDTH: f32 = 240.0;
const ACTIONS_WIDTH: f32 = 200.0;
const COLUMN_GAP: f32 = 16.0;
/// The narrowest the name and sentence get while the owners or the tag still show.
const MIN_NAME_WIDTH: f32 = 160.0;
/// An entry's least height inside its padding, as the canvas's rows.
const ENTRY_HEIGHT: f32 = 40.0;
/// A country's head: its name over an ink rule.
const GROUP_HEAD_HEIGHT: f32 = 36.0;

/// The community list and the clock its ages count to. The list is read from GitHub only when Browse opens and its saved
/// copy is old or missing, and when the user asks.
pub struct BrowseContext<'a> {
    pub community: &'a mut Community,
    /// The user's country, from their Location, whose group comes first.
    pub market: Option<&'a str>,
    /// Now on the wall clock, which the words count to, and the same moment in UTC, which the list keeps.
    pub now: NaiveDateTime,
    pub now_utc: DateTime<Utc>,
}

impl BrowseContext<'_> {
    /// `at` on the wall clock, counted from now, so it reads the same wherever the clock is.
    fn local(&self, at: DateTime<Utc>) -> NaiveDateTime {
        self.now + (at - self.now_utc)
    }

    /// How long ago `at` was, short: "12 min ago" within the hour, then its day and time.
    fn short_age(&self, at: DateTime<Utc>) -> String {
        match (self.now_utc - at).num_minutes() {
            ..1 => "just now".to_owned(),
            minutes @ 1..60 => format!("{minutes} min ago"),
            _ => self.local(at).format("%a %-d %b, %H:%M").to_string(),
        }
    }

    /// How long ago `at` was, in a sentence: "12 minutes ago" within the hour, then "on" its day.
    fn long_age(&self, at: DateTime<Utc>) -> String {
        match (self.now_utc - at).num_minutes() {
            ..1 => "just now".to_owned(),
            1 => "1 minute ago".to_owned(),
            minutes @ 2..60 => format!("{minutes} minutes ago"),
            _ => self.local(at).format("on %a %-d %b").to_string(),
        }
    }
}

/// What the user asked of Browse that the screen answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BrowseAsk {
    /// Add a portal, for a market with no community portal.
    AddPortal,
}

/// The community list: the search and the list's age, what GitHub said, the warning about other people's code, then
/// the portals by country. `installed` holds the installed portals' folder names. Nothing here starts a run: Install…
/// only opens the launcher, and View code ↗ opens the browser on click.
pub(super) fn browse(ui: &mut Ui, query: &mut String, installed: &[&str], cx: &mut BrowseContext) -> Option<BrowseAsk> {
    let (view, mut retry) = ui
        .horizontal(|ui| {
            ui.allocate_ui(vec2(SEARCH_WIDTH.min(ui.available_width()), FIELD_HEIGHT), |ui| search_field(ui, query, "Search by name or country"));
            // Read after the search, so a typed letter filters the list on the same frame.
            let view = cx.community.view(installed, cx.market, query);
            let retry = ui.with_layout(Layout::right_to_left(Align::Center), |ui| age_and_retry(ui, &view, cx)).inner;
            (view, retry)
        })
        .inner;
    // Try again comes back by itself once GitHub's wait ends.
    if let Some(left) = view.wait_until.and_then(|until| (until - cx.now_utc).to_std().ok()) {
        ui.ctx().request_repaint_after(left);
    }
    ui.add_space(ITEM_GAP * 2.0);
    if let Some((title, body)) = github_said(&view, cx) {
        prompt_box_tone(ui, Tone::Interview, |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP / 2.0;
            desk_label(ui, title, Type::STRONG, Tone::Interview);
            desk_label(ui, body, Type::BODY, Ink::Text);
        });
        ui.add_space(ITEM_GAP * 2.0);
    }
    note(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            status_dot(ui, Tone::Interview);
            lead_line(
                ui,
                "Community portals are code other people wrote, and it runs next to your career data.",
                "Install… opens a task: Claude reads the code and tells you what it does before anything is installed.",
            );
        });
    });
    ui.add_space(ITEM_GAP * 2.0);
    let mut ask = None;
    if view.state == CommunityState::Loading && view.saved_at.is_none() {
        loading_list(ui);
    } else if view.saved_at.is_none() {
        retry |= no_list(ui, waiting(&view, cx));
    } else {
        ask = list(ui, &view, cx.market, query);
    }
    if retry {
        cx.community.refresh(cx.now_utc);
    }
    ask
}

/// Whether GitHub asks Jobdesk to wait before it reads the list again.
fn waiting(view: &CommunityView, cx: &BrowseContext) -> Option<String> {
    let until = view.wait_until.filter(|until| cx.now_utc < *until)?;
    Some(format!("GitHub asks Jobdesk to wait. Try again after {}.", cx.local(until).format("%H:%M")))
}

/// The list's age at the right, then Refresh, or Try again after a failure. Returns whether either was clicked.
fn age_and_retry(ui: &mut Ui, view: &CommunityView, cx: &BrowseContext) -> bool {
    if view.state == CommunityState::Loading {
        desk_label(ui, "Reading the list from GitHub…", Type::NOTE, Ink::Muted);
        return false;
    }
    let label = if view.state == CommunityState::Ready { "Refresh" } else { "Try again" };
    let wait = waiting(view, cx);
    let clicked = enabled_when(ui, wait.is_none(), wait.as_deref().unwrap_or_default(), |ui| small_button(ui, label)).clicked();
    ui.add_space(ROW_ITEM_GAP - ui.spacing().item_spacing.x);
    if let Some(saved) = view.saved_at {
        let words = match view.state {
            CommunityState::Ready => format!("From GitHub, updated {}", cx.short_age(saved)),
            _ => format!("Saved list from {}", cx.short_age(saved)),
        };
        desk_label(ui, words, Type::NOTE, Ink::Muted);
    }
    clicked
}

/// What GitHub said, when the last read failed: a title and a sentence, in Jobdesk's own words.
fn github_said(view: &CommunityView, cx: &BrowseContext) -> Option<(&'static str, String)> {
    let failure = view.failure?;
    if view.state == CommunityState::Loading {
        return None;
    }
    let saved = view.saved_at.map(|saved| cx.long_age(saved));
    let wait = view.wait_until.filter(|until| cx.now_utc < *until).map(|until| cx.local(until).format("%H:%M").to_string());
    // With nothing saved, the list's own place says it could not be read, unless GitHub asks Jobdesk to wait.
    if saved.is_none() && wait.is_none() {
        return None;
    }
    let kept = |advice: &str| match &saved {
        Some(saved) => format!("The list below is the one Jobdesk saved {saved}. {advice}"),
        None => advice.to_owned(),
    };
    Some(match (failure, wait) {
        (CommunityFailure::Limited { .. } | CommunityFailure::Refused(403 | 429), Some(wait)) => {
            let body = match &saved {
                Some(saved) => format!("Try again after {wait}. Until then, the list below is the one saved {saved}."),
                None => format!("Try again after {wait}."),
            };
            ("GitHub asks Jobdesk to wait before it looks again.", body)
        }
        (CommunityFailure::TimedOut | CommunityFailure::Unreachable, _) => ("Could not reach GitHub.", kept("Check your connection, then try again.")),
        (CommunityFailure::BadAnswer, _) => ("GitHub's answer could not be read.", kept("Try again later.")),
        _ => ("GitHub could not send the list.", kept("Try again later.")),
    })
}

/// Grey lines where the first list will be.
fn loading_list(ui: &mut Ui) {
    paper(ui, false, |ui| {
        // Clipped to the room left, as the list is.
        ScrollArea::vertical().id_salt("community_loading").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            loading_groups(ui);
        });
    });
}

/// Three countries of two grey entries each.
fn loading_groups(ui: &mut Ui) {
    for _ in 0..3 {
        group_head(ui, |ui| card_skeleton(ui, 110.0, SKELETON_LINE + 2.0));
        for at in 0..2 {
            list_row(ui, at == 0, |ui| {
                columns(
                    ui,
                    Needs::FULL,
                    |ui| {
                        ui.spacing_mut().item_spacing.y = ITEM_GAP;
                        card_skeleton(ui, 150.0, SKELETON_LINE);
                        card_skeleton(ui, 320.0, SKELETON_LINE);
                    },
                    |ui| card_skeleton(ui, 120.0, SKELETON_LINE),
                    |ui| card_skeleton(ui, 90.0, SKELETON_LINE),
                    |ui| card_skeleton(ui, 100.0, SKELETON_LINE),
                );
            });
        }
    }
}

/// No list was ever saved and none could be read. Returns whether Try again was clicked.
fn no_list(ui: &mut Ui, wait: Option<String>) -> bool {
    let mut retry = false;
    let body = "Jobdesk reads it from GitHub and has no saved copy yet. Check your connection, then try again. Your own portals are not affected.";
    empty_state(ui, "The community list could not be read", body, |ui| {
        retry = enabled_when(ui, wait.is_none(), wait.as_deref().unwrap_or_default(), |ui| primary_button(ui, "Try again")).clicked();
        github_link(ui, "Open the list on GitHub ↗", &list_url());
    });
    retry
}

/// A link to an address Jobdesk built itself, opened in the browser only on click.
fn github_link(ui: &mut Ui, text: &str, url: &str) {
    if link(ui, text).on_hover_text(url).clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

/// The portals by country on paper that scrolls. A market with no portal says so, and offers Add a portal.
fn list(ui: &mut Ui, view: &CommunityView, market: Option<&str>, query: &str) -> Option<BrowseAsk> {
    let mut ask = None;
    paper(ui, false, |ui| {
        ScrollArea::vertical().id_salt("community_list").auto_shrink([false, false]).max_height(ui.available_height()).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let market = market.filter(|_| query.trim().is_empty() && !view.groups.iter().any(|group| group.your_market));
            if let Some(market) = market {
                ask = no_market_portal(ui, market);
            }
            let needs = needs(ui, view);
            for group in &view.groups {
                country(ui, group, needs);
            }
            if view.groups.is_empty() && market.is_none() {
                list_row(ui, true, |ui| {
                    let words = if query.trim().is_empty() {
                        "The community list holds no portal yet.".to_owned()
                    } else {
                        "No community portal matches your search.".to_owned()
                    };
                    desk_label(ui, words, Type::BODY, Ink::Muted);
                });
            }
        });
    });
    ask
}

/// The user's market, which holds no community portal yet.
fn no_market_portal(ui: &mut Ui, market: &str) -> Option<BrowseAsk> {
    group_head(ui, |ui| market_title(ui, market));
    let mut ask = None;
    list_row(ui, true, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            ui.add(Label::new(rich(ui, format!("No community portals for {market} yet."), Type::BODY, Ink::Muted)).truncate());
            if small_button(ui, "Add a portal").clicked() {
                ask = Some(BrowseAsk::AddPortal);
            }
        });
    });
    ask
}

fn market_title(ui: &mut Ui, country: &str) {
    ui.add(Label::new(rich(ui, country, Type::SECTION, Ink::Text)).truncate());
    desk_label(ui, "your market", Type::NOTE, Ink::Muted);
}

/// A country's head, then its portals.
fn country(ui: &mut Ui, group: &CountryGroup, needs: Needs) {
    let count = group.portals.len();
    group_head(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            desk_label(ui, if count == 1 { "1 portal".to_owned() } else { format!("{count} portals") }, Type::NOTE, Ink::Muted);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                if group.your_market {
                    market_title(ui, &group.country);
                } else {
                    ui.add(Label::new(rich(ui, &group.country, Type::SECTION, Ink::Text)).truncate());
                }
            });
        });
    });
    for (at, portal) in group.portals.iter().enumerate() {
        entry(ui, portal, at == 0, needs);
    }
}

/// A head over an ink rule, its words set in as the rows' are.
fn group_head(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let rect = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), GROUP_HEAD_HEIGHT));
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        table_cell(ui, CellRule::Head, |ui| {
            ui.add_space(f32::from(INSET) - TABLE_CELL_PADDING.0);
            add(ui);
        });
    });
    ui.advance_cursor_after_rect(rect);
}

/// One community portal: its name over its sentence, its owners as words, whether someone looks after it, then View
/// code ↗ and Installed or Install…. Every word from GitHub stays on one line, cut with an ellipsis.
fn entry(ui: &mut Ui, portal: &CommunityPortalView, first: bool, needs: Needs) {
    list_row(ui, first, |ui| {
        columns(
            ui,
            needs,
            |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.add(Label::new(rich(ui, &portal.name, Type::STRONG, Ink::Text)).truncate());
                if let Some(sentence) = &portal.sentence {
                    ui.add(Label::new(rich(ui, sentence, Type::NOTE, Ink::Muted)).truncate());
                }
            },
            |ui| owners(ui, portal),
            |ui| upkeep(ui, portal),
            |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
                    if portal.installed {
                        tag(ui, "Installed", Tone::Hired);
                    } else {
                        let ready = all_visible(&portal.install_prompt);
                        enabled_when(ui, ready, INSTALL_REFUSED, |ui| small_command_action_hinted(ui, "Install…", &portal.install_prompt, INSTALL_HINT));
                    }
                    github_link(ui, "View code ↗", &portal.code_url);
                });
            },
        );
    });
}

/// The widths the owners and tag columns ask for: their widest content across the list, each capped at
/// `OWNERS_WIDTH` and `TAG_WIDTH`, so every row lines up and the room their words do not use goes to the names.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Needs {
    owners: f32,
    tag: f32,
}

impl Needs {
    /// Both columns at their widest, for the grey lines while the list loads.
    const FULL: Self = Self { owners: OWNERS_WIDTH, tag: TAG_WIDTH };
}

/// The widest owners and tag of the portals listed, laid out unseen in a sizing pass, each capped at its column's width.
fn needs(ui: &mut Ui, view: &CommunityView) -> Needs {
    let probe = |ui: &mut Ui, salt: (usize, &str), cap: f32, add: &dyn Fn(&mut Ui)| {
        let rect = Rect::from_min_size(ui.cursor().min, vec2(cap, ENTRY_HEIGHT));
        let layout = Layout::left_to_right(Align::Center);
        let mut unseen = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(rect).layout(layout).sizing_pass().invisible());
        add(&mut unseen);
        unseen.min_rect().width().min(cap)
    };
    let mut needs = Needs { owners: 0.0, tag: 0.0 };
    for (at, portal) in view.groups.iter().flat_map(|group| &group.portals).enumerate() {
        needs.owners = needs.owners.max(probe(ui, (at, "owners"), OWNERS_WIDTH, &|ui| owners(ui, portal)));
        needs.tag = needs.tag.max(probe(ui, (at, "tag"), TAG_WIDTH, &|ui| upkeep(ui, portal)));
    }
    needs
}

/// The widths of an entry's name, owners and tag in `width`, after the actions and the gaps. The actions keep their
/// width, at the row's right. The owners and the tag take no more than their content `needs`, so the name gets the rest.
/// When the room runs short, the name keeps `MIN_NAME_WIDTH`, and the owners and then the tag give up their width first,
/// so a narrow row or a large text size never pushes View code ↗ or Install… out of reach.
fn column_widths(width: f32, needs: Needs) -> (f32, f32, f32) {
    let shared = (width - ACTIONS_WIDTH - COLUMN_GAP * 3.0).max(0.0);
    let spare = (shared - MIN_NAME_WIDTH).max(0.0);
    let tag = needs.tag.min(TAG_WIDTH).min(spare);
    let owners = needs.owners.min(OWNERS_WIDTH).min(spare - tag);
    (shared - tag - owners, owners, tag)
}

/// An entry's four columns side by side: the name, the owners, the tag and the actions, each centred on the row, with
/// widths from `column_widths`. Each column is clipped to its own room. The row is as tall as the name column, and at
/// least `ENTRY_HEIGHT`.
fn columns(ui: &mut Ui, needs: Needs, name: impl FnOnce(&mut Ui), owners: impl FnOnce(&mut Ui), state: impl FnOnce(&mut Ui), actions: impl FnOnce(&mut Ui)) {
    let width = ui.available_width();
    let (name_width, owners_width, tag_width) = column_widths(width, needs);
    let top = ui.cursor().min;
    let room = Rect::from_min_size(top, vec2(name_width, f32::MAX));
    let mut first = ui.new_child(UiBuilder::new().id_salt("name").max_rect(room).layout(Layout::top_down(Align::Min)));
    first.set_width(name_width);
    name(&mut first);
    let height = first.min_rect().height().max(ENTRY_HEIGHT);
    let column = |ui: &mut Ui, salt: &str, left: f32, column_width: f32| {
        let rect = Rect::from_min_size(egui::pos2(left, top.y), vec2(column_width, height));
        let mut child = ui.new_child(UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::left_to_right(Align::Center)));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child
    };
    let mut left = top.x + name_width + COLUMN_GAP;
    if owners_width > 0.0 {
        owners(&mut column(ui, "owners", left, owners_width));
    }
    left += owners_width + COLUMN_GAP;
    if tag_width > 0.0 {
        state(&mut column(ui, "state", left, tag_width));
    }
    let actions_left = (top.x + width - ACTIONS_WIDTH).max(top.x);
    actions(&mut column(ui, "actions", actions_left, ACTIONS_WIDTH));
    ui.advance_cursor_after_rect(Rect::from_min_size(top, vec2(width, height)));
}

/// A portal's owners on GitHub as plain words, cut with an ellipsis and whole on hover. None reads nothing.
fn owners(ui: &mut Ui, portal: &CommunityPortalView) {
    if !portal.owners.is_empty() {
        ui.add(Label::new(rich(ui, portal.owners.join(", "), Type::NOTE, Ink::Muted)).truncate());
    }
}

/// The upkeep tag, cut when its column gives way.
fn upkeep(ui: &mut Ui, portal: &CommunityPortalView) {
    if let Some((words, tone)) = upkeep_tag(portal) {
        fitted_row_tag(ui, words, tone);
    }
}

/// The month its owners marked the portal unmaintained, or Maintained when it has owners and no such mark. A portal
/// with no owners has no tag, because nobody says either way.
fn upkeep_tag(portal: &CommunityPortalView) -> Option<(String, Tone)> {
    if portal.owners.is_empty() {
        return None;
    }
    Some(match portal.unmaintained_since {
        Some(since) => (since.format("Unmaintained since %Y-%m").to_string(), Tone::Interview),
        None => ("Maintained".to_owned(), Tone::Plain),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOM: f32 = ACTIONS_WIDTH + COLUMN_GAP * 3.0;

    /// Short owners and a short tag take only their own width, and the name gets the rest.
    #[test]
    fn owners_and_tag_take_only_what_their_words_need() {
        let needs = Needs { owners: 95.0, tag: 100.0 };
        assert_eq!(column_widths(1200.0, needs), (1200.0 - ROOM - 195.0, 95.0, 100.0));
        assert_eq!(column_widths(1200.0, Needs { owners: 500.0, tag: 500.0 }), (1200.0 - ROOM - OWNERS_WIDTH - TAG_WIDTH, OWNERS_WIDTH, TAG_WIDTH), "capped");
    }

    fn listed(owners: &[&str], unmaintained_since: Option<chrono::NaiveDate>) -> CommunityPortalView {
        CommunityPortalView {
            folder: "northfield-search".into(),
            name: "Northfield".into(),
            sentence: None,
            owners: owners.iter().map(|owner| (*owner).to_owned()).collect(),
            unmaintained_since,
            installed: false,
            code_url: String::new(),
            install_prompt: String::new(),
        }
    }

    /// Maintained needs owners: a portal no rule names, or a list with no `CODEOWNERS`, has no tag.
    #[test]
    fn only_a_portal_with_owners_has_an_upkeep_tag() {
        let since = chrono::NaiveDate::from_ymd_opt(2026, 3, 1);
        assert_eq!(upkeep_tag(&listed(&["@jdoe"], None)), Some(("Maintained".to_owned(), Tone::Plain)));
        assert_eq!(upkeep_tag(&listed(&["@jdoe"], since)), Some(("Unmaintained since 2026-03".to_owned(), Tone::Interview)));
        assert_eq!(upkeep_tag(&listed(&[], None)), None);
        assert_eq!(upkeep_tag(&listed(&[], since)), None);
    }

    /// When the room runs short, the name keeps its least width, and the owners give way before the tag.
    #[test]
    fn the_owners_then_the_tag_give_way() {
        let needs = Needs { owners: 95.0, tag: 185.0 };
        assert_eq!(column_widths(ROOM + MIN_NAME_WIDTH + 190.0, needs), (MIN_NAME_WIDTH, 5.0, 185.0));
        assert_eq!(column_widths(ROOM + MIN_NAME_WIDTH + 100.0, needs), (MIN_NAME_WIDTH, 0.0, 100.0));
        assert_eq!(column_widths(ROOM + 50.0, needs), (50.0, 0.0, 0.0), "the actions keep their width");
    }
}
