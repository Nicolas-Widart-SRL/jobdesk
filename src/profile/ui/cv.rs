use std::path::PathBuf;

use chrono::NaiveDateTime;
use egui::{Align, ColorImage, Frame, Label, Layout, Margin, Response, ScrollArea, TextWrapMode, TextureFilter, TextureHandle, TextureOptions, Ui, vec2};

use crate::profile::application::{CvDocument, CvShelf, CvTag, PageCount, PageImage, PageState, PageStore, Shelf, meta_words, state_line};
use crate::profile::ui::rail::{CV_DOCUMENTS, row_words, rows};
use crate::profile::ui::screen::{BUSY, ProfileRequest};
use crate::shared_kernel::visible_text::visible_chars;
use crate::ui_kit::tokens::{BUTTON_GAP, DEAL_PADDING, EMPTY_WIDTH, ITEM_GAP, ROW_ITEM_GAP, RUN_SIDE, SCREEN_GAP};
use crate::ui_kit::{
    Ink, PageSet, Tone, Type, blank_page, desk_label, divider, empty_note, enabled_when, faint_mono, fitted_pages, headed_paper, prompt_box_tone, rail_title,
    rich, row_tag, ruled_code_box, small_button, small_command_action_hinted, split_paper, spoken_pick_row, tag,
};

/// What the head says under CV documents while no document shows.
const LEAD: &str = "The PDFs built from your profile, as they last built.";
const EMPTY_TITLE: &str = "No CV documents in your folder yet";
const EMPTY: &str =
    "This page shows each PDF in cv/, and each .tex there not built yet. /apply writes one for every application, and /build-cv builds its PDF.";
const HIDDEN_TITLE: &str = "Your CV documents are hidden, not gone";
const HIDDEN: &str =
    "Jobdesk could not read the folder that holds them, so it shows none. Nothing in it was changed. Jobdesk reads it again as soon as the folder changes.";
/// What the foot says at its right.
const SHOWS: &str = "Jobdesk shows these PDFs. /build-cv builds them.";
/// The wording Settings › General uses for its own button.
const SHOW_FILE: &str = if cfg!(target_os = "macos") { "Show in Finder" } else { "Show in Explorer" };
const RENDERING: &str = "Rendering the pages…";
const NO_PDF: &str = "No PDF yet. It shows here once /build-cv has built it.";
const NOT_RENDERED: &str = "This PDF could not be rendered.";
const UNCHANGED: &str = "The file itself is unchanged, and Open PDF still opens it in your PDF viewer. What the renderer said:";
/// How many grey pages show while a PDF renders: a CV keeps to two.
const LOADING_PAGES: usize = 2;

/// A rendered PDF: its path and modification time, so a PDF that `/build-cv` rewrites renders again.
type Key = (PathBuf, u64);

/// What stays between frames: the document picked by its stem, and the picked document's pages as textures.
#[derive(Default)]
pub(super) struct CvScreen {
    picked: Option<String>,
    /// Only the pages on show: another document's are released when it is picked.
    textures: Option<(Key, Vec<TextureHandle>)>,
}

/// What the picked document's pages area shows.
enum Shown {
    /// A `.tex` with no PDF.
    Blank,
    Loading,
    /// How many pages the PDF holds, which can be more than rendered.
    Ready(usize),
    /// The renderer's reason, as plain text.
    Failed(String),
}

/// The tone each tag takes: what a build would fix in `interview`, a refused render in `closed`.
fn tone(tag: CvTag) -> Tone {
    match tag {
        CvTag::Current => Tone::Plain,
        CvTag::SourceNewer | CvTag::NotBuilt => Tone::Interview,
        CvTag::RenderFailed => Tone::Closed,
    }
}

/// What the action's hover says: the line it opens, and that nothing runs yet.
fn build_hint(line: &str) -> String {
    format!("Opens the launcher with {line}. Nothing runs until you start it there.")
}

fn key(doc: &CvDocument) -> Option<Key> {
    doc.pdf.as_ref().map(|pdf| (pdf.path.clone(), pdf.modified))
}

/// The pages a render gave, as textures that keep their detail when drawn smaller.
fn textures_for(ui: &Ui, images: &[PageImage]) -> Vec<TextureHandle> {
    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let color = ColorImage::from_rgba_unmultiplied([image.width, image.height], &image.rgba);
            ui.ctx().load_texture(format!("cv_page_{index}"), color, options)
        })
        .collect()
}

