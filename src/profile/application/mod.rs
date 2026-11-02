mod marks;
mod view;

pub use marks::{Mark, UpdatedMarks};
pub use view::{BlockView, ProfileView, Role, SectionView, TopicCounts, TopicView, profile_view, topic_counts};

pub use crate::profile::domain::{
    Block, BlockKey, CLAUDE_PROFILE, Fill, Group, ProfileFile, ProfileStore, RoleHeading, SaveCheck, SectionRef, TOPICS, TopicId, TopicSpec, blank_words,
    blocks, check, find, heading_matches, insert_at, replace, role_heading, section_blocks, topic_fill,
};
