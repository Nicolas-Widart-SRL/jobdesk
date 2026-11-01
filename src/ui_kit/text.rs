use std::ops::Range;
use std::sync::Arc;

use egui::{Align2, Color32, Frame, Galley, Margin, OutputCommand, Rect, Response, RichText, TextStyle, TextWrapMode, Ui, WidgetText, vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::{WebUrl, markdown};
use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::tokens::{DOC_MEASURE, GUTTER, QUOTE_RULE, ROW_GAP, Type};

/// Text in a named type style. The way a screen asks for anything beyond body text.
pub fn rich(ui: &Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> RichText {
    RichText::new(text).font(style.font()).color(ink.into().color(&Palette::of(ui)))
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::BODY, Ink::Muted);
    ui.label(text)
}

pub fn faint_mono(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::MONO, Ink::Faint);
    ui.label(text)
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::STRONG, Ink::Text);
    ui.label(text)
}

/// Something went wrong. Closed is the one status that reads as failure.
pub fn alert(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::BODY, crate::ui_kit::Tone::Closed);
    ui.label(text)
}

pub fn table_head(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::TABLE_HEAD, Ink::Muted);
    ui.label(text)
}

pub fn score(ui: &mut Ui, text: impl Into<String>) -> Response {
    let text = rich(ui, text, Type::SCORE, Ink::Text);
    ui.label(text)
}

/// A large count, read at a glance.
pub fn figure(ui: &mut Ui, text: impl Into<String>, ink: impl Into<Ink>) -> Response {
    let text = rich(ui, text, Type::FIGURE, ink);
    ui.label(text)
}

/// A strong line over a muted one, each cut to one line, the pair centred in the height it has. Such as a role in a table row.
pub fn title_over(ui: &mut Ui, title: &str, detail: &str) -> Response {
    let p = Palette::of(ui);
    let width = ui.available_width();
    let title = line(ui, title, Type::STRONG, p.text, width);
    let detail = line(ui, detail, Type::BODY, p.muted, width);
    ui.vertical(|ui| {
        ui.add_space(((ui.available_height() - title.size().y - detail.size().y) / 2.0).max(0.0));
        ui.label(title);
        ui.label(detail);
    })
    .response
}

/// Markdown from Claude or a prep pack, rendered as ARCHITECTURE.md#trust says. Every Markdown render goes through here.
/// Images never load: each shows as its alt text. A link opens only on click, and only for `http` or `https`.
pub fn markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    // The image pass runs once per distinct text, and is kept while the text stays on screen.
    let text = ui.ctx().memory_mut(|memory| memory.caches.cache::<ImageFreeCache>().get(text).clone());
    let before = ui.ctx().output(|output| output.commands.len());
    CommonMarkViewer::new().show(ui, cache, &text);
    ui.ctx().output_mut(|output| keep_web_links(&mut output.commands, before));
}

/// A message as a document: one readable column, headings held to the section size, and the paragraph around `marked`
/// set off by a turn-coloured rule. A range that is not inside one top-level paragraph marks nothing, so no list item,
/// table row or code fence is ever cut. Every part goes through `markdown`, so ARCHITECTURE.md#trust holds for each.
pub fn document(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, marked: Option<Range<usize>>) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(DOC_MEASURE));
        let styles = &mut ui.style_mut().text_styles;
        styles.insert(TextStyle::Heading, Type::SECTION.font());
        styles.insert(TextStyle::Body, Type::TAKE.font());
        let outline = marked.as_ref().map(|_| Outline::of(text));
        let Some((outline, span)) = outline.and_then(|outline| marked.and_then(|range| outline.paragraph_around(text, range)).map(|span| (outline, span))) else {
            ui.push_id(("doc_part", 0), |ui| markdown(ui, cache, text));
            return;
        };
        let before = outline.part(text, 0..span.start);
        let marked = outline.part(text, span.clone());
        let after = outline.part(text, span.end..text.len());
        if let Some(before) = &before {
            ui.push_id(("doc_part", 0), |ui| markdown(ui, cache, before));
            ui.add_space(ROW_GAP);
        }
        if let Some(marked) = &marked {
            let rule = Palette::of(ui).turn;
            let shown = Frame::new().inner_margin(Margin { left: GUTTER + QUOTE_RULE as i8, ..Default::default() }).show(ui, |ui| {
                ui.push_id(("doc_part", 1), |ui| markdown(ui, cache, marked));
            });
            let edge = shown.response.rect;
            ui.painter().rect_filled(Rect::from_min_size(edge.left_top(), vec2(QUOTE_RULE, edge.height())), 0.0, rule);
        }
        if let Some(after) = &after {
            ui.add_space(ROW_GAP);
            ui.push_id(("doc_part", 2), |ui| markdown(ui, cache, after));
        }
    });
}

