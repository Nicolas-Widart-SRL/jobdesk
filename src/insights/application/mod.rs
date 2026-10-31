mod search_insights;
mod skill_gaps;

pub use search_insights::{SearchInsights, search_insights};
pub use skill_gaps::{SkillGapService, SkillGapView};

pub use crate::insights::domain::{SkillGap, StudyItem};
