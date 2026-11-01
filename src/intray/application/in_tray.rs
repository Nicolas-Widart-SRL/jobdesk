use chrono::NaiveDate;

use crate::discovery::application::Lane;
use crate::intray::application::compose::{Sources, derive_cards};
use crate::intray::domain::{Action, Card, CardKind, Decision, DecisionStore, Effect, Outcome, SkipReason, HORIZON_DAYS, rank};
use crate::shared_kernel::WebUrl;

/// What a card asks of the rest of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Command(String),
    Dossier(String),
    Run(String),
    Posting(WebUrl),
    Copy(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleared {
    pub name: String,
    pub label: String,
}

/// A dated card on the horizon strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub on: NaiveDate,
    pub label: String,
    pub key: String,
    pub kind: CardKind,
    /// False once decided. A card set aside still counts, so its date stays in view.
    pub active: bool,
}

/// The tray as dealt from one snapshot, top card first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Queue {
    pub cards: Vec<Card>,
    /// One name per card set aside. These cards are neither pending nor cleared.
    pub set_aside: Vec<String>,
    /// Today's log, newest first.
    pub cleared: Vec<Cleared>,
    /// Today's last decision, while it can be undone. Dealt with the log, so a logged one is always its newest entry.
    pub undo: Option<UndoOffer>,
    pub marks: Vec<Mark>,
    pub unranked: usize,
    pub today: NaiveDate,
}

impl Queue {
    /// Two cards of one company are named once, in the order they first appear.
    pub fn set_aside_distinct(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for name in &self.set_aside {
            if !names.contains(&name.as_str()) {
                names.push(name);
            }
        }
        names
    }

    pub fn set_aside_names(&self) -> String {
        self.set_aside_distinct().join(", ")
    }

    /// Pending cards whose date has passed. A card due today is not overdue.
    pub fn overdue(&self) -> usize {
        self.cards.iter().filter(|card| card.due.as_ref().is_some_and(|due| due.on < self.today)).count()
    }

    /// The two biggest kinds, most first. With more than two kinds the second line folds the rest into `None`, meaning other.
    pub fn by_kind(&self) -> Vec<(usize, Option<CardKind>)> {
        let mut counts: Vec<(usize, CardKind)> = Vec::new();
        for card in &self.cards {
            match counts.iter_mut().find(|(_, kind)| *kind == card.kind) {
                Some((count, _)) => *count += 1,
                None => counts.push((1, card.kind)),
            }
        }
        counts.sort_by_key(|&(count, kind)| (std::cmp::Reverse(count), kind));
        if counts.len() > 2 {
            let rest = counts[1..].iter().map(|(count, _)| count).sum();
            counts.truncate(1);
            let mut lines: Vec<(usize, Option<CardKind>)> = counts.into_iter().map(|(count, kind)| (count, Some(kind))).collect();
            lines.push((rest, None));
            return lines;
        }
        counts.into_iter().map(|(count, kind)| (count, Some(kind))).collect()
    }

    /// Says so when nothing, set aside or not, is dated inside the horizon.
    pub fn horizon_note(&self) -> Option<String> {
        if !self.marks.is_empty() {
            return None;
        }
        let other = if self.overdue() > 0 { "other " } else { "" };
        Some(format!("No {other}deadlines in the next {HORIZON_DAYS} days"))
    }

    /// Today's log under the undo offer. A logged offer already stands for the newest entry, so that entry is left out.
    pub fn log_under(&self) -> &[Cleared] {
        let logged = self.undo.as_ref().is_some_and(|undo| undo.logged);
        self.cleared.get(usize::from(logged)..).unwrap_or(&[])
    }

    pub fn headline(&self) -> String {
        let minutes: usize = self.cards.iter().map(|card| card.kind.minutes()).sum();
        match self.cards.len() {
            0 if self.set_aside.is_empty() => "Nothing needs you".to_owned(),
            0 => format!("{} set aside", self.set_aside.len()),
            1 => format!("1 decision, about {minutes} {}", if minutes == 1 { "minute" } else { "minutes" }),
            count => format!("{count} decisions, about {minutes} minutes"),
        }
    }
}

/// The last decision, for as long as it can be taken back: until the next one, and only on the day it was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOffer {
    pub name: String,
    pub note: String,
    /// The whole offer in one sentence, for screen readers.
    pub spoken: String,
    /// Whether the decision is also in today's log. Later never is.
    pub logged: bool,
    pub asks_reason: bool,
    pub reason: Option<SkipReason>,
}

/// Owns the stored decisions and deals the queue from them.
pub struct InTray {
    store: Option<Box<dyn DecisionStore>>,
    decisions: Vec<Decision>,
    can_undo: bool,
    reading: Option<String>,
    /// Cards brought back together. They deal ahead of the rest until each is decided, for this session and day only.
    returned: Vec<String>,
    returned_on: Option<NaiveDate>,
    /// Whether the last decision was on a card brought back, so undoing it puts the card back in front.
    undo_returned: bool,
    issue: Option<String>,
}