/// Where a message's top-level paragraphs sit, and what a part cut from it must carry along to read the same.
#[derive(Default)]
struct Outline {
    paragraphs: Vec<Range<usize>>,
    /// Link reference definitions.
    links: Vec<Range<usize>>,
    /// Top-level footnote definitions, by label.
    notes: Vec<(String, Range<usize>)>,
    /// Footnote references, by label.
    mentions: Vec<(String, Range<usize>)>,
}

impl Outline {
    fn of(text: &str) -> Self {
        let mut outline = Self::default();
        let mut parser = Parser::new_ext(text, markdown::options()).into_offset_iter();
        let mut depth = 0;
        for (event, range) in &mut parser {
            match event {
                Event::Start(tag) => {
                    if depth == 0 {
                        match tag {
                            Tag::Paragraph => outline.paragraphs.push(range),
                            Tag::FootnoteDefinition(label) => outline.notes.push((label.to_string(), range)),
                            _ => {}
                        }
                    }
                    depth += 1;
                }
                Event::End(_) => depth -= 1,
                Event::FootnoteReference(label) => outline.mentions.push((label.to_string(), range)),
                _ => {}
            }
        }
        outline.links = parser.reference_definitions().iter().map(|(_, definition)| definition.span.clone()).collect();
        outline.links.sort_by_key(|span| span.start);
        outline
    }

    /// The whole top-level paragraph `range` lies inside, or `None` when the range is empty, reversed, out of bounds,
    /// off a character boundary, or not inside a single top-level paragraph.
    fn paragraph_around(&self, text: &str, range: Range<usize>) -> Option<Range<usize>> {
        if range.is_empty() || !text.is_char_boundary(range.start) || !text.is_char_boundary(range.end) {
            return None;
        }
        self.paragraphs.iter().find(|paragraph| paragraph.start <= range.start && range.end <= paragraph.end).cloned()
    }

    /// The text of `span` ready to render alone: its footnote definitions cut out, then the reference definitions
    /// from elsewhere and the footnote definitions it mentions put after it. `None` when nothing but space is left.
    fn part(&self, text: &str, span: Range<usize>) -> Option<String> {
        let inside = |range: &Range<usize>| span.start <= range.start && range.end <= span.end;
        let (mut body, mut at) = (String::new(), span.start);
        for (_, note) in self.notes.iter().filter(|(_, note)| inside(note)) {
            body.push_str(&text[at..note.start]);
            at = note.end;
        }
        body.push_str(&text[at..span.end]);
        if body.trim().is_empty() {
            return None;
        }
        for link in self.links.iter().filter(|link| !inside(link)) {
            body.push_str("\n\n");
            body.push_str(text[link.clone()].trim_end());
        }
        for (label, note) in &self.notes {
            if self.mentions.iter().any(|(mention, at)| mention == label && inside(at)) {
                body.push_str("\n\n");
                body.push_str(text[note.clone()].trim_end());
            }
        }
        Some(body)
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

pub(in crate::ui_kit) fn paint_centred(ui: &Ui, rect: Rect, galley: Arc<Galley>, color: Color32) {
    let at = Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size()).min;
    ui.painter().galley(at, galley, color);
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
        document(ui, cache, text, None);
    }

