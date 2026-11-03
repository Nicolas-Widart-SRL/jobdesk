use chrono::NaiveDateTime;

use crate::dossier::application::PdfDocument;
use crate::profile::domain::{CvFiles, CvTag, FileStamp, Freshness, Shelf, buildable, freshness, label, shelf, tag};
use crate::settings::application::{age_words, size_short};
use crate::shared_kernel::visible_text::visible_chars;

/// One stem in `cv/`, with what its files say about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CvDocument {
    pub stem: String,
    pub label: String,
    pub shelf: Shelf,
    pub tex: Option<FileStamp>,
    pub pdf: Option<FileStamp>,
    pub freshness: Freshness,
}

impl CvDocument {
    /// The line the launcher opens with, only when a build would help and the stem is safe to type.
    pub fn build_line(&self) -> Option<String> {
        (self.freshness.out_of_date() && buildable(&self.stem)).then(|| format!("/build-cv {}", self.stem))
    }

    pub fn pdf_document(&self) -> Option<PdfDocument> {
        let pdf = self.pdf.as_ref()?;
        Some(PdfDocument { label: self.label.clone(), path: pdf.path.clone(), modified: pdf.modified })
    }

    /// The file name under the label: the PDF's, or else the `.tex`'s, without hidden characters.
    pub fn shown_name(&self) -> String {
        self.pdf.as_ref().or(self.tex.as_ref()).map(|file| visible_chars(&file.name).collect()).unwrap_or_default()
    }

    /// The later of the two modification times.
    fn newest(&self) -> u64 {
        self.tex.iter().chain(&self.pdf).map(|file| file.modified).max().unwrap_or_default()
    }
}

/// The list of CV documents, or that `cv/` could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CvShelf {
    Read(Vec<CvDocument>),
    Unread,
}

impl Default for CvShelf {
    fn default() -> Self {
        CvShelf::Read(Vec::new())
    }
}

/// Masters first, by stem. Applications follow, newest first, ties by stem. A stem with neither file is dropped.
pub fn cv_shelf(files: Vec<CvFiles>) -> CvShelf {
    let mut documents: Vec<CvDocument> = files
        .into_iter()
        .filter_map(|files| {
            let freshness = freshness(files.tex.as_ref(), files.pdf.as_ref())?;
            Some(CvDocument { label: label(&files.stem), shelf: shelf(&files.stem), stem: files.stem, tex: files.tex, pdf: files.pdf, freshness })
        })
        .collect();
    documents.sort_by(|a, b| match a.shelf {
        Shelf::Master => a.shelf.cmp(&b.shelf).then_with(|| a.stem.cmp(&b.stem)),
        Shelf::Application => a.shelf.cmp(&b.shelf).then_with(|| b.newest().cmp(&a.newest())).then_with(|| a.stem.cmp(&b.stem)),
    });
    CvShelf::Read(documents)
}

/// The state word on the rail row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailWord {
    OutOfDate(usize),
    Documents(usize),
    NoneYet,
    Hidden,
}

impl RailWord {
    pub fn words(self) -> String {
        match self {
            RailWord::OutOfDate(count) => format!("{count} out of date"),
            RailWord::Documents(1) => "1 document".to_owned(),
            RailWord::Documents(count) => format!("{count} documents"),
            RailWord::NoneYet => "None yet".to_owned(),
            RailWord::Hidden => "Hidden".to_owned(),
        }
    }
}

pub fn rail_word(shelf: &CvShelf) -> RailWord {
    match shelf {
        CvShelf::Unread => RailWord::Hidden,
        CvShelf::Read(documents) if documents.is_empty() => RailWord::NoneYet,
        CvShelf::Read(documents) => match documents.iter().filter(|doc| doc.freshness.out_of_date()).count() {
            0 => RailWord::Documents(documents.len()),
            stale => RailWord::OutOfDate(stale),
        },
    }
}

/// How many pages rendered, and how many the PDF holds: a render stops after its first few.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCount {
    pub shown: usize,
    pub total: usize,
}