impl InTray {
    /// A store that cannot be read is left alone, so saving cannot overwrite what it holds.
    pub fn new(store: Box<dyn DecisionStore>) -> Self {
        match store.load() {
            Ok(decisions) => Self { store: Some(store), decisions, ..Self::unsaved() },
            Err(error) => Self { issue: Some(error.to_string()), ..Self::unsaved() },
        }
    }

    /// Decisions last until the window closes. For a machine with no app data folder, and tests.
    pub fn unsaved() -> Self {
        Self { store: None, decisions: Vec::new(), can_undo: false, reading: None, returned: Vec::new(), returned_on: None, undo_returned: false, issue: None }
    }

    pub fn issue(&self) -> Option<&str> {
        self.issue.as_deref()
    }

    fn is_cleared(&self, key: &str, today: NaiveDate) -> bool {
        self.decisions.iter().any(|decision| decision.key == key && decision.clears_card(today))
    }

    fn is_set_aside(&self, key: &str, today: NaiveDate) -> bool {
        self.decisions.iter().any(|decision| decision.key == key && decision.sets_aside(today))
    }

    fn was_kept(&self, key: &str) -> bool {
        self.decisions.iter().any(|decision| decision.key == key && decision.outcome == Outcome::Kept)
    }

    fn returned(&self, today: NaiveDate) -> &[String] {
        if self.returned_on == Some(today) { &self.returned } else { &[] }
    }

    /// Cards brought back on another day deal by rank again.
    fn forget_returns_before(&mut self, today: NaiveDate) {
        if self.returned_on != Some(today) {
            self.returned.clear();
            self.returned_on = None;
        }
    }

    /// Derives the cards again. Paused runs come first, then cards brought back today, each in rank order.
    /// The card being read stays on top of them all, so new work lands behind it.
    pub fn queue(&self, sources: &Sources) -> Queue {
        let today = sources.today;
        let derived = derive_cards(sources, |key| self.was_kept(key));
        let marks = derived
            .iter()
            .filter(|card| card.is_on_horizon(today))
            .filter_map(|card| {
                let due = card.due.as_ref()?;
                Some(Mark { on: due.on, label: due.label.clone(), key: card.key.clone(), kind: card.kind, active: !self.is_cleared(&card.key, today) })
            })
            .collect();
        let (aside, mut cards): (Vec<Card>, Vec<Card>) = derived.into_iter().filter(|card| !self.is_cleared(&card.key, today)).partition(|card| self.is_set_aside(&card.key, today));
        let set_aside = aside.into_iter().map(|card| card.name).collect();
        rank(&mut cards, today);
        let returned = self.returned(today);
        cards.sort_by_key(|card| if card.pauses_a_run() { 0 } else if returned.contains(&card.key) { 1 } else { 2 });
        if let Some(at) = self.reading.as_ref().and_then(|key| cards.iter().position(|card| &card.key == key)) {
            cards[..=at].rotate_right(1);
        }
        let cleared = self.decisions.iter().rev().filter(|decision| decision.is_logged_on(today)).map(|decision| Cleared { name: decision.name.clone(), label: decision.label() }).collect();
        Queue { cards, set_aside, cleared, undo: self.undo_offer(today), marks, unranked: sources.board.count(Lane::New), today }
    }

    /// Puts this card on top and keeps it there until it is decided.
    pub fn read(&mut self, key: &str) {
        if self.reading.as_deref() != Some(key) {
            self.reading = Some(key.to_owned());
        }
    }

    /// Away from the tray nothing is being read, so the next visit starts from the ranking.
    pub fn stop_reading(&mut self) {
        self.reading = None;
    }

    fn record(&mut self, card: &Card, outcome: Outcome, reason: Option<SkipReason>, today: NaiveDate) {
        self.decisions.push(Decision { key: card.key.clone(), name: card.name.clone(), outcome, reason, on: today });
        self.can_undo = true;
        self.reading = None;
        self.forget_returns_before(today);
        self.undo_returned = self.returned(today).contains(&card.key);
        self.returned.retain(|key| key != &card.key);
        self.save();
    }

    fn save(&mut self) {
        let result = self.store.as_ref().map_or(Ok(()), |store| store.save(&self.decisions));
        self.issue = result.err().map(|error| error.to_string());
    }

    /// Drops the matching decisions. Undo stays on offer only if its decision is still the last.
    fn take_back(&mut self, matches: impl Fn(&Decision) -> bool) {
        let last_stays = self.decisions.last().is_some_and(|last| !matches(last));
        self.decisions.retain(|decision| !matches(decision));
        self.can_undo &= last_stays;
        self.save();
    }

    /// Puts a card on top, returning it first if it was set aside.
    pub fn pull_forward(&mut self, key: &str, today: NaiveDate) {
        if self.is_set_aside(key, today) {
            self.take_back(|decision| decision.key == key && decision.sets_aside(today));
        }
        self.read(key);
    }

    /// Returns every card set aside today ahead of the rest, the card being read included.
    pub fn bring_back_all(&mut self, today: NaiveDate) {
        self.forget_returns_before(today);
        self.returned_on = Some(today);
        for decision in self.decisions.iter().filter(|decision| decision.sets_aside(today)) {
            if !self.returned.contains(&decision.key) {
                self.returned.push(decision.key.clone());
            }
        }
        self.take_back(|decision| decision.sets_aside(today));
        self.reading = None;
    }

