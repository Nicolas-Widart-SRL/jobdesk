use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Galley, Label, OutputCommand, Pos2, Rect, Response, RichText, TextStyle, TextWrapMode, Ui, WidgetText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::{WebUrl, markdown};
use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{CLICKABLE, backed, on_desk};
use crate::ui_kit::tokens::{DOC_MEASURE, Type};

/// Text in a named type style. The way a screen asks for anything beyond body text.
pub fn rich(ui: &Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> RichText {
    RichText::new(text).font(style.font()).color(ink.into().color(&Palette::of(ui)))
}

/// The largest type in a laid-out text.
fn largest(job: &LayoutJob) -> f32 {
    job.sections.iter().map(|section| section.format.font_id.size).fold(0.0, f32::max)
}

/// Straight on the desk, text at `Type::TAKE` size or smaller keeps the grid out of its glyphs. Larger type reads over the lines.
fn needs_backing(ui: &Ui, size: f32) -> bool {
    size <= Type::TAKE.size && on_desk(ui)
}

/// Every label in the kit is added here, with `size` read from the label's own text by the callers below.
fn show_label(ui: &mut Ui, label: Label, size: f32) -> Response {
    if needs_backing(ui, size) { backed(ui, |ui| ui.add(label)) } else { ui.add(label) }
}

/// Text inside `add` can be selected and copied. Labels elsewhere take no click, so a click on a row's text picks the row.
/// It never sits inside something clickable, because a selectable label would take that click.
pub fn copyable<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    debug_assert!(!ui.stack().iter().any(|node| node.tags().contains(CLICKABLE)), "copyable text sits inside something clickable");
    ui.scope(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        add(ui)
    })
    .inner
}

