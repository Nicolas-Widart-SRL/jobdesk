use std::collections::{HashMap, HashSet};

use chrono::NaiveDateTime;

use crate::profile::application::view::{ProfileView, TopicView};
use crate::profile::domain::{BlockKey, TopicId};

/// What a run changed in a topic, for the head line of its pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub at: NaiveDateTime,
    /// The headings that changed. A `##` block reads as "the section".
    pub blocks: Vec<String>,
}

/// One block of a topic as a run left it, so two views can be compared.
/// A `##` with children counts only its own words, so a changed child marks the child alone.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Unit {
    key: BlockKey,
    label: String,
    text: String,
}

fn units(topic: &TopicView) -> Vec<Unit> {
    let label = |key: &BlockKey| if key.level == 2 { "the section".to_owned() } else { key.heading.clone() };
    let mut found = Vec::new();
    for section in &topic.sections {
        let text = if section.children.is_empty() { section.block.text.clone() } else { section.block.body.clone() };
        found.push(Unit { key: section.block.key.clone(), label: label(&section.block.key), text });
        found.extend(section.children.iter().map(|child| Unit { key: child.key.clone(), label: label(&child.key), text: child.text.clone() }));
    }
    found
}

/// The headings that differ between two states of a topic, each once, in file order.
fn changed_labels(before: &[Unit], after: &[Unit]) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    let mut push = |label: &str| {
        if !labels.iter().any(|known| known == label) {
            labels.push(label.to_owned());
        }
    };
    for unit in after {
        if before.iter().find(|old| old.key == unit.key).is_none_or(|old| old.text != unit.text) {
            push(&unit.label);
        }
    }
    for old in before {
        if !after.iter().any(|unit| unit.key == old.key) {
            push(&old.label);
        }
    }
    labels
}

/// Which topics a run changed: a mark on the rail until the user opens the topic, and a head line until they leave it.
#[derive(Debug, Default)]
pub struct UpdatedMarks {
    baseline: Option<HashMap<TopicId, Vec<Unit>>>,
    rail: HashSet<TopicId>,
    heads: HashMap<TopicId, Mark>,
}

impl UpdatedMarks {
    /// Remembers the topics as they are before a run. A topic that could not be read is left out.
    pub fn begin(&mut self, before: &ProfileView) {
        self.baseline = Some(before.topics.iter().filter(|topic| topic.readable).map(|topic| (topic.id, units(topic))).collect());
    }

    /// Marks the topics whose blocks differ from the baseline, then drops the baseline. Without `begin` it marks nothing.
    pub fn finish(&mut self, after: &ProfileView, at: NaiveDateTime) {
        let Some(baseline) = self.baseline.take() else { return };
        for topic in after.topics.iter().filter(|topic| topic.readable) {
            let Some(before) = baseline.get(&topic.id) else { continue };
            let blocks = changed_labels(before, &units(topic));
            if !blocks.is_empty() {
                self.rail.insert(topic.id);
                self.heads.insert(topic.id, Mark { at, blocks });
            }
        }
    }

    /// The user opened the topic: the rail mark goes, and the head line stays until they leave.
    pub fn open(&mut self, topic: TopicId) {
        self.rail.remove(&topic);
    }

    pub fn rail_mark(&self, topic: TopicId) -> bool {
        self.rail.contains(&topic)
    }

    pub fn head_line(&self, topic: TopicId) -> Option<&Mark> {
        self.heads.get(&topic)
    }

    pub fn leave(&mut self, topic: TopicId) {
        self.rail.remove(&topic);
        self.heads.remove(&topic);
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::profile::application::view::profile_view;
    use crate::profile::domain::ProfileFile;

    const CANDIDATE: &str = "## Identity\nName: Ada Quill\n\n## Professional Experience\nIntro.\n\n### Lead - Calder (2021 – now)\nShipped.\n\n### Dev - Acme (2018 – 2021)\nBuilt.\n\n## Education\nMSc\n";

    fn view(candidate: &str) -> ProfileView {
        let candidate = candidate.to_owned();
        profile_view(&|file| Ok((file == ProfileFile::Candidate).then(|| candidate.clone()))).0
    }

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, 1).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    #[test]
    fn a_change_to_one_block_marks_only_its_topic() {
        let mut marks = UpdatedMarks::default();
        marks.begin(&view(CANDIDATE));
        marks.finish(&view(&CANDIDATE.replace("Shipped.", "Shipped twice.")), at());
        assert!(marks.rail_mark(TopicId::Experience));
        assert!(!marks.rail_mark(TopicId::Identity));
        assert!(!marks.rail_mark(TopicId::Education));
        let head = marks.head_line(TopicId::Experience).unwrap();
        assert_eq!(head.blocks, ["Lead - Calder (2021 – now)"]);
        assert_eq!(head.at, at());
    }

    #[test]
    fn a_changed_section_body_reads_as_the_section() {
        let mut marks = UpdatedMarks::default();
        marks.begin(&view(CANDIDATE));
        marks.finish(&view(&CANDIDATE.replace("MSc", "MSc, Porto")), at());
        assert_eq!(marks.head_line(TopicId::Education).unwrap().blocks, ["the section"]);
    }

    #[test]
    fn opening_clears_the_rail_and_leaving_clears_the_head_line() {
        let mut marks = UpdatedMarks::default();
        marks.begin(&view(CANDIDATE));
        marks.finish(&view(&CANDIDATE.replace("MSc", "BSc")), at());
        marks.open(TopicId::Education);
        assert!(!marks.rail_mark(TopicId::Education));
        assert!(marks.head_line(TopicId::Education).is_some());
        marks.leave(TopicId::Education);
        assert!(marks.head_line(TopicId::Education).is_none());
    }

    #[test]
    fn finish_without_begin_marks_nothing() {
        let mut marks = UpdatedMarks::default();
        marks.finish(&view(CANDIDATE), at());
        assert!(!marks.rail_mark(TopicId::Identity));
        assert!(marks.head_line(TopicId::Identity).is_none());
        marks.begin(&view(CANDIDATE));
        marks.finish(&view(CANDIDATE), at());
        marks.finish(&view(&CANDIDATE.replace("MSc", "BSc")), at());
        assert!(!marks.rail_mark(TopicId::Education));
    }
}