    pub fn later(&mut self, card: &Card, today: NaiveDate) {
        self.record(card, Outcome::Later, None, today);
    }

    pub fn skip(&mut self, card: &Card, today: NaiveDate) {
        if card.can_skip {
            self.record(card, Outcome::Skipped, None, today);
        }
    }

    /// Only a card's own actions come here, and none of them runs anything by itself.
    pub fn follow(&mut self, card: &Card, action: &Action, today: NaiveDate) -> Option<Request> {
        match &action.effect {
            Effect::Command(line) => {
                self.record(card, Outcome::Followed, None, today);
                Some(Request::Command(line.clone()))
            }
            Effect::Skip(reason) => {
                self.record(card, Outcome::Skipped, Some(*reason), today);
                None
            }
            Effect::Keep => {
                self.record(card, Outcome::Kept, None, today);
                None
            }
            Effect::Dossier(folder) => Some(Request::Dossier(folder.clone())),
            Effect::Run(id) => Some(Request::Run(id.clone())),
            Effect::Posting(url) => Some(Request::Posting(url.clone())),
            Effect::Copy(text) => Some(Request::Copy(text.clone())),
        }
    }

    /// The decision undo would take back. One made on another day stays, so undo never reaches past midnight.
    fn undoable(&self, today: NaiveDate) -> Option<&Decision> {
        self.decisions.last().filter(|last| self.can_undo && last.on == today)
    }

    fn undo_offer(&self, today: NaiveDate) -> Option<UndoOffer> {
        let last = self.undoable(today)?;
        let asks_reason = last.outcome == Outcome::Skipped;
        let note = match last.outcome {
            Outcome::Later => "Set aside until tomorrow".to_owned(),
            _ if asks_reason && last.reason.is_none() => "Skipped. Why? Optional.".to_owned(),
            _ => last.label(),
        };
        let spoken = format!("Last decision: {}, {}. Press U to undo.", last.outcome.verb(), last.name);
        Some(UndoOffer { name: last.name.clone(), note, spoken, logged: last.is_logged_on(today), asks_reason, reason: last.reason })
    }

    /// The card returns to the top, the way it was before the decision.
    pub fn undo(&mut self, today: NaiveDate) {
        if self.undoable(today).is_none() {
            return;
        }
        self.can_undo = false;
        self.reading = self.decisions.pop().map(|decision| decision.key);
        if let Some(key) = self.reading.clone().filter(|key| self.undo_returned && !self.returned.contains(key)) {
            self.returned.push(key);
        }
        self.undo_returned = false;
        self.save();
    }