/// Text in `style` and `ink`, as a label that wraps as the `Ui` says.
pub(in crate::ui_kit) fn add_text(ui: &mut Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> Response {
    let label = Label::new(rich(ui, text, style, ink));
    show_label(ui, label, style.size)
}

/// The same, cut to one line with the whole text on hover.
pub(in crate::ui_kit) fn add_cut_text(ui: &mut Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> Response {
    let label = Label::new(rich(ui, text, style, ink)).truncate();
    show_label(ui, label, style.size)
}

/// A galley laid out before it is placed, such as a head's subtitle.
pub(in crate::ui_kit) fn add_galley(ui: &mut Ui, galley: Arc<Galley>) -> Response {
    let size = largest(&galley.job);
    show_label(ui, Label::new(galley), size)
}

/// Text in several runs of type, wrapped or cut by `mode`.
pub(in crate::ui_kit) fn add_job(ui: &mut Ui, job: LayoutJob, mode: TextWrapMode) -> Response {
    let size = largest(&job);
    show_label(ui, Label::new(job).wrap_mode(mode), size)
}

/// Text in a named type style, as a label. On the desk, small text keeps the grid out of it.
pub fn desk_label(ui: &mut Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> Response {
    add_text(ui, text, style, ink)
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    add_text(ui, text, Type::BODY, Ink::Muted)
}

pub fn faint_mono(ui: &mut Ui, text: impl Into<String>) -> Response {
    add_text(ui, text, Type::MONO, Ink::Faint)
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) -> Response {
    add_text(ui, text, Type::STRONG, Ink::Text)
}

/// Something went wrong. Closed is the one status that reads as failure.
pub fn alert(ui: &mut Ui, text: impl Into<String>) -> Response {
    copyable(ui, |ui| add_text(ui, text, Type::BODY, crate::ui_kit::Tone::Closed))
}

pub fn table_head(ui: &mut Ui, text: impl Into<String>) -> Response {
    add_text(ui, text, Type::TABLE_HEAD, Ink::Muted)
}

pub fn score(ui: &mut Ui, text: impl Into<String>) -> Response {
    add_text(ui, text, Type::SCORE, Ink::Text)
}

/// A large count, read at a glance.
pub fn figure(ui: &mut Ui, text: impl Into<String>, ink: impl Into<Ink>) -> Response {
    add_text(ui, text, Type::FIGURE, ink)
}

/// A strong line over a muted one, each cut to one line, the pair centred in the height it has. Such as a role in a table row.
pub fn title_over(ui: &mut Ui, title: &str, detail: &str) -> Response {
    let p = Palette::of(ui);
    let width = ui.available_width();
    let title = line(ui, title, Type::STRONG, p.text, width);
    let detail = line(ui, detail, Type::BODY, p.muted, width);
    ui.vertical(|ui| {
        ui.add_space(((ui.available_height() - title.size().y - detail.size().y) / 2.0).max(0.0));
        add_galley(ui, title);
        add_galley(ui, detail);
    })
    .response
}

/// One wrapped sentence that leads with a bold phrase, such as a verdict.
pub fn lead_line(ui: &mut Ui, strong: &str, rest: &str) -> Response {
    let p = Palette::of(ui);
    let mut job = LayoutJob::default();
    job.append(strong, 0.0, TextFormat::simple(Type::STRONG.font(), p.text));
    job.append(&format!(" {rest}"), 0.0, TextFormat::simple(Type::BODY.font(), p.text));
    add_job(ui, job, TextWrapMode::Wrap)
}

/// Markdown from Claude or a prep pack, rendered as ARCHITECTURE.md#trust says. Every Markdown render goes through here.
/// Images never load: each shows as its alt text. A link opens only on click, and only for `http` or `https`.
pub fn markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    // The image pass runs once per distinct text, and is kept while the text stays on screen.
    let text = ui.ctx().memory_mut(|memory| memory.caches.cache::<ImageFreeCache>().get(text).clone());
    let before = ui.ctx().output(|output| output.commands.len());
    copyable(ui, |ui| CommonMarkViewer::new().show(ui, cache, &text));
    ui.ctx().output_mut(|output| keep_web_links(&mut output.commands, before));
}

/// A message as a document: one readable column that never grows past the space it has, with headings held to the
/// section size. A top-level table leaves the column for the whole width, and scrolls only past that.
/// Every part goes through `markdown`, so ARCHITECTURE.md#trust holds.
pub fn document(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    let parts = ui.ctx().memory_mut(|memory| memory.caches.cache::<PartsCache>().get(text).clone());
    ui.scope(|ui| {
        let styles = &mut ui.style_mut().text_styles;
        styles.insert(TextStyle::Heading, Type::SECTION.font());
        styles.insert(TextStyle::Body, Type::TAKE.font());
        for (index, part) in parts.iter().enumerate() {
            // The blank line one viewer would leave between two blocks, less the gap every widget already keeps.
            if part.spaced {
                let line = ui.fonts_mut(|fonts| fonts.row_height(&Type::TAKE.font()));
                ui.add_space(line - ui.spacing().item_spacing.y);
            }
            // Each part numbers its tables from one, so each gets its own id.
            ui.push_id(("document part", index), |ui| {
                let width = ui.available_width();
                ui.set_max_width(if part.table { width } else { width.min(DOC_MEASURE) });
                markdown(ui, cache, &part.text);
            });
        }
    });
}

/// A run of a document shown at one width: a top-level table, or the prose between tables.
struct Part {
    text: String,
    table: bool,
    /// Whether a blank line comes before it, as it would inside one viewer.
    spaced: bool,
}

#[derive(Default)]
struct Parts;

impl egui::cache::ComputerMut<&str, Arc<[Part]>> for Parts {
    fn compute(&mut self, text: &str) -> Arc<[Part]> {
        parts(text).into()
    }
}

type PartsCache = egui::cache::FrameCache<Arc<[Part]>, Parts>;

/// `text` cut around its top-level tables. A text without one stays whole.
/// Each part of a cut text starts with every link reference the whole text defines, so a link resolves in any part.
fn parts(text: &str) -> Vec<Part> {
    let parser = Parser::new_ext(text, markdown::options());
    let mut definitions: Vec<_> = parser.reference_definitions().iter().map(|(label, link)| (link.span.start, reference(label, &link.dest, link.title.as_deref()))).collect();
    let (mut tables, mut headings, mut depth) = (Vec::new(), Vec::new(), 0);
    for (event, range) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::Table(_)) if depth == 0 => {
                tables.push(range);
                depth += 1;
            }
            Event::Start(Tag::Heading { .. }) if depth == 0 => {
                headings.push(range.start);
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
    }
    if tables.is_empty() {
        return vec![Part { text: text.to_owned(), table: false, spaced: false }];
    }
    definitions.sort();
    let head: String = definitions.into_iter().map(|(_, line)| line + "\n").chain(["\n".to_owned()]).collect();
    // The viewer sets a heading off by itself, even at the top.
    let part = |range: std::ops::Range<usize>, table| {
        let spaced = !headings.iter().any(|&at| at >= range.start && text[range.start..at].trim().is_empty());
        Part { text: format!("{head}{}", &text[range]), table, spaced }
    };
    let mut out = Vec::new();
    let mut from = 0;
    for table in tables {
        if !text[from..table.start].trim().is_empty() {
            out.push(part(from..table.start, false));
        }
        from = table.end;
        out.push(part(table, true));
    }
    if !text[from..].trim().is_empty() {
        out.push(part(from..text.len(), false));
    }
    out[0].spaced = false;
    out
}

/// One link reference definition, written so it reads back the same. The label keeps its own escapes, because labels
/// match as written.
fn reference(label: &str, destination: &str, title: Option<&str>) -> String {
    match title {
        Some(title) => format!("[{label}]: <{}> \"{}\"", escaped(destination), escaped(title)),
        None => format!("[{label}]: <{}>", escaped(destination)),
    }
}

#[derive(Default)]
struct ImageFree;

impl egui::cache::ComputerMut<&str, Arc<str>> for ImageFree {
    fn compute(&mut self, text: &str) -> Arc<str> {
        without_images(text).into()
    }
}

type ImageFreeCache = egui::cache::FrameCache<Arc<str>, ImageFree>;

/// Drops any url opened after `from` that is not `http` or `https`.
fn keep_web_links(commands: &mut Vec<OutputCommand>, from: usize) {
    let mut index = 0;
    commands.retain(|command| {
        let safe = index < from || !matches!(command, OutputCommand::OpenUrl(open) if WebUrl::parse(&open.url).is_none());
        index += 1;
        safe
    });
}

/// Every ASCII punctuation mark escaped, so the text cannot form Markdown again.
fn escaped(text: &str) -> String {
    text.chars().fold(String::new(), |mut out, c| {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
        out
    })
}

/// One pass: each outermost image becomes its escaped alt text.
fn replace_images(text: &str) -> Option<String> {
    let mut out = String::new();
    let (mut copied, mut depth, mut alt, mut found) = (0, 0, String::new(), false);
    for (event, range) in Parser::new_ext(text, markdown::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { .. }) => {
                if depth == 0 {
                    out.push_str(&text[copied..range.start]);
                    copied = range.end;
                    alt.clear();
                    found = true;
                }
                depth += 1;
            }
            Event::End(TagEnd::Image) => {
                depth -= 1;
                if depth == 0 {
                    out.push_str(&escaped(&alt));
                }
            }
            Event::Text(part) | Event::Code(part) if depth > 0 => alt.push_str(&part),
            Event::SoftBreak | Event::HardBreak if depth > 0 => alt.push(' '),
            _ => {}
        }
    }
    out.push_str(&text[copied..]);
    found.then_some(out)
}

