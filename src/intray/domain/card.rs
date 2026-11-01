use chrono::NaiveDate;

use crate::intray::domain::SkipReason;
use crate::shared_kernel::WebUrl;
use crate::shared_kernel::dates::days_from;

/// How far ahead a date counts as a deadline, the span of Coming up.
pub const HORIZON_DAYS: i64 = 14;

/// Declared in the order cards of one tier and date are dealt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CardKind {
    Permission,
    Question,
    Turn,
    Offer,
    Send,
    Prep,
    GuardFlag,
    FollowUp,
    Triage,
    Close,
}

impl CardKind {
    pub fn minutes(self) -> usize {
        match self {
            Self::Send | Self::Offer => 3,
            Self::Prep => 2,
            _ => 1,
        }
    }

    fn pauses_a_run(self) -> bool {
        matches!(self, Self::Permission | Self::Question | Self::Turn)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    PausedRun,
    Deadline,
    Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    Pass,
    Flag,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRow {
    pub label: &'static str,
    pub value: String,
    pub check: Check,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Empty,
    Triage { score: u8, band: String, gates: Vec<GateRow>, strengths: Vec<String>, gaps: Vec<String> },
    Lines(Vec<(&'static str, String)>),
    Code { label: String, text: String },
    Options(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Opens the launcher on this line. Nothing runs until the user starts it there.
    Command(String),
    Dossier(String),
    Run(String),
    Posting(WebUrl),
    Copy(String),
    Skip(SkipReason),
    Keep,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub effect: Effect,
    /// For a command: the stable key of the task it opens, which a decision stores in place of the label.
    pub task: Option<&'static str>,
}

impl Action {
    pub fn new(label: impl Into<String>, effect: Effect) -> Self {
        Self { label: label.into(), effect, task: None }
    }

    pub fn opening(self, task: &'static str) -> Self {
        debug_assert!(crate::intray::domain::task_name(task).is_some(), "{task} is not a task a decision can name");
        Self { task: Some(task), ..self }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    pub on: NaiveDate,
    pub label: String,
}

/// What a card is about, so a screen can tell which of its rows wait in Today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    /// The job's key.
    Job(String),
    Application { company: String, role: String },
    /// The run's id.
    Run(String),
}

/// One pending decision. `primary` is the Y key: permission, question, guard and Your turn cards have none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Stable across snapshots, and new when the underlying state moves on.
    pub key: String,
    pub kind: CardKind,
    pub name: String,
    /// The company on a job or application card, shown above the title. None on a paused run.
    pub over: Option<String>,
    pub subject: Subject,
    pub title: String,
    pub subtitle: String,
    pub take: String,
    pub when: String,
    pub due: Option<Due>,
    /// Orders cards of one kind on one date, lowest first.
    pub weight: i64,
    pub body: Body,
    pub primary: Option<Action>,
    pub others: Vec<Action>,
    pub can_skip: bool,
    pub note: String,
    /// The slash command the primary action opens in the launcher, if it has one.
    pub command: Option<String>,
}

impl Card {
    fn days_left(&self, today: NaiveDate) -> Option<i64> {
        self.due.as_ref().map(|due| days_from(today, due.on))
    }

    fn tier(&self, today: NaiveDate) -> Tier {
        if self.kind.pauses_a_run() {
            Tier::PausedRun
        } else if self.days_left(today).is_some_and(|days| days < HORIZON_DAYS) {
            Tier::Deadline
        } else {
            Tier::Rest
        }
    }

    pub fn is_on_horizon(&self, today: NaiveDate) -> bool {
        self.days_left(today).is_some_and(|days| (0..HORIZON_DAYS).contains(&days))
    }

    pub fn pauses_a_run(&self) -> bool {
        self.kind.pauses_a_run()
    }
}

/// Paused runs, then deadlines by date, then the rest. A passed deadline sorts first of all dates.
pub fn rank(cards: &mut [Card], today: NaiveDate) {
    cards.sort_by_key(|card| {
        let tier = card.tier(today);
        let date = card.due.as_ref().filter(|_| tier == Tier::Deadline).map(|due| due.on);
        (tier, date, card.kind, card.weight)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
    }

    fn card(key: &str, kind: CardKind, days: Option<u64>) -> Card {
        Card {
            key: key.into(),
            kind,
            name: String::new(),
            over: None,
            subject: Subject::Run(String::new()),
            title: String::new(),
            subtitle: String::new(),
            take: String::new(),
            when: String::new(),
            due: days.map(|days| Due { on: today() + chrono::Days::new(days), label: String::new() }),
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
    fn deals_paused_runs_then_deadlines_then_the_rest() {
        let mut cards = vec![
            card("close", CardKind::Close, None),
            card("triage far", CardKind::Triage, Some(40)),
            card("triage soon", CardKind::Triage, Some(5)),
            card("send sooner", CardKind::Send, Some(3)),
            card("follow up", CardKind::FollowUp, None),
            card("question", CardKind::Question, None),
            card("permission", CardKind::Permission, None),
        ];

        rank(&mut cards, today());

        let keys: Vec<_> = cards.iter().map(|card| card.key.as_str()).collect();
        assert_eq!(keys, ["permission", "question", "send sooner", "triage soon", "follow up", "triage far", "close"]);
    }

    #[test]
    fn weight_orders_one_kind() {
        let mut cards = vec![Card { weight: -70, ..card("second", CardKind::Triage, None) }, Card { weight: -80, ..card("first", CardKind::Triage, None) }];
        rank(&mut cards, today());
        assert_eq!(cards[0].key, "first");
    }

    #[test]
    fn the_horizon_covers_fourteen_days_from_today() {
        assert!(card("a", CardKind::Send, Some(0)).is_on_horizon(today()));
        assert!(card("a", CardKind::Send, Some(13)).is_on_horizon(today()));
        assert!(!card("a", CardKind::Send, Some(14)).is_on_horizon(today()));
    }

    #[test]
    fn a_deadline_ranks_first_only_inside_the_horizon() {
        let mut cards = vec![card("fourteen", CardKind::Send, Some(14)), card("follow up", CardKind::FollowUp, None), card("thirteen", CardKind::Triage, Some(13))];
        rank(&mut cards, today());
        let keys: Vec<_> = cards.iter().map(|card| card.key.as_str()).collect();
        assert_eq!(keys, ["thirteen", "fourteen", "follow up"], "a date 14 days out ranks with the rest, by kind");
    }
}