impl CvScreen {
    /// Asks `pages` for the document's pages, which starts their render the first time, and gives what the pages area
    /// shows with how many pages rendered out of how many the PDF holds. Rendered pages become textures.
    /// Without a store nothing renders, so the pages stay loading.
    fn pages(&mut self, ui: &Ui, doc: &CvDocument, pages: Option<&mut PageStore>) -> (Shown, Option<PageCount>) {
        let (Some(pdf), Some(key)) = (doc.pdf_document(), key(doc)) else { return (Shown::Blank, None) };
        let Some(store) = pages else { return (Shown::Loading, None) };
        match store.pages(&pdf) {
            PageState::Loading => (Shown::Loading, None),
            PageState::Ready(rendered) => {
                if self.textures.as_ref().is_none_or(|(shown, _)| *shown != key) {
                    self.textures = Some((key, textures_for(ui, &rendered.pages)));
                }
                let count = PageCount { shown: rendered.pages.len(), total: rendered.total.max(rendered.pages.len()) };
                (Shown::Ready(count.total), Some(count))
            }
            PageState::Failed(reason) => (Shown::Failed(visible_chars(&reason).collect()), None),
        }
    }
}

/// Whether the store already found it could not render the document's PDF. Only a PDF opened before can say so: the
/// list never starts a render.
fn render_failed(pages: Option<&PageStore>, doc: &CvDocument) -> bool {
    doc.pdf_document().zip(pages).is_some_and(|(pdf, store)| matches!(store.known(&pdf), Some(PageState::Failed(_))))
}

/// The CV documents on paper: the list at the left, and the picked one beside it, which falls back to the first when
/// the stem picked names none of them. Only the picked document renders, as page images and nothing else. A build only
/// opens the launcher: nothing here starts a run. An empty or unread `cv/` says so instead.
/// Returns the file the head asked to open or show, by the name `cv/` listed it under.
pub(super) fn cv_paper(
    ui: &mut Ui,
    shelf: &CvShelf,
    screen: &mut CvScreen,
    mut pages: Option<&mut PageStore>,
    busy: bool,
    now: NaiveDateTime,
) -> Option<ProfileRequest> {
    let (documents, build_cv) = match shelf {
        CvShelf::Unread => return lone(ui, HIDDEN_TITLE, HIDDEN),
        CvShelf::Read { documents, build_cv } => (documents, *build_cv),
    };
    let Some(first) = documents.first() else { return lone(ui, EMPTY_TITLE, EMPTY) };
    let shown = screen.picked.as_deref().and_then(|stem| documents.iter().find(|doc| doc.stem == stem)).unwrap_or(first);
    let (pages_shown, count) = screen.pages(ui, shown, pages.as_deref_mut());
    if !matches!(pages_shown, Shown::Ready(_)) {
        screen.textures = None;
    }
    let meta = Meta { pages: count, rendering: matches!(pages_shown, Shown::Loading), failed: matches!(pages_shown, Shown::Failed(_)), build_cv };
    let (mut chosen, mut request) = (None, None);
    let (kept, store) = (&*screen, pages.as_deref());
    split_paper(
        ui,
        RUN_SIDE,
        |ui| chosen = list(ui, documents, store, &shown.stem, build_cv, now),
        |ui| {
            request = head(ui, shown, meta, busy, now);
            let textures = kept.textures.as_ref().map_or(&[][..], |(_, textures)| textures.as_slice());
            body(ui, &pages_shown, textures, &shown.label);
        },
        foot,
    );
    if let Some(stem) = chosen {
        screen.picked = Some(stem);
    }
    request
}

/// Masters, then each application's CV, as pickable rows. Returns the stem picked this frame.
fn list(ui: &mut Ui, documents: &[CvDocument], pages: Option<&PageStore>, shown: &str, build_cv: bool, now: NaiveDateTime) -> Option<String> {
    let mut chosen = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    for (shelf, title) in [(Shelf::Master, "Masters"), (Shelf::Application, "For applications")] {
        let mut group = documents.iter().filter(|doc| doc.shelf == shelf).peekable();
        if group.peek().is_none() {
            continue;
        }
        rail_title(ui, title);
        rows(ui, |ui| {
            for doc in group {
                if doc_row(ui, doc, render_failed(pages, doc), doc.stem == shown, build_cv, now).clicked() {
                    chosen = Some(doc.stem.clone());
                }
            }
        });
    }
    chosen
}

