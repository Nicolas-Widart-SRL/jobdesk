/// A source the snapshot reads that can fail on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Jobs,
    Applications,
    SkillGaps,
    /// A profile file of the checkout. The others still read.
    Profile,
    /// The `cv/` folder. The rest of the profile still reads.
    CvDocuments,
    /// Today's decisions in Jobdesk's own data folder. The tray reports this one, not the snapshot.
    Decisions,
    /// The portal folders under `.agents/skills`. The search file they share is reported as a profile file.
    Portals,
}
