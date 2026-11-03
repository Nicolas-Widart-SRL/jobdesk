use std::path::PathBuf;

use chrono::NaiveDateTime;

use crate::shared_kernel::visible_text::visible_chars;

/// One file in `cv/`, as the folder listing saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    pub name: String,
    /// The file under the checkout's root, links unresolved, as the dossier names it, so its pages render once for both.
    pub path: PathBuf,
    /// Seconds since the epoch.
    pub modified: u64,
    /// The same moment in local time.
    pub at: NaiveDateTime,
    pub bytes: u64,
}

/// The files that share one stem in `cv/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CvFiles {
    pub stem: String,
    pub tex: Option<FileStamp>,
    pub pdf: Option<FileStamp>,
}

/// How a PDF stands against the `.tex` it is built from. Only the two modification times count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Current,
    /// A PDF with no `.tex` beside it: nothing can build it again.
    NoSource,
    SourceNewer,
    NotBuilt,
}

impl Freshness {
    /// Whether a build would help.
    pub fn out_of_date(self) -> bool {
        matches!(self, Freshness::SourceNewer | Freshness::NotBuilt)
    }
}

/// `None` when neither file exists.
pub fn freshness(tex: Option<&FileStamp>, pdf: Option<&FileStamp>) -> Option<Freshness> {
    match (tex, pdf) {
        (Some(tex), Some(pdf)) if tex.modified > pdf.modified => Some(Freshness::SourceNewer),
        (Some(_), Some(_)) => Some(Freshness::Current),
        (None, Some(_)) => Some(Freshness::NoSource),
        (Some(_), None) => Some(Freshness::NotBuilt),
        (None, None) => None,
    }
}

/// The one tag a document shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CvTag {
    Current,
    SourceNewer,
    NotBuilt,
    RenderFailed,
}

impl CvTag {
    pub fn word(self) -> &'static str {
        match self {
            CvTag::Current => "Current",
            CvTag::SourceNewer => "Source newer",
            CvTag::NotBuilt => "Not built yet",
            CvTag::RenderFailed => "Render failed",
        }
    }
}

/// A render failure turns only a PDF that is otherwise current into Render failed: a build may fix the others.
pub fn tag(freshness: Freshness, render_failed: bool) -> CvTag {
    match freshness {
        Freshness::SourceNewer => CvTag::SourceNewer,
        Freshness::NotBuilt => CvTag::NotBuilt,
        Freshness::Current | Freshness::NoSource if render_failed => CvTag::RenderFailed,
        Freshness::Current | Freshness::NoSource => CvTag::Current,
    }
}

/// The two groups of the list, in the order they show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Shelf {
    Master,
    Application,
}

const MASTER: &str = "main_example";

/// The shelf comes from the visible stem, the one `label` reads, so a hidden character cannot split the two.
pub fn shelf(stem: &str) -> Shelf {
    if visible_chars(stem).collect::<String>().starts_with(MASTER) { Shelf::Master } else { Shelf::Application }
}

/// The name a document goes by. The stem comes from a file name, so hidden characters are dropped.
/// Underscores and tabs read as spaces. A stem with nothing left after that reads "Main" when it had only the `main_` prefix and
/// "Untitled CV" when it had nothing visible at all.
pub fn label(stem: &str) -> String {
    let clean: String = visible_chars(stem).collect();
    let spaced = |text: &str| text.replace(['_', '\t'], " ").trim().to_owned();
    if shelf(&clean) == Shelf::Master {
        let rest = spaced(clean[MASTER.len()..].trim_start_matches('_'));
        return if rest.is_empty() { "Master CV".to_owned() } else { format!("Master CV, {rest}") };
    }
    let rest = spaced(clean.strip_prefix("main_").unwrap_or(&clean));
    let words = if rest.is_empty() { spaced(&clean) } else { rest };
    let mut letters = words.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => "Untitled CV".to_owned(),
    }
}

