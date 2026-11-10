use chrono::NaiveDate;

use crate::discovery::application::JobBoard;
use crate::portals::domain::{Manifest, Origin, Quotes, SearchPlan, origin};
use crate::shared_kernel::LoadError;

/// The installed portals and the search terms they share, as the Portals screen and Setup show them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PortalsView {
    /// Sorted by name, in any case.
    pub portals: Vec<PortalView>,
    /// False when `.agents/skills` could not be read, so `portals` is not known to be empty.
    pub installed_read: bool,
    pub plan: SearchPlan,
    /// False when the search file could not be read, so `plan` is not known to be empty.
    pub plan_read: bool,
}

/// One installed portal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortalView {
    /// The folder under `.agents/skills`, such as `linkedin-search`.
    pub folder: String,
    /// The board's plain name, such as LinkedIn.
    pub name: String,
    /// The country it searches, when Jobdesk knows it.
    pub country: Option<String>,
    pub sentence: Option<String>,
    /// Whether `/scrape` searches it.
    pub on: bool,
    pub origin: Origin,
    pub badges: Vec<Badge>,
    /// The saved jobs it found and the latest day one was first seen, or `None` when the saved jobs could not be read.
    pub jobs: Option<(usize, Option<NaiveDate>)>,
    pub quotes: Quotes,
    /// The folder name is safe to put on a command line, such as `/scrape health <folder>`: only letters, digits, `_`,
    /// `.` and `-`, and no `-` first.
    pub safe_name: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    NeedsKey,
    PersonalUse,
}

/// Setup's one line: how many portals are on, and the jobs every installed one found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortalSummary {
    pub on: usize,
    pub total: usize,
    /// `None` when the saved jobs could not be read.
    pub jobs_found: Option<usize>,
}

/// The launcher line that checks every installed portal.
pub const CHECK_ALL_LINE: &str = "/scrape health";
/// The launcher line that changes the search terms every portal shares: the `/setup` section the Profile screen's Job
/// search topic opens.
pub const SEARCH_TERMS_LINE: &str = "/setup --section search";

impl PortalView {
    /// `/scrape health <folder>`, only for a folder name that is safe on a command line.
    pub fn check_line(&self) -> Option<String> {
        self.safe_name.then(|| format!("{CHECK_ALL_LINE} {}", self.folder))
    }
}

impl PortalsView {
    /// Jobs found sums every installed portal, on or off.
    pub fn summary(&self) -> PortalSummary {
        PortalSummary {
            on: self.portals.iter().filter(|portal| portal.on).count(),
            total: self.portals.len(),
            jobs_found: self.portals.iter().map(|portal| portal.jobs.map(|(count, _)| count)).sum(),
        }
    }

    /// Names each portal's origin and country again from `community`, the community list's folders, each with its
    /// country, as `Community::installed_hints` gives them. The shell holds that list apart from the folder's snapshot,
    /// so it names it on each snapshot it takes and each time the list changes.
    pub fn set_community(&mut self, community: &[(String, String)]) {
        for portal in &mut self.portals {
            (portal.origin, portal.country) = origin(&portal.folder, community);
        }
    }

    /// The installed portals' folder names, so the community list can say which of its portals are installed.
    pub fn folders(&self) -> Vec<&str> {
        self.portals.iter().map(|portal| portal.folder.as_str()).collect()
    }
}

/// `installed` holds each portal's folder and manifest. `board` is `None` when the saved jobs could not be read. No
/// portal reads Community until `set_community` names the community list.
pub fn portals_view(installed: Result<Vec<(String, Manifest)>, LoadError>, plan: Result<SearchPlan, LoadError>, board: Option<&JobBoard>) -> PortalsView {
    let (installed_read, plan_read) = (installed.is_ok(), plan.is_ok());
    let mut portals: Vec<PortalView> = installed.unwrap_or_default().into_iter().map(|(folder, manifest)| portal_view(folder, manifest, board)).collect();
    portals.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.folder.cmp(&b.folder)));
    PortalsView { portals, installed_read, plan: plan.unwrap_or_default(), plan_read }
}

fn portal_view(folder: String, manifest: Manifest, board: Option<&JobBoard>) -> PortalView {
    let (origin, country) = origin(&folder, &[]);
    let badges = [(manifest.needs_key, Badge::NeedsKey), (manifest.personal_use, Badge::PersonalUse)];
    PortalView {
        name: manifest.name,
        country,
        sentence: manifest.sentence,
        on: manifest.enabled,
        origin,
        badges: badges.into_iter().filter_map(|(applies, badge)| applies.then_some(badge)).collect(),
        jobs: board.map(|board| board.portal_jobs(&folder)),
        quotes: manifest.quotes,
        safe_name: safe_name(&folder),
        folder,
    }
}

