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