    /// Names why the last skip was made, while undo still offers it.
    pub fn give_reason(&mut self, reason: SkipReason, today: NaiveDate) {
        if self.undoable(today).is_none_or(|last| last.outcome != Outcome::Skipped) {
            return;
        }
        if let Some(last) = self.decisions.last_mut() {
            last.reason = Some(reason);
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::discovery::application::{FreelanceGates, Gate, GateReading, JobBoard, JobView, LaneView};
    use crate::intray::domain::{Body, Due};
    use crate::runs::application::{AgentEvent, Prompt, PromptKind, Run, RunRequest};
    use crate::shared_kernel::LoadError;
    use crate::tracking::application::{ApplicationCard, Pipeline, PipelineLane, ReachedStage, StatusBucket};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn job(company: &str, score: u8) -> JobView {
        let read = || GateReading { gate: Gate::Pass, value: "x".into() };
        JobView {
            key: company.to_lowercase(), title: "Lead".into(), company: company.into(), location: None, url: None, portal: None,
            deadline: None, days_to_deadline: None, score: Some(score), verdict: None, scraper_fit: None,
            location_gate: None, language_gate: None, language_note: None,
            freelance: FreelanceGates { contract: read(), day_rate: read(), commute: read() },
            guard: Some("clean".into()), strengths: vec![], gaps: vec![], fetch_attempts: None, fetch_error: None,
            signals: vec![], closing_soon: false, guard_flagged: false,
        }
    }

    fn board(jobs: Vec<JobView>) -> JobBoard {
        JobBoard { lanes: vec![LaneView { lane: Lane::Shortlist, jobs }], total_seen: 2 }
    }

    fn dealt(tray: &InTray, board: &JobBoard) -> Vec<String> {
        queue(tray, board, today()).cards.iter().map(|card| card.name.clone()).collect()
    }

    fn queue(tray: &InTray, board: &JobBoard, today: NaiveDate) -> Queue {
        tray.queue(&Sources { board, pipeline: &Pipeline::default(), archives: &HashMap::new(), runs: &[], today })
    }

    fn top(tray: &InTray, board: &JobBoard) -> Card {
        queue(tray, board, today()).cards.remove(0)
    }

    #[derive(Default, Clone)]
    struct Memory(Rc<RefCell<Vec<Decision>>>);

    impl DecisionStore for Memory {
        fn load(&self) -> Result<Vec<Decision>, LoadError> {
            Ok(self.0.borrow().clone())
        }
        fn save(&self, decisions: &[Decision]) -> Result<(), LoadError> {
            *self.0.borrow_mut() = decisions.to_vec();
            Ok(())
        }
    }

    struct Corrupt;

    impl DecisionStore for Corrupt {
        fn load(&self) -> Result<Vec<Decision>, LoadError> {
            Err(LoadError::new("decisions.json", "not JSON"))
        }
        fn save(&self, _: &[Decision]) -> Result<(), LoadError> {
            panic!("a store that could not be read must not be overwritten");
        }
    }

    #[test]
    fn later_returns_the_card_tomorrow() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 80)]);
        tray.later(&top(&tray, &jobs), today());
        assert!(dealt(&tray, &jobs).is_empty());
        assert_eq!(queue(&tray, &jobs, today().succ_opt().unwrap()).cards.len(), 1);
    }

    #[test]
    fn skip_stays_and_undo_restores_the_card_on_top() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        tray.skip(&top(&tray, &jobs), today());
        assert_eq!(dealt(&tray, &jobs), ["Beta"]);

        tray.read("triage:beta");
        tray.undo(today());

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Beta"]);
        assert_eq!(tray.undo_offer(today()), None);
    }

    #[test]
    fn only_the_last_decision_can_be_undone() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        tray.skip(&top(&tray, &jobs), today());
        tray.skip(&top(&tray, &jobs), today());
        tray.undo(today());
        tray.undo(today());
        assert_eq!(dealt(&tray, &jobs), ["Beta"]);
    }

    #[test]
    fn skip_needs_a_card_that_allows_it() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        let card = Card { can_skip: false, ..top(&tray, &jobs) };
        tray.skip(&card, today());
        assert_eq!(dealt(&tray, &jobs), ["Acme"]);
    }

    #[test]
    fn a_new_higher_card_lands_behind_the_one_being_read() {
        let mut tray = InTray::unsaved();
        tray.read(&top(&tray, &board(vec![job("Calm", 70)])).key);

        assert_eq!(dealt(&tray, &board(vec![job("Calm", 70), job("Urgent", 95)])), ["Calm", "Urgent"]);

        tray.stop_reading();
        assert_eq!(dealt(&tray, &board(vec![job("Urgent", 95), job("Calm", 70)])), ["Urgent", "Calm"]);
    }

    #[test]
    fn deciding_moves_on_to_the_ranking() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Second", 90), job("Third", 80)]);
        tray.read("triage:third");
        tray.later(&top(&tray, &jobs), today());
        assert_eq!(dealt(&tray, &jobs), ["Top", "Second"]);
    }

    #[test]
    fn a_followed_command_goes_to_the_launcher_and_can_be_undone() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        let card = top(&tray, &jobs);
        let request = tray.follow(&card, card.primary.as_ref().unwrap(), today());
        assert_eq!(request, Some(Request::Command("/apply".into())));
        assert!(dealt(&tray, &jobs).is_empty());
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!((offer.name.as_str(), offer.note.as_str()), ("Acme", "Command opened"));
        tray.undo(today());
        assert_eq!(dealt(&tray, &jobs), ["Acme"]);
    }

    #[test]
    fn opening_a_dossier_decides_nothing() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        let card = top(&tray, &jobs);
        let action = Action::new("Open dossier", Effect::Dossier("acme".into()));
        assert_eq!(tray.follow(&card, &action, today()), Some(Request::Dossier("acme".into())));
        assert_eq!(dealt(&tray, &jobs), ["Acme"]);
        assert_eq!(tray.undo_offer(today()), None);
    }

    #[test]
    fn a_skip_reason_is_stored_and_a_store_survives_a_reopen() {
        let memory = Memory::default();
        let jobs = board(vec![job("Acme", 90)]);
        let mut tray = InTray::new(Box::new(memory.clone()));
        tray.skip(&top(&tray, &jobs), today());
        assert!(tray.undo_offer(today()).unwrap().asks_reason);
        tray.give_reason(SkipReason::Domain, today());

        let reopened = InTray::new(Box::new(memory.clone()));
        assert!(dealt(&reopened, &jobs).is_empty());
        assert_eq!(memory.0.borrow()[0].reason, Some(SkipReason::Domain));
        assert_eq!(queue(&reopened, &jobs, today()).cleared[0].label, "Skipped, domain");
    }

    #[test]
    fn an_unreadable_store_reports_and_stays_untouched() {
        let mut tray = InTray::new(Box::new(Corrupt));
        assert!(tray.issue().unwrap().contains("not JSON"));
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&top(&tray, &jobs), today());
        assert!(dealt(&tray, &jobs).is_empty());
    }

    #[test]
    fn a_mark_goes_quiet_only_when_its_card_is_cleared() {
        let mut tray = InTray::unsaved();
        let on = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let jobs = board(vec![JobView { deadline: Some(on), days_to_deadline: Some(5), ..job("Meridian Rail", 80) }]);
        assert_eq!(queue(&tray, &jobs, today()).marks[0].label, "Meridian closes");
        let card = top(&tray, &jobs);

        tray.later(&card, today());
        assert!(queue(&tray, &jobs, today()).marks[0].active);

        tray.undo(today());
        tray.skip(&card, today());
        assert!(!queue(&tray, &jobs, today()).marks[0].active);
    }

    #[test]
    fn a_card_set_aside_is_neither_pending_nor_cleared() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.later(&top(&tray, &jobs), today());

        let aside = queue(&tray, &jobs, today());
        assert!(aside.cards.is_empty() && aside.cleared.is_empty());
        assert_eq!(aside.set_aside, ["Acme"]);
        assert_eq!(aside.headline(), "1 set aside");
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!((offer.name.as_str(), offer.note.as_str()), ("Acme", "Set aside until tomorrow"));
    }

    #[test]
    fn a_mark_pulls_a_set_aside_card_back_to_the_top() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80), job("Gamma", 70)]);
        tray.later(&top(&tray, &jobs), today());
        tray.later(&top(&tray, &jobs), today());

        tray.pull_forward("triage:acme", today());

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Gamma"]);
        assert_eq!(queue(&tray, &jobs, today()).set_aside, ["Beta"]);
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!((offer.name.as_str(), offer.note.as_str()), ("Beta", "Set aside until tomorrow"), "the last decision still stands");
    }

    #[test]
    fn pulling_back_the_last_card_set_aside_ends_its_undo() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&Card { key: "other".into(), name: "Older".into(), ..top(&tray, &jobs) }, today());
        tray.later(&top(&tray, &jobs), today());

        tray.pull_forward("triage:acme", today());

        assert_eq!(tray.undo_offer(today()), None);
        tray.undo(today());
        assert_eq!(dealt(&tray, &jobs), ["Acme"]);
        assert_eq!(queue(&tray, &jobs, today()).cleared.len(), 1, "the older skip stands");
    }

    #[test]
    fn bring_them_back_returns_every_card_set_aside() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        tray.later(&top(&tray, &jobs), today());
        tray.later(&top(&tray, &jobs), today());
        assert_eq!(queue(&tray, &jobs, today()).headline(), "2 set aside");

        tray.bring_back_all(today());

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Beta"]);
        assert!(queue(&tray, &jobs, today()).set_aside.is_empty());
    }

    fn later_on(tray: &mut InTray, jobs: &JobBoard, key: &str) {
        let card = queue(tray, jobs, today()).cards.into_iter().find(|card| card.key == key).unwrap();
        tray.later(&card, today());
    }

    #[test]
    fn cards_brought_back_deal_ahead_of_the_rest_in_rank_order() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Acme", 80), job("Beta", 70)]);
        later_on(&mut tray, &jobs, "triage:beta");
        later_on(&mut tray, &jobs, "triage:acme");
        tray.read("triage:top");

        tray.bring_back_all(today());

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Beta", "Top"]);
    }

    #[test]
    fn a_card_brought_back_stays_ahead_until_it_is_decided() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Acme", 80), job("Beta", 70)]);
        later_on(&mut tray, &jobs, "triage:acme");
        later_on(&mut tray, &jobs, "triage:beta");
        tray.bring_back_all(today());

        tray.skip(&top(&tray, &jobs), today());

        assert_eq!(dealt(&tray, &jobs), ["Beta", "Top"]);
        tray.skip(&top(&tray, &jobs), today());
        assert_eq!(dealt(&tray, &jobs), ["Top"]);
    }

    #[test]
    fn a_cleared_tray_brings_its_cards_back_ahead_of_new_work() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 80), job("Beta", 70)]);
        tray.later(&top(&tray, &jobs), today());
        tray.later(&top(&tray, &jobs), today());
        assert!(dealt(&tray, &jobs).is_empty());

        tray.bring_back_all(today());

        let grown = board(vec![job("Urgent", 99), job("Acme", 80), job("Beta", 70)]);
        assert_eq!(dealt(&tray, &grown), ["Acme", "Beta", "Urgent"]);
    }

    #[test]
    fn a_paused_run_still_deals_ahead_of_cards_brought_back() {
        let runs = [paused_run()];
        let jobs = board(vec![job("Top", 95), job("Acme", 80), job("Beta", 70)]);
        let pipeline = Pipeline::default();
        let archives = HashMap::new();
        let sources = Sources { board: &jobs, pipeline: &pipeline, archives: &archives, runs: &runs, today: today() };
        let mut tray = InTray::unsaved();
        for key in ["triage:beta", "triage:acme"] {
            let card = tray.queue(&sources).cards.into_iter().find(|card| card.key == key).unwrap();
            tray.later(&card, today());
        }

        tray.bring_back_all(today());

        let kinds: Vec<(CardKind, String)> = tray.queue(&sources).cards.into_iter().map(|card| (card.kind, card.name)).collect();
        assert_eq!(kinds[0].0, CardKind::Question);
        assert_eq!(kinds[1..].iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>(), ["Acme", "Beta", "Top"]);
    }

    #[test]
    fn a_paused_run_brought_back_keeps_its_rank_among_paused_runs() {
        let permission = PromptKind::Permission { tool: "Bash".into(), detail: "ls".into(), can_remember: false };
        let runs = [paused_on("r1", PromptKind::Questions(vec![])), paused_on("r2", permission)];
        let jobs = board(vec![job("Acme", 80)]);
        let pipeline = Pipeline::default();
        let archives = HashMap::new();
        let sources = Sources { board: &jobs, pipeline: &pipeline, archives: &archives, runs: &runs, today: today() };
        let kinds = |tray: &InTray| tray.queue(&sources).cards.into_iter().map(|card| card.kind).collect::<Vec<_>>();
        let mut tray = InTray::unsaved();
        assert_eq!(kinds(&tray), [CardKind::Permission, CardKind::Question, CardKind::Triage]);

        let question = tray.queue(&sources).cards.into_iter().find(|card| card.kind == CardKind::Question).unwrap();
        tray.later(&question, today());
        tray.bring_back_all(today());

        assert_eq!(kinds(&tray), [CardKind::Permission, CardKind::Question, CardKind::Triage]);
    }

    #[test]
    fn a_turn_card_ranks_with_paused_runs() {
        let request = RunRequest { run_id: "r2".into(), line: "/apply x".into(), model: None, budget_usd: 3.0, started_at: String::new(), resume_session: None };
        let mut waiting = Run::new(&request);
        waiting.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 1 });
        let runs = [waiting, paused_run()];
        let jobs = board(vec![job("Acme", 80)]);
        let pipeline = Pipeline::default();
        let archives = HashMap::new();
        let sources = Sources { board: &jobs, pipeline: &pipeline, archives: &archives, runs: &runs, today: today() };

        let kinds: Vec<CardKind> = InTray::unsaved().queue(&sources).cards.into_iter().map(|card| card.kind).collect();

        assert_eq!(kinds, [CardKind::Question, CardKind::Turn, CardKind::Triage]);
    }

    #[test]
    fn undoing_a_decision_on_a_card_brought_back_keeps_it_in_front() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Acme", 80)]);
        later_on(&mut tray, &jobs, "triage:acme");
        tray.bring_back_all(today());
        tray.skip(&top(&tray, &jobs), today());
        assert_eq!(dealt(&tray, &jobs), ["Top"]);

        tray.undo(today());
        tray.stop_reading();

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Top"]);
    }

    #[test]
    fn undoing_a_decision_on_a_plain_card_does_not_bring_it_forward() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Acme", 80), job("Beta", 70)]);
        later_on(&mut tray, &jobs, "triage:beta");
        tray.bring_back_all(today());
        let plain = queue(&tray, &jobs, today()).cards.into_iter().find(|card| card.key == "triage:top").unwrap();
        tray.skip(&plain, today());

        tray.undo(today());
        tray.stop_reading();

        assert_eq!(dealt(&tray, &jobs), ["Beta", "Top", "Acme"]);
    }

    #[test]
    fn cards_brought_back_deal_by_rank_again_the_next_day() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Top", 95), job("Acme", 80)]);
        later_on(&mut tray, &jobs, "triage:acme");
        tray.bring_back_all(today());
        assert_eq!(dealt(&tray, &jobs), ["Acme", "Top"]);

        let tomorrow = today().succ_opt().unwrap();
        let names = |tray: &InTray| queue(tray, &jobs, tomorrow).cards.into_iter().map(|card| card.name).collect::<Vec<_>>();
        assert_eq!(names(&tray), ["Top", "Acme"]);

        let other = Card { key: "other".into(), ..top(&tray, &jobs) };
        tray.skip(&other, tomorrow);
        assert!(tray.returned.is_empty() && tray.returned_on.is_none(), "a decision on a new day drops the old returns");
        assert_eq!(names(&tray), ["Top", "Acme"]);
    }

    #[test]
    fn a_reason_updates_the_offer_note() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&top(&tray, &jobs), today());
        assert_eq!(tray.undo_offer(today()).unwrap().note, "Skipped. Why? Optional.");

        tray.give_reason(SkipReason::Scope, today());
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!(offer.note, "Skipped, scope");
        assert_eq!(offer.reason, Some(SkipReason::Scope));
        assert_eq!(offer.spoken, "Last decision: Skipped, Acme. Press U to undo.");
    }

    #[test]
    fn later_is_offered_but_not_logged() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.later(&top(&tray, &jobs), today());
        let offer = tray.undo_offer(today()).unwrap();
        assert!(!offer.logged);
        assert!(offer.note.contains("until tomorrow"), "{}", offer.note);
        assert_eq!(offer.spoken, "Last decision: Set aside, Acme. Press U to undo.");
    }

    #[test]
    fn a_logged_offer_is_the_first_cleared_row() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80), job("Gamma", 70)]);
        tray.skip(&top(&tray, &jobs), today());
        tray.skip(&top(&tray, &jobs), today());
        let dealt = queue(&tray, &jobs, today());
        let offer = dealt.undo.as_ref().unwrap();
        assert!(offer.logged);
        assert_eq!(dealt.cleared[0].name, offer.name);
        let under: Vec<&str> = dealt.log_under().iter().map(|row| row.name.as_str()).collect();
        assert_eq!(under, ["Acme"], "the offer stands for its own entry");

        tray.undo(today());
        tray.later(&top(&tray, &jobs), today());
        let dealt = queue(&tray, &jobs, today());
        let offer = dealt.undo.as_ref().unwrap();
        assert!(!offer.logged);
        assert_eq!(offer.name, "Beta");
        assert_eq!(dealt.cleared[0].name, "Acme", "the older skip heads the log");
        assert_eq!(dealt.log_under().len(), 1, "a Later offer hides nothing");
    }

    #[test]
    fn yesterdays_decision_cannot_be_undone_today() {
        let tomorrow = today().succ_opt().unwrap();
        let jobs = board(vec![job("Acme", 90)]);
        let decide: [fn(&mut InTray, &Card); 3] = [
            |tray, card| tray.skip(card, today()),
            |tray, card| tray.later(card, today()),
            |tray, card| {
                tray.follow(card, card.primary.as_ref().unwrap(), today());
            },
        ];
        for decide in decide {
            let mut tray = InTray::unsaved();
            let card = top(&tray, &jobs);
            decide(&mut tray, &card);
            assert!(queue(&tray, &jobs, today()).undo.is_some());
            let decided = tray.decisions.clone();

            assert_eq!(queue(&tray, &jobs, tomorrow).undo, None);
            tray.undo(tomorrow);
            assert_eq!(tray.decisions, decided, "U leaves yesterday's decision alone");
        }
    }

    #[test]
    fn a_reason_after_undo_leaves_the_older_skip_alone() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        tray.skip(&top(&tray, &jobs), today());
        tray.skip(&top(&tray, &jobs), today());
        tray.undo(today());

        tray.give_reason(SkipReason::Rate, today());
        assert_eq!(tray.decisions.len(), 1);
        assert_eq!(tray.decisions[0].name, "Acme");
        assert_eq!(tray.decisions[0].reason, None, "undo has moved on, so the reason has nothing to name");
    }

    fn paused_run() -> Run {
        paused_on("r1", PromptKind::Questions(vec![]))
    }

    fn paused_on(id: &str, kind: PromptKind) -> Run {
        let request = RunRequest { run_id: id.into(), line: "/apply x".into(), model: None, budget_usd: 3.0, started_at: String::new(), resume_session: None };
        let mut run = Run::new(&request);
        let prompt = Prompt { request_id: "p".into(), kind, input_json: "{}".into(), suggestions_json: None };
        run.apply(AgentEvent::PromptRaised(prompt));
        run
    }

    #[test]
    fn a_paused_run_stays_paused_when_its_card_is_set_aside() {
        let runs = [paused_run()];
        let empty = JobBoard::default();
        let pipeline = Pipeline::default();
        let archives = HashMap::new();
        let sources = Sources { board: &empty, pipeline: &pipeline, archives: &archives, runs: &runs, today: today() };
        let mut tray = InTray::unsaved();

        tray.later(&tray.queue(&sources).cards[0].clone(), today());

        assert_eq!(tray.queue(&sources).headline(), "1 set aside");
        assert!(runs[0].needs_user());
    }

    #[test]
    fn headline_counts_minutes() {
        let card = |kind| Card {
            key: String::new(), kind, name: String::new(), title: String::new(), subtitle: String::new(), take: String::new(), when: String::new(),
            due: None::<Due>, weight: 0, body: Body::Empty, primary: None, others: vec![], can_skip: false, note: String::new(), trace: String::new(),
        };
        let queue = |cards| Queue { cards, ..Default::default() };
        assert_eq!(queue(vec![]).headline(), "Nothing needs you");
        let aside = Queue { set_aside: vec!["A".into(), "A".into(), "B".into()], ..Default::default() };
        assert_eq!(aside.headline(), "3 set aside");
        assert_eq!(aside.set_aside_names(), "A, B");
        assert_eq!(queue(vec![card(CardKind::Close)]).headline(), "1 decision, about 1 minute");
        assert_eq!(queue(vec![card(CardKind::Send), card(CardKind::Close)]).headline(), "2 decisions, about 4 minutes");
    }

    fn pending(kind: CardKind, due: Option<NaiveDate>) -> Card {
        Card {
            key: String::new(), kind, name: String::new(), title: String::new(), subtitle: String::new(), take: String::new(), when: String::new(),
            due: due.map(|on| Due { on, label: String::new() }), weight: 0, body: Body::Empty, primary: None, others: vec![], can_skip: false, note: String::new(), trace: String::new(),
        }
    }

    fn of_kinds(kinds: &[(usize, CardKind)]) -> Queue {
        let cards = kinds.iter().flat_map(|&(count, kind)| (0..count).map(move |_| pending(kind, None))).collect();
        Queue { cards, ..Default::default() }
    }

    #[test]
    fn overdue_counts_pending_cards_past_their_date() {
        let day = |offset: i64| Some(today() + chrono::Duration::days(offset));
        let cards = vec![
            pending(CardKind::Triage, day(-2)), pending(CardKind::Triage, day(-1)), pending(CardKind::Triage, day(0)),
            pending(CardKind::Triage, day(3)), pending(CardKind::Triage, None),
        ];
        assert_eq!(Queue { cards, today: today(), ..Default::default() }.overdue(), 2);
    }

    #[test]
    fn kinds_sort_by_count_then_kind_order() {
        assert_eq!(of_kinds(&[(5, CardKind::Triage), (2, CardKind::Send)]).by_kind(), [(5, Some(CardKind::Triage)), (2, Some(CardKind::Send))]);
        assert_eq!(of_kinds(&[(1, CardKind::Triage), (1, CardKind::Send)]).by_kind(), [(1, Some(CardKind::Send)), (1, Some(CardKind::Triage))]);
        assert_eq!(of_kinds(&[(3, CardKind::Close)]).by_kind(), [(3, Some(CardKind::Close))]);
        assert_eq!(of_kinds(&[]).by_kind(), []);
    }

    #[test]
    fn more_than_two_kinds_fold_into_other() {
        let queue = of_kinds(&[(4, CardKind::Triage), (2, CardKind::Send), (1, CardKind::FollowUp), (1, CardKind::Close)]);
        assert_eq!(queue.by_kind(), [(4, Some(CardKind::Triage)), (4, None)]);
    }

    #[test]
    fn set_aside_names_are_listed_once() {
        let aside = Queue { set_aside: vec!["A".into(), "A".into(), "B".into()], ..Default::default() };
        assert_eq!(aside.set_aside_distinct(), ["A", "B"]);
    }

    #[test]
    fn the_note_shows_only_when_no_mark_is_in_the_window() {
        let empty = Queue { today: today(), ..Default::default() };
        assert_eq!(empty.horizon_note().as_deref(), Some("No deadlines in the next 14 days"));

        let late = Queue { cards: vec![pending(CardKind::Triage, Some(today() - chrono::Duration::days(1)))], today: today(), ..Default::default() };
        assert_eq!(late.horizon_note().as_deref(), Some("No other deadlines in the next 14 days"));

        let mark = Mark { on: today(), label: "Acme closes".into(), key: "triage:acme".into(), kind: CardKind::Triage, active: false };
        assert_eq!(Queue { marks: vec![mark], today: today(), ..Default::default() }.horizon_note(), None);
    }

    fn closing_in(company: &str, score: u8, days: i64) -> JobView {
        JobView { deadline: Some(today() + chrono::Duration::days(days)), days_to_deadline: Some(days), ..job(company, score) }
    }

    #[test]
    fn a_date_past_the_strip_ranks_with_the_rest_and_has_no_mark() {
        let tray = InTray::unsaved();
        let far = board(vec![job("Plain", 90), closing_in("Fourteen", 70, HORIZON_DAYS)]);
        let dealt_far = queue(&tray, &far, today());
        assert_eq!(dealt(&tray, &far), ["Plain", "Fourteen"]);
        assert!(dealt_far.marks.is_empty());
        assert_eq!(dealt_far.horizon_note().as_deref(), Some("No deadlines in the next 14 days"));

        let near = board(vec![job("Plain", 90), closing_in("Thirteen", 70, HORIZON_DAYS - 1)]);
        let dealt_near = queue(&tray, &near, today());
        assert_eq!(dealt(&tray, &near), ["Thirteen", "Plain"]);
        assert_eq!(dealt_near.marks.iter().map(|mark| mark.key.as_str()).collect::<Vec<_>>(), ["triage:thirteen"]);
        assert_eq!(dealt_near.horizon_note(), None);
    }

    #[test]
    fn an_overdue_card_counts_until_it_is_set_aside() {
        // An expired posting is never triaged, so the overdue card is an offer whose reply date has passed.
        let offer = ApplicationCard {
            company: "Juniper".into(), role: "Lead".into(), sector: String::new(), channel: String::new(),
            status_label: String::new(), bucket: StatusBucket::Offer, applied_on: String::new(), deadline: Some(today() - chrono::Days::new(2)), deadline_state: None,
            contact_person: String::new(), fit_rating: None, notes: String::new(), source: None, days_quiet: None,
            follow_ups_sent: 0, follow_up_due: false, stale: false, reached: ReachedStage::Applied,
            is_rejection: false, is_final: false, cv_file: String::new(), cover_letter_file: String::new(),
            archive_folder: None, outcome: None,
        };
        let pipeline = Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Offer, cards: vec![offer] }] };
        let dealt = |tray: &InTray| tray.queue(&Sources { board: &JobBoard::default(), pipeline: &pipeline, archives: &HashMap::new(), runs: &[], today: today() });
        let mut tray = InTray::unsaved();

        let before = dealt(&tray);
        assert_eq!(before.overdue(), 1);
        assert!(before.marks.is_empty());
        assert_eq!(before.horizon_note().as_deref(), Some("No other deadlines in the next 14 days"));

        tray.later(&before.cards[0], today());

        let after = dealt(&tray);
        assert_eq!(after.overdue(), 0);
        assert_eq!(after.set_aside, ["Juniper"]);
        assert_eq!(after.horizon_note().as_deref(), Some("No deadlines in the next 14 days"));
    }
}
