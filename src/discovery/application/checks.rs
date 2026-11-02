use crate::discovery::application::JobView;
use crate::discovery::domain::{FreelanceGates, Gate, Verdict};
use crate::shared_kernel::CheckState;

const NOT_STATED: &str = "Not stated";
/// Where every score comes from. Jobs says it in its subtitle, so only Today adds it to a summary that passes all.
pub const AD_ALONE: &str = "Scored from the job ad alone.";

/// What one check on a posting is about. Its label and its sentence both come from here, so no screen matches on words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckKind {
    Contract,
    DayRate,
    Where,
    Commute,
    Language,
    Safety,
}

impl CheckKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Contract => "Contract",
            Self::DayRate => "Day rate",
            Self::Where => "Where",
            Self::Commute => "Commute",
            Self::Language => "Language",
            Self::Safety => "Safety check",
        }
    }

    /// Why a failed check counts against the job.
    fn failed(self) -> &'static str {
        match self {
            Self::Contract => "The posting is not open to freelancers.",
            Self::DayRate => "The stated rate is under your floor.",
            Self::Commute => "The commute is over your limit.",
            Self::Where => "The location does not suit you.",
            Self::Language => "The language does not suit you.",
            Self::Safety => "The safety check flagged this ad.",
        }
    }

    /// What to look at when the ad leaves this check unclear. A flagged ad the user kept says so.
    fn unclear(self, kept: bool) -> &'static str {
        match self {
            Self::Contract => "The ad does not say it is freelance, so check before you apply.",
            Self::DayRate => "The day rate is not in the ad, so check it before you apply.",
            Self::Commute => "The commute is not clear from the ad, so check it before you apply.",
            Self::Where => "The work mode is not clear from the ad, so check it before you apply.",
            Self::Language => "The language needs a look, so check it before you apply.",
            Self::Safety if kept => "The safety check flagged this ad and you kept it.",
            Self::Safety => "The safety check flagged this ad, so read what it found before you apply.",
        }
    }
}

impl From<Gate> for CheckState {
    fn from(gate: Gate) -> Self {
        match gate {
            Gate::Pass => Self::Pass,
            Gate::Flag => Self::Unclear,
            Gate::Fail => Self::Fail,
        }
    }
}

/// One check: what it is about, what the posting says and how it came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRow {
    pub kind: CheckKind,
    pub value: String,
    pub state: CheckState,
}

/// The checks on a posting and the sentence that sums them up, the same on Today and on Jobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobChecks {
    pub rows: Vec<CheckRow>,
    /// The summary without `AD_ALONE`.
    pub summary: String,
}

impl JobChecks {
    /// `kept` says the user read a flagged ad and kept it.
    pub fn of(job: &JobView, kept: bool) -> Self {
        let rows = check_rows(job);
        let summary = summary(&job.freelance, &rows, kept);
        Self { rows, summary }
    }

    pub fn passes_all(&self) -> bool {
        self.rows.iter().all(|row| row.state == CheckState::Pass)
    }
}

/// The checks that can suggest a skip. Each needs a Skip action, so a match on this stays exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreelanceGate {
    Contract,
    DayRate,
    Commute,
}

impl FreelanceGate {
    pub fn kind(self) -> CheckKind {
        match self {
            Self::Contract => CheckKind::Contract,
            Self::DayRate => CheckKind::DayRate,
            Self::Commute => CheckKind::Commute,
        }
    }
}

/// The first freelance gate that fails, which is why Claude suggests a skip.
pub fn failed_gate(gates: &FreelanceGates) -> Option<FreelanceGate> {
    [(FreelanceGate::Contract, &gates.contract), (FreelanceGate::DayRate, &gates.day_rate), (FreelanceGate::Commute, &gates.commute)]
        .into_iter()
        .find(|(_, reading)| reading.gate == Gate::Fail)
        .map(|(gate, _)| gate)
}

fn check_rows(job: &JobView) -> Vec<CheckRow> {
    let row = |kind, value: String, state| CheckRow { kind, value, state };
    let gates = &job.freelance;
    let mut rows = vec![
        row(CheckKind::Contract, gates.contract.value.clone(), gates.contract.gate.into()),
        row(CheckKind::DayRate, gates.day_rate.value.replace(", floor ", ", your floor "), gates.day_rate.gate.into()),
    ];
    if let Some(gate) = job.location_gate {
        rows.push(row(CheckKind::Where, job.location.clone().unwrap_or_else(|| NOT_STATED.to_owned()), gate.into()));
    }
    rows.push(row(CheckKind::Commute, gates.commute.value.clone(), gates.commute.gate.into()));
    if let Some(gate) = job.language_gate {
        let unsaid = if gate == Gate::Pass { "No conflict" } else { NOT_STATED };
        rows.push(row(CheckKind::Language, job.language_note.clone().unwrap_or_else(|| unsaid.to_owned()), gate.into()));
    }
    rows.push(match job.guard.as_deref() {
        Some(guard) if guard.trim() == "clean" => row(CheckKind::Safety, "Nothing hidden in the ad".to_owned(), CheckState::Pass),
        Some(guard) => row(CheckKind::Safety, guard.to_owned(), CheckState::Unclear),
        None => row(CheckKind::Safety, "Not checked".to_owned(), CheckState::Unchecked),
    });
    rows
}

fn number_word(count: usize) -> String {
    ["No", "One", "Two", "Three", "Four", "Five", "Six"].get(count).map_or_else(|| count.to_string(), |word| (*word).to_owned())
}