fn safe_name(folder: &str) -> bool {
    !folder.is_empty() && !folder.starts_with('-') && folder.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::fixtures::{board, view};
    use crate::discovery::application::{JobView, Lane};
    use crate::portals::domain::parse_manifest;

    fn installed(manifests: &[(&str, &str)]) -> Result<Vec<(String, Manifest)>, LoadError> {
        Ok(manifests.iter().map(|(folder, text)| (folder.to_string(), parse_manifest(text, folder))).collect())
    }

    fn sample() -> Result<Vec<(String, Manifest)>, LoadError> {
        installed(&[
            ("northfield-search", "---\nenabled: false\n---\n\n# Northfield Search Skill\n\nExport `NORTHFIELD_API_TOKEN` first.\n"),
            ("linkedin-search", "# LinkedIn Search Skill\n\n## Personal use only\n"),
            ("-odd name", "# alder Search Skill\n"),
        ])
    }

    fn jobs_board() -> JobBoard {
        let on = |portal: &str, day: u32| JobView { portal: Some(portal.into()), first_seen: NaiveDate::from_ymd_opt(2026, 10, day), ..view(portal) };
        board(vec![(Lane::Shortlist, vec![on("linkedin-search", 28), on("linkedin-search", 30), on("northfield-search", 29)])])
    }

    #[test]
    fn portals_sort_by_name_and_carry_what_they_say() {
        let view = portals_view(sample(), Ok(SearchPlan::default()), Some(&jobs_board()));
        let names: Vec<&str> = view.portals.iter().map(|portal| portal.name.as_str()).collect();
        assert_eq!(names, ["alder", "LinkedIn", "Northfield"]);
        let linkedin = &view.portals[1];
        assert_eq!((linkedin.origin, linkedin.country.as_deref()), (Origin::BuiltIn, Some("Worldwide")));
        assert_eq!(linkedin.badges, [Badge::PersonalUse]);
        assert_eq!(linkedin.jobs, Some((2, NaiveDate::from_ymd_opt(2026, 10, 30))));
        let northfield = &view.portals[2];
        assert_eq!((northfield.origin, northfield.country.as_deref(), northfield.on), (Origin::MadeByYou, None, false));
        assert_eq!(northfield.badges, [Badge::NeedsKey]);
        assert_eq!(view.portals[0].jobs, Some((0, None)));
        assert!(view.installed_read && view.plan_read);
    }

    #[test]
    fn only_plain_folder_names_are_safe() {
        let view = portals_view(sample(), Ok(SearchPlan::default()), None);
        let safe: Vec<(&str, bool)> = view.portals.iter().map(|portal| (portal.folder.as_str(), portal.safe_name)).collect();
        assert_eq!(safe, [("-odd name", false), ("linkedin-search", true), ("northfield-search", true)]);
        assert!(!safe_name("-search") && !safe_name("") && !safe_name("north;rm") && safe_name("north_2.v-search"));
    }

    #[test]
    fn only_a_safe_folder_gets_its_own_check_line() {
        let view = portals_view(sample(), Ok(SearchPlan::default()), None);
        let lines: Vec<Option<String>> = view.portals.iter().map(PortalView::check_line).collect();
        assert_eq!(lines, [None, Some("/scrape health linkedin-search".into()), Some("/scrape health northfield-search".into())]);
    }

    /// Change search terms… opens what the Profile screen's Job search topic opens, as the terms live in one file.
    #[test]
    fn the_search_terms_line_is_the_job_search_topics() {
        use crate::profile::application::TopicId;
        let line = TopicId::JobSearch.spec().setup_section.map(|section| format!("/setup --section {section}"));
        assert_eq!(line.as_deref(), Some(SEARCH_TERMS_LINE));
    }

    #[test]
    fn community_hints_name_the_origin_and_country() {
        let community = [("northfield-search".to_owned(), "Belgium".to_owned())];
        let mut view = portals_view(sample(), Ok(SearchPlan::default()), None);
        assert_eq!(view.portals[2].origin, Origin::MadeByYou);
        view.set_community(&community);
        assert_eq!((view.portals[2].origin, view.portals[2].country.as_deref()), (Origin::Community, Some("Belgium")));
    }

    #[test]
    fn the_summary_counts_portals_on_and_every_portals_jobs() {
        let view = portals_view(sample(), Ok(SearchPlan::default()), Some(&jobs_board()));
        assert_eq!(view.summary(), PortalSummary { on: 2, total: 3, jobs_found: Some(3) }, "an off portal's jobs count too");
        let unread = portals_view(sample(), Ok(SearchPlan::default()), None);
        assert_eq!(unread.summary().jobs_found, None);
        assert!(unread.portals.iter().all(|portal| portal.jobs.is_none()));
    }

    #[test]
    fn a_failed_read_is_not_an_empty_list() {
        let error = || LoadError::new(".agents/skills", "permission denied");
        let view = portals_view(Err(error()), Err(error()), None);
        assert!(view.portals.is_empty());
        assert!(!view.installed_read && !view.plan_read);
    }
}
