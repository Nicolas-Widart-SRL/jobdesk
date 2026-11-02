use crate::profile::domain::{
    Block, BlockKey, Fill, Group, ProfileFile, SectionRef, TOPICS, TopicId, TopicSpec, blocks, heading_len, role_heading, select, topic_fill,
};
use crate::shared_kernel::LoadError;
use crate::shared_kernel::placeholders::{Piece, heading_content, placeholder_count, shown_pieces};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileView {
    pub topics: Vec<TopicView>,
    /// Whether any profile file exists in the checkout.
    pub any_found: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicView {
    pub id: TopicId,
    pub title: &'static str,
    pub group: Group,
    pub fill: Fill,
    /// The file names and headings the footnote of the topic names.
    pub files: Vec<&'static str>,
    pub headings: Vec<String>,
    pub sections: Vec<SectionView>,
    /// `/setup --section <s>`, when `/setup` has a section for the topic.
    pub setup_line: Option<String>,
    /// False when a file of the topic failed to read.
    pub readable: bool,
    /// True when a section the topic names is not in a file that was found.
    pub missing_heading: bool,
}

/// A block a topic names. A `##` section holds its `###` blocks as `children`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionView {
    pub block: BlockView,
    pub children: Vec<BlockView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockView {
    pub key: BlockKey,
    pub heading: String,
    /// The block as it stands in the file, without the blank lines after it.
    pub text: String,
    /// The block without its heading line. For a `##` with children it stops before the first child.
    pub body: String,
    pub role: Option<Role>,
    /// The open tokens the page shows, in the heading line and `body`. A child counts its own, so the blocks of a topic
    /// add up to its fill.
    pub placeholders: usize,
}

/// A role's parts as the page shows them, each with a blank exactly where the block's count found a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub title: Vec<Piece>,
    pub company: Vec<Piece>,
    pub dates: Vec<Piece>,
}

enum Loaded {
    Failed,
    Absent,
    Text { text: String, blocks: Vec<Block> },
}

/// Reads each profile file once and composes the topics from the blocks they name.
/// Only the `###` blocks of `CLAUDE.md` that a topic names reach the view.
pub fn profile_view(read: &dyn Fn(ProfileFile) -> Result<Option<String>, LoadError>) -> (ProfileView, Vec<LoadError>) {
    let mut errors = Vec::new();
    let sources: Vec<(ProfileFile, Loaded)> = ProfileFile::ALL
        .iter()
        .map(|&file| {
            let source = match read(file) {
                Ok(Some(text)) => Loaded::Text { blocks: blocks(&text), text },
                Ok(None) => Loaded::Absent,
                Err(error) => {
                    errors.push(error);
                    Loaded::Failed
                }
            };
            (file, source)
        })
        .collect();
    let any_found = sources.iter().any(|(_, source)| matches!(source, Loaded::Text { .. }));
    let source_of = |file: ProfileFile| sources.iter().find(|(found, _)| *found == file).map(|(_, source)| source);
    let topics = TOPICS.iter().map(|spec| topic_view(spec, &source_of)).collect();
    (ProfileView { topics, any_found }, errors)
}

fn topic_view<'a>(spec: &TopicSpec, source_of: &dyn Fn(ProfileFile) -> Option<&'a Loaded>) -> TopicView {
    let mut sections = Vec::new();
    let mut readable = true;
    let mut missing_heading = false;
    for section in spec.sections {
        match source_of(section.file) {
            Some(Loaded::Failed) => readable = false,
            Some(Loaded::Text { text, blocks }) => {
                let chosen = select(blocks, section);
                missing_heading |= chosen.is_empty() && section.heading.is_some();
                sections.extend(chosen.iter().map(|block| section_view(text, blocks, section.file, block)));
            }
            Some(Loaded::Absent) | None => {}
        }
    }
    let texts: Vec<&str> = sections.iter().map(|section| section.block.text.as_str()).collect();
    TopicView {
        id: spec.id,
        title: spec.title,
        group: spec.group,
        fill: topic_fill(&texts, readable),
        files: files_named(spec.sections),
        headings: headings_named(spec.sections),
        sections,
        setup_line: spec.setup_section.map(|section| format!("/setup --section {section}")),
        readable,
        missing_heading,
    }
}

fn files_named(sections: &[SectionRef]) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for section in sections {
        if !names.contains(&section.file.name()) {
            names.push(section.file.name());
        }
    }
    names
}

