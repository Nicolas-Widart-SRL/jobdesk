mod cv;
mod editor;
mod marks;
mod view;

pub use cv::{CvDocument, CvShelf, PageCount, RailWord, StateLine, cv_shelf, meta_words, rail_word, state_line};
pub use editor::{ProfileEditor, SaveOutcome};
pub use marks::{Mark, UpdatedMarks};
pub use view::{BlockView, ProfileView, Role, SectionView, TopicCounts, TopicView, home_country, profile_view, topic_counts};

/// Why Update with Claude… waits while a profile file it would write cannot be read. Settings › Your terms says the same.
pub const UNREADABLE: &str = "Fix the file first, so Claude does not write over it.";

pub use crate::dossier::application::{PageImage, PageState, PageStore, PdfDocument, RenderedPages};
pub use crate::profile::domain::{
    Block, BlockKey, Fill, Group, ProfileFile, ProfileStore, SectionRef, TOPICS, TopicId, TopicSpec, blocks, role_heading, topic_fill,
};
pub use crate::profile::domain::{CvFiles, CvTag, FileStamp, Freshness, Shelf};
