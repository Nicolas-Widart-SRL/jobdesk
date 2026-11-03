use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::discovery::application::Lane;
use crate::intray::application::compose::{Sources, derive_cards, guarded_job};
use crate::intray::domain::{Action, Card, CardKind, Decision, DecisionStore, Effect, HORIZON_DAYS, HORIZON_WORDS, Outcome, SkipReason, Subject, rank};
use crate::shared_kernel::{LoadError, WebUrl};

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

/// A dated card in Coming up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub on: NaiveDate,
    pub label: String,
    /// The card's own name, for a sentence that names the company.
    pub name: String,
    pub key: String,
    pub kind: CardKind,
    /// False once decided. A card set aside still counts, so its date stays in view.
    pub active: bool,
}

/// How many Coming up rows show before the rest are counted.
pub const AGENDA_ROWS: usize = 6;

/// Today's decisions against the cards dealt today. Cards set aside are in neither number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayTally {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// A pending card whose date passed this many days ago.
    Late(i64),
    /// The card being read, late by this many days.
    LateNow(i64),
    /// The card being read.
    Now,
    Done,
    Waiting,
}

/// One dated line of Coming up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaRow {
    pub on: NaiveDate,
    pub text: String,
    pub key: String,
    pub state: RowState,
}

/// The cards set aside, which come back on the next day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Returning {
    pub on: NaiveDate,
    pub count: usize,
    /// Each company once, in the order they first appear.
    pub names: Vec<String>,
}

/// What Coming up shows: the return row, the dated rows, and the line that closes the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agenda {
    pub returning: Option<Returning>,
    pub rows: Vec<AgendaRow>,
    pub closing: String,
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

    /// Pending cards whose date has passed. A card due today is not overdue.
    pub fn overdue(&self) -> usize {
        self.cards.iter().filter(|card| card.due.as_ref().is_some_and(|due| due.on < self.today)).count()
    }

    /// Today's log under the undo offer. A logged offer already stands for the newest entry, so that entry is left out.
    pub fn log_under(&self) -> &[Cleared] {
        let logged = self.undo.as_ref().is_some_and(|undo| undo.logged);
        self.cleared.get(usize::from(logged)..).unwrap_or(&[])
    }
}

impl Queue {
    pub fn tally(&self) -> DayTally {
        let done = self.cleared.len();
        DayTally { done, total: done + self.cards.len() }
    }

    /// With a source missing, an empty list may only mean its cards could not be dealt, so it claims nothing is left.
    pub fn today_headline(&self, missing: &[&str]) -> String {
        let count = self.cards.len();
        if count == 0 && !missing.is_empty() {
            "Nothing to show yet".to_owned()
        } else if count == 0 {
            "All done for today".to_owned()
        } else if self.cards.iter().all(|card| card.kind == CardKind::Triage) {
            if count == 1 { "1 new match to look at".to_owned() } else { format!("{count} new matches to look at") }
        } else if count == 1 {
            "1 thing needs you".to_owned()
        } else {
            format!("{count} things need you")
        }
    }

    /// `missing` names the sources that could not be read, as they should read at the start of a sentence.
    pub fn today_subtitle(&self, missing: &[&str]) -> String {
        let done = self.cleared.len();
        let mut parts: Vec<String> = Vec::new();
        let gone = missing.split_last().map(|(last, rest)| {
            let names = if rest.is_empty() { (*last).to_owned() } else { format!("{} and {last}", rest.join(", ")) };
            format!("{names} are missing from this list.")
        });
        if self.cards.is_empty() {
            match done {
                0 => {}
                1 => parts.push("1 decision.".to_owned()),
                _ => parts.push(format!("{done} decisions.")),
            }
            let next = self.marks.iter().filter(|mark| mark.active && mark.on >= self.today).min_by_key(|mark| mark.on);
            match next {
                Some(mark) => parts.push(format!("Your next deadline is {}, {}.", mark.on.format("%a %-d %b"), mark.name)),
                // A source that could not be read may hold what waits, so the list never claims to be complete.
                None if done == 0 && gone.is_none() => parts.push("Nothing waits on you.".to_owned()),
                None => {}
            }
            parts.extend(gone);
            return parts.join(" ");
        }
        let minutes: usize = self.cards.iter().map(|card| card.kind.minutes()).sum();
        parts.push(format!("About {minutes} {}.", if minutes == 1 { "minute" } else { "minutes" }));
        if done > 0 {
            parts.push(format!("{done} done so far today."));
        }
        match self.overdue() {
            0 => {}
            1 => parts.push("1 is past its deadline.".to_owned()),
            late => parts.push(format!("{late} are past their deadline.")),
        }
        if done == 0 && gone.is_none() && self.cards.iter().all(|card| card.due.is_none()) {
            parts.push("Nothing has a deadline yet.".to_owned());
        }
        parts.extend(gone);
        parts.join(" ")
    }

