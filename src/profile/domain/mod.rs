mod cv;
mod document;
mod fill;
mod topic;

pub use cv::{CvFiles, CvTag, FileStamp, Freshness, Shelf, buildable, freshness, label, shelf, tag};
pub use document::{Block, BlockKey, blocks, find, heading_len, insert_at, replace, role_heading, select};
pub use fill::{Fill, SaveCheck, check, topic_fill};
pub use topic::{Group, ProfileFile, SectionRef, TOPICS, TopicId, TopicSpec};

use crate::shared_kernel::LoadError;

/// Reads and writes the profile files of a checkout. A file that does not exist reads as `None`.
pub trait ProfileStore {
    fn read(&self, file: ProfileFile) -> Result<Option<String>, LoadError>;
    fn write(&self, file: ProfileFile, text: &str) -> Result<(), LoadError>;
}
