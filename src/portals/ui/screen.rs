use egui::{Align, Layout, Rect, Ui, UiBuilder, pos2};

use crate::portals::application::{CHECK_ALL_LINE, ChangeError, Origin, PortalView, PortalsView};
use crate::portals::ui::add::{AddForm, add};
use crate::portals::ui::browse::{BrowseAsk, BrowseContext, browse};
use crate::portals::ui::detail::{Detail, DetailAsk, detail, loading_detail};
use crate::portals::ui::rail::{loading_rail, rail};
use crate::ui_kit::tokens::{GUTTER, ROW_ITEM_GAP, SCREEN_GAP};
use crate::ui_kit::{
    Load, ScreenState, Subtitle, Title, button, command_action, empty_note, empty_state, enabled_when, page_body, page_head, primary_button, segmented,
};

/// Why every switch and every action waits while a run works: one run at a time, and `/scrape` reads the same files.
pub const BUSY: &str = "A task is running. Turn portals on or off once it ends.";
/// What a built-in portal shows in place of Remove….
pub(super) const BUILT_IN: &str = "Built-in portals stay installed. Turn one off to skip it.";
/// Where Remove moves a portal's folder, in the system's own words.
pub(super) const TRASH: &str = if cfg!(target_os = "macos") { "the Trash" } else { "the Recycle Bin" };
/// The head's subtitle, before the count.
const LEAD: &str = "The job boards Claude searches when you find new jobs.";
/// The list's width, from the canvas. It never takes more than this share of the screen.
const LIST_WIDTH: f32 = 430.0;
const LIST_SHARE: f32 = 0.45;

/// The screen's three views, under its head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Segment {
    #[default]
    Yours,
    Browse,
    Add,
}

/// What stays between frames: the view on show, the portal on paper, the order the list froze in, Remove…'s ask, the
/// last change that failed, the community search and what Add a portal holds.
#[derive(Default)]
pub struct PortalsScreen {
    segment: Segment,
    /// The community list was asked for since Browse came on show, so it is asked for once each time, never each frame.
    browse_opened: bool,
    /// What the community list is searched for.
    query: String,
    form: AddForm,
    /// The picked portal's folder. Until one is picked, or once it is gone, the first row is.
    picked: Option<String>,
    /// Each folder in the order the list first showed it since the screen opened, with the group it went in: on or off.
    /// A flipped switch keeps its row in place, so nothing moves under the pointer, until the screen opens again.
    order: Option<Vec<(String, bool)>>,
    /// Remove…'s ask, as `confirm_popover` holds it.
    confirm: Option<bool>,
    /// The folder whose switch or Remove failed last, and why, in words.
    failure: Option<(String, String)>,
}

/// What the user asked the shell to change. Both wait while a run works, and the shell checks again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortalsRequest {
    /// Turn the portal in `folder` on or off.
    Switch { folder: String, on: bool },
    /// Move the portal in `folder` to the Trash, once the user answered Remove….
    Remove { folder: String, origin: Origin },
}

impl PortalsScreen {
    /// The screen is no longer on show: the list sorts again when it opens, Remove…'s ask closes, and Browse asks for
    /// the community list again when it shows. The view, the pick, the search and the form stay.
    pub fn put_away(&mut self) {
        self.order = None;
        self.confirm = None;
        self.browse_opened = false;
    }

    /// Shows `segment`, as its tab does.
    pub fn show_segment(&mut self, segment: Segment) {
        self.segment = segment;
    }

    /// What became of a request. A failure stays under the portal's actions until its next change.
    pub fn settle(&mut self, request: &PortalsRequest, result: Result<(), ChangeError>) {
        let (folder, removing) = match request {
            PortalsRequest::Switch { folder, .. } => (folder, false),
            PortalsRequest::Remove { folder, .. } => (folder, true),
        };
        self.failure = result.err().map(|error| (folder.clone(), failure_words(&error, removing)));
    }
}

