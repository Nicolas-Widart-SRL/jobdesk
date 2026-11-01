use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, Galley, Label, OutputCommand, Rect, Response, RichText, TextStyle, TextWrapMode, Ui, WidgetText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::{WebUrl, markdown};
use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{backed, on_desk};
use crate::ui_kit::tokens::{DOC_MEASURE, Type};

/// Text in a named type style. The way a screen asks for anything beyond body text.
pub fn rich(ui: &Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> RichText {
    RichText::new(text).font(style.font()).color(ink.into().color(&Palette::of(ui)))
}

/// Adds `label`, set in `style`. Straight on the desk, text at `Type::TAKE` size or smaller backs itself with the desk's base,
/// so no grid line runs through its glyphs. Larger type reads over the lines.
pub(in crate::ui_kit) fn add_text(ui: &mut Ui, label: Label, style: Type) -> Response {
    if style.size <= Type::TAKE.size && on_desk(ui) { backed(ui, |ui| ui.add(label)) } else { ui.add(label) }
}

/// Text in a named type style, as a label. On the desk, small text keeps the grid out of it.
pub fn label(ui: &mut Ui, text: impl Into<String>, style: Type, ink: impl Into<Ink>) -> Response {
    let text = rich(ui, text, style, ink);
    add_text(ui, Label::new(text), style)
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> Response {
    label(ui, text, Type::BODY, Ink::Muted)
}

pub fn faint_mono(ui: &mut Ui, text: impl Into<String>) -> Response {
    label(ui, text, Type::MONO, Ink::Faint)
}

pub fn strong(ui: &mut Ui, text: impl Into<String>) -> Response {
    label(ui, text, Type::STRONG, Ink::Text)
}

/// Something went wrong. Closed is the one status that reads as failure.
pub fn alert(ui: &mut Ui, text: impl Into<String>) -> Response {
    label(ui, text, Type::BODY, crate::ui_kit::Tone::Closed)
}

pub fn table_head(ui: &mut Ui, text: impl Into<String>) -> Response {
    label(ui, text, Type::TABLE_HEAD, Ink::Muted)
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

/// One wrapped sentence that leads with a bold phrase, such as a verdict.
pub fn lead_line(ui: &mut Ui, strong: &str, rest: &str) -> Response {
    let p = Palette::of(ui);
    let mut job = LayoutJob::default();
    job.append(strong, 0.0, TextFormat::simple(Type::STRONG.font(), p.text));
    job.append(&format!(" {rest}"), 0.0, TextFormat::simple(Type::BODY.font(), p.text));
    ui.add(Label::new(job).wrap())
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

/// A message as a document: one readable column that never grows past the space it has, with headings held to the
/// section size. It goes through `markdown`, so ARCHITECTURE.md#trust holds.
pub fn document(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(DOC_MEASURE));
        let styles = &mut ui.style_mut().text_styles;
        styles.insert(TextStyle::Heading, Type::SECTION.font());
        styles.insert(TextStyle::Body, Type::TAKE.font());
        markdown(ui, cache, text);
    });
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
}
