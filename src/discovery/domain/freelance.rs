use crate::discovery::domain::Gate;

const NOT_FREELANCE: [&str; 9] = [
    "not freelance",
    "no freelance",
    "employee-only",
    "employee only",
    "permanent only",
    "permanent contract",
    "permanent position",
    "permanent role",
    "employment contract",
];
const FREELANCE: [&str; 4] = ["freelance", "contractor", "self-employed", "zelfstandig"];
const RATE_WORDS: [&str; 6] = ["rate", "rates", "daily", "tarif", "dagprijs", "tjm"];
const PER_DAY: [&str; 3] = ["per day", "a day", "/day"];
const CURRENCY: [&str; 4] = ["eur", "euro", "euros", "€"];
const FLOOR: &str = "floor";
/// Below this a figure is not a day rate, above it an annual salary or a budget.
const DAY_RATE_RANGE: std::ops::RangeInclusive<u32> = 100..=5000;
const NOT_STATED: &str = "Not stated";

/// What the candidate profile sets for these gates. Empty when the profile does not say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FreelanceTerms {
    pub day_rate_floor: Option<u32>,
    pub commute: Vec<CommuteTier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommuteTier {
    pub name: String,
    pub places: Vec<String>,
    pub note: String,
}

/// One gate and what it read. A gate with nothing to read flags, because silence is not a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateReading {
    pub gate: Gate,
    pub value: String,
}

impl GateReading {
    fn new(gate: Gate, value: impl Into<String>) -> Self {
        Self { gate, value: value.into() }
    }
}

/// `/rank` stores no contract, rate or commute fields, so these read what it does store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreelanceGates {
    pub contract: GateReading,
    pub day_rate: GateReading,
    pub commute: GateReading,
}

impl FreelanceGates {
    pub fn read(statements: &[&str], location: Option<&str>, location_failed: bool, terms: &FreelanceTerms) -> Self {
        Self {
            contract: contract_gate(statements),
            day_rate: rate_gate(statements, terms.day_rate_floor),
            commute: commute_gate(location, location_failed, &terms.commute),
        }
    }
}

fn contract_gate(statements: &[&str]) -> GateReading {
    let mentioning = |terms: &[&str]| statements.iter().find(|line| terms.iter().any(|term| line.to_lowercase().contains(term)));
    if let Some(line) = mentioning(&NOT_FREELANCE) {
        return GateReading::new(Gate::Fail, *line);
    }
    mentioning(&FREELANCE).map_or_else(|| GateReading::new(Gate::Flag, NOT_STATED), |line| GateReading::new(Gate::Pass, *line))
}

fn rate_gate(statements: &[&str], floor: Option<u32>) -> GateReading {
    let rate = statements.iter().find_map(|line| stated_rate(line));
    match (rate, floor) {
        (Some(rate), Some(floor)) => GateReading::new(if rate < floor { Gate::Fail } else { Gate::Pass }, format!("EUR {rate}, floor {floor}")),
        (Some(rate), None) => GateReading::new(Gate::Flag, format!("EUR {rate}, no floor in profile")),
        (None, Some(floor)) => GateReading::new(Gate::Flag, format!("{NOT_STATED}, floor {floor}")),
        (None, None) => GateReading::new(Gate::Flag, NOT_STATED),
    }
}

fn is_currency(word: &str) -> bool {
    CURRENCY.contains(&word.trim_matches(|c: char| !c.is_alphanumeric() && c != '€'))
}

/// The first amount with a currency on a line about a day rate, skipping the floor it may quote.
fn stated_rate(line: &str) -> Option<u32> {
    let lowered = line.to_lowercase();
    let is_about_rate =
        PER_DAY.iter().any(|term| lowered.contains(term)) || lowered.split(|c: char| !c.is_alphanumeric()).any(|word| RATE_WORDS.contains(&word));
    if !is_about_rate {
        return None;
    }
    let words: Vec<&str> = lowered.split_whitespace().collect();
    (0..words.len()).find_map(|at| {
        let amount = amount_in(words[at]).filter(|amount| DAY_RATE_RANGE.contains(amount))?;
        let before = at.checked_sub(1).map(|at| words[at]);
        let after = words.get(at + 1).copied();
        let priced = words[at].contains('€') || words[at].contains("eur") || before.is_some_and(is_currency) || after.is_some_and(is_currency);
        let beside_floor = at.checked_sub(2).map(|at| words[at]) == Some(FLOOR) || before == Some(FLOOR) || after == Some(FLOOR);
        (priced && !beside_floor).then_some(amount)
    })
}

