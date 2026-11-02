#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopicId {
    Identity,
    Experience,
    Education,
    Skills,
    Publications,
    References,
    Statements,
    HowIWork,
    Goals,
    Stories,
    JobSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    OnYourCv,
    GuidesClaude,
}

/// A file of the checkout that holds part of the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileFile {
    Candidate,
    Behavioral,
    Evaluation,
    CvTemplates,
    InterviewPrep,
    ClaudeMd,
    SearchQueries,
}

impl ProfileFile {
    pub const ALL: [ProfileFile; 7] = [
        ProfileFile::Candidate,
        ProfileFile::Behavioral,
        ProfileFile::Evaluation,
        ProfileFile::CvTemplates,
        ProfileFile::InterviewPrep,
        ProfileFile::ClaudeMd,
        ProfileFile::SearchQueries,
    ];

    /// The path from the checkout root, with `/` between parts.
    pub fn relative_path(self) -> &'static str {
        match self {
            ProfileFile::Candidate => ".claude/skills/job-application-assistant/01-candidate-profile.md",
            ProfileFile::Behavioral => ".claude/skills/job-application-assistant/02-behavioral-profile.md",
            ProfileFile::Evaluation => ".claude/skills/job-application-assistant/04-job-evaluation.md",
            ProfileFile::CvTemplates => ".claude/skills/job-application-assistant/05-cv-templates.md",
            ProfileFile::InterviewPrep => ".claude/skills/job-application-assistant/07-interview-prep.md",
            ProfileFile::ClaudeMd => "CLAUDE.md",
            ProfileFile::SearchQueries => ".claude/skills/job-scraper/search-queries.md",
        }
    }

    pub fn name(self) -> &'static str {
        let path = self.relative_path();
        path.rsplit('/').next().unwrap_or(path)
    }
}

/// Where a topic reads from in one file.
/// `parent: None, heading: Some(h)` is a `##` named `h`.
/// `parent: Some(p), heading: Some(h)` is a `###` named `h` under the `##` named `p`.
/// `parent: None, heading: None` is every `##` in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionRef {
    pub file: ProfileFile,
    pub parent: Option<&'static str>,
    pub heading: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicSpec {
    pub id: TopicId,
    pub title: &'static str,
    pub group: Group,
    pub sections: &'static [SectionRef],
    /// The `--section` value of `/setup`, when the command has one for this topic.
    pub setup_section: Option<&'static str>,
}

/// The `##` of `CLAUDE.md` whose `###` blocks hold the profile. The rest of that file is not read.
pub const CLAUDE_PROFILE: &str = "Candidate Profile";

const fn top(file: ProfileFile, heading: &'static str) -> SectionRef {
    SectionRef { file, parent: None, heading: Some(heading) }
}

const fn under(file: ProfileFile, parent: &'static str, heading: &'static str) -> SectionRef {
    SectionRef { file, parent: Some(parent), heading: Some(heading) }
}

const fn every(file: ProfileFile) -> SectionRef {
    SectionRef { file, parent: None, heading: None }
}

const fn claude(heading: &'static str) -> SectionRef {
    under(ProfileFile::ClaudeMd, CLAUDE_PROFILE, heading)
}

const fn search(heading: &'static str) -> SectionRef {
    top(ProfileFile::SearchQueries, heading)
}

const SCORING: &str = "Scoring Dimensions";

pub const TOPICS: [TopicSpec; 11] = [
    TopicSpec {
        id: TopicId::Identity,
        title: "Identity and languages",
        group: Group::OnYourCv,
        sections: &[top(ProfileFile::Candidate, "Identity"), claude("Identity")],
        setup_section: Some("identity"),
    },
    TopicSpec {
        id: TopicId::Experience,
        title: "Experience",
        group: Group::OnYourCv,
        sections: &[top(ProfileFile::Candidate, "Professional Experience"), claude("Professional Experience")],
        setup_section: Some("experience"),
    },
    TopicSpec {
        id: TopicId::Education,
        title: "Education",
        group: Group::OnYourCv,
        sections: &[top(ProfileFile::Candidate, "Education"), claude("Education")],
        setup_section: Some("education"),
    },
    TopicSpec {
        id: TopicId::Skills,
        title: "Skills",
        group: Group::OnYourCv,
        sections: &[
            top(ProfileFile::Candidate, "Technical Skills"),
            under(ProfileFile::Evaluation, SCORING, "Technical Skills Match"),
            claude("Technical Skills"),
            claude("Certifications"),
        ],
        setup_section: Some("skills"),
    },
    TopicSpec {
        id: TopicId::Publications,
        title: "Projects and publications",
        group: Group::OnYourCv,
        sections: &[
            top(ProfileFile::Candidate, "Independent Projects"),
            top(ProfileFile::Candidate, "Publications"),
            top(ProfileFile::Candidate, "Awards"),
            claude("Publications"),
            claude("Awards"),
        ],
        setup_section: Some("publications"),
    },
    TopicSpec {
        id: TopicId::References,
        title: "References",
        group: Group::OnYourCv,
        sections: &[top(ProfileFile::Candidate, "References")],
        setup_section: Some("references"),
    },
    TopicSpec {
        id: TopicId::Statements,
        title: "Profile statements",
        group: Group::OnYourCv,
        sections: &[under(ProfileFile::CvTemplates, "Section-by-Section Tailoring", "Profile Statement / Elevator Pitch")],
        setup_section: None,
    },
    TopicSpec {
        id: TopicId::HowIWork,
        title: "How I work",
        group: Group::GuidesClaude,
        sections: &[every(ProfileFile::Behavioral), claude("Behavioral Profile")],
        setup_section: Some("behavioral"),
    },
    TopicSpec {
        id: TopicId::Goals,
        title: "Goals and deal-breakers",
        group: Group::GuidesClaude,
        sections: &[
            under(ProfileFile::Evaluation, SCORING, "Location & Logistics"),
            under(ProfileFile::Evaluation, SCORING, "Career Alignment & Motivation"),
            claude("What Excites You"),
            claude("Target Sectors"),
            claude("Deal-breakers"),
        ],
        setup_section: Some("goals"),
    },
    TopicSpec {
        id: TopicId::Stories,
        title: "Interview stories",
        group: Group::GuidesClaude,
        sections: &[top(ProfileFile::InterviewPrep, "Ready-Made STAR Examples")],
        setup_section: None,
    },
    TopicSpec {
        id: TopicId::JobSearch,
        title: "Job search",
        group: Group::GuidesClaude,
        sections: &[search("Query Categories"), search("Location Filter"), search("Language Filter"), search("Date Filter")],
        setup_section: Some("search"),
    },
];

impl TopicId {
    pub fn spec(self) -> &'static TopicSpec {
        TOPICS.iter().find(|spec| spec.id == self).expect("every topic has a spec")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_topic_has_one_spec_and_files_have_names() {
        for spec in &TOPICS {
            assert_eq!(spec.id.spec().title, spec.title);
            assert!(!spec.sections.is_empty());
        }
        assert_eq!(ProfileFile::Candidate.name(), "01-candidate-profile.md");
        assert_eq!(ProfileFile::ClaudeMd.name(), "CLAUDE.md");
        assert_eq!(ProfileFile::SearchQueries.relative_path(), ".claude/skills/job-scraper/search-queries.md");
    }
}
