use std::collections::BTreeMap;

/// Where an application stands, in this context's own terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Drafted,
    Active,
    Interview,
    Offer,
    Hired,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Furthest {
    Applied,
    Interview,
    Offer,
    Hired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationFact {
    pub progress: Progress,
    pub furthest: Furthest,
    pub sector: String,
    pub channel: String,
    pub is_rejection: bool,
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tally {
    pub drafted: usize,
    pub active: usize,
    pub interview: usize,
    pub offer: usize,
    pub hired: usize,
    pub closed: usize,
}

/// `/html-report`'s statistics. Drafted rows were never submitted and stay out of all of them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SearchReport {
    pub sent: usize,
    pub resolved: usize,
    pub tally: Tally,
    pub reached_interview: usize,
    pub reached_offer: usize,
    pub reached_hired: usize,
    pub rejection_rate: Option<f32>,
    pub by_sector: Vec<(String, usize)>,
    pub by_channel: Vec<(String, usize)>,
}

const CALIBRATION_MINIMUM: usize = 3;

impl SearchReport {
    pub fn can_calibrate(&self) -> bool {
        self.resolved >= CALIBRATION_MINIMUM
    }

    pub fn from_facts(facts: &[ApplicationFact]) -> Self {
        let mut report = Self::default();
        report.tally.drafted = facts.iter().filter(|f| f.progress == Progress::Drafted).count();
        let sent: Vec<_> = facts.iter().filter(|f| f.progress != Progress::Drafted).collect();
        report.sent = sent.len();
        report.resolved = sent.iter().filter(|fact| fact.is_final).count();
        for fact in &sent {
            match fact.progress {
                Progress::Active => report.tally.active += 1,
                Progress::Interview => report.tally.interview += 1,
                Progress::Offer => report.tally.offer += 1,
                Progress::Hired => report.tally.hired += 1,
                Progress::Closed => report.tally.closed += 1,
                Progress::Drafted => {}
            }
        }
        let reached = |stage| sent.iter().filter(|f| f.furthest >= stage).count();
        report.reached_interview = reached(Furthest::Interview);
        report.reached_offer = reached(Furthest::Offer);
        report.reached_hired = reached(Furthest::Hired);

        let resolved = sent.iter().filter(|f| f.is_final).count();
        let rejected = sent.iter().filter(|f| f.is_rejection).count();
        report.rejection_rate = (resolved > 0).then(|| rejected as f32 / resolved as f32);

        report.by_sector = counted(sent.iter().map(|f| f.sector.as_str()));
        report.by_channel = counted(sent.iter().map(|f| f.channel.as_str()));
        report
    }

    pub fn past_screen_rate(&self) -> Option<f32> {
        (self.sent > 0).then(|| self.reached_interview as f32 / self.sent as f32)
    }
}

fn counted<'a>(values: impl Iterator<Item = &'a str>) -> Vec<(String, usize)> {
    let mut counts = BTreeMap::<String, usize>::new();
    for value in values {
        let name = if value.trim().is_empty() { "Unknown" } else { value.trim() };
        *counts.entry(name.to_owned()).or_default() += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(progress: Progress, furthest: Furthest, is_rejection: bool, is_final: bool) -> ApplicationFact {
        ApplicationFact { progress, furthest, sector: "Finance".into(), channel: String::new(), is_rejection, is_final }
    }

    #[test]
    fn drafted_stays_out_of_statistics() {
        let report = SearchReport::from_facts(&[fact(Progress::Drafted, Furthest::Applied, false, false)]);
        assert_eq!((report.sent, report.tally.drafted), (0, 1));
        assert_eq!(report.past_screen_rate(), None);
        assert!(report.by_sector.is_empty());
    }

    #[test]
    fn funnel_uses_history_and_rejection_rate_ignores_declines() {
        let report = SearchReport::from_facts(&[
            fact(Progress::Closed, Furthest::Interview, true, true),
            fact(Progress::Closed, Furthest::Offer, false, true),
            fact(Progress::Active, Furthest::Applied, false, false),
        ]);
        assert_eq!((report.reached_interview, report.reached_offer), (2, 1));
        assert_eq!(report.rejection_rate, Some(0.5));
        assert_eq!(report.by_channel, vec![("Unknown".to_owned(), 3)]);
    }
}
