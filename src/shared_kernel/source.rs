/// A source the snapshot reads that can fail on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Jobs,
    Applications,
    SkillGaps,
    /// Today's decisions in Jobdesk's own data folder. The tray reports this one, not the snapshot.
    Decisions,
}