/// What to check first: the first row that fails, else the first that is unclear or unchecked.
fn weakest(rows: &[CheckRow], kept: bool) -> &'static str {
    let row = rows.iter().find(|row| row.state == CheckState::Fail).or_else(|| rows.iter().find(|row| row.state != CheckState::Pass));
    match row.map(|row| (row.kind, row.state)) {
        Some((kind, CheckState::Fail)) => kind.failed(),
        Some((_, CheckState::Unchecked)) => "The ad has not had a safety check yet.",
        Some((kind, _)) => kind.unclear(kept),
        None => "One check needs a look before you apply.",
    }
}

/// A failed freelance gate leads, because it is why Claude suggests a skip. Otherwise the passes, then the weakest check.
fn summary(gates: &FreelanceGates, rows: &[CheckRow], kept: bool) -> String {
    if let Some(gate) = failed_gate(gates) {
        return format!("Claude suggests you skip this. {}", gate.kind().failed());
    }
    let passing = rows.iter().filter(|row| row.state == CheckState::Pass).count();
    if passing == rows.len() {
        return format!("All {} checks pass.", number_word(rows.len()).to_lowercase());
    }
    let passes = if passing == 1 { "check passes" } else { "checks pass" };
    format!("{} {passes}. {}", number_word(passing), weakest(rows, kept))
}

/// The verdict and the score: "Strong fit, 84 of 100."
pub fn fit_band(verdict: Option<Verdict>, score: u8) -> String {
    let words = match verdict {
        Some(Verdict::Strong) => "Strong fit",
        Some(Verdict::Good) => "Good fit",
        Some(Verdict::Moderate) => "Moderate fit",
        Some(Verdict::Weak) => "Weak fit",
        Some(Verdict::Poor) => "Poor fit",
        None => return format!("Scored {score} of 100."),
    };
    format!("{words}, {score} of 100.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::application::{GateReading, Signal};

    fn read(gate: Gate) -> GateReading {
        GateReading { gate, value: "x".into() }
    }

    fn job() -> JobView {
        JobView {
            key: "acme".into(),
            title: "Lead".into(),
            company: "Acme".into(),
            location: None,
            url: None,
            portal: None,
            found_on: None,
            deadline: None,
            days_to_deadline: None,
            score: Some(80),
            verdict: None,
            scraper_fit: None,
            location_gate: None,
            language_gate: None,
            language_note: None,
            freelance: FreelanceGates { contract: read(Gate::Pass), day_rate: read(Gate::Pass), commute: read(Gate::Pass) },
            guard: Some("clean".into()),
            strengths: vec![],
            gaps: vec![],
            fetch_attempts: None,
            fetch_error: None,
            signals: Vec::<Signal>::new(),
            closing_soon: false,
            guard_flagged: false,
        }
    }

    fn safety(checks: &JobChecks) -> &CheckRow {
        checks.rows.iter().find(|row| row.kind == CheckKind::Safety).unwrap()
    }

    #[test]
    fn a_flagged_ad_is_not_an_unchecked_one() {
        let flagged = JobView { guard: Some("flagged: 1 hidden block".into()), guard_flagged: true, ..job() };
        let kept = JobChecks::of(&flagged, true);
        assert_eq!(safety(&kept).state, CheckState::Unclear);
        assert_eq!(kept.summary, "Three checks pass. The safety check flagged this ad and you kept it.");
        assert_eq!(JobChecks::of(&flagged, false).summary, "Three checks pass. The safety check flagged this ad, so read what it found before you apply.");

        let unchecked = JobChecks::of(&JobView { guard: None, ..job() }, false);
        assert_eq!((safety(&unchecked).state, safety(&unchecked).value.as_str()), (CheckState::Unchecked, "Not checked"));
        assert_eq!(unchecked.summary, "Three checks pass. The ad has not had a safety check yet.");
    }

    #[test]
    fn a_clean_guard_passes_with_spaces_around_it() {
        let checks = JobChecks::of(&JobView { guard: Some(" clean\n".into()), ..job() }, false);
        assert_eq!((safety(&checks).state, safety(&checks).value.as_str()), (CheckState::Pass, "Nothing hidden in the ad"));
        assert_eq!(checks.summary, "All four checks pass.");
        assert!(checks.passes_all());
    }

    #[test]
    fn labels_come_from_the_kind() {
        let both = JobView { location_gate: Some(Gate::Pass), language_gate: Some(Gate::Flag), ..job() };
        let checks = JobChecks::of(&both, false);
        assert_eq!(
            checks.rows.iter().map(|row| row.kind.label()).collect::<Vec<_>>(),
            ["Contract", "Day rate", "Where", "Commute", "Language", "Safety check"]
        );
        assert_eq!(checks.rows[4].value, "Not stated", "an unclear language with no note");
        assert_eq!(checks.summary, "Five checks pass. The language needs a look, so check it before you apply.");
    }

    #[test]
    fn a_failed_freelance_gate_leads_the_summary() {
        let rate = JobView { freelance: FreelanceGates { day_rate: read(Gate::Fail), ..job().freelance }, ..job() };
        assert_eq!(failed_gate(&rate.freelance), Some(FreelanceGate::DayRate));
        assert_eq!(JobChecks::of(&rate, false).summary, "Claude suggests you skip this. The stated rate is under your floor.");
    }
}
