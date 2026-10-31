mod search_insights;
mod skill_gaps;
mod workflow;

pub use search_insights::{SearchInsights, search_insights};
pub use skill_gaps::{SkillGapService, SkillGapView};
pub use workflow::{WorkflowBand, workflow_band};

pub use crate::insights::domain::{SkillGap, StudyItem};
