use crate::insights::domain::{SkillGapReport, SkillGapSource};
use crate::shared_kernel::LoadError;

pub type SkillGapView = SkillGapReport;

pub struct SkillGapService<S> {
    source: S,
}

impl<S: SkillGapSource> SkillGapService<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    pub fn latest(&self) -> Result<Option<SkillGapView>, LoadError> {
        self.source.latest()
    }
}

/// One line of the skill gap table. `priority` and `source` are empty when the report gives none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapRow {
    pub skill: String,
    pub priority: String,
    pub is_new: bool,
    pub source: String,
}

/// The heatmap's rows, then each new gap the heatmap does not name, so no new gap goes unseen.
pub fn gap_rows(report: &SkillGapView) -> Vec<GapRow> {
    let is_new = |skill: &str| report.new_gaps.iter().any(|bullet| same_skill(new_gap(bullet).0, skill));
    let heatmap = report.gaps.iter().map(|gap| GapRow {
        skill: gap.skill.clone(),
        priority: gap.priority.clone(),
        is_new: is_new(&gap.skill),
        source: format!("{} · {}", gap.kind, gap.source),
    });
    let unlisted = report.new_gaps.iter().map(|bullet| new_gap(bullet)).filter(|(skill, _)| !report.gaps.iter().any(|gap| same_skill(skill, &gap.skill)));
    let unlisted = unlisted.map(|(skill, source)| GapRow { skill: skill.to_owned(), priority: String::new(), is_new: true, source: source.to_owned() });
    heatmap.chain(unlisted).collect()
}

/// A new gap bullet such as `TOGAF (4 jobs)` or `TOGAF: 4 jobs`, split into the skill and what the report says about it.
fn new_gap(bullet: &str) -> (&str, &str) {
    let bullet = bullet.trim();
    if let Some((skill, rest)) = bullet.split_once(" (")
        && let Some(note) = rest.strip_suffix(')')
    {
        return (skill.trim(), note.trim());
    }
    [": ", " — ", " – ", " - "]
        .iter()
        .find_map(|separator| bullet.split_once(separator))
        .map_or((bullet, ""), |(skill, note)| (skill.trim(), note.trim()))
}

fn same_skill(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insights::domain::SkillGap;

    fn gap(skill: &str) -> SkillGap {
        SkillGap { priority: "High".into(), skill: skill.into(), kind: "Hard".into(), source: "4 jobs".into() }
    }

    #[test]
    fn new_gaps_mark_their_heatmap_row_or_get_their_own() {
        let report = SkillGapReport { gaps: vec![gap("TOGAF"), gap("Azure")], new_gaps: vec!["togaf".into(), "Kafka Streams (3 jobs)".into(), "SAFe: 1 job".into()], ..Default::default() };
        let rows = gap_rows(&report);
        let seen: Vec<_> = rows.iter().map(|row| (row.skill.as_str(), row.priority.as_str(), row.is_new, row.source.as_str())).collect();
        assert_eq!(seen, vec![
            ("TOGAF", "High", true, "Hard · 4 jobs"),
            ("Azure", "High", false, "Hard · 4 jobs"),
            ("Kafka Streams", "", true, "3 jobs"),
            ("SAFe", "", true, "1 job"),
        ]);
    }

    #[test]
    fn a_bullet_that_names_a_heatmap_row_with_a_note_still_matches_it() {
        let report = SkillGapReport { gaps: vec![gap("TOGAF")], new_gaps: vec!["TOGAF (4 jobs)".into()], ..Default::default() };
        assert_eq!(gap_rows(&report).len(), 1);
        assert!(gap_rows(&report)[0].is_new);
    }
}