fn headings_named(sections: &[SectionRef]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for heading in sections.iter().filter_map(|section| section.heading) {
        if !names.iter().any(|name| name == heading) {
            names.push(heading.to_owned());
        }
    }
    names
}

fn section_view(text: &str, all: &[Block], file: ProfileFile, block: &Block) -> SectionView {
    let children: Vec<&Block> = if block.level == 2 {
        all.iter().filter(|child| child.level == 3 && child.range.start >= block.range.start && child.range.end <= block.range.end).collect()
    } else {
        Vec::new()
    };
    let own_end = children.first().map_or(block.range.end, |first| first.range.start);
    SectionView {
        block: block_view(text, file, block, own_end),
        children: children.into_iter().map(|child| block_view(text, file, child, child.range.end)).collect(),
    }
}

/// `own_end` is where the block's own words stop: the end of the block, or the start of its first child.
fn block_view(text: &str, file: ProfileFile, block: &Block, own_end: usize) -> BlockView {
    let full = text[block.range.clone()].trim_end();
    let body_start = (block.range.start + heading_len(full)).min(own_end);
    let own = &text[block.range.start..own_end];
    let role = if block.level == 3 { role_of(own) } else { None };
    let body = text[body_start..own_end].trim().to_owned();
    BlockView {
        key: BlockKey { file, level: block.level, heading: block.heading.clone(), parent: block.parent.clone(), nth: block.nth },
        heading: block.heading.clone(),
        text: full.to_owned(),
        placeholders: placeholder_count(own),
        body,
        role,
    }
}

/// The role a `###` block's heading line names, its parts cut from the line by source position, so each blank in them
/// is one the block's count found.
fn role_of(own: &str) -> Option<Role> {
    let role = role_heading(own[heading_content(own)?].trim())?;
    let pieces = |part: &str| {
        let start = part.as_ptr() as usize - own.as_ptr() as usize;
        shown_pieces(own, start..start + part.len())
    };
    Some(Role { title: pieces(role.title), company: pieces(role.company), dates: pieces(role.dates) })
}

/// How the topics add up: `filled` of `total`, every topic. `hidden` of them could not be read, so they are neither filled nor not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopicCounts {
    pub filled: usize,
    pub total: usize,
    pub hidden: usize,
    pub any_found: bool,
}

