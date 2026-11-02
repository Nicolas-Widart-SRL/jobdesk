mod document;
mod fill;
mod topic;

pub use document::{Block, BlockKey, RoleHeading, blocks, find, heading_len, heading_matches, insert_at, replace, role_heading, section_blocks, select};
pub use fill::{Fill, SaveCheck, check, open_placeholders, topic_fill};
pub use topic::{CLAUDE_PROFILE, Group, ProfileFile, SectionRef, TOPICS, TopicId, TopicSpec};

pub use crate::shared_kernel::placeholders::blank_words;

use crate::shared_kernel::LoadError;

/// Reads and writes the profile files of a checkout. A file that does not exist reads as `None`.
pub trait ProfileStore {
    fn read(&self, file: ProfileFile) -> Result<Option<String>, LoadError>;
    fn write(&self, file: ProfileFile, text: &str) -> Result<(), LoadError>;
}
