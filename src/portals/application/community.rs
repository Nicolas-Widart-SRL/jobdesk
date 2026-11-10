use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

use crate::portals::domain::{CommunityFailure, CommunityList, CommunitySource, ListedPortal, REFUSAL_WAIT, code_url, country_name, install_prompt};

/// A saved list older than this is read again when the community list opens.
pub const STALE_AFTER: TimeDelta = TimeDelta::hours(1);

/// The community portal list: the one saved last, and reads of it from GitHub. Reading it is not a run.
pub struct Community {
    source: Box<dyn CommunitySource>,
    list: Option<CommunityList>,
    reading: bool,
    /// Why the last read failed, until a read succeeds.
    failure: Option<CommunityFailure>,
    /// No read starts before this time: GitHub's rate limit, or a minute after it refused with 403 or 429.
    wait_until: Option<DateTime<Utc>>,
}

/// What the community list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommunityState {
    /// A read is under way.
    Loading,
    /// The last read succeeded, or the saved list has not needed one.
    Ready,
    /// The last read failed, and the saved list shows.
    Offline,
    /// GitHub's limit for requests without sign-in is used up until this time. The saved list shows, if there is one.
    Limited { until: DateTime<Utc> },
    /// No list was ever read, and none is under way.
    NoList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityView {
    pub state: CommunityState,
    /// Why the last read failed, for the words of an offline note.
    pub failure: Option<CommunityFailure>,
    /// Try again stays off until this time, while GitHub asks Jobdesk to wait.
    pub wait_until: Option<DateTime<Utc>>,
    /// When the list shown was read.
    pub saved_at: Option<DateTime<Utc>>,
    /// The user's market first, then Worldwide, then the other countries by name.
    pub groups: Vec<CountryGroup>,
}

/// The community portals of one country.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryGroup {
    /// The country's English name, such as Denmark or Worldwide.
    pub country: String,
    pub your_market: bool,
    /// By name, in any case.
    pub portals: Vec<CommunityPortalView>,
}

/// One portal of the community list. Its name, sentence and owners come from GitHub: untrusted, though already on one
/// line with hidden characters removed. Its address and task are built by Jobdesk from validated parts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityPortalView {
    pub folder: String,
    pub name: String,
    pub sentence: Option<String>,
    pub owners: Vec<String>,
    pub unmaintained_since: Option<NaiveDate>,
    /// A portal of the same folder name is installed.
    pub installed: bool,
    /// View code ↗.
    pub code_url: String,
    /// The task Install… opens in the launcher.
    pub install_prompt: String,
}

impl Community {
    /// `saved` is the list saved last, read at launch without the network.
    pub fn new(source: Box<dyn CommunitySource>, saved: Option<CommunityList>) -> Self {
        Self { source, list: saved, reading: false, failure: None, wait_until: None }
    }

    /// Reads the list again when the user opens it and there is none, or it is over an hour old. Returns whether a read
    /// started. None starts while one is under way or GitHub asks Jobdesk to wait.
    pub fn open(&mut self, now: DateTime<Utc>) -> bool {
        let stale = self.list.as_ref().is_none_or(|list| list.saved_at > now || now - list.saved_at >= STALE_AFTER);
        stale && self.refresh(now)
    }

    /// Reads the list again, unless a read is under way or GitHub asks Jobdesk to wait. Returns whether a read started.
    pub fn refresh(&mut self, now: DateTime<Utc>) -> bool {
        if self.reading || self.wait_until.is_some_and(|until| now < until) {
            return false;
        }
        self.reading = true;
        self.source.fetch(self.list.clone());
        true
    }

    /// Takes the answers that arrived by `now`. A failure keeps the saved list. A 403 or 429 that is not the rate limit
    /// still means wait, so no read starts for a minute after it, as GitHub advises. Returns whether a new list came,
    /// so the installed portals can be named from it again.
    pub fn poll(&mut self, now: DateTime<Utc>) -> bool {
        let mut listed = false;
        while let Some(answer) = self.source.poll() {
            self.reading = false;
            match answer {
                Ok(list) => {
                    listed = true;
                    self.list = Some(list);
                    self.failure = None;
                    self.wait_until = None;
                }
                Err(failure) => {
                    self.failure = Some(failure);
                    self.wait_until = match failure {
                        CommunityFailure::Limited { until } => Some(until),
                        CommunityFailure::Refused(403 | 429) => Some(now + REFUSAL_WAIT),
                        _ => None,
                    };
                }
            }
        }
        listed
    }