pub fn topic_counts(view: &ProfileView) -> TopicCounts {
    let hidden = view.topics.iter().filter(|topic| topic.fill == Fill::Hidden).count();
    let filled = view.topics.iter().filter(|topic| topic.fill == Fill::Filled).count();
    TopicCounts { filled, total: view.topics.len(), hidden, any_found: view.any_found }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANDIDATE: &str = "# Candidate Profile\n\n## Identity\nName: Ada Quill\n\n## Professional Experience\nIntro line.\n\n### Lead Engineer - Calder Freight (2021 – present)\nShipped a thing.\n\n### [JOB_TITLE] - [COMPANY] ([START] - [END])\n- [ACHIEVEMENT]\n\n## Education\n[DEGREE] at [SCHOOL]\n\n## Technical Skills\nRust\n";
    const CLAUDE_MD: &str =
        "# Notes\n\n## Candidate Profile\n\n### Identity\nAda, Lisbon\n\n### Deal-breakers\nNo travel.\n\n## Repo Structure\nsecret-layout-text\n";

    fn view_of(files: &[(ProfileFile, &str)]) -> ProfileView {
        let files: Vec<(ProfileFile, String)> = files.iter().map(|(file, text)| (*file, text.to_string())).collect();
        profile_view(&|file| Ok(files.iter().find(|(found, _)| *found == file).map(|(_, text)| text.clone()))).0
    }

    fn topic(view: &ProfileView, id: TopicId) -> &TopicView {
        view.topics.iter().find(|topic| topic.id == id).unwrap()
    }

    #[test]
    fn topics_read_the_blocks_they_name() {
        let view = view_of(&[(ProfileFile::Candidate, CANDIDATE), (ProfileFile::ClaudeMd, CLAUDE_MD)]);
        assert!(view.any_found);
        assert_eq!(view.topics.len(), 11);
        let identity = topic(&view, TopicId::Identity);
        assert_eq!(identity.fill, Fill::Filled);
        assert_eq!(identity.sections.len(), 2);
        assert_eq!(identity.sections[0].block.body, "Name: Ada Quill");
        assert_eq!(identity.files, ["01-candidate-profile.md", "CLAUDE.md"]);
        assert_eq!(identity.setup_line.as_deref(), Some("/setup --section identity"));
        assert!(!identity.missing_heading);
        let experience = topic(&view, TopicId::Experience);
        assert_eq!(experience.fill, Fill::ToFill(5));
        let section = &experience.sections[0];
        assert_eq!(section.block.body, "Intro line.");
        assert_eq!(section.children.len(), 2);
        assert_eq!(section.children[0].role.as_ref().map(|role| role.dates.clone()), Some(vec![Piece::Text("2021 – present".to_owned())]));
        assert_eq!(section.children[1].placeholders, 5);
        assert_eq!(section.block.placeholders, 0);
        assert_eq!(section.children[0].key.parent.as_deref(), Some("Professional Experience"));
        assert_eq!(topic(&view, TopicId::Education).fill, Fill::ToFill(2));
        assert!(topic(&view, TopicId::Skills).missing_heading);
        assert!(topic(&view, TopicId::Statements).setup_line.is_none());
    }

    #[test]
    fn a_missing_section_in_a_found_file_is_flagged() {
        let view = view_of(&[(ProfileFile::Candidate, "## Identity\nx\n")]);
        assert!(topic(&view, TopicId::Education).missing_heading);
        let none = view_of(&[]);
        assert!(!none.any_found);
        assert!(!topic(&none, TopicId::Education).missing_heading);
    }

    #[test]
    fn claude_md_shows_only_the_named_profile_blocks() {
        let view = view_of(&[(ProfileFile::ClaudeMd, CLAUDE_MD)]);
        let shown: Vec<&str> = view
            .topics
            .iter()
            .flat_map(|topic| topic.sections.iter())
            .flat_map(|section| std::iter::once(&section.block).chain(section.children.iter()))
            .flat_map(|block| [block.text.as_str(), block.body.as_str(), block.heading.as_str()])
            .collect();
        assert!(!shown.is_empty());
        assert!(shown.iter().all(|text| !text.contains("Repo Structure") && !text.contains("secret-layout-text")));
        assert!(shown.iter().all(|text| !text.contains("Candidate Profile")));
        assert_eq!(topic(&view, TopicId::Goals).sections[0].block.heading, "Deal-breakers");
    }

    #[test]
    fn front_matter_never_reaches_a_topic() {
        let template = "---\nframework_version: 1.1.1\n---\n\n# Behavioral Profile\n\n## Working Style\n<!-- Describe how you work [PACE] -->\n";
        let view = view_of(&[(ProfileFile::Behavioral, template)]);
        let how = topic(&view, TopicId::HowIWork);
        assert_eq!(how.sections.len(), 1);
        assert_eq!(how.sections[0].block.heading, "Working Style");
        assert_eq!(how.fill, Fill::NotStarted);
    }

    #[test]
    fn the_count_and_the_fill_agree_on_comments() {
        let view = view_of(&[(ProfileFile::Candidate, "## Identity\n<!-- Format: [CITY], [COUNTRY] -->\nLisbon, Portugal\n")]);
        let identity = topic(&view, TopicId::Identity);
        assert_eq!(identity.sections[0].block.placeholders, 0);
        assert_eq!(identity.fill, Fill::Filled);
    }

    #[test]
    fn a_blank_counts_once_across_a_section_and_its_children() {
        let text = "## Professional Experience\nIntro [SUMMARY_X]\n\n### Lead - Calder (2021 – now)\n[ACHIEVEMENT_1] and [ACHIEVEMENT_2]\n\n### [JOB_TITLE] - [COMPANY] ([START] - [END])\n[ACHIEVEMENT_3]\n";
        let view = view_of(&[(ProfileFile::Candidate, text)]);
        let experience = topic(&view, TopicId::Experience);
        let section = &experience.sections[0];
        let counted: usize = section.block.placeholders + section.children.iter().map(|child| child.placeholders).sum::<usize>();
        assert_eq!(counted, 8);
        assert_eq!((section.block.placeholders, section.children[0].placeholders, section.children[1].placeholders), (1, 2, 5));
        assert_eq!(experience.fill, Fill::ToFill(counted));
    }

    #[test]
    fn a_section_of_empty_child_headings_is_not_started() {
        let text = "## Technical Skills\n\n### Programming & ML\n\n### Domain Expertise\n";
        let view = view_of(&[(ProfileFile::Candidate, text)]);
        assert_eq!(topic(&view, TopicId::Skills).fill, Fill::NotStarted);
    }

    #[test]
    fn a_failed_read_hides_the_topic_and_reports_the_error() {
        let (view, errors) =
            profile_view(&|file| if file == ProfileFile::Evaluation { Err(LoadError::new("04-job-evaluation.md", "denied")) } else { Ok(None) });
        assert_eq!(errors.len(), 1);
        assert!(!topic(&view, TopicId::Goals).readable);
        assert_eq!(topic(&view, TopicId::Goals).fill, Fill::Hidden);
        assert_eq!(topic(&view, TopicId::Identity).fill, Fill::NotStarted);
        let counts = topic_counts(&view);
        assert_eq!((counts.filled, counts.total, counts.hidden, counts.any_found), (0, 11, 2, false));
    }

    /// A role's parts and its count come from the one page of its block, heading line included.
    #[test]
    fn a_role_draws_and_counts_from_its_page() {
        let text = "## Professional Experience\n\n### Lead \\[TEAM_NAME] - Acme (2020)\nShipped.\n\n### Lead &#91;TEAM_NAME&#93; - Acme (2020)\nShipped.\n\n### [JOB_TITLE] - Acme (2020)\nShipped.\n";
        let view = view_of(&[(ProfileFile::Candidate, text)]);
        let roles = &topic(&view, TopicId::Experience).sections[0].children;
        let shown: Vec<(Vec<Piece>, usize)> = roles.iter().map(|role| (role.role.as_ref().unwrap().title.clone(), role.placeholders)).collect();
        let text = |text: &str| vec![Piece::Text(text.to_owned())];
        assert_eq!(shown, [(text("Lead [TEAM_NAME]"), 0), (text("Lead [TEAM_NAME]"), 0), (vec![Piece::Blank("to fill: job title".to_owned())], 1)]);
        assert_eq!(roles[2].body, "Shipped.");
    }

    /// A block counts what its page shows: a token in a comment's HTML block is not drawn as text, and one in code is.
    #[test]
    fn a_block_counts_what_it_shows() {
        let text = "## Identity\n<!-- e.g. Ada --> [FIRST_NAME]\nRun `<!-- [YOUR_CITY] -->`\n";
        let view = view_of(&[(ProfileFile::Candidate, text)]);
        let identity = &topic(&view, TopicId::Identity).sections[0].block;
        assert_eq!(identity.placeholders, 1);
        assert!(identity.text.contains("<!-- e.g. Ada -->"), "the editor keeps the raw text");
        assert_eq!(topic(&view, TopicId::Identity).fill, Fill::ToFill(1));
        let comments = view_of(&[(ProfileFile::Candidate, "## Identity\n<!-- Your name and city -->\n\n## Education\nBSc\n")]);
        assert_eq!(topic(&comments, TopicId::Identity).fill, Fill::NotStarted);
        // `<!-->` is an empty comment, so the words after it show, and the topic is started.
        let empty = view_of(&[(ProfileFile::Candidate, "## Identity\n<!-->Open to relocation: no [CITY_NAME]\n\n## Education\nBSc\n")]);
        assert_eq!(topic(&empty, TopicId::Identity).fill, Fill::Filled);
    }

    /// A closed heading or a comment after it still names a role, cut from the heading's own content.
    #[test]
    fn a_closed_or_annotated_heading_is_still_a_role() {
        for heading in [
            "### Lead Engineer - Calder Freight (2021 – present) ###",
            "### Lead Engineer - Calder Freight (2021 – present) <!-- check dates -->",
            "### [JOB_TITLE] - Calder Freight (2021 – present) ##",
        ] {
            let text = format!("## Professional Experience\n\n{heading}\nShipped.\n");
            let view = view_of(&[(ProfileFile::Candidate, &text)]);
            let role = topic(&view, TopicId::Experience).sections[0].children[0].role.clone().unwrap_or_else(|| panic!("{heading} is no role"));
            assert_eq!(role.company, [Piece::Text("Calder Freight".to_owned())], "{heading}");
            assert_eq!(role.dates, [Piece::Text("2021 – present".to_owned())], "{heading}");
        }
        let view = view_of(&[(ProfileFile::Candidate, "## Professional Experience\n\n### [JOB_TITLE] - Calder Freight (2021 – present) ##\nShipped.\n")]);
        let child = &topic(&view, TopicId::Experience).sections[0].children[0];
        assert_eq!(child.role.as_ref().unwrap().title, [Piece::Blank("to fill: job title".to_owned())]);
        assert_eq!(child.placeholders, 1);
    }
}