/// Markdown with every image swapped for its alt text, so no loader is ever asked for one.
/// The parser finds the images, so `![` inside code stays as written.
fn without_images(text: &str) -> String {
    if !text.contains("![") {
        return text.to_owned();
    }
    let mut text = text.to_owned();
    // Escaped alt text cannot form a new image, so the second pass only confirms the first.
    for _ in 0..2 {
        match replace_images(&text) {
            Some(next) => text = next,
            None => return text,
        }
    }
    // Unreachable in practice: show the whole text as plain characters rather than risk an image.
    escaped(&text)
}

/// The height `text` takes wrapped at `width`, for rows sized before they are drawn.
pub fn wrapped_height(ui: &Ui, text: &str, style: Type, width: f32) -> f32 {
    WidgetText::from(RichText::new(text).font(style.font())).into_galley(ui, Some(TextWrapMode::Wrap), width, style.font()).size().y
}

/// A single line cut to `width`, for widgets that paint their own text.
pub(in crate::ui_kit) fn line(ui: &Ui, text: &str, style: Type, color: Color32, width: f32) -> Arc<Galley> {
    let text = RichText::new(text).font(style.font()).color(color);
    WidgetText::from(text).into_galley(ui, Some(TextWrapMode::Truncate), width, style.font())
}