    /// `installed` holds the installed portals' folder names. `market` is the user's country, such as Belgium, whose
    /// group comes first. `query` keeps the portals whose name, country or sentence holds it, in any case.
    pub fn view(&self, installed: &[&str], market: Option<&str>, query: &str) -> CommunityView {
        let state = match (self.reading, self.failure, &self.list) {
            (true, _, _) => CommunityState::Loading,
            (false, Some(CommunityFailure::Limited { until }), _) => CommunityState::Limited { until },
            (false, Some(_), Some(_)) => CommunityState::Offline,
            (false, None, Some(_)) => CommunityState::Ready,
            (false, _, None) => CommunityState::NoList,
        };
        let query = query.trim().to_lowercase();
        let mut groups: Vec<CountryGroup> = Vec::new();
        for portal in self.list.iter().flat_map(|list| &list.portals) {
            let country = country_name(&portal.country);
            let found = [Some(portal.name.as_str()), Some(country.as_str()), portal.sentence.as_deref()];
            if !query.is_empty() && !found.into_iter().flatten().any(|text| text.to_lowercase().contains(&query)) {
                continue;
            }
            let view = self.portal_view(portal, installed);
            match groups.iter_mut().find(|group| group.country == country) {
                Some(group) => group.portals.push(view),
                None => {
                    let your_market = market.is_some_and(|market| is_market(market, &country, portal.country.as_str()));
                    groups.push(CountryGroup { country, your_market, portals: vec![view] });
                }
            }
        }
        for group in &mut groups {
            group.portals.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.folder.cmp(&b.folder)));
        }
        groups.sort_by_key(|group| (!group.your_market, group.country != "Worldwide", group.country.to_lowercase()));
        CommunityView { state, failure: self.failure, wait_until: self.wait_until, saved_at: self.list.as_ref().map(|list| list.saved_at), groups }
    }

    fn portal_view(&self, portal: &ListedPortal, installed: &[&str]) -> CommunityPortalView {
        let ownership = self.list.as_ref().map(|list| list.codeowners.ownership(&portal.country, &portal.folder)).unwrap_or_default();
        CommunityPortalView {
            folder: portal.folder.as_str().to_owned(),
            name: portal.name.clone(),
            sentence: portal.sentence.clone(),
            owners: ownership.owners,
            unmaintained_since: ownership.unmaintained_since,
            installed: installed.contains(&portal.folder.as_str()),
            code_url: code_url(&portal.country, &portal.folder),
            install_prompt: install_prompt(&portal.country, &portal.folder),
        }
    }

    /// Each listed folder with its country's name, for the installed portals' origin and country.
    pub fn installed_hints(&self) -> Vec<(String, String)> {
        let portals = self.list.iter().flat_map(|list| &list.portals);
        portals.map(|portal| (portal.folder.as_str().to_owned(), country_name(&portal.country))).collect()
    }
}