/// "2 pages, 84 KB, built 3 days ago", or "8 pages, the first 6 shown, …" when the render stopped short. A `.tex` with
/// no PDF reads "written 3 days ago".
pub fn meta_words(doc: &CvDocument, pages: Option<PageCount>, now: NaiveDateTime) -> String {
    let Some(pdf) = &doc.pdf else {
        return doc.tex.as_ref().map(|tex| format!("written {}", age_words(tex.at, now))).unwrap_or_default();
    };
    let built = format!("{}, built {}", size_short(pdf.bytes), age_words(pdf.at, now));
    match pages {
        Some(PageCount { shown, total }) if shown < total => format!("{total} pages, the first {shown} shown, {built}"),
        Some(PageCount { total: 1, .. }) => format!("1 page, {built}"),
        Some(PageCount { total, .. }) => format!("{total} pages, {built}"),
        None => built,
    }
}

/// The tag, the sentence beside it, and the build that would help as its label and the line the launcher opens with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateLine {
    pub tag: CvTag,
    pub sentence: String,
    pub action: Option<(&'static str, String)>,
}

pub fn state_line(doc: &CvDocument, render_failed: bool, now: NaiveDateTime) -> StateLine {
    let tag = tag(doc.freshness, render_failed);
    let (sentence, label) = match (tag, doc.freshness) {
        (CvTag::RenderFailed, _) => ("The file is there, but Jobdesk could not draw its pages.".to_owned(), None),
        (_, Freshness::NoSource) => ("No .tex sits beside this PDF, so there is nothing to build it from again.".to_owned(), None),
        (_, Freshness::SourceNewer) => {
            let when = doc.tex.as_ref().map(|tex| age_words(tex.at, now)).unwrap_or_default();
            (format!("Its .tex changed {when}, after this PDF was built. Run /build-cv in the launcher to build it again."), Some("Rebuild the PDF…"))
        }
        (_, Freshness::NotBuilt) => ("Its .tex has no PDF beside it yet. Run /build-cv in the launcher to build one.".to_owned(), Some("Build the PDF…")),
        (_, Freshness::Current) => ("Built after its .tex last changed.".to_owned(), None),
    };
    let action = label.and_then(|label| doc.build_line().map(|line| (label, line)));
    StateLine { tag, sentence, action }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::NaiveDate;

    use super::*;

    fn at(day: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 3, day).unwrap().and_hms_opt(9, 0, 0).unwrap()
    }

    fn stamp(name: &str, day: u32, bytes: u64) -> FileStamp {
        FileStamp { name: name.to_owned(), path: PathBuf::from("cv").join(name), modified: u64::from(day) * 86_400, at: at(day), bytes }
    }

    fn files(stem: &str, tex: Option<u32>, pdf: Option<u32>) -> CvFiles {
        CvFiles {
            stem: stem.to_owned(),
            tex: tex.map(|day| stamp(&format!("{stem}.tex"), day, 2_000)),
            pdf: pdf.map(|day| stamp(&format!("{stem}.pdf"), day, 84_000)),
        }
    }

    fn doc(stem: &str, tex: Option<u32>, pdf: Option<u32>) -> CvDocument {
        match cv_shelf(vec![files(stem, tex, pdf)]) {
            CvShelf::Read(mut docs) => docs.remove(0),
            CvShelf::Unread => unreachable!(),
        }
    }

    fn stems(shelf: CvShelf) -> Vec<String> {
        match shelf {
            CvShelf::Read(docs) => docs.into_iter().map(|doc| doc.stem).collect(),
            CvShelf::Unread => Vec::new(),
        }
    }

    #[test]
    fn masters_come_first_then_applications_newest_first() {
        let shelf = cv_shelf(vec![
            files("main_acme_dev", Some(3), Some(4)),
            files("main_example_b", Some(1), Some(1)),
            files("main_tessel_lead", Some(9), Some(2)),
            files("main_example", Some(1), Some(1)),
            files("main_birch_ops", Some(4), None),
            files("empty", None, None),
        ]);
        assert_eq!(stems(shelf), ["main_example", "main_example_b", "main_tessel_lead", "main_acme_dev", "main_birch_ops"]);
    }

    #[test]
    fn equal_times_break_by_stem() {
        let shelf = cv_shelf(vec![files("main_b", Some(5), Some(5)), files("main_a", Some(5), Some(5))]);
        assert_eq!(stems(shelf), ["main_a", "main_b"]);
    }

    #[test]
    fn a_document_carries_its_label_shelf_and_freshness() {
        let doc = doc("main_example_engineer", Some(1), Some(2));
        assert_eq!((doc.label.as_str(), doc.shelf, doc.freshness), ("Master CV, engineer", Shelf::Master, Freshness::Current));
    }

    #[test]
    fn the_rail_word_counts_what_is_out_of_date_first() {
        let read = |docs: Vec<CvFiles>| rail_word(&cv_shelf(docs));
        let stale = vec![files("main_a", Some(5), Some(2)), files("main_b", Some(1), None), files("main_c", Some(1), Some(2))];
        assert_eq!(read(stale), RailWord::OutOfDate(2));
        assert_eq!(RailWord::OutOfDate(2).words(), "2 out of date");
        let fresh = vec![files("main_a", Some(1), Some(2)), files("main_b", None, Some(2))];
        assert_eq!(read(fresh), RailWord::Documents(2));
        assert_eq!(RailWord::Documents(2).words(), "2 documents");
        assert_eq!(read(vec![files("main_a", Some(1), Some(2))]), RailWord::Documents(1));
        assert_eq!(RailWord::Documents(1).words(), "1 document");
        assert_eq!(read(Vec::new()), RailWord::NoneYet);
        assert_eq!(RailWord::NoneYet.words(), "None yet");
        assert_eq!(rail_word(&CvShelf::Unread), RailWord::Hidden);
        assert_eq!(RailWord::Hidden.words(), "Hidden");
        assert_eq!(CvShelf::default(), CvShelf::Read(Vec::new()));
    }

    #[test]
    fn a_build_line_needs_an_out_of_date_document_and_a_plain_stem() {
        assert_eq!(doc("main_tessel_lead", Some(5), Some(2)).build_line().as_deref(), Some("/build-cv main_tessel_lead"));
        assert_eq!(doc("main_tessel_lead", Some(5), None).build_line().as_deref(), Some("/build-cv main_tessel_lead"));
        assert_eq!(doc("main_tessel_lead", Some(1), Some(2)).build_line(), None);
        assert_eq!(doc("a b", Some(5), Some(2)).build_line(), None);
    }

    #[test]
    fn only_a_pdf_gives_a_document_to_render() {
        assert_eq!(doc("main_tessel_lead", Some(1), None).pdf_document(), None);
        let both = doc("main_tessel_lead", Some(1), Some(2)).pdf_document().unwrap();
        assert_eq!((both.label.as_str(), both.modified), ("Tessel lead", 2 * 86_400));
        assert_eq!(both.path, PathBuf::from("cv/main_tessel_lead.pdf"));
    }

    #[test]
    fn the_shown_name_prefers_the_pdf() {
        assert_eq!(doc("main_a", Some(1), Some(2)).shown_name(), "main_a.pdf");
        assert_eq!(doc("main_a", Some(1), None).shown_name(), "main_a.tex");
    }

    #[test]
    fn the_shown_name_drops_hidden_characters() {
        let mut doc = doc("main_acme", Some(1), Some(2));
        doc.pdf.as_mut().unwrap().name = "main_acme\u{202E}fdp.pdf".to_owned();
        assert_eq!(doc.shown_name(), "main_acmefdp.pdf");
    }

    #[test]
    fn a_stem_with_a_hidden_character_sorts_by_the_shelf_its_label_reads() {
        let shelf = cv_shelf(vec![files("main_acme", Some(9), Some(9)), files("main\u{200b}_example_lead", Some(1), Some(1))]);
        let CvShelf::Read(docs) = shelf else { unreachable!() };
        assert_eq!((docs[0].shelf, docs[0].label.as_str()), (Shelf::Master, "Master CV, lead"));
        assert_eq!(docs[1].shelf, Shelf::Application);
    }

    #[test]
    fn meta_words_read_in_three_shapes() {
        let now = at(14);
        let built = doc("main_a", Some(9), Some(11));
        let all = |count| Some(PageCount { shown: count, total: count });
        assert_eq!(meta_words(&built, all(2), now), "2 pages, 84 KB, built 3 days ago");
        assert_eq!(meta_words(&built, all(1), now), "1 page, 84 KB, built 3 days ago");
        assert_eq!(meta_words(&built, None, now), "84 KB, built 3 days ago");
        assert_eq!(meta_words(&doc("main_a", Some(11), None), all(2), now), "written 3 days ago");
    }

    /// A render that stopped short never passes for the whole PDF.
    #[test]
    fn meta_words_say_when_only_the_first_pages_show() {
        let built = doc("main_a", Some(9), Some(11));
        let cut = Some(PageCount { shown: 6, total: 8 });
        assert_eq!(meta_words(&built, cut, at(14)), "8 pages, the first 6 shown, 84 KB, built 3 days ago");
    }

    fn line(stem: &str, tex: Option<u32>, pdf: Option<u32>, failed: bool) -> StateLine {
        state_line(&doc(stem, tex, pdf), failed, at(14))
    }

    #[test]
    fn current_says_it_was_built_after_the_tex() {
        let line = line("main_a", Some(1), Some(2), false);
        assert_eq!(line.tag, CvTag::Current);
        assert_eq!(line.sentence, "Built after its .tex last changed.");
        assert_eq!(line.action, None);
    }

    #[test]
    fn a_pdf_with_no_tex_says_nothing_can_build_it() {
        let line = line("main_a", None, Some(2), false);
        assert_eq!(line.tag, CvTag::Current);
        assert_eq!(line.sentence, "No .tex sits beside this PDF, so there is nothing to build it from again.");
        assert_eq!(line.action, None);
    }

    #[test]
    fn a_newer_tex_offers_a_rebuild() {
        let line = line("main_a", Some(11), Some(2), false);
        assert_eq!(line.tag, CvTag::SourceNewer);
        assert_eq!(line.sentence, "Its .tex changed 3 days ago, after this PDF was built. Run /build-cv in the launcher to build it again.");
        assert_eq!(line.action, Some(("Rebuild the PDF…", "/build-cv main_a".to_owned())));
    }

    #[test]
    fn a_tex_with_no_pdf_offers_a_build() {
        let line = line("main_a", Some(11), None, false);
        assert_eq!(line.tag, CvTag::NotBuilt);
        assert_eq!(line.sentence, "Its .tex has no PDF beside it yet. Run /build-cv in the launcher to build one.");
        assert_eq!(line.action, Some(("Build the PDF…", "/build-cv main_a".to_owned())));
    }

    #[test]
    fn a_render_failure_says_the_pages_could_not_be_drawn() {
        for (tex, pdf) in [(Some(1), Some(2)), (None, Some(2))] {
            let line = line("main_a", tex, pdf, true);
            assert_eq!(line.tag, CvTag::RenderFailed);
            assert_eq!(line.sentence, "The file is there, but Jobdesk could not draw its pages.");
            assert_eq!(line.action, None);
        }
    }

    #[test]
    fn a_stale_pdf_keeps_its_rebuild_when_the_render_failed() {
        let line = line("main_a", Some(11), Some(2), true);
        assert_eq!(line.tag, CvTag::SourceNewer);
        assert_eq!(line.action, Some(("Rebuild the PDF…", "/build-cv main_a".to_owned())));
    }

    #[test]
    fn a_stem_that_cannot_be_typed_offers_no_build() {
        assert_eq!(line("a b", Some(11), None, false).action, None);
    }
}