    /// The document marking the last paragraph.
    fn marking_the_last_paragraph(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
        let start = text.rfind("\n\n").map_or(0, |at| at + 2);
        document(ui, cache, text, Some(start..text.len()));
    }

    #[test]
    fn document_keeps_only_web_links() {
        let source = "[terminal](file:///Applications/Utilities/Terminal.app) and [posting](https://tessel.example/jobs/1)\n\n[script](javascript:alert(1)) and [role](https://tessel.example/jobs/2)";
        for render in [unmarked as Render, marking_the_last_paragraph] {
            assert_eq!(opened_through(render, source, "terminal"), Vec::<String>::new());
            assert_eq!(opened_through(render, source, "script"), Vec::<String>::new());
            assert_eq!(opened_through(render, source, "posting"), vec!["https://tessel.example/jobs/1".to_owned()]);
            assert_eq!(opened_through(render, source, "role"), vec!["https://tessel.example/jobs/2".to_owned()]);
        }
    }

    #[test]
    fn document_shows_images_as_alt_text() {
        let source: &'static str = "First ![company logo](https://x.example/l.png) paragraph.\n\nAsk ![your badge](file:///etc/badge.png) now?";
        for render in [unmarked as Render, marking_the_last_paragraph] {
            let mut cache = CommonMarkCache::default();
            let mut harness = Harness::builder().with_size([600.0, 200.0]).build_ui(move |ui| render(ui, &mut cache, source));
            harness.run();
            assert!(harness.query_by_label_contains("company logo").is_some());
            assert!(harness.query_by_label_contains("your badge").is_some());
            assert!(harness.query_by_label_contains("x.example").is_none());
            assert!(harness.query_by_label_contains("badge.png").is_none());
        }
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
            document(ui, &mut cache, "# Fit\n\nBody text of the evaluation.", None);
        });
        harness.run();
        let heading = harness.get_by_label("Fit").rect().height();
        assert!(heading <= *line.borrow() + 2.0, "heading is {heading} px, one section line is {}", line.borrow());
    }

    /// The range of the line that starts at `start`.
    fn line_from(text: &str, start: &str) -> Option<Range<usize>> {
        let from = text.find(start)?;
        Some(from..text[from..].find('\n').map_or(text.len(), |end| from + end))
    }

    fn marking_the_question(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
        document(ui, cache, text, line_from(text, "Shall"));
    }

    fn shown(render: Render, source: &'static str, size: [f32; 2]) -> Harness<'static> {
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size(size).build_ui(move |ui| render(ui, &mut cache, source));
        harness.run();
        harness
    }

    #[test]
    fn a_bad_range_renders_everything() {
        let text = "Caf\u{e9} first.\n\nSecond paragraph.";
        let outline = Outline::of(text);
        // Inside the two-byte `\u{e9}`, past the end, reversed, empty, and across two paragraphs.
        for range in [4..9, 0..text.len() + 1, Range { start: 12, end: 3 }, 5..5, 2..20] {
            assert_eq!(outline.paragraph_around(text, range.clone()), None, "{range:?}");
        }
        assert_eq!(outline.paragraph_around(text, 16..20), Some(14..text.len()));
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([600.0, 200.0]).build_ui(move |ui| document(ui, &mut cache, text, Some(4..9)));
        harness.run();
        harness.get_by_label_contains("first.");
        harness.get_by_label_contains("Second paragraph.");
    }

    #[test]
    fn only_a_top_level_paragraph_is_marked() {
        let text = "Intro.\n\n- first item\n- second item\n\n```\nlet fence = 1;\n```\n\n> quoted words\n\nShall I apply?";
        let outline = Outline::of(text);
        for inside in ["second item", "let fence", "quoted words"] {
            let at = text.find(inside).unwrap();
            assert_eq!(outline.paragraph_around(text, at..at + inside.len()), None, "{inside}");
        }
        // A range inside a paragraph marks the whole paragraph.
        let at = text.find("apply").unwrap();
        assert_eq!(outline.paragraph_around(text, at..at + 5), Some(text.find("Shall").unwrap()..text.len()));
        // A mark in a list item renders the whole text, once.
        let in_the_list = |ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str| document(ui, cache, text, line_from(text, "second item"));
        let harness = shown(in_the_list, text, [600.0, 400.0]);
        harness.get_by_label_contains("first item");
        assert_eq!(harness.query_all_by_label_contains("Shall I apply?").count(), 1);
    }

    #[test]
    fn a_split_keeps_the_links_the_message_defines() {
        let source = "See [the posting][p].\n\nShall I apply? [ask][p]\n\nAfter that, [the role][p].\n\n[p]: https://x.example";
        for label in ["the posting", "ask", "the role"] {
            assert_eq!(opened_through(marking_the_question, source, label), vec!["https://x.example".to_owned()], "{label}");
        }
        let hostile = "See [the posting][p].\n\nShall I apply? [ask][p]\n\nAfter that, [the role][p].\n\n[p]: file:///etc/passwd";
        for label in ["the posting", "ask", "the role"] {
            assert_eq!(opened_through(marking_the_question, hostile, label), Vec::<String>::new(), "{label}");
        }
    }

    #[test]
    fn a_split_keeps_the_footnotes_it_mentions() {
        let source = "Intro.\n\nShall I apply?[^1]\n\nAfter.\n\n[^1]: Because of the rate.";
        let harness = shown(marking_the_question, source, [600.0, 300.0]);
        assert_eq!(harness.query_all_by_label_contains("Because of the rate.").count(), 1);
        harness.get_by_label_contains("After.");
    }

    /// Whether egui painted an id clash warning in the last frame.
    fn painted_an_id_clash(harness: &Harness<'_>) -> bool {
        fn clash(shape: &egui::Shape) -> bool {
            match shape {
                egui::Shape::Text(text) => text.galley.text().contains('\u{1f525}'),
                egui::Shape::Vec(shapes) => shapes.iter().any(clash),
                _ => false,
            }
        }
        harness.output().shapes.iter().any(|clipped| clash(&clipped.shape))
    }

    #[test]
    fn the_parts_of_a_document_share_no_ids() {
        let source = "| Gate | Result |\n|---|---|\n| Contract | Pass |\n\nShall I apply?\n\n| Gate | Result |\n|---|---|\n| Contract | Pass |";
        let harness = shown(marking_the_question, source, [600.0, 400.0]);
        assert_eq!(harness.query_all_by_label_contains("Contract").count(), 2);
        assert!(!painted_an_id_clash(&harness));
    }

    #[test]
    fn a_marked_question_starts_where_an_unmarked_one_does() {
        let source = "Shall I apply?";
        let top = |render: Render| shown(render, source, [400.0, 200.0]).get_by_label_contains("Shall").rect().min.y;
        assert_eq!(top(marking_the_question), top(unmarked));
    }

    #[test]
    fn content_after_the_marked_paragraph_still_renders() {
        let source = "Intro.\n\nShall I apply?\n\nAfter, ![chart](file:///x.png) and [the role](https://x.example/2) and [script](javascript:alert(1)).";
        let harness = shown(marking_the_question, source, [600.0, 300.0]);
        harness.get_by_label_contains("chart");
        assert_eq!(opened_through(marking_the_question, source, "the role"), vec!["https://x.example/2".to_owned()]);
        assert_eq!(opened_through(marking_the_question, source, "script"), Vec::<String>::new());
    }
}