/// Whether the user's `market` names this country: by its name in any case, or, when `market` is two letters, by its
/// code. `home_country` gives two letters only when they are the whole Location, never a state after a city.
fn is_market(market: &str, country: &str, code: &str) -> bool {
    let market = market.trim();
    let is_code = market.len() == 2 && market.bytes().all(|b| b.is_ascii_alphabetic());
    market.eq_ignore_ascii_case(country) || is_code && market.eq_ignore_ascii_case(code)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::portals::domain::{CountryCode, FolderName, GitSha, parse_codeowners};
    use crate::portals::infrastructure::ScriptedCommunity;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 9, 0, 0).unwrap()
    }

    fn portal(country: &str, folder: &str, name: &str, sentence: &str) -> ListedPortal {
        let (country, folder) = (CountryCode::parse(country).unwrap(), FolderName::parse(folder).unwrap());
        ListedPortal::from_parts(country, folder, GitSha::parse(&"a".repeat(40)).unwrap(), name, Some(sentence)).unwrap()
    }

    fn list(saved_at: DateTime<Utc>) -> CommunityList {
        CommunityList {
            saved_at,
            portals: vec![
                portal("dk", "jobnet-search", "Jobnet", "Danish listings from Jobnet.dk."),
                portal("global", "freehire-search", "freehire", "Open-source IT job board."),
                portal("be", "northfield-search", "Northfield", "Belgian jobs, Flemish and Walloon."),
                portal("be", "alder-search", "Alder", "Jobs in Wallonia."),
                portal("at", "kestrel-search", "Kestrel", "Austrian listings."),
            ],
            codeowners: parse_codeowners("* @fallback-desk\n/portals/be/alder-search/ @lena-vos # unmaintained since 2025-07\n"),
            codeowners_sha: None,
        }
    }

    fn community(saved: Option<CommunityList>, answers: Vec<Result<CommunityList, CommunityFailure>>) -> Community {
        Community::new(Box::new(ScriptedCommunity::new(answers)), saved)
    }

    fn countries(view: &CommunityView) -> Vec<(&str, bool)> {
        view.groups.iter().map(|group| (group.country.as_str(), group.your_market)).collect()
    }

    #[test]
    fn the_market_comes_first_then_worldwide_then_by_name() {
        let community = community(Some(list(now())), vec![]);
        let view = community.view(&[], Some("belgium"), "");
        assert_eq!(countries(&view), [("Belgium", true), ("Worldwide", false), ("Austria", false), ("Denmark", false)]);
        let names: Vec<&str> = view.groups[0].portals.iter().map(|portal| portal.name.as_str()).collect();
        assert_eq!(names, ["Alder", "Northfield"]);
        assert_eq!(countries(&community.view(&[], Some(" DK "), ""))[0], ("Denmark", true), "a market by its code");
        assert_eq!(countries(&community.view(&[], None, ""))[0], ("Worldwide", false));
        assert_eq!(countries(&community.view(&[], Some("Atlantis"), "")).len(), 4, "a market with no portal adds no group");
    }

    /// The Location of a profile whose Identity holds only that line.
    fn market_of(location: &str) -> Option<String> {
        let text = format!("## Identity\n- **Location:** {location}\n");
        let file = crate::profile::application::ProfileFile::Candidate;
        let (view, _) = crate::profile::application::profile_view(&|read| Ok((read == file).then(|| text.clone())));
        crate::profile::application::home_country(&view)
    }

    #[test]
    fn a_state_after_the_city_is_no_market() {
        let codes = ["ca", "ga", "il", "de", "ma", "co", "dk"];
        let portals = codes.iter().map(|code| portal(code, &format!("{code}-search"), "Board", "Jobs.")).collect();
        let community = community(Some(CommunityList { portals, ..list(now()) }), vec![]);
        let marked = |location: &str| -> Vec<String> {
            let market = market_of(location);
            community.view(&[], market.as_deref(), "").groups.into_iter().filter(|group| group.your_market).map(|group| group.country).collect()
        };
        for location in ["Los Angeles, CA", "Atlanta, GA", "Chicago, IL", "Dover, DE", "Boston, MA", "Denver, CO", "Copenhagen, DK"] {
            assert!(marked(location).is_empty(), "{location}");
        }
        assert_eq!(marked("Copenhagen, denmark"), ["Denmark"], "by name, in any case");
        assert_eq!(marked("DK"), ["Denmark"], "a code alone is the whole Location");
        assert_eq!(marked("Lyon, Canada"), ["Canada"]);
    }

    #[test]
    fn search_matches_name_country_and_sentence() {
        let community = community(Some(list(now())), vec![]);
        let folders = |query: &str| -> Vec<String> {
            community.view(&[], None, query).groups.iter().flat_map(|group| group.portals.iter().map(|portal| portal.folder.clone())).collect()
        };
        assert_eq!(folders("NORTH"), ["northfield-search"]);
        assert_eq!(folders("denmark"), ["jobnet-search"]);
        assert_eq!(folders("wallo"), ["alder-search", "northfield-search"]);
        assert_eq!(folders("worldwide"), ["freehire-search"]);
        assert!(folders("nothing like it").is_empty());
        assert_eq!(folders("  ").len(), 5);
    }

    #[test]
    fn an_entry_carries_its_owners_and_whether_it_is_installed() {
        let community = community(Some(list(now())), vec![]);
        let view = community.view(&["alder-search", "linkedin-search"], Some("Belgium"), "");
        let [alder, northfield] = &view.groups[0].portals[..] else { panic!("{view:?}") };
        assert!(alder.installed && !northfield.installed);
        assert_eq!((alder.owners.as_slice(), alder.unmaintained_since), (&["@lena-vos".to_owned()][..], NaiveDate::from_ymd_opt(2025, 7, 1)));
        assert_eq!((northfield.owners.as_slice(), northfield.unmaintained_since), (&["@fallback-desk".to_owned()][..], None));
        assert_eq!(northfield.code_url, "https://github.com/ai-job-search-dev/community-portals/tree/main/portals/be/northfield-search");
        assert!(
            northfield.install_prompt.starts_with("Copy https://github.com/ai-job-search-dev/community-portals/tree/main/portals/be/northfield-search into")
        );
    }

    #[test]
    fn the_install_task_opens_the_launcher() {
        let community = community(Some(list(now())), vec![]);
        for group in community.view(&[], None, "").groups {
            for portal in group.portals {
                let mut launcher = crate::runs::ui::Launcher::default();
                assert!(launcher.prefill(&portal.install_prompt), "{}", portal.install_prompt);
                assert_eq!(launcher.words(), Some(portal.install_prompt.as_str()));
            }
        }
    }

    #[test]
    fn a_fresh_saved_list_needs_no_read() {
        let mut community = community(Some(list(now() - TimeDelta::minutes(59))), vec![Ok(list(now()))]);
        assert!(!community.open(now()));
        assert_eq!(community.view(&[], None, "").state, CommunityState::Ready);
        assert_eq!(community.view(&[], None, "").saved_at, Some(now() - TimeDelta::minutes(59)));
        assert!(community.refresh(now()), "Try again reads it anyway");
        assert_eq!(community.view(&[], None, "").state, CommunityState::Loading);
        assert!(!community.refresh(now()), "never two reads at once");
        community.poll(now());
        assert_eq!((community.view(&[], None, "").state, community.view(&[], None, "").saved_at), (CommunityState::Ready, Some(now())));
    }

    #[test]
    fn an_old_missing_or_future_list_is_read_at_open() {
        for saved in [None, Some(list(now() - TimeDelta::hours(1))), Some(list(now() + TimeDelta::days(1)))] {
            let mut community = community(saved, vec![Ok(list(now()))]);
            assert!(community.open(now()));
            community.poll(now());
            assert_eq!(community.view(&[], None, "").saved_at, Some(now()));
        }
        assert_eq!(community(None, vec![]).view(&[], None, "").state, CommunityState::NoList);
    }

    #[test]
    fn a_failure_keeps_the_saved_list() {
        let saved = list(now() - TimeDelta::hours(3));
        let mut community = community(Some(saved.clone()), vec![Err(CommunityFailure::TimedOut), Ok(list(now()))]);
        assert!(community.open(now()));
        community.poll(now());
        let view = community.view(&[], None, "");
        assert_eq!((view.state, view.failure, view.saved_at), (CommunityState::Offline, Some(CommunityFailure::TimedOut), Some(saved.saved_at)));
        assert_eq!(view.groups.len(), 4);
        assert!(community.refresh(now()));
        community.poll(now());
        let view = community.view(&[], None, "");
        assert_eq!((view.state, view.failure), (CommunityState::Ready, None));
        let mut empty = super::tests::community(None, vec![Err(CommunityFailure::Unreachable)]);
        empty.open(now());
        empty.poll(now());
        let view = empty.view(&[], None, "");
        assert_eq!((view.state, view.groups.len()), (CommunityState::NoList, 0));
    }

    #[test]
    fn a_rate_limit_refuses_reads_until_its_reset() {
        let until = now() + TimeDelta::minutes(20);
        let mut community = community(None, vec![Err(CommunityFailure::Limited { until }), Ok(list(now()))]);
        assert!(community.open(now()));
        community.poll(now());
        assert_eq!(community.view(&[], None, "").state, CommunityState::Limited { until });
        assert!(!community.refresh(now() + TimeDelta::minutes(19)));
        assert!(!community.open(now() + TimeDelta::minutes(19)));
        assert!(community.refresh(until));
        community.poll(now());
        assert_eq!(community.view(&[], None, "").state, CommunityState::Ready);
    }

    #[test]
    fn a_bare_refusal_403_or_429_waits_a_minute() {
        for code in [403, 429] {
            let refused = Err(CommunityFailure::Refused(code));
            let mut community = community(Some(list(now() - TimeDelta::hours(2))), vec![refused, Ok(list(now()))]);
            assert!(community.open(now()));
            community.poll(now());
            let view = community.view(&[], None, "");
            assert_eq!((view.state, view.failure), (CommunityState::Offline, Some(CommunityFailure::Refused(code))), "it stays a refusal");
            assert_eq!(view.wait_until, Some(now() + TimeDelta::minutes(1)));
            assert!(!community.refresh(now() + TimeDelta::seconds(59)));
            assert!(community.refresh(now() + TimeDelta::minutes(1)));
            community.poll(now() + TimeDelta::minutes(1));
            assert_eq!(community.view(&[], None, "").wait_until, None);
        }
        let mut down = community(None, vec![Err(CommunityFailure::Refused(503))]);
        down.open(now());
        down.poll(now());
        assert!(down.refresh(now()), "another refusal waits for nothing");
    }

    #[test]
    fn hints_name_each_listed_folder_and_its_country() {
        let hints = community(Some(list(now())), vec![]).installed_hints();
        assert!(hints.contains(&("northfield-search".to_owned(), "Belgium".to_owned())));
        assert!(hints.contains(&("freehire-search".to_owned(), "Worldwide".to_owned())));
        assert!(community(None, vec![]).installed_hints().is_empty());
    }
}