/// Whether a stem is safe to put in `/build-cv <stem>`: plain name characters, and no leading dash that would read as an option.
pub fn buildable(stem: &str) -> bool {
    !stem.is_empty() && !stem.starts_with('-') && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn stamp(modified: u64) -> FileStamp {
        FileStamp {
            name: "x".to_owned(),
            path: PathBuf::from("cv/x"),
            modified,
            at: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
            bytes: 1,
        }
    }

    #[test]
    fn a_pdf_older_than_its_tex_is_stale() {
        assert_eq!(freshness(Some(&stamp(10)), Some(&stamp(20))), Some(Freshness::Current));
        assert_eq!(freshness(Some(&stamp(20)), Some(&stamp(10))), Some(Freshness::SourceNewer));
        assert_eq!(freshness(Some(&stamp(10)), Some(&stamp(10))), Some(Freshness::Current));
    }

    #[test]
    fn a_lone_file_reads_by_its_kind() {
        assert_eq!(freshness(None, Some(&stamp(10))), Some(Freshness::NoSource));
        assert_eq!(freshness(Some(&stamp(10)), None), Some(Freshness::NotBuilt));
        assert_eq!(freshness(None, None), None);
    }

    #[test]
    fn only_a_build_that_helps_is_out_of_date() {
        assert!(Freshness::SourceNewer.out_of_date());
        assert!(Freshness::NotBuilt.out_of_date());
        assert!(!Freshness::Current.out_of_date());
        assert!(!Freshness::NoSource.out_of_date());
    }

    #[test]
    fn a_render_failure_keeps_the_tag_a_build_may_fix() {
        assert_eq!(tag(Freshness::NoSource, true), CvTag::RenderFailed);
        assert_eq!(tag(Freshness::Current, true), CvTag::RenderFailed);
        assert_eq!(tag(Freshness::SourceNewer, true), CvTag::SourceNewer);
        assert_eq!(tag(Freshness::NotBuilt, true), CvTag::NotBuilt);
        assert_eq!(tag(Freshness::Current, false), CvTag::Current);
        assert_eq!(tag(Freshness::NoSource, false), CvTag::Current);
    }

    #[test]
    fn tags_read_in_words() {
        assert_eq!(CvTag::Current.word(), "Current");
        assert_eq!(CvTag::SourceNewer.word(), "Source newer");
        assert_eq!(CvTag::NotBuilt.word(), "Not built yet");
        assert_eq!(CvTag::RenderFailed.word(), "Render failed");
    }

    #[test]
    fn masters_sort_before_applications() {
        assert_eq!(shelf("main_example"), Shelf::Master);
        assert_eq!(shelf("main_example_engineer"), Shelf::Master);
        assert_eq!(shelf("main_tessel_platform_lead"), Shelf::Application);
        assert!(Shelf::Master < Shelf::Application);
    }

    #[test]
    fn labels_read_from_the_stem() {
        assert_eq!(label("main_example"), "Master CV");
        assert_eq!(label("main_example_engineer"), "Master CV, engineer");
        assert_eq!(label("main_tessel_platform_lead"), "Tessel platform lead");
        assert_eq!(label("resume"), "Resume");
        assert_eq!(label("main_"), "Main");
        assert_eq!(label("main_ac\u{202e}me"), "Acme");
    }

    #[test]
    fn a_label_is_never_empty() {
        assert_eq!(label("main___"), "Main");
        assert_eq!(label("main_\t"), "Main");
        assert_eq!(label("_"), "Untitled CV");
        assert_eq!(label(""), "Untitled CV");
        assert_eq!(label("\u{200b}\u{202e}"), "Untitled CV");
    }

    #[test]
    fn a_tab_reads_as_a_space() {
        assert_eq!(label("main_acme\tplatform"), "Acme platform");
        assert_eq!(label("main_example\tlead"), "Master CV, lead");
    }

    #[test]
    fn the_shelf_and_the_label_read_the_same_stem() {
        let hidden = "main\u{200b}_example_lead";
        assert_eq!(shelf(hidden), Shelf::Master);
        assert_eq!(label(hidden), "Master CV, lead");
    }

    #[test]
    fn only_plain_stems_can_be_built() {
        assert!(buildable("main_tessel_platform_lead"));
        assert!(buildable("main-a1"));
        for stem in ["", "-x", "--all", "--list", "a b", "a;b", "café", "a\u{202e}"] {
            assert!(!buildable(stem), "{stem:?}");
        }
    }
}