/// Why a change failed, in words. A file's reason shows as plain text.
fn failure_words(error: &ChangeError, removing: bool) -> String {
    match error {
        ChangeError::Busy => BUSY.to_owned(),
        ChangeError::BuiltIn => BUILT_IN.to_owned(),
        ChangeError::NoFrontmatter => {
            "Jobdesk could not turn this portal off: its SKILL.md has no settings block at the top to hold the switch. Nothing was changed.".to_owned()
        }
        ChangeError::Failed(error) if removing => format!("Jobdesk could not move this portal to {TRASH}. {error}"),
        ChangeError::Failed(error) => format!("Jobdesk could not turn this portal on or off. {error}"),
    }
}

/// The head, then three views: Your portals, the list beside the picked portal's detail; Browse community; and Add a
/// portal. `state.load` is the portals' own: a folder that could not be read hides them, and never calls them none. While
/// `busy`, every switch and action on your portals waits and says why. Every action that reaches Claude only opens the
/// launcher. Browse asks `community` for its list once each time it comes on show. Returns the switch or removal the user
/// asked for.
pub fn show(
    ui: &mut Ui,
    screen: &mut PortalsScreen,
    state: &ScreenState,
    view: &PortalsView,
    busy: bool,
    mut community: BrowseContext,
) -> Option<PortalsRequest> {
    let reading = state.reading();
    let failed = state.load == Load::Failed;
    let summary = view.summary();
    let subtitle = match () {
        _ if reading => "Reading your portals…".to_owned(),
        _ if failed => LEAD.to_owned(),
        _ if summary.total == 0 => format!("{LEAD} None installed."),
        _ => format!("{LEAD} {} of {} on.", summary.on, summary.total),
    };
    page_head(ui, Title::Slab("Portals"), Some(Subtitle::Text(&subtitle)), |ui| {
        if !reading {
            enabled_when(ui, !busy, BUSY, |ui| command_action(ui, "Check all portals…", CHECK_ALL_LINE));
        }
    });
    page_body(ui, |ui| {
        (state.alerts)(ui);
        let count = (!reading && !failed).then_some(summary.total);
        segments(ui, screen, count);
        ui.add_space(SCREEN_GAP / 2.0);
        match screen.segment {
            Segment::Yours => yours(ui, screen, view, busy, reading, failed),
            Segment::Browse => {
                // Before the list draws, so a read that starts shows as one at once.
                if !screen.browse_opened {
                    community.community.open(community.now_utc);
                    screen.browse_opened = true;
                }
                if browse(ui, &mut screen.query, &view.folders(), &mut community) == Some(BrowseAsk::AddPortal) {
                    screen.segment = Segment::Add;
                }
                None
            }
            Segment::Add => {
                add(ui, &mut screen.form);
                None
            }
        }
    })
}

/// Your portals with its count, once the folder is read, then Browse community and Add a portal.
fn segments(ui: &mut Ui, screen: &mut PortalsScreen, count: Option<usize>) {
    let yours = count.map_or_else(|| "Your portals".to_owned(), |count| format!("Your portals  {count}"));
    let options = [(Segment::Yours, yours), (Segment::Browse, "Browse community".to_owned()), (Segment::Add, "Add a portal".to_owned())];
    if let Some(segment) = segmented(ui, screen.segment, &options) {
        screen.segment = segment;
    }
    if screen.segment != Segment::Browse {
        screen.browse_opened = false;
    }
}

/// Your portals: the list beside the picked portal, or why there is none to show.
fn yours(ui: &mut Ui, screen: &mut PortalsScreen, view: &PortalsView, busy: bool, reading: bool, failed: bool) -> Option<PortalsRequest> {
    if reading {
        columns(ui, loading_rail, loading_detail);
        return None;
    }
    if failed {
        let body = "They come back as soon as Jobdesk can read the folder. A task you start still searches them, because Claude reads the folder itself.";
        empty_note(ui, "Your portals are hidden, not gone", body);
        return None;
    }
    if view.portals.is_empty() {
        let body = "Claude has no job board to search, so finding new jobs falls back to Google alone. Install one from the community, or have Claude build one for a board you use. Portals are folders in .agents/skills, and this one holds none.";
        empty_state(ui, "No portals yet", body, |ui| {
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            if primary_button(ui, "Browse community").clicked() {
                screen.segment = Segment::Browse;
            }
            if button(ui, "Add a portal").clicked() {
                screen.segment = Segment::Add;
            }
        });
        return None;
    }
    list_and_detail(ui, screen, view, busy)
}

