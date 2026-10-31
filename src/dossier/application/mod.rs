mod dossier_view;
mod page_store;

pub use dossier_view::{Dossier, RunTranscript, compose_dossier};
pub use page_store::{PageState, PageStore};

pub use crate::dossier::domain::{
    ApplicationRef, ArchiveContents, ArchiveRepository, CheckItem, CheckResult, KeywordRow, NamedText, PageImage, PageRasterizer, PdfDocument, PostingText,
    ResearchNote, RunFindings,
};
pub use crate::tracking::application::{OutcomeRecord, StatusBucket};