/// One document: its label, cut to the row with the whole on hover and on focus, and its tag when it is not Current.
/// A PDF the store could not render says so.
fn doc_row(ui: &mut Ui, doc: &CvDocument, failed: bool, on: bool, build_cv: bool, now: NaiveDateTime) -> Response {
    let mark = Some(state_line(doc, failed, build_cv, now).tag).filter(|&tag| tag != CvTag::Current);
    let spoken = match mark {
        Some(tag) => format!("{}, {}", doc.label, tag.word()),
        None => doc.label.clone(),
    };
    let response = spoken_pick_row(ui, on, &spoken, Some(&doc.label), |ui| {
        row_words(ui, |ui| {
            if let Some(tag) = mark {
                row_tag(ui, tag.word(), tone(tag));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(Label::new(rich(ui, &doc.label, Type::BODY, Ink::Text)).truncate().show_tooltip_when_elided(false));
            });
        });
    });
    // The focus tip and a hover tip share one popup, so a focused row shows only the focus tip.
    if response.has_focus() { response } else { response.on_hover_text(&doc.label) }
}

/// What the head knows of the render: how many pages it gave, whether it is still going, and whether the renderer
/// refused the PDF. Also whether the folder has `/build-cv` to offer a build with.
#[derive(Clone, Copy)]
struct Meta {
    pages: Option<PageCount>,
    rendering: bool,
    failed: bool,
    build_cv: bool,
}

/// The picked document's head: its label, its file name and what it weighs, with Open PDF and Show in Finder at the
/// right, then its state line, over a hairline. Returns the file asked to open or show.
fn head(ui: &mut Ui, doc: &CvDocument, meta: Meta, busy: bool, now: NaiveDateTime) -> Option<ProfileRequest> {
    let (x, y) = DEAL_PADDING;
    let mut request = None;
    Frame::new().inner_margin(Margin::symmetric(x, y)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = BUTTON_GAP;
            // Show in Finder selects the PDF, or the `.tex` when there is none.
            if let Some(file) = doc.pdf.as_ref().or(doc.tex.as_ref())
                && small_button(ui, SHOW_FILE).clicked()
            {
                request = Some(ProfileRequest::Reveal(file.name.clone()));
            }
            if let Some(pdf) = &doc.pdf
                && small_button(ui, "Open PDF").clicked()
            {
                request = Some(ProfileRequest::OpenPdf(pdf.name.clone()));
            }
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| ui.add(Label::new(rich(ui, &doc.label, Type::DEAL, Ink::Text)).wrap()));
        });
        // The meta line runs under the buttons too. Where the room ends, its next part moves whole to a line of its own.
        ui.horizontal_wrapped(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            ui.spacing_mut().item_spacing.x = ROW_ITEM_GAP;
            desk_label(ui, doc.shown_name(), Type::MONO, Ink::Text);
            desk_label(ui, meta_words(doc, meta.pages, now), Type::BODY, Ink::Muted);
            if meta.rendering {
                desk_label(ui, RENDERING, Type::BODY, Ink::Muted);
            }
        });
        ui.add_space(ITEM_GAP);
        state(ui, doc, meta, busy, now);
    });
    divider(ui);
    request
}

/// The tag, what it means, and at the right the build that would help, which opens the launcher and waits while a run
/// works. A folder with no `/build-cv` shows no build at all.
fn state(ui: &mut Ui, doc: &CvDocument, meta: Meta, busy: bool, now: NaiveDateTime) {
    let line = state_line(doc, meta.failed, meta.build_cv, now);
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), height), Layout::right_to_left(Align::Center), |ui| {
        if let Some((label, command)) = &line.action {
            enabled_when(ui, !busy, BUSY, |ui| small_command_action_hinted(ui, label, command, &build_hint(command)));
            ui.add_space(ITEM_GAP * 2.0);
        }
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            tag(ui, line.tag.word(), tone(line.tag));
            ui.add(Label::new(rich(ui, &line.sentence, Type::NOTE, Ink::Muted)).wrap());
        });
    });
}

/// The pages under the head, scrolling on their own: fitted to the room's height while they render and once they have,
/// an empty page before a PDF is built, or why the renderer refused it.
fn body(ui: &mut Ui, shown: &Shown, textures: &[TextureHandle], label: &str) {
    let (x, gap) = (f32::from(DEAL_PADDING.0), SCREEN_GAP / 2.0);
    let room = (ui.available_size() - vec2(2.0 * x, 2.0 * gap)).max(vec2(0.0, 0.0));
    ScrollArea::vertical().id_salt("cv_pages").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(gap);
        ui.horizontal_top(|ui| {
            ui.add_space(x);
            ui.allocate_ui_with_layout(room, Layout::top_down(Align::Min), |ui| match shown {
                Shown::Blank => blank_page(ui, NO_PDF),
                Shown::Loading => fitted_pages(ui, PageSet::Loading(LOADING_PAGES)),
                Shown::Ready(total) => fitted_pages(ui, PageSet::Ready { pages: textures, of: *total, alt: label }),
                Shown::Failed(reason) => refused(ui, reason),
            });
        });
        ui.add_space(gap);
    });
}

