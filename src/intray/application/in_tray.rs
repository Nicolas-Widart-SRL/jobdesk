use chrono::NaiveDate;

use crate::discovery::application::Lane;
use crate::intray::application::compose::{Sources, derive_cards};
use crate::intray::domain::{Action, Card, CardKind, Decision, DecisionStore, Effect, Outcome, SkipReason, rank};
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
    pub cleared: Vec<Cleared>,
    pub marks: Vec<Mark>,
    pub unranked: usize,
    pub today: NaiveDate,
}

impl Queue {
    /// Two cards of one company are named once.
    pub fn set_aside_names(&self) -> String {
        let mut names: Vec<&str> = Vec::new();
        for name in &self.set_aside {
            if !names.contains(&name.as_str()) {
                names.push(name);
            }
        }
        names.join(", ")
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

/// The last decision, for as long as it can be taken back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOffer {
    pub text: String,
    pub asks_reason: bool,
    pub reason: Option<SkipReason>,
}

/// Owns the stored decisions and deals the queue from them.
pub struct InTray {
    store: Option<Box<dyn DecisionStore>>,
    decisions: Vec<Decision>,
    can_undo: bool,
    reading: Option<String>,
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
        Self { store: None, decisions: Vec::new(), can_undo: false, reading: None, issue: None }
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

    /// Derives the cards again. The card being read stays on top, so new work lands behind it.
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
        if let Some(at) = self.reading.as_ref().and_then(|key| cards.iter().position(|card| &card.key == key)) {
            cards[..=at].rotate_right(1);
        }
        let cleared = self.decisions.iter().rev().filter(|decision| decision.is_logged_on(today)).map(|decision| Cleared { name: decision.name.clone(), label: decision.label() }).collect();
        Queue { cards, set_aside, cleared, marks, unranked: sources.board.count(Lane::New), today }
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

    pub fn bring_back_all(&mut self, today: NaiveDate) {
        self.take_back(|decision| decision.sets_aside(today));
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

    pub fn undo_offer(&self) -> Option<UndoOffer> {
        let last = self.decisions.last().filter(|_| self.can_undo)?;
        Some(UndoOffer { text: format!("{}: {}", last.label(), last.name), asks_reason: last.outcome == Outcome::Skipped, reason: last.reason })
    }

    /// The card returns to the top, the way it was before the decision.
    pub fn undo(&mut self) {
        if !self.can_undo {
            return;
        }
        self.can_undo = false;
        self.reading = self.decisions.pop().map(|decision| decision.key);
        self.save();
    }

    pub fn give_reason(&mut self, reason: SkipReason) {
        if let Some(last) = self.decisions.last_mut().filter(|last| last.outcome == Outcome::Skipped) {
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
    use crate::tracking::application::Pipeline;

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
        tray.undo();

        assert_eq!(dealt(&tray, &jobs), ["Acme", "Beta"]);
        assert_eq!(tray.undo_offer(), None);
    }

    #[test]
    fn only_the_last_decision_can_be_undone() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        tray.skip(&top(&tray, &jobs), today());
        tray.skip(&top(&tray, &jobs), today());
        tray.undo();
        tray.undo();
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
        assert_eq!(tray.undo_offer().unwrap().text, "Command opened: Acme");
        tray.undo();
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
        assert_eq!(tray.undo_offer(), None);
    }

    #[test]
    fn a_skip_reason_is_stored_and_a_store_survives_a_reopen() {
        let memory = Memory::default();
        let jobs = board(vec![job("Acme", 90)]);
        let mut tray = InTray::new(Box::new(memory.clone()));
        tray.skip(&top(&tray, &jobs), today());
        assert!(tray.undo_offer().unwrap().asks_reason);
        tray.give_reason(SkipReason::Domain);

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

        tray.undo();
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
        assert_eq!(tray.undo_offer().unwrap().text, "Set aside: Acme");
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
        assert_eq!(tray.undo_offer().unwrap().text, "Set aside: Beta", "the last decision still stands");
    }

    #[test]
    fn pulling_back_the_last_card_set_aside_ends_its_undo() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&Card { key: "other".into(), name: "Older".into(), ..top(&tray, &jobs) }, today());
        tray.later(&top(&tray, &jobs), today());

        tray.pull_forward("triage:acme", today());

        assert_eq!(tray.undo_offer(), None);
        tray.undo();
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

    #[test]
    fn a_paused_run_stays_paused_when_its_card_is_set_aside() {
        let request = RunRequest { run_id: "r1".into(), line: "/apply x".into(), model: None, budget_usd: 3.0, started_at: String::new(), resume_session: None };
        let mut run = Run::new(&request);
        let prompt = Prompt { request_id: "p".into(), kind: PromptKind::Questions(vec![]), input_json: "{}".into(), suggestions_json: None };
        run.apply(AgentEvent::PromptRaised(prompt));
        let runs = [run];
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
}
