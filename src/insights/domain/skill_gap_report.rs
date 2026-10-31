use chrono::NaiveDate;

use crate::shared_kernel::LoadError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillGap {
    pub priority: String,
    pub skill: String,
    pub kind: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudyItem {
    pub topic: String,
    pub kind: String,
    pub time: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillGapReport {
    pub file_name: String,
    pub dated: Option<NaiveDate>,
    pub mode: String,
    pub gaps: Vec<SkillGap>,
    pub study_order: Vec<StudyItem>,
    pub total_time: Option<String>,
    pub gaps_closed: Vec<String>,
    pub new_gaps: Vec<String>,
    /// Shown when the sections cannot be read, because the report is generated prose.
    pub raw: String,
}

pub trait SkillGapSource {
    fn latest(&self) -> Result<Option<SkillGapReport>, LoadError>;
}