/// The renderer refused the PDF: what that means for the file, then what it said, as plain text.
fn refused(ui: &mut Ui, reason: &str) {
    ui.set_max_width(EMPTY_WIDTH);
    prompt_box_tone(ui, Tone::Closed, |ui| {
        ui.spacing_mut().item_spacing.y = ITEM_GAP + 2.0;
        ui.label(rich(ui, NOT_RENDERED, Type::STRONG, Tone::Closed));
        ui.add(Label::new(rich(ui, UNCHANGED, Type::BODY, Ink::Text)).wrap());
        ruled_code_box(ui, |ui| ui.add(Label::new(rich(ui, reason, Type::MONO, Ink::Text)).wrap()));
    });
}

/// CV documents with no document to show: the head, then why, over the foot.
fn lone(ui: &mut Ui, title: &str, body: &str) -> Option<ProfileRequest> {
    headed_paper(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing.y = ITEM_GAP - 2.0;
            ui.label(rich(ui, CV_DOCUMENTS, Type::DEAL, Ink::Text));
            ui.add(Label::new(rich(ui, LEAD, Type::BODY, Ink::Muted)).wrap());
        },
        |ui| {
            ui.add_space(SCREEN_GAP / 2.0);
            empty_note(ui, title, body);
        },
        foot,
    );
    None
}