/// `640`, `€640/day`, `1,200` and the low end of `640-750`. `65k` is not a day rate.
fn amount_in(word: &str) -> Option<u32> {
    let start = word.find(|c: char| c.is_ascii_digit())?;
    let rest = &word[start..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.')).unwrap_or(rest.len());
    if rest[end..].starts_with('k') {
        return None;
    }
    let digits: String = rest[..end].trim_end_matches([',', '.']).chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn fold(text: &str) -> String {
    let plain = |c| match c {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        other => other,
    };
    text.to_lowercase().chars().map(plain).collect()
}

fn commute_gate(location: Option<&str>, location_failed: bool, tiers: &[CommuteTier]) -> GateReading {
    if location_failed {
        return GateReading::new(Gate::Fail, "Failed the location check");
    }
    let Some(location) = location else { return GateReading::new(Gate::Flag, NOT_STATED) };
    let place = fold(location);
    if place.contains("remote") && !place.contains("hybrid") && !place.contains("no remote") {
        return GateReading::new(Gate::Pass, "Remote");
    }
    let tiered = tiers.iter().find_map(|tier| tier.places.iter().find(|name| place.contains(&fold(name))).map(|name| (tier, name)));
    match tiered {
        Some((tier, name)) => GateReading::new(tier_gate(&tier.name), format!("{name}, {}", tier.note)),
        None if tiers.is_empty() => GateReading::new(Gate::Flag, NOT_STATED),
        None => GateReading::new(Gate::Flag, "Not in your commute tiers"),
    }
}

/// The evaluation framework allows its borderline tier only with mostly remote work.
fn tier_gate(name: &str) -> Gate {
    match name {
        "borderline" => Gate::Flag,
        "too far" => Gate::Fail,
        _ => Gate::Pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiers() -> Vec<CommuteTier> {
        let tier = |name: &str, places: &[&str], note: &str| CommuteTier {
            name: name.into(),
            places: places.iter().map(|p| (*p).into()).collect(),
            note: note.into(),
        };
        vec![tier("ideal", &["Namur"], "<45 min"), tier("acceptable", &["Brussels", "Liège"], "up to 1h15"), tier("borderline", &["Ghent"], "1h30-2h")]
    }

    #[test]
    fn contract_needs_a_stated_line() {
        assert_eq!(contract_gate(&["Solution Architect Tech Lead (Freelance possible)"]).gate, Gate::Pass);
        assert_eq!(contract_gate(&["Java Lead", "Employee-only contract"]), GateReading::new(Gate::Fail, "Employee-only contract"));
        assert_eq!(contract_gate(&["Not freelance, payroll only"]).gate, Gate::Fail);
        assert_eq!(contract_gate(&["Java Lead"]), GateReading::new(Gate::Flag, NOT_STATED));
    }

    #[test]
    fn the_quarkus_cdi_skill_is_not_a_french_contract() {
        assert_eq!(contract_gate(&["Quarkus and CDI match"]).gate, Gate::Flag);
    }

    #[test]
    fn reads_rate_wordings() {
        let cases = [
            ("Stated day rate EUR 640", Some(640)),
            ("€780/day through a broker", Some(780)),
            ("Rate 1,200 EUR", Some(1200)),
            ("Rate EUR 640-750", Some(640)),
            ("Rate EUR 640, under the floor EUR 700", Some(640)),
            ("Under the floor EUR 700, stated rate EUR 640", Some(640)),
            ("Strategic programme of EUR 900", None),
            ("Salary EUR 65000", None),
            ("Rate around EUR 65k", None),
            ("Rate is negotiable", None),
        ];
        for (line, rate) in cases {
            assert_eq!(stated_rate(line), rate, "{line}");
        }
    }

    #[test]
    fn floor_decides_the_rate_gate() {
        let below = rate_gate(&["Day rate EUR 640"], Some(700));
        assert_eq!(below, GateReading::new(Gate::Fail, "EUR 640, floor 700"));
        assert_eq!(rate_gate(&["Day rate EUR 700"], Some(700)).gate, Gate::Pass);
        assert_eq!(rate_gate(&["Java"], Some(700)), GateReading::new(Gate::Flag, "Not stated, floor 700"));
        assert_eq!(rate_gate(&["Day rate EUR 640"], None).gate, Gate::Flag);
    }

    #[test]
    fn commute_follows_the_profile_tiers() {
        assert_eq!(commute_gate(Some("Namur, hybrid"), false, &tiers()), GateReading::new(Gate::Pass, "Namur, <45 min"));
        assert_eq!(commute_gate(Some("Liege"), false, &tiers()).gate, Gate::Pass);
        assert_eq!(commute_gate(Some("Ghent, hybrid"), false, &tiers()).gate, Gate::Flag);
        assert_eq!(commute_gate(Some("Remote, EU"), false, &tiers()), GateReading::new(Gate::Pass, "Remote"));
        assert_eq!(commute_gate(Some("Paris"), false, &tiers()), GateReading::new(Gate::Flag, "Not in your commute tiers"));
        assert_eq!(commute_gate(None, false, &tiers()).gate, Gate::Flag);
        assert_eq!(commute_gate(Some("Namur"), true, &tiers()).gate, Gate::Fail);
    }
}