    /// Late cards first, then the dated marks day by day. Rows show up to `AGENDA_ROWS`, and the rest are counted.
    pub fn agenda(&self) -> Agenda {
        let top = self.cards.first().map(|card| card.key.as_str());
        let mut late: Vec<AgendaRow> = self
            .cards
            .iter()
            .filter_map(|card| {
                let due = card.due.as_ref().filter(|due| due.on < self.today)?;
                let days = (self.today - due.on).num_days();
                let state = if top == Some(card.key.as_str()) { RowState::LateNow(days) } else { RowState::Late(days) };
                Some(AgendaRow { on: due.on, text: due.label.clone(), key: card.key.clone(), state })
            })
            .collect();
        late.sort_by_key(|row| row.on);
        let position = |key: &str| self.cards.iter().position(|card| card.key == key).unwrap_or(usize::MAX);
        let mut marks: Vec<&Mark> = self.marks.iter().collect();
        marks.sort_by_key(|mark| (mark.on, position(&mark.key)));
        let dated = marks.into_iter().map(|mark| {
            let state = if !mark.active {
                RowState::Done
            } else if top == Some(mark.key.as_str()) {
                RowState::Now
            } else {
                RowState::Waiting
            };
            AgendaRow { on: mark.on, text: mark.label.clone(), key: mark.key.clone(), state }
        });
        let mut rows: Vec<AgendaRow> = late.into_iter().chain(dated).collect();
        let cut = rows.len().saturating_sub(AGENDA_ROWS);
        rows.truncate(AGENDA_ROWS);
        let until = (self.today + chrono::Duration::days(HORIZON_DAYS - 1)).format("%a %-d %b");
        let closing = if rows.is_empty() {
            format!("No deadlines in the next {}.", HORIZON_WORDS.1)
        } else if cut > 0 {
            format!("{cut} more before {until}.")
        } else {
            format!("Nothing else dated before {until}.")
        };
        let returning = (!self.set_aside.is_empty()).then(|| Returning {
            on: self.today + chrono::Duration::days(1),
            count: self.set_aside.len(),
            names: self.set_aside_distinct().into_iter().map(str::to_owned).collect(),
        });
        Agenda { returning, rows, closing }
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

/// What waits in Today, so the Jobs and Applications screens can mark it. Build it only where one of them is drawn.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Waiting {
    /// The key of every job with a card pending.
    pub jobs: HashSet<String>,
    /// The role of every application with a card pending, by company, both lowercased as a card key is.
    pub applications: HashMap<String, HashSet<String>>,
}

impl Queue {
    pub fn waiting(&self) -> Waiting {
        let mut waiting = Waiting::default();
        for card in &self.cards {
            match &card.subject {
                Subject::Job(key) => {
                    waiting.jobs.insert(key.clone());
                }
                Subject::Application { company, role } => {
                    waiting.applications.entry(company.to_lowercase()).or_default().insert(role.to_lowercase());
                }
                Subject::Run(_) => {}
            }
        }
        waiting
    }
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
    /// Kept as a `LoadError`, whose text is already stripped of hidden characters.
    issue: Option<LoadError>,
}

impl InTray {
    /// A store that cannot be read is left alone, so saving cannot overwrite what it holds.
    pub fn new(store: Box<dyn DecisionStore>) -> Self {
        match store.load() {
            Ok(decisions) => Self { store: Some(store), decisions, ..Self::unsaved() },
            Err(error) => Self { issue: Some(error), ..Self::unsaved() },
        }
    }

    /// Decisions last until the window closes. For a machine with no app data folder, and tests.
    pub fn unsaved() -> Self {
        Self { store: None, decisions: Vec::new(), can_undo: false, reading: None, returned: Vec::new(), returned_on: None, undo_returned: false, issue: None }
    }

    pub fn issue(&self) -> Option<&LoadError> {
        self.issue.as_ref()
    }