fn list_and_detail(ui: &mut Ui, screen: &mut PortalsScreen, view: &PortalsView, busy: bool) -> Option<PortalsRequest> {
    let [on, off] = listed(view, &mut screen.order);
    let first = on.first().or(off.first()).copied()?;
    let portal = screen.picked.as_deref().and_then(|folder| view.portals.iter().find(|portal| portal.folder == folder)).unwrap_or(first);
    let failure = screen.failure.as_ref().filter(|(folder, _)| *folder == portal.folder).map(|(_, words)| words.as_str());
    let (listed, detailed) = columns(
        ui,
        |ui| rail(ui, [&on, &off], &portal.folder, busy),
        |ui| {
            ui.push_id(("portal detail", &portal.folder), |ui| {
                let shown = Detail { portal, plan: &view.plan, plan_read: view.plan_read, busy, failure };
                detail(ui, &shown, &mut screen.confirm)
            })
            .inner
        },
    );
    let (asked, picked) = match (listed.switch, detailed) {
        // A flipped row is picked too, so its detail shows what became of it.
        (Some((folder, on)), _) => (Some(PortalsRequest::Switch { folder: folder.clone(), on }), Some(folder)),
        (None, Some(DetailAsk::Switch(on))) => (Some(PortalsRequest::Switch { folder: portal.folder.clone(), on }), listed.pick),
        (None, Some(DetailAsk::Remove)) => (Some(PortalsRequest::Remove { folder: portal.folder.clone(), origin: portal.origin }), listed.pick),
        (None, None) => (None, listed.pick),
    };
    if let Some(folder) = picked.filter(|folder| *folder != portal.folder) {
        screen.picked = Some(folder);
        screen.confirm = None;
    }
    asked
}

/// The portals in their two groups, on then off, each in the order the list first showed it since the screen opened.
/// A portal that came since joins the end of its group, and one that went is left out.
fn listed<'a>(view: &'a PortalsView, order: &mut Option<Vec<(String, bool)>>) -> [Vec<&'a PortalView>; 2] {
    let order = order.get_or_insert_with(|| {
        let (on, off): (Vec<&PortalView>, Vec<&PortalView>) = view.portals.iter().partition(|portal| portal.on);
        on.into_iter().chain(off).map(|portal| (portal.folder.clone(), portal.on)).collect()
    });
    let group = |on: bool| usize::from(!on);
    let mut groups = [Vec::new(), Vec::new()];
    for (folder, on) in order.iter() {
        if let Some(portal) = view.portals.iter().find(|portal| portal.folder == *folder) {
            groups[group(*on)].push(portal);
        }
    }
    for portal in view.portals.iter().filter(|portal| !order.iter().any(|(folder, _)| *folder == portal.folder)) {
        groups[group(portal.on)].push(portal);
    }
    groups
}