/// A widget that paints its own cut text names the whole text on hover. `text` shows as plain text, never as Markdown.
pub(in crate::ui_kit) fn cut_hover(response: Response, galley: &Galley, text: &str) -> Response {
    if galley.elided { response.on_hover_text(text) } else { response }
}

/// Every galley a widget paints itself goes down here. `under` is the fill the widget painted beneath the text, if any:
/// with none, small text straight on the desk gets a patch of the desk's base, so no grid line runs through it.
pub(in crate::ui_kit) fn paint_text(ui: &Ui, at: Pos2, galley: Arc<Galley>, color: Color32, under: Color32) {
    if under == Color32::TRANSPARENT && needs_backing(ui, largest(&galley.job)) {
        ui.painter().rect_filled(Rect::from_min_size(at, galley.size()), 0.0, Palette::of(ui).sheet);
    }
    ui.painter().galley(at, galley, color);
}

/// `galley` centred in `rect`, over `under` as `paint_text` takes it.
pub(in crate::ui_kit) fn paint_centred(ui: &Ui, rect: Rect, galley: Arc<Galley>, color: Color32, under: Color32) {
    let at = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size()).min;
    paint_text(ui, at, galley, color, under);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;

    type Render = fn(&mut Ui, &mut CommonMarkCache, &'static str);

    /// Clicks the link named `label` and returns every url a frame asked to open.
    fn opened_through(render: Render, source: &'static str, label: &str) -> Vec<String> {
        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&opened);
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([400.0, 200.0]).build_ui(move |ui| {
            render(ui, &mut cache, source);
            let urls: Vec<String> = ui.ctx().output(|output| {
                output.commands.iter().filter_map(|command| if let OutputCommand::OpenUrl(open) = command { Some(open.url.clone()) } else { None }).collect()
            });
            seen.borrow_mut().extend(urls);
        });
        harness.run();
        harness.get_by_label(label).click();
        harness.run();
        opened.take()
    }

    #[test]
    fn copyable_turns_selection_on_only_inside() {
        let (outside, inside) = (Rc::new(RefCell::new(None)), Rc::new(RefCell::new(None)));
        let (seen_outside, seen_inside) = (Rc::clone(&outside), Rc::clone(&inside));
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
            *seen_outside.borrow_mut() = Some(ui.add(Label::new("Plain label")).sense);
            *seen_inside.borrow_mut() = Some(copyable(ui, |ui| ui.add(Label::new("Copyable label"))).sense);
        });
        harness.run();
        let senses_click = |sense: Option<egui::Sense>| sense.is_some_and(|sense| sense.senses_click());
        assert!(!senses_click(*outside.borrow()));
        assert!(senses_click(*inside.borrow()));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "copyable text sits inside something clickable")]
    fn copyable_inside_a_pick_row_panics() {
        let mut harness = Harness::builder().with_size([400.0, 200.0]).build_ui(|ui| {
            crate::ui_kit::pick_row(ui, false, |ui| {
                copyable(ui, |ui| ui.label("Copyable label"));
            });
        });
        harness.run();
    }

    #[test]
    fn markdown_links_open_only_for_the_web() {
        assert_eq!(opened_through(markdown, "[terminal](file:///Applications/Utilities/Terminal.app)", "terminal"), Vec::<String>::new());
        assert_eq!(opened_through(markdown, "[script](javascript:alert(1))", "script"), Vec::<String>::new());
        assert_eq!(opened_through(markdown, "[posting](https://tessel.example/jobs/1)", "posting"), vec!["https://tessel.example/jobs/1".to_owned()]);
    }

    #[test]
    fn markdown_images_become_their_alt_text() {
        assert_eq!(without_images("Hi ![a *logo*](https://x.example/l.png) there"), "Hi a logo there");
        assert_eq!(without_images("![ref][r]\n\n[r]: file:///etc/x.png"), "ref\n\n[r]: file:///etc/x.png");
        assert_eq!(without_images("[![badge](https://x.example/b.svg)](https://x.example)"), "[badge](https://x.example)");
        assert_eq!(without_images("![\\!\\[x\\](y)](z)"), "\\!\\[x\\]\\(y\\)");
        assert_eq!(without_images("no images, just ! and [x]"), "no images, just ! and [x]");
        assert_eq!(without_images("![two\nlines](u)"), "two lines");
    }

    #[test]
    fn image_syntax_in_code_stays_as_written() {
        let code = "Use `![a](b)` here.\n\n```\n![logo](file:///x.png)\n```\n";
        assert_eq!(without_images(code), code);
        assert_eq!(without_images("`![a](b)` and ![c](d)"), "`![a](b)` and c");
    }

    fn unmarked(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
        document(ui, cache, text);
    }

    #[test]
    fn document_keeps_only_web_links() {
        let source = "[terminal](file:///Applications/Utilities/Terminal.app) and [posting](https://tessel.example/jobs/1)\n\n[script](javascript:alert(1)) and [role](https://tessel.example/jobs/2)";
        assert_eq!(opened_through(unmarked, source, "terminal"), Vec::<String>::new());
        assert_eq!(opened_through(unmarked, source, "script"), Vec::<String>::new());
        assert_eq!(opened_through(unmarked, source, "posting"), vec!["https://tessel.example/jobs/1".to_owned()]);
        assert_eq!(opened_through(unmarked, source, "role"), vec!["https://tessel.example/jobs/2".to_owned()]);
    }

    #[test]
    fn document_shows_images_as_alt_text() {
        let source: &'static str = "First ![company logo](https://x.example/l.png) paragraph.\n\nThen ![your badge](file:///etc/badge.png) now.";
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([600.0, 200.0]).build_ui(move |ui| unmarked(ui, &mut cache, source));
        harness.run();
        assert!(harness.query_by_label_contains("company logo").is_some());
        assert!(harness.query_by_label_contains("your badge").is_some());
        assert!(harness.query_by_label_contains("x.example").is_none());
        assert!(harness.query_by_label_contains("badge.png").is_none());
    }

    #[test]
    fn document_caps_the_top_heading() {
        let line = Rc::new(RefCell::new(0.0));
        let seen = Rc::clone(&line);
        let mut cache = CommonMarkCache::default();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([800.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            *seen.borrow_mut() = wrapped_height(ui, "Fit", Type::SECTION, f32::INFINITY);
            document(ui, &mut cache, "# Fit\n\nBody text of the evaluation.");
        });
        harness.run();
        let heading = harness.get_by_label("Fit").rect().height();
        assert!(heading <= *line.borrow() + 2.0, "heading is {heading} px, one section line is {}", line.borrow());
    }

    #[test]
    fn document_never_grows_past_its_column() {
        let long = "word ".repeat(300);
        let mut cache = CommonMarkCache::default();
        let (narrow, wide) = (Rc::new(RefCell::new(0.0)), Rc::new(RefCell::new(0.0)));
        let (seen_narrow, seen_wide) = (Rc::clone(&narrow), Rc::clone(&wide));
        let mut harness = Harness::builder().with_size([1200.0, 800.0]).build_ui(move |ui| {
            ui.scope(|ui| {
                ui.set_max_width(400.0);
                *seen_narrow.borrow_mut() = ui.scope(|ui| document(ui, &mut cache, &long)).response.rect.width();
            });
            let mut cache = CommonMarkCache::default();
            *seen_wide.borrow_mut() = ui.scope(|ui| document(ui, &mut cache, &long)).response.rect.width();
        });
        harness.run();
        assert!(*narrow.borrow() <= 400.0, "in a 400 px parent the document is {} px", narrow.borrow());
        assert!(*wide.borrow() <= DOC_MEASURE, "in a wide parent the document is {} px", wide.borrow());
    }

    const WIDE_TABLE: &str = "| Role | Score | Notes |\n|---|---|---|\n| Platform engineer at Tessel | 4.2 | Event sourcing and Kafka match the posting, and the team runs the same payments stack the last role used. |\n| Staff engineer at Kestrel | 3.1 | A strong product, but the role leans on mobile work and card schemes, two gaps the CV cannot close yet. |";

    #[test]
    fn a_document_table_takes_the_whole_width() {
        let source = format!("{}\n\n{WIDE_TABLE}\n\nAfter the table.", "word ".repeat(300));
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([1400.0, 900.0]).build_ui(move |ui| document(ui, &mut cache, &source));
        harness.run();
        let paragraph = harness.get_by_label_contains("word word").rect().width();
        let table = harness.get_by_label_contains("two gaps the CV").rect().right() - harness.get_by_label("Role").rect().left();
        assert!(paragraph <= DOC_MEASURE, "the paragraph is {paragraph} px");
        assert!(table > DOC_MEASURE, "the table is {table} px");
    }

    #[test]
    fn a_reference_link_resolves_across_a_table() {
        let source: &'static str = "See [the posting][tessel] first.\n\n| Area | Match |\n|---|---|\n| Skills | [strong][tessel] |\n\nThen [the role][kestrel].\n\n[tessel]: https://tessel.example/jobs/1\n[kestrel]: https://kestrel.example/jobs/2 \"Kestrel\"";
        assert_eq!(opened_through(unmarked, source, "the posting"), vec!["https://tessel.example/jobs/1".to_owned()]);
        assert_eq!(opened_through(unmarked, source, "strong"), vec!["https://tessel.example/jobs/1".to_owned()]);
        assert_eq!(opened_through(unmarked, source, "the role"), vec!["https://kestrel.example/jobs/2".to_owned()]);
    }

    #[test]
    fn a_document_cut_around_tables_keeps_its_parts() {
        let cut = parts("1. one\n2. two\n\n| A |\n|---|\n| x |\n\n3. three\n\n[r]: <https://x.example/a b> \"t\\\"q\"");
        let tables: Vec<bool> = cut.iter().map(|part| part.table).collect();
        assert_eq!(tables, vec![false, true, false]);
        assert!(cut[2].text.contains("3. three"));
        for part in cut.iter() {
            let parser = Parser::new_ext(&part.text, markdown::options());
            let link = parser.reference_definitions().get("r").expect("each part defines the reference");
            assert_eq!((&*link.dest, link.title.as_deref()), ("https://x.example/a b", Some("t\"q")));
        }
        assert_eq!(parts("no table [r]\n\n[r]: https://x.example").len(), 1);
    }

    /// Where each label sits when `source` shows as a document, or as one viewer held to the measure.
    fn label_tops(source: &'static str, cut: bool, labels: &[&str]) -> Vec<f32> {
        let mut cache = CommonMarkCache::default();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([800.0, 600.0]).with_context_setup(setup).build_ui(move |ui| {
            if cut {
                document(ui, &mut cache, source);
            } else {
                ui.scope(|ui| {
                    ui.set_max_width(DOC_MEASURE);
                    let styles = &mut ui.style_mut().text_styles;
                    styles.insert(TextStyle::Heading, Type::SECTION.font());
                    styles.insert(TextStyle::Body, Type::TAKE.font());
                    markdown(ui, &mut cache, source);
                });
            }
        });
        harness.run();
        labels.iter().map(|label| harness.get_by_label(label).rect().top()).collect()
    }

    #[test]
    fn a_document_cut_around_a_table_keeps_its_blank_lines() {
        let source = "Gates\n\n| Gate | Result |\n|---|---|\n| Salary | Check |\n\nStrengths\n\n- One\n\nShall I?";
        let labels = ["Gate", "Strengths", "Shall I?"];
        let (cut, whole) = (label_tops(source, true, &labels), label_tops(source, false, &labels));
        for ((label, cut), whole) in labels.iter().zip(cut).zip(whole) {
            assert!((cut - whole).abs() <= 2.0, "{label} sits at {cut}, one viewer puts it at {whole}");
        }
    }
}