    /// How many decisions the open workspace has stored.
    pub fn decision_count(&self) -> usize {
        self.decisions.len()
    }

    /// Forgets every decision, so today deals every card again. An unreadable store is left as it is:
    /// saving would overwrite what it holds.
    pub fn clear_decisions(&mut self) {
        if self.issue.is_some() {
            return;
        }
        self.decisions.clear();
        self.can_undo = false;
        self.reading = None;
        self.returned.clear();
        self.returned_on = None;
        self.undo_returned = false;
        self.save();
    }

    fn is_cleared(&self, key: &str, today: NaiveDate) -> bool {
        self.decisions.iter().any(|decision| decision.key == key && decision.clears_card(today))
    }

    fn is_set_aside(&self, key: &str, today: NaiveDate) -> bool {
        self.decisions.iter().any(|decision| decision.key == key && decision.sets_aside(today))
    }

    /// The key of every job whose flagged ad the user kept, so Jobs words it as Today does.
    pub fn kept_jobs(&self) -> HashSet<String> {
        let kept = self.decisions.iter().filter(|decision| decision.outcome == Outcome::Kept);
        kept.filter_map(|decision| guarded_job(&decision.key)).map(str::to_owned).collect()
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
                Some(Mark {
                    on: due.on,
                    label: due.label.clone(),
                    name: card.name.clone(),
                    key: card.key.clone(),
                    kind: card.kind,
                    active: !self.is_cleared(&card.key, today),
                })
            })
            .collect();
        let (aside, mut cards): (Vec<Card>, Vec<Card>) =
            derived.into_iter().filter(|card| !self.is_cleared(&card.key, today)).partition(|card| self.is_set_aside(&card.key, today));
        let set_aside = aside.into_iter().map(|card| card.name).collect();
        rank(&mut cards, today);
        let returned = self.returned(today);
        cards.sort_by_key(|card| {
            if card.pauses_a_run() {
                0
            } else if returned.contains(&card.key) {
                1
            } else {
                2
            }
        });
        if let Some(at) = self.reading.as_ref().and_then(|key| cards.iter().position(|card| &card.key == key)) {
            cards[..=at].rotate_right(1);
        }
        let cleared = self
            .decisions
            .iter()
            .rev()
            .filter(|decision| decision.is_logged_on(today))
            .map(|decision| Cleared { name: decision.name.clone(), label: decision.label() })
            .collect();
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

    fn record(&mut self, card: &Card, outcome: Outcome, reason: Option<SkipReason>, task: Option<String>, today: NaiveDate) {
        self.decisions.push(Decision { key: card.key.clone(), name: card.name.clone(), outcome, reason, task, on: today });
        self.can_undo = true;
        self.reading = None;
        self.forget_returns_before(today);
        self.undo_returned = self.returned(today).contains(&card.key);
        self.returned.retain(|key| key != &card.key);
        self.save();
    }

    /// With no store nothing is saved, so a load error stays until a real save succeeds.
    fn save(&mut self) {
        if let Some(store) = &self.store {
            self.issue = store.save(&self.decisions).err();
        }
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
        self.record(card, Outcome::Later, None, None, today);
    }

    pub fn skip(&mut self, card: &Card, today: NaiveDate) {
        if card.can_skip {
            self.record(card, Outcome::Skipped, None, None, today);
        }
    }