/// The list at the left, at most `LIST_WIDTH`, and the detail beside it, both as tall as the room left.
fn columns<A, B>(ui: &mut Ui, side: impl FnOnce(&mut Ui) -> A, main: impl FnOnce(&mut Ui) -> B) -> (A, B) {
    let area = ui.available_rect_before_wrap();
    let width = LIST_WIDTH.min(area.width() * LIST_SHARE);
    let list_rect = Rect::from_min_max(area.min, pos2(area.left() + width, area.bottom()));
    let page_rect = Rect::from_min_max(pos2((list_rect.right() + f32::from(GUTTER)).min(area.right()), area.top()), area.max);
    let column = |rect: Rect, salt: &str| UiBuilder::new().id_salt(salt).max_rect(rect).layout(Layout::top_down(Align::Min));
    let mut list_ui = ui.new_child(column(list_rect, "portals_list_column"));
    list_ui.set_width(list_rect.width());
    let listed = side(&mut list_ui);
    let mut page_ui = ui.new_child(column(page_rect, "portals_page_column"));
    page_ui.set_width(page_rect.width());
    let detailed = main(&mut page_ui);
    ui.advance_cursor_after_rect(area);
    (listed, detailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portals::application::Quotes;
    use crate::shared_kernel::LoadError;

    fn portal(folder: &str, on: bool) -> PortalView {
        PortalView {
            folder: folder.into(),
            name: folder.into(),
            country: None,
            sentence: None,
            on,
            origin: Origin::MadeByYou,
            badges: vec![],
            jobs: None,
            quotes: Quotes::Unknown,
            safe_name: true,
        }
    }

    fn view(portals: &[(&str, bool)]) -> PortalsView {
        PortalsView { portals: portals.iter().map(|(folder, on)| portal(folder, *on)).collect(), installed_read: true, ..PortalsView::default() }
    }

    fn folders(groups: &[Vec<&PortalView>; 2]) -> [Vec<String>; 2] {
        groups.clone().map(|group| group.iter().map(|portal| portal.folder.clone()).collect())
    }

    /// Remove names the place the system keeps removed files: the Trash on macOS, the Recycle Bin on Windows.
    #[test]
    fn remove_names_the_systems_bin() {
        let expected = if cfg!(target_os = "macos") { "the Trash" } else { "the Recycle Bin" };
        assert_eq!(TRASH, expected);
    }

    #[test]
    fn the_list_puts_portals_on_first() {
        let mut order = None;
        let portals = view(&[("a", false), ("b", true), ("c", true)]);
        assert_eq!(folders(&listed(&portals, &mut order)), [vec!["b".to_owned(), "c".into()], vec!["a".into()]]);
    }

    /// A flipped switch keeps its row in place until the screen opens again, when it joins its new group.
    #[test]
    fn a_flipped_portal_keeps_its_place_until_the_list_sorts_again() {
        let mut order = None;
        listed(&view(&[("a", true), ("b", true)]), &mut order);
        let flipped = view(&[("a", false), ("b", true), ("c", false)]);
        assert_eq!(folders(&listed(&flipped, &mut order)), [vec!["a".to_owned(), "b".into()], vec!["c".into()]], "a stays, and c joins the end");
        let gone = view(&[("b", true)]);
        assert_eq!(folders(&listed(&gone, &mut order)), [vec!["b".to_owned()], vec![]]);
        let mut screen = PortalsScreen { order, ..PortalsScreen::default() };
        screen.put_away();
        assert_eq!(folders(&listed(&flipped, &mut screen.order)), [vec!["b".to_owned()], vec!["a".into(), "c".into()]]);
    }

    #[test]
    fn a_failure_is_kept_for_its_portal_until_the_next_change() {
        let mut screen = PortalsScreen::default();
        let remove = PortalsRequest::Remove { folder: "north-search".into(), origin: Origin::MadeByYou };
        screen.settle(&remove, Err(ChangeError::Failed(LoadError::new(".agents/skills/north-search", "in use"))));
        let (folder, words) = screen.failure.clone().unwrap();
        assert_eq!(folder, "north-search");
        assert!(words.starts_with(&format!("Jobdesk could not move this portal to {TRASH}.")), "{words}");
        assert!(words.contains("in use"), "{words}");
        screen.settle(&PortalsRequest::Switch { folder: "north-search".into(), on: false }, Err(ChangeError::NoFrontmatter));
        assert!(screen.failure.as_ref().unwrap().1.contains("Nothing was changed"));
        screen.settle(&remove, Ok(()));
        assert_eq!(screen.failure, None);
    }
}