/// Where the documents live, and that Jobdesk only shows them.
fn foot(ui: &mut Ui) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        desk_label(ui, SHOWS, Type::NOTE, Ink::Muted);
        ui.add_space(ITEM_GAP * 2.0);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            desk_label(ui, "Kept in ", Type::NOTE, Ink::Muted);
            faint_mono(ui, "cv/");
            desk_label(ui, ", beside the ", Type::NOTE, Ink::Muted);
            faint_mono(ui, ".tex");
            desk_label(ui, " each PDF is built from", Type::NOTE, Ink::Muted);
        });
    });
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use chrono::NaiveDate;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::dossier::application::PageRasterizer;
    use crate::profile::application::{CvFiles, FileStamp, RenderedPages, cv_shelf};

    fn setup(ctx: &egui::Context) {
        crate::ui_kit::install_fonts(ctx);
        crate::ui_kit::install_theme(ctx);
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 31).unwrap().and_hms_opt(9, 30, 0).unwrap()
    }

    fn stamp(name: &str, hours: i64) -> FileStamp {
        let at = now() - chrono::Duration::hours(hours);
        FileStamp { name: name.to_owned(), path: Path::new("/fictional/cv").join(name), modified: at.and_utc().timestamp().unsigned_abs(), at, bytes: 84_000 }
    }

    /// Master CV with its `.tex` and PDF, and an application's CV with only its `.tex`, in a folder that has
    /// `/build-cv` or not.
    fn shelf(build_cv: bool) -> CvShelf {
        cv_shelf(
            vec![
                CvFiles { stem: "main_example".into(), tex: Some(stamp("main_example.tex", 72)), pdf: Some(stamp("main_example.pdf", 48)) },
                CvFiles { stem: "main_halyard_labs_tech_lead".into(), tex: Some(stamp("main_halyard_labs_tech_lead.tex", 1)), pdf: None },
            ],
            build_cv,
        )
    }

    /// What the paper asked last, kept beside the screen it asked from.
    type State = (CvScreen, Option<ProfileRequest>, Option<PageStore>);

    fn paper(pages: Option<PageStore>) -> Harness<'static, State> {
        paper_on(shelf(true), pages)
    }

    fn paper_on(shelf: CvShelf, pages: Option<PageStore>) -> Harness<'static, State> {
        let mut harness = Harness::builder().with_size([1100.0, 760.0]).with_context_setup(setup).build_ui_state(
            move |ui, (screen, asked, pages): &mut State| {
                if let Some(pages) = pages.as_mut() {
                    pages.poll();
                }
                if let Some(request) = cv_paper(ui, &shelf, screen, pages.as_mut(), false, now()) {
                    *asked = Some(request);
                }
            },
            (CvScreen::default(), None, pages),
        );
        harness.run_steps(3);
        harness
    }

    fn press(harness: &mut Harness<'static, State>, label: &str) -> Option<ProfileRequest> {
        harness.state_mut().1 = None;
        harness.get_by_role_and_label(egui::accesskit::Role::Button, label).click();
        harness.run_steps(3);
        harness.state_mut().1.take()
    }

    /// The head asks the shell for the file by the name `cv/` listed, and opens nothing itself.
    #[test]
    fn open_pdf_and_show_in_finder_ask_for_the_listed_file() {
        let mut harness = paper(None);
        assert_eq!(press(&mut harness, "Open PDF"), Some(ProfileRequest::OpenPdf("main_example.pdf".into())));
        assert_eq!(press(&mut harness, SHOW_FILE), Some(ProfileRequest::Reveal("main_example.pdf".into())));
    }

    /// A `.tex` with no PDF has nothing to open, and Show in Finder selects the `.tex`.
    #[test]
    fn a_tex_with_no_pdf_shows_only_the_tex() {
        let mut harness = paper(None);
        harness.get_by_label("Halyard labs tech lead, Not built yet").click();
        harness.run_steps(3);
        assert!(harness.query_by_label("Open PDF").is_none());
        assert!(harness.query_by_label(NO_PDF).is_some(), "an empty page says why");
        assert_eq!(press(&mut harness, SHOW_FILE), Some(ProfileRequest::Reveal("main_halyard_labs_tech_lead.tex".into())));
    }

    /// A folder with no `/build-cv` keeps the tag and says why no build shows.
    #[test]
    fn a_folder_with_no_build_cv_offers_no_build() {
        let pick = |build_cv| {
            let mut harness = paper_on(shelf(build_cv), None);
            harness.get_by_label("Halyard labs tech lead, Not built yet").click();
            harness.run_steps(3);
            harness
        };
        let harness = pick(true);
        assert!(harness.query_by_label("Build the PDF…").is_some());
        let harness = pick(false);
        assert!(harness.query_by_label("Build the PDF…").is_none(), "no build, not even a disabled one");
        let sentence =
            "Its .tex has no PDF beside it yet. This folder has no /build-cv command, so update your ai-job-search checkout to a version that has it.";
        assert!(harness.query_by_label(sentence).is_some());
        assert!(harness.query_by_label(SHOW_FILE).is_some());
    }

    /// Without a store the pages never render, so they say they are rendering.
    #[test]
    fn pages_with_no_store_stay_rendering() {
        let harness = paper(None);
        assert!(harness.query_by_label(RENDERING).is_some());
    }

    struct Refusing;

    impl PageRasterizer for Refusing {
        fn render(&self, _: &Path) -> Result<RenderedPages, String> {
            Err("Bad\u{202e}font\u{1b}[31m program\u{200b} in page 1".into())
        }
    }

    /// A refused render says so in the box and on the row, and its reason reaches the screen only as visible text.
    #[test]
    fn a_refused_render_shows_its_reason_as_plain_text() {
        let mut harness = paper(Some(PageStore::new(Arc::new(Refusing), Arc::new(|| {}))));
        let start = Instant::now();
        while harness.query_by_label(NOT_RENDERED).is_none() && start.elapsed() < Duration::from_secs(10) {
            harness.run_steps(1);
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.run_steps(3);
        assert!(harness.query_by_label(NOT_RENDERED).is_some());
        assert!(harness.query_by_label("Badfont program in page 1").is_some(), "control, bidi and zero-width characters are gone");
        assert!(harness.query_by_label_contains("\u{202e}").is_none());
        assert!(harness.query_by_label("Master CV, Render failed").is_some(), "the row says so too");
        assert!(harness.query_by_label("The file is there, but Jobdesk could not draw its pages.").is_some());
        assert!(harness.query_by_label("Open PDF").is_some(), "the viewer may still open it");
    }

    #[test]
    fn each_tag_takes_its_tone() {
        assert_eq!(tone(CvTag::Current), Tone::Plain);
        assert_eq!(tone(CvTag::SourceNewer), Tone::Interview);
        assert_eq!(tone(CvTag::NotBuilt), Tone::Interview);
        assert_eq!(tone(CvTag::RenderFailed), Tone::Closed);
    }

    #[test]
    fn the_hint_names_the_line_and_says_nothing_runs() {
        assert_eq!(
            build_hint("/build-cv main_tessel_platform_lead"),
            "Opens the launcher with /build-cv main_tessel_platform_lead. Nothing runs until you start it there."
        );
    }
}