    /// Only a card's own actions come here, and none of them runs anything by itself.
    pub fn follow(&mut self, card: &Card, action: &Action, today: NaiveDate) -> Option<Request> {
        match &action.effect {
            Effect::Command(line) => {
                self.record(card, Outcome::Followed, None, action.task.map(str::to_owned), today);
                Some(Request::Command(line.clone()))
            }
            Effect::Skip(reason) => {
                self.record(card, Outcome::Skipped, Some(*reason), None, today);
                None
            }
            Effect::Keep => {
                self.record(card, Outcome::Kept, None, None, today);
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
        let note = last.note();
        let spoken = format!("Last decision on {}: {}. Press U to undo.", last.name, note.trim_end_matches('.'));
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
            key: company.to_lowercase(),
            title: "Lead".into(),
            company: company.into(),
            location: None,
            url: None,
            portal: None,
            found_on: None,
            deadline: None,
            days_to_deadline: None,
            score: Some(score),
            verdict: None,
            scraper_fit: None,
            location_gate: None,
            language_gate: None,
            language_note: None,
            freelance: FreelanceGates { contract: read(), day_rate: read(), commute: read() },
            guard: Some("clean".into()),
            strengths: vec![],
            gaps: vec![],
            fetch_attempts: None,
            fetch_error: None,
            signals: vec![],
            closing_soon: false,
            guard_flagged: false,
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
        assert_eq!((offer.name.as_str(), offer.note.as_str()), ("Acme", "Task opened: draft application"));
        assert_eq!(offer.spoken, "Last decision on Acme: Task opened: draft application. Press U to undo.");
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
        assert_eq!(queue(&reopened, &jobs, today()).cleared[0].label, "Skipped for domain");
    }

    #[test]
    fn clearing_decisions_saves_an_empty_list_and_offers_no_undo() {
        let memory = Memory::default();
        let jobs = board(vec![job("Acme", 90), job("Beta", 80)]);
        let mut tray = InTray::new(Box::new(memory.clone()));
        tray.skip(&top(&tray, &jobs), today());
        tray.later(&top(&tray, &jobs), today());
        assert_eq!(tray.decision_count(), 2);
        tray.bring_back_all(today());
        assert_eq!(tray.returned.len(), 1);

        tray.clear_decisions();

        assert_eq!(tray.decision_count(), 0);
        assert!(memory.0.borrow().is_empty());
        assert_eq!(tray.undo_offer(today()), None);
        assert_eq!(dealt(&tray, &jobs), ["Acme", "Beta"]);
        assert!(tray.returned.is_empty() && tray.returned_on.is_none() && tray.reading.is_none());
    }

    /// Loads fine, then fails to save: the tray has a store and an issue at once.
    struct FailsToSave(Rc<RefCell<usize>>);

    impl DecisionStore for FailsToSave {
        fn load(&self) -> Result<Vec<Decision>, LoadError> {
            Ok(Vec::new())
        }
        fn save(&self, _: &[Decision]) -> Result<(), LoadError> {
            *self.0.borrow_mut() += 1;
            Err(LoadError::new("decisions.json", "disk full"))
        }
    }

    #[test]
    fn clearing_with_an_unsaved_issue_changes_and_saves_nothing() {
        let saves = Rc::new(RefCell::new(0));
        let mut tray = InTray::new(Box::new(FailsToSave(saves.clone())));
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&top(&tray, &jobs), today());
        assert!(tray.issue().is_some());
        assert_eq!((tray.decision_count(), *saves.borrow()), (1, 1));

        tray.clear_decisions();

        assert_eq!((tray.decision_count(), *saves.borrow()), (1, 1));
        assert!(tray.undo_offer(today()).is_some());
    }

    #[test]
    fn clearing_a_store_that_never_loaded_saves_nothing() {
        let mut tray = InTray::new(Box::new(Corrupt));
        tray.clear_decisions();
        assert!(tray.issue().is_some_and(|issue| issue.to_string().contains("not JSON")));
    }

    #[test]
    fn an_unreadable_store_reports_and_stays_untouched() {
        let mut tray = InTray::new(Box::new(Corrupt));
        assert!(tray.issue().unwrap().to_string().contains("not JSON"));
        let jobs = board(vec![job("Acme", 90)]);
        tray.skip(&top(&tray, &jobs), today());
        assert!(dealt(&tray, &jobs).is_empty());
        assert!(tray.issue().is_some_and(|issue| issue.to_string().contains("not JSON")), "nothing was saved, so the alert stays");
    }

    #[test]
    fn a_mark_goes_quiet_only_when_its_card_is_cleared() {
        let mut tray = InTray::unsaved();
        let on = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
        let jobs = board(vec![JobView { deadline: Some(on), days_to_deadline: Some(5), ..job("Meridian Rail", 80) }]);
        assert_eq!(queue(&tray, &jobs, today()).marks[0].label, "Meridian Rail closes");
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
        assert_eq!(aside.today_headline(&[]), "All done for today");
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
        assert_eq!(queue(&tray, &jobs, today()).today_headline(&[]), "All done for today");

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
        let request = RunRequest {
            run_id: "r2".into(),
            line: "/apply x".into(),
            model: None,
            budget_usd: 3.0,
            started_at: String::new(),
            resume_session: None,
            sign_in: crate::shared_kernel::SignIn::ClaudeSignIn,
        };
        let mut waiting = Run::new(&request);
        waiting.apply(AgentEvent::AssistantText { text: "Which posting first?".into(), nested: false });
        waiting.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 1 });
        // A turn card waits for every run to be idle, so the run with the question asked after its turn ended.
        let mut asking = Run::new(&RunRequest { run_id: "r1".into(), ..request.clone() });
        asking.apply(AgentEvent::TurnEnded { failed: false, message: None, cost_usd: 0.5, duration_ms: 1 });
        asking.apply(AgentEvent::PromptRaised(Prompt {
            request_id: "p".into(),
            kind: PromptKind::Questions(vec![]),
            input_json: "{}".into(),
            suggestions_json: None,
        }));
        let runs = [waiting, asking];
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
        assert_eq!(offer.note, "Skipped for scope");
        assert_eq!(offer.reason, Some(SkipReason::Scope));
        assert_eq!(offer.spoken, "Last decision on Acme: Skipped for scope. Press U to undo.");
    }

    #[test]
    fn later_is_offered_but_not_logged() {
        let mut tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        tray.later(&top(&tray, &jobs), today());
        let offer = tray.undo_offer(today()).unwrap();
        assert!(!offer.logged);
        assert!(offer.note.contains("until tomorrow"), "{}", offer.note);
        assert_eq!(offer.spoken, "Last decision on Acme: Set aside until tomorrow. Press U to undo.");
    }

    #[test]
    fn the_spoken_offer_says_what_the_note_shows() {
        let jobs = board(vec![job("Acme", 90)]);
        let hears = |note: &str| format!("Last decision on Acme: {note}. Press U to undo.");

        let mut tray = InTray::unsaved();
        tray.later(&top(&tray, &jobs), today());
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!((offer.note.as_str(), offer.spoken.as_str()), ("Set aside until tomorrow", hears("Set aside until tomorrow").as_str()));

        let mut tray = InTray::unsaved();
        tray.skip(&top(&tray, &jobs), today());
        let offer = tray.undo_offer(today()).unwrap();
        assert_eq!((offer.note.as_str(), offer.spoken.as_str()), ("Skipped. Why? Optional.", hears("Skipped. Why? Optional").as_str()));
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
        let request = RunRequest {
            run_id: id.into(),
            line: "/apply x".into(),
            model: None,
            budget_usd: 3.0,
            started_at: String::new(),
            resume_session: None,
            sign_in: crate::shared_kernel::SignIn::ClaudeSignIn,
        };
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

        assert_eq!(tray.queue(&sources).today_headline(&[]), "All done for today");
        assert!(runs[0].needs_user());
    }

    #[test]
    fn the_queue_names_the_jobs_and_applications_waiting_on_you() {
        let tray = InTray::unsaved();
        let jobs = board(vec![job("Acme", 90)]);
        let offer = ApplicationCard {
            company: "Beta".into(),
            role: "Dev".into(),
            sector: String::new(),
            channel: String::new(),
            status_label: String::new(),
            bucket: StatusBucket::Offer,
            applied_on: String::new(),
            sent_on: None,
            deadline: None,
            deadline_state: None,
            contact_person: String::new(),
            fit_rating: None,
            notes: String::new(),
            source: None,
            days_quiet: None,
            days_sent: None,
            days_followed_up: None,
            follow_ups_sent: 0,
            follow_up_due: false,
            stale: false,
            reached: ReachedStage::Applied,
            is_rejection: false,
            is_final: false,
            cv_file: String::new(),
            cover_letter_file: String::new(),
            archive_folder: None,
            outcome: None,
        };
        let pipeline = Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Offer, cards: vec![offer] }] };
        let queue = tray.queue(&Sources { board: &jobs, pipeline: &pipeline, archives: &HashMap::new(), runs: &[], today: today() });
        let waiting = queue.waiting();
        assert_eq!(waiting.jobs, HashSet::from(["acme".to_owned()]));
        assert_eq!(waiting.applications, HashMap::from([("beta".to_owned(), HashSet::from(["dev".to_owned()]))]), "lowercased, as the card key is");
    }

    fn pending(kind: CardKind, due: Option<NaiveDate>) -> Card {
        Card {
            key: String::new(),
            kind,
            name: String::new(),
            over: None,
            subject: Subject::Run(String::new()),
            title: String::new(),
            subtitle: String::new(),
            take: String::new(),
            when: String::new(),
            due: due.map(|on| Due { on, label: String::new() }),
            weight: 0,
            body: Body::Empty,
            primary: None,
            others: vec![],
            can_skip: false,
            note: String::new(),
            command: None,
        }
    }

    #[test]
    fn overdue_counts_pending_cards_past_their_date() {
        let day = |offset: i64| Some(today() + chrono::Duration::days(offset));
        let cards = vec![
            pending(CardKind::Triage, day(-2)),
            pending(CardKind::Triage, day(-1)),
            pending(CardKind::Triage, day(0)),
            pending(CardKind::Triage, day(3)),
            pending(CardKind::Triage, None),
        ];
        assert_eq!(Queue { cards, today: today(), ..Default::default() }.overdue(), 2);
    }

    #[test]
    fn set_aside_names_are_listed_once() {
        let aside = Queue { set_aside: vec!["A".into(), "A".into(), "B".into()], ..Default::default() };
        assert_eq!(aside.set_aside_distinct(), ["A", "B"]);
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
        assert_eq!(dealt_far.agenda().closing, "No deadlines in the next two weeks.");

        let near = board(vec![job("Plain", 90), closing_in("Thirteen", 70, HORIZON_DAYS - 1)]);
        let dealt_near = queue(&tray, &near, today());
        assert_eq!(dealt(&tray, &near), ["Thirteen", "Plain"]);
        assert_eq!(dealt_near.marks.iter().map(|mark| mark.key.as_str()).collect::<Vec<_>>(), ["triage:thirteen"]);
        assert_eq!(dealt_near.agenda().rows.len(), 1);
    }

    #[test]
    fn an_overdue_card_counts_until_it_is_set_aside() {
        // An expired posting is never triaged, so the overdue card is an offer whose reply date has passed.
        let offer = ApplicationCard {
            company: "Juniper".into(),
            role: "Lead".into(),
            sector: String::new(),
            channel: String::new(),
            status_label: String::new(),
            bucket: StatusBucket::Offer,
            applied_on: String::new(),
            sent_on: None,
            deadline: Some(today() - chrono::Days::new(2)),
            deadline_state: None,
            contact_person: String::new(),
            fit_rating: None,
            notes: String::new(),
            source: None,
            days_quiet: None,
            days_sent: None,
            days_followed_up: None,
            follow_ups_sent: 0,
            follow_up_due: false,
            stale: false,
            reached: ReachedStage::Applied,
            is_rejection: false,
            is_final: false,
            cv_file: String::new(),
            cover_letter_file: String::new(),
            archive_folder: None,
            outcome: None,
        };
        let pipeline = Pipeline { lanes: vec![PipelineLane { bucket: StatusBucket::Offer, cards: vec![offer] }] };
        let dealt =
            |tray: &InTray| tray.queue(&Sources { board: &JobBoard::default(), pipeline: &pipeline, archives: &HashMap::new(), runs: &[], today: today() });
        let mut tray = InTray::unsaved();

        let before = dealt(&tray);
        assert_eq!(before.overdue(), 1);
        assert!(before.marks.is_empty());
        assert_eq!(before.agenda().rows.iter().map(|row| row.state).collect::<Vec<_>>(), [RowState::LateNow(2)]);

        tray.later(&before.cards[0], today());

        let after = dealt(&tray);
        assert_eq!(after.overdue(), 0);
        assert_eq!(after.set_aside, ["Juniper"]);
        assert_eq!(after.agenda().closing, "No deadlines in the next two weeks.");
    }

    fn day(offset: i64) -> NaiveDate {
        today() + chrono::Duration::days(offset)
    }

    fn named(key: &str, name: &str, kind: CardKind, due: Option<(i64, &str)>) -> Card {
        let card = pending(kind, due.map(|(offset, _)| day(offset)));
        Card { key: key.into(), name: name.into(), due: due.map(|(offset, label)| Due { on: day(offset), label: label.into() }), ..card }
    }

    fn marked(key: &str, name: &str, offset: i64, active: bool) -> Mark {
        Mark { on: day(offset), label: format!("{name} closes"), name: name.into(), key: key.into(), kind: CardKind::Triage, active }
    }

    fn logged(count: usize) -> Vec<Cleared> {
        (0..count).map(|at| Cleared { name: format!("Done {at}"), label: "Skipped".into() }).collect()
    }

    fn today_queue(cards: Vec<Card>) -> Queue {
        Queue { cards, today: today(), ..Default::default() }
    }

    #[test]
    fn today_headline_names_new_matches_when_every_card_is_one() {
        assert_eq!(today_queue(vec![pending(CardKind::Triage, None)]).today_headline(&[]), "1 new match to look at");
        assert_eq!(today_queue(vec![pending(CardKind::Triage, None), pending(CardKind::Triage, None)]).today_headline(&[]), "2 new matches to look at");
    }

    #[test]
    fn today_headline_counts_things_otherwise() {
        assert_eq!(today_queue(vec![pending(CardKind::Send, None)]).today_headline(&[]), "1 thing needs you");
        assert_eq!(today_queue(vec![pending(CardKind::Triage, None), pending(CardKind::Close, None)]).today_headline(&[]), "2 things need you");
    }

    #[test]
    fn today_headline_is_all_done_with_cards_set_aside() {
        assert_eq!(today_queue(vec![]).today_headline(&[]), "All done for today");
        let aside = Queue { set_aside: vec!["Acme".into()], ..today_queue(vec![]) };
        assert_eq!(aside.today_headline(&[]), "All done for today");
    }

    #[test]
    fn today_subtitle_counts_minutes_done_and_overdue() {
        let due = |offset| Some((offset, "closes"));
        let cards = vec![named("a", "A", CardKind::Send, due(-1)), named("b", "B", CardKind::Close, due(4))];
        let one_late = Queue { cleared: logged(3), ..today_queue(cards) };
        assert_eq!(one_late.today_subtitle(&[]), "About 4 minutes. 3 done so far today. 1 is past its deadline.");

        let cards = vec![named("a", "A", CardKind::Close, due(-3)), named("b", "B", CardKind::Triage, due(-1))];
        assert_eq!(today_queue(cards).today_subtitle(&[]), "About 2 minutes. 2 are past their deadline.");

        let undated = today_queue(vec![pending(CardKind::Close, None)]);
        assert_eq!(undated.today_subtitle(&[]), "About 1 minute. Nothing has a deadline yet.");

        let undated_but_started = Queue { cleared: logged(1), ..undated };
        assert_eq!(undated_but_started.today_subtitle(&[]), "About 1 minute. 1 done so far today.");
    }

    #[test]
    fn today_subtitle_names_missing_sources() {
        let queue = Queue { cleared: logged(1), ..today_queue(vec![pending(CardKind::Close, None)]) };
        assert_eq!(queue.today_subtitle(&["Applications"]), "About 1 minute. 1 done so far today. Applications are missing from this list.");
        assert_eq!(queue.today_subtitle(&["Applications", "Jobs"]), "About 1 minute. 1 done so far today. Applications and Jobs are missing from this list.");
        assert_eq!(
            queue.today_subtitle(&["Applications", "Jobs", "Tasks"]),
            "About 1 minute. 1 done so far today. Applications, Jobs and Tasks are missing from this list."
        );
    }

    #[test]
    fn clear_subtitle_names_the_next_deadline() {
        let marks = vec![marked("late", "Harbor Bank", 1, false), marked("b", "Ardent Software", 6, true), marked("a", "Meridian Rail Systems", 5, true)];
        let clear = Queue { cleared: logged(11), marks, ..today_queue(vec![]) };
        assert_eq!(clear.today_subtitle(&[]), "11 decisions. Your next deadline is Thu 5 Nov, Meridian Rail Systems.");

        let one = Queue { cleared: logged(1), ..today_queue(vec![]) };
        assert_eq!(one.today_subtitle(&[]), "1 decision.");

        let only_deadline = Queue { marks: vec![marked("a", "Acme", 0, true)], ..today_queue(vec![]) };
        assert_eq!(only_deadline.today_subtitle(&[]), "Your next deadline is Sat 31 Oct, Acme.");

        assert_eq!(today_queue(vec![]).today_subtitle(&[]), "Nothing waits on you.");
    }

    #[test]
    fn a_missing_source_holds_back_what_the_list_cannot_know() {
        assert_eq!(today_queue(vec![]).today_headline(&["Applications"]), "Nothing to show yet");
        assert_eq!(today_queue(vec![pending(CardKind::Close, None)]).today_headline(&["Applications"]), "1 thing needs you");
        let undated = today_queue(vec![pending(CardKind::Close, None)]);
        assert_eq!(undated.today_subtitle(&["Jobs"]), "About 1 minute. Jobs are missing from this list.");
    }

    #[test]
    fn a_clear_list_with_a_missing_source_never_says_nothing_waits() {
        assert_eq!(today_queue(vec![]).today_subtitle(&["Applications"]), "Applications are missing from this list.");
        let one = Queue { cleared: logged(1), ..today_queue(vec![]) };
        assert_eq!(one.today_subtitle(&["Jobs", "Applications"]), "1 decision. Jobs and Applications are missing from this list.");
    }

    #[test]
    fn tally_leaves_out_cards_set_aside() {
        let queue = Queue {
            cleared: logged(3),
            set_aside: vec!["Acme".into(), "Beta".into()],
            ..today_queue(vec![pending(CardKind::Close, None), pending(CardKind::Close, None)])
        };
        assert_eq!(queue.tally(), DayTally { done: 3, total: 5 });
        assert_eq!(today_queue(vec![]).tally(), DayTally { done: 0, total: 0 });
    }

    #[test]
    fn agenda_puts_late_cards_first_and_marks_now_and_done() {
        let cards = vec![
            named("now", "Meridian", CardKind::Send, Some((5, "Meridian closes"))),
            named("old", "Older", CardKind::Close, Some((-5, "Older closed"))),
            named("recent", "Recent", CardKind::Close, Some((-1, "Recent closed"))),
            named("wait", "Waiting", CardKind::Triage, Some((5, "Waiting closes"))),
        ];
        let marks = vec![marked("wait", "Waiting", 5, true), marked("gone", "Gone", 2, false), marked("now", "Meridian", 5, true)];
        let agenda = Queue { marks, ..today_queue(cards) }.agenda();

        let rows: Vec<(&str, RowState)> = agenda.rows.iter().map(|row| (row.key.as_str(), row.state)).collect();
        assert_eq!(
            rows,
            [("old", RowState::Late(5)), ("recent", RowState::Late(1)), ("gone", RowState::Done), ("now", RowState::Now), ("wait", RowState::Waiting)]
        );
        assert_eq!(agenda.rows[0].text, "Older closed");
        assert_eq!(agenda.rows[0].on, day(-5));
        assert_eq!(agenda.rows[3].text, "Meridian closes");
        assert_eq!(agenda.closing, "Nothing else dated before Fri 13 Nov.");
        assert_eq!(agenda.returning, None);
    }

    /// A late card on top is the one being read: its row keeps its lateness and says so.
    #[test]
    fn agenda_marks_a_late_card_on_top_as_now() {
        let cards =
            vec![named("top", "Harbor", CardKind::Close, Some((-4, "Harbor closed"))), named("next", "Quarry", CardKind::Close, Some((-2, "Quarry closed")))];
        let agenda = today_queue(cards).agenda();

        let rows: Vec<(&str, RowState)> = agenda.rows.iter().map(|row| (row.key.as_str(), row.state)).collect();
        assert_eq!(rows, [("top", RowState::LateNow(4)), ("next", RowState::Late(2))]);
    }

    #[test]
    fn agenda_caps_rows_and_counts_the_rest() {
        let marks = (0..AGENDA_ROWS as i64 + 2).map(|offset| marked(&format!("k{offset}"), "Acme", offset, true)).collect();
        let agenda = Queue { marks, ..today_queue(vec![]) }.agenda();
        assert_eq!(agenda.rows.len(), AGENDA_ROWS);
        assert_eq!(agenda.closing, "2 more before Fri 13 Nov.");

        let exact = (0..AGENDA_ROWS as i64).map(|offset| marked(&format!("k{offset}"), "Acme", offset, true)).collect();
        assert_eq!(Queue { marks: exact, ..today_queue(vec![]) }.agenda().closing, "Nothing else dated before Fri 13 Nov.");
    }

    #[test]
    fn agenda_says_no_deadlines_when_nothing_is_dated() {
        let agenda = today_queue(vec![pending(CardKind::Triage, None)]).agenda();
        assert!(agenda.rows.is_empty());
        assert_eq!(agenda.closing, "No deadlines in the next two weeks.");
    }

    #[test]
    fn agenda_offers_tomorrows_return_row() {
        let queue = Queue { set_aside: vec!["Acme".into(), "Beta".into(), "Acme".into()], ..today_queue(vec![]) };
        let returning = queue.agenda().returning.unwrap();
        assert_eq!(returning, Returning { on: day(1), count: 3, names: vec!["Acme".into(), "Beta".into()] });
    }
}
