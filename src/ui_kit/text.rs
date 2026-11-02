use std::ops::Range;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Galley, Label, OutputCommand, Pos2, Rect, Response, RichText, Shape, Stroke, TextStyle, TextWrapMode, Ui, WidgetText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer, RenderHtmlFn};
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

use crate::shared_kernel::markdown::only_comments;
use crate::shared_kernel::placeholders::Piece;
use crate::shared_kernel::visible_text::visible_chars;
use crate::shared_kernel::{WebUrl, markdown};
use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{CLICKABLE, backed, on_desk};
use crate::ui_kit::tokens::{DOC_MEASURE, HAIRLINE, HIT_OVERHANG, HIT_TOP, Type};

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

/// Text in `style` and `ink`, wrapped to the room it has, with each hit of a search for `query` highlighted.
/// The highlighter is the open tab's `mark` swipe without its rule: a hit means "here". An empty `query` marks nothing.
pub fn hit_mark(ui: &mut Ui, text: &str, style: Type, ink: impl Into<Ink>, query: &str) -> Response {
    let galley = wrapped(ui, text, style, ink.into().color(&Palette::of(ui)), ui.available_width());
    add_marked(ui, galley, text, query)
}

/// `galley`, laid out from `text`, as a label with each hit of `query` highlighted under its glyphs.
pub(in crate::ui_kit) fn add_marked(ui: &mut Ui, galley: Arc<Galley>, text: &str, query: &str) -> Response {
    // The swipes go under the text, so their place is kept before the label paints.
    let under = ui.painter().add(Shape::Noop);
    let response = add_galley(ui, galley.clone());
    let hits = hit_ranges(text, query);
    if !hits.is_empty() {
        let fill = Palette::of(ui).mark;
        ui.painter().set(under, Shape::Vec(hit_swipes(&galley, response.rect.min, &hits, fill)));
    }
    response
}

/// `galley` painted at `at` with each hit of `query` highlighted, as `add_marked` shows it but with no label of its own,
/// for a control that names itself, such as a checkbox whose words are part of it.
pub(in crate::ui_kit) fn paint_marked(ui: &Ui, at: Pos2, galley: Arc<Galley>, text: &str, query: &str) {
    let hits = hit_ranges(text, query);
    if !hits.is_empty() {
        let fill = Palette::of(ui).mark;
        ui.painter().extend(hit_swipes(&galley, at, &hits, fill));
    }
    paint_text(ui, at, galley, Color32::PLACEHOLDER, Color32::TRANSPARENT);
}

/// Where `query` shows in `text`, as char ranges, matched without regard to case. Hits never overlap.
/// A query of only spaces matches nothing.
pub(in crate::ui_kit) fn hit_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let query: Vec<char> = query.trim().chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    let mut hits = Vec::new();
    let mut at = 0;
    while !query.is_empty() && at + query.len() <= chars.len() {
        if query.iter().enumerate().all(|(k, &q)| same(chars[at + k], q)) {
            hits.push(at..at + query.len());
            at += query.len();
        } else {
            at += 1;
        }
    }
    hits
}

/// The swipe under each hit in `galley` laid at `at`, one per row a hit runs over, over the lower part of the line.
fn hit_swipes(galley: &Galley, at: Pos2, hits: &[Range<usize>], fill: Color32) -> Vec<Shape> {
    let mut shapes = Vec::new();
    let mut first = 0;
    for row in &galley.rows {
        let count = row.glyphs.len();
        for hit in hits {
            let (from, to) = (hit.start.max(first), hit.end.min(first + count));
            if from >= to {
                continue;
            }
            let rect = row.rect();
            let left = at.x + row.pos.x + row.glyphs[from - first].pos.x - HIT_OVERHANG;
            let right = at.x + row.pos.x + row.glyphs[to - 1 - first].max_x() + HIT_OVERHANG;
            let top = at.y + rect.top() + rect.height() * HIT_TOP;
            shapes.push(Shape::rect_filled(Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, at.y + rect.bottom())), 0.0, fill));
        }
        first += count + usize::from(row.ends_with_newline);
    }
    shapes
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
    render(ui, cache, text, None);
}

/// Profile text, rendered as `markdown` renders any text: placeholders show as written. A block of HTML that holds only
/// comments shows nothing, so a template's notes stay off the paper without the text being cut.
pub fn profile_markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    render(ui, cache, text, Some(&comments_hidden));
}

/// `markdown`, with `html` handed each block of HTML when it is given.
fn render(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, html: Option<&RenderHtmlFn>) {
    // The image pass runs once per distinct text, and is kept while the text stays on screen.
    let text = ui.ctx().memory_mut(|memory| memory.caches.cache::<ImageFreeCache>().get(text).clone());
    let before = ui.ctx().output(|output| output.commands.len());
    let viewer = CommonMarkViewer::new().render_html_fn(html);
    copyable(ui, |ui| viewer.show(ui, cache, &text));
    ui.ctx().output_mut(|output| keep_web_links(&mut output.commands, before));
}

/// A block of HTML on the Profile paper: nothing when it holds only comments, otherwise its text as written, as the
/// viewer shows HTML it is not handed off.
fn comments_hidden(ui: &mut Ui, html: &str) {
    if !only_comments(html) {
        ui.label(html.trim_end());
    }
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
/// A footnote definition renders where it stands, so it cannot be copied the same way: a text that would lose a
/// footnote marker to the cut stays whole.
fn parts(text: &str) -> Vec<Part> {
    let whole = || vec![Part { text: text.to_owned(), table: false, spaced: false }];
    let parser = Parser::new_ext(text, markdown::options());
    let mut definitions: Vec<_> =
        parser.reference_definitions().iter().map(|(label, link)| (link.span.start, reference(label, &link.dest, link.title.as_deref()))).collect();
    let (mut tables, mut headings, mut blocks, mut notes, mut depth) = (Vec::new(), Vec::new(), Vec::new(), 0, 0);
    for (event, range) in parser.into_offset_iter() {
        if depth == 0 && matches!(event, Event::Start(_) | Event::Rule) {
            blocks.push(range.start);
        }
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
            Event::FootnoteReference(_) => notes += 1,
            _ => {}
        }
    }
    if tables.is_empty() {
        return whole();
    }
    definitions.sort();
    let head: String = definitions.into_iter().map(|(_, line)| line + "\n").chain(["\n".to_owned()]).collect();
    // The viewer sets a heading off by itself, even at the top.
    let part = |range: std::ops::Range<usize>, table| {
        let spaced = !headings.iter().any(|&at| at >= range.start && text[range.start..at].trim().is_empty());
        Part { text: format!("{head}{}", &text[range]), table, spaced }
    };
    // Prose with no block in it, such as link references alone, draws nothing, so it takes no part and no blank line.
    let drawn = |range: &std::ops::Range<usize>| blocks.iter().any(|at| range.contains(at));
    let mut out = Vec::new();
    let mut from = 0;
    for table in tables {
        if drawn(&(from..table.start)) {
            out.push(part(from..table.start, false));
        }
        from = table.end;
        out.push(part(table, true));
    }
    if drawn(&(from..text.len())) {
        out.push(part(from..text.len(), false));
    }
    let kept: usize =
        out.iter().map(|part| Parser::new_ext(&part.text, markdown::options()).filter(|event| matches!(event, Event::FootnoteReference(_))).count()).sum();
    if kept != notes {
        return whole();
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

/// The look of a placeholder to fill, as a text format so it flows in a line: `interview` ink in the semibold face at
/// `size`, the size of the text around it, underlined in a hairline of the same ink.
pub fn blank_mark(ui: &Ui, size: f32) -> TextFormat {
    let ink = Palette::of(ui).interview;
    TextFormat { underline: Stroke::new(HAIRLINE, ink), ..TextFormat::simple(FontId::new(size, Type::STRONG.face.family()), ink) }
}

/// A line Jobdesk lays out itself, such as a role's title, from the pieces `placeholders::shown_pieces` gives: text in
/// `style` and `ink`, and each blank in `blank_mark`. A blank comes only from a token the count found, never from the
/// text. Hidden and reordering characters are removed first, as for any untrusted line.
pub(in crate::ui_kit) fn blank_line(ui: &Ui, pieces: &[Piece], style: Type, ink: impl Into<Ink>) -> LayoutJob {
    let plain = TextFormat::simple(style.font(), ink.into().color(&Palette::of(ui)));
    let mark = blank_mark(ui, style.size);
    let mut job = LayoutJob::default();
    for piece in pieces {
        match piece {
            Piece::Text(text) => job.append(&visible_chars(text).collect::<String>(), 0.0, plain.clone()),
            Piece::Blank(words) => job.append(&visible_chars(words).collect::<String>(), 0.0, mark.clone()),
        }
    }
    job
}

/// The height `text` takes wrapped at `width`, for rows sized before they are drawn.
pub fn wrapped_height(ui: &Ui, text: &str, style: Type, width: f32) -> f32 {
    WidgetText::from(RichText::new(text).font(style.font())).into_galley(ui, Some(TextWrapMode::Wrap), width, style.font()).size().y
}

/// Text wrapped to `width`, for widgets that lay out their own.
pub(in crate::ui_kit) fn wrapped(ui: &Ui, text: &str, style: Type, color: Color32, width: f32) -> Arc<Galley> {
    WidgetText::from(RichText::new(text).font(style.font()).color(color)).into_galley(ui, Some(TextWrapMode::Wrap), width, style.font())
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
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([400.0, 200.0]).with_context_setup(setup).build_ui(move |ui| {
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
    fn a_search_hits_every_place_regardless_of_case() {
        assert_eq!(hit_ranges("Spend at most, spend less", "spend"), vec![0..5, 15..20]);
        assert_eq!(hit_ranges("Reduce motion", " MOTION "), vec![7..13]);
        assert_eq!(hit_ranges("Été café", "é"), vec![0..1, 2..3, 7..8]);
        assert!(hit_ranges("Theme", "").is_empty());
        assert!(hit_ranges("Theme", "   ").is_empty());
        assert!(hit_ranges("Theme", "themes").is_empty());
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
        let area = Rc::new(RefCell::new(Rect::NOTHING));
        let seen = Rc::clone(&area);
        let mut harness = Harness::builder().with_size([1400.0, 900.0]).build_ui(move |ui| {
            *seen.borrow_mut() = ui.scope(|ui| document(ui, &mut cache, &source)).response.rect;
        });
        harness.run();
        let area = *area.borrow();
        let paragraph = harness.get_by_label_contains("word word").rect().width();
        // Labels in a table cell never wrap, so their own rects run past a capped table. The document's rect is what the
        // cap holds back, and the last cell has to sit inside it.
        let last = harness.get_by_label_contains("two gaps the CV").rect();
        assert!(paragraph <= DOC_MEASURE, "the paragraph is {paragraph} px");
        assert!(area.width() > DOC_MEASURE, "the document is {} px", area.width());
        assert!(last.right() <= area.right(), "the last cell ends at {}, the document at {}", last.right(), area.right());
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

    /// `source` as a document, or as one viewer held to the measure.
    fn shown(ui: &mut Ui, cache: &mut CommonMarkCache, source: &str, cut: bool) {
        if cut {
            document(ui, cache, source);
        } else {
            ui.scope(|ui| {
                ui.set_max_width(DOC_MEASURE);
                let styles = &mut ui.style_mut().text_styles;
                styles.insert(TextStyle::Heading, Type::SECTION.font());
                styles.insert(TextStyle::Body, Type::TAKE.font());
                markdown(ui, cache, source);
            });
        }
    }

    /// Where each label sits when `source` shows as a document, or as one viewer held to the measure.
    fn label_tops(source: &'static str, cut: bool, labels: &[&str]) -> Vec<f32> {
        let mut cache = CommonMarkCache::default();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([800.0, 600.0]).with_context_setup(setup).build_ui(move |ui| shown(ui, &mut cache, source, cut));
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

    #[test]
    fn a_document_ends_where_one_viewer_ends() {
        let source = "Gates\n\n| Gate | Result |\n|---|---|\n| Salary | Check |\n\n[tessel]: https://tessel.example/jobs/1\n";
        let ends = |cut: bool| {
            let bottom = Rc::new(RefCell::new(0.0));
            let seen = Rc::clone(&bottom);
            let mut cache = CommonMarkCache::default();
            let setup = |ctx: &egui::Context| {
                crate::ui_kit::install_fonts(ctx);
                crate::ui_kit::install_theme(ctx);
            };
            let mut harness = Harness::builder().with_size([800.0, 600.0]).with_context_setup(setup).build_ui(move |ui| {
                *seen.borrow_mut() = ui.scope(|ui| shown(ui, &mut cache, source, cut)).response.rect.bottom();
            });
            harness.run();
            bottom.take()
        };
        let (cut, whole) = (ends(true), ends(false));
        assert!((cut - whole).abs() <= 2.0, "the document ends at {cut}, one viewer ends at {whole}");
    }

    /// Every label's text, in order, when `source` renders as a document, or as one viewer.
    fn labels_shown(source: &'static str, cut: bool) -> Vec<String> {
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([800.0, 900.0]).build_ui(move |ui| shown(ui, &mut cache, source, cut));
        harness.run();
        harness.query_all_by_role(egui::accesskit::Role::Label).filter_map(|node| node.value()).collect()
    }

    #[test]
    fn a_footnote_resolves_across_a_table() {
        let table = "| Gate | Result |\n|---|---|\n| Salary | Check |";
        let cell = "| Gate | Result |\n|---|---|\n| Salary | Check[^1] |";
        let sources: [&'static str; 3] = [
            Box::leak(format!("Salary is not stated[^1].\n\n{table}\n\nAfter the table.\n\n[^1]: Ask on the first call.").into_boxed_str()),
            Box::leak(format!("Before the table.\n\n{cell}\n\nAfter the table.\n\n[^1]: Ask on the first call.").into_boxed_str()),
            Box::leak(format!("[^1]: Ask on the first call.\n\n{table}\n\nSalary is not stated[^1].").into_boxed_str()),
        ];
        for source in sources {
            let (cut, whole) = (labels_shown(source, true), labels_shown(source, false));
            assert_eq!(cut, whole, "{source}");
            assert!(whole.iter().any(|label| label == "1"), "{source} shows no footnote marker: {whole:?}");
            assert_eq!(whole.iter().filter(|label| *label == "Ask on the first call.").count(), 1, "{source}");
        }
        // A footnote kept inside one part leaves the table free to widen.
        assert_eq!(parts("Intro.\n\n| A |\n|---|\n| x |\n\nAfter[^1].\n\n[^1]: A note.").len(), 3);
    }

    fn profiled(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
        profile_markdown(ui, cache, text);
    }

    /// Every text `render` paints for `source`, joined in paint order, and how many of its shapes are in the blank's ink.
    fn painted_text(render: Render, source: &'static str) -> (String, usize) {
        let found = Rc::new(RefCell::new((String::new(), 0)));
        let seen = Rc::clone(&found);
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([800.0, 900.0]).with_context_setup(setup).build_ui(move |ui| {
            render(ui, &mut cache, source);
            let ink = Palette::of(ui).interview;
            let layer = ui.layer_id();
            *seen.borrow_mut() = ui.ctx().graphics(|layers| {
                let (mut text, mut blanks) = (String::new(), 0);
                for clipped in layers.get(layer).map(|list| list.all_entries().collect::<Vec<_>>()).unwrap_or_default() {
                    if let egui::Shape::Text(shape) = &clipped.shape {
                        text.push_str(&shape.galley.job.text);
                        blanks += usize::from(shape.galley.job.sections.iter().any(|section| section.format.color == ink));
                    }
                }
                (text, blanks)
            });
        });
        harness.run();
        found.take()
    }

    /// Every round's inputs: each token the page counts shows on the paper as written, and the body draws no blank.
    #[test]
    fn every_counted_token_shows_on_the_paper() {
        let sources: [&'static str; 41] = [
            "Name: [FIRST_NAME] [LAST_NAME]\n\n- Budget you owned: [PLACEHOLDER]",
            "### Role - [COMPANY] ([DATES])\n\nBracketed [[NAME]] and a[0], [x], [AB], [Note], - [ ] task",
            "- German: [LEVEL, e.g. \"Native\" / \"C2\"] and [CONTEXT - what was happening]",
            "Use the ` key for [FIRST_NAME]. Owner: `[NAME]",
            "An escaped backslash, then \\\\[NAME], and an escaped bracket \\[NAME]",
            "[FIRST_NAME] [LAST_NAME]\n\n[SECTOR_1]: [EXAMPLE_COMPANIES]\n[x]: https://x.example/$1$\n\nSee [x].",
            "- [SECTOR_1]: [EXAMPLE_COMPANIES]\n- [SECTOR_1]: [EXAMPLE_COMPANIES]\n",
            "[r]: https://x.example\n[r]: https://y.example/[PATH]\n\nSee [r].",
            "[SECTOR_1]: [EXAMPLE_COMPANIES]\n[Sector_1]: [OTHER_COMPANIES]\n\nSee [sector_1] and [OWNER_NAME].",
            "See [Salary $100k].\n\n[Salary $100k]: https://x.example",
            "See [FIRST_NAME].\n\n[FIRST_NAME]: https://x.example",
            "See `[FIRST_NAME]` and [LINK](https://x.example).",
            "```\n[YOUR_CITY]\n```",
            "Open [[NAME]](https://x.example) now.",
            "Pay [AMOUNT]5 and [LEVEL, e.g. \"Native\"]",
            "[Salary guide $100k-$150k](https://x.example)",
            "[FIRST_NAME] and [see $0$](https://x.example)",
            "# Pay $1$ [DAY_RATE]\n\n| Rate |\n|---|\n| $2$ [RATE_NOTE] |\n\n> Quote $3$ [QUOTE_TEXT]\n\n- Item $4$ [ITEM_TEXT]\n\n![alt $5$ [ALT_TEXT]](https://x.example/a.png)",
            "Term [TERM_NAME]\n: Defined $6$ [TERM_TEXT][^n]\n\n[^n]: Note $7$ [NOTE_TEXT]",
            "## Goals [GOAL_NAME]\n\n### Lead - [COMPANY] ([DATES])",
            "Cloud [AWS, GCP], remote [EU], and [2024 UPDATE]",
            "[LinkedIn](https://linkedin.com/in/[YOUR_HANDLE]) and ![logo](https://x.example/[LOGO_FILE].png)",
            "Search `[YOUR_JOB_BOARD] [YOUR_CITY]` for \\name{[FIRST_NAME]}",
            "```ini\n[DEFAULT]\n[YOUR_SECTION]\n```",
            "<!-- all [PLACEHOLDER] tokens -->\n\nName: <b>[FIRST_NAME]</b> [LAST_NAME]",
            "Salary: [LEVEL - about $50k] and [FLOOR - $40k at least]",
            "| Skill | Level |\n|---|---|\n| [SKILL_1] | `[LEVEL_1]` |",
            "Code such as `to fill` and [a link](https://tessel.example/jobs/1), then [CITY_NAME]$ and $[ROLE_NAME]",
            "### Lead \\[TEAM_NAME] - Acme (2020)\nShipped.",
            "### Lead &#91;TEAM_NAME&#93; - Acme (2020)\nShipped.",
            "### [JOB_TITLE] - Acme (2020)\nShipped.",
            "### Identity\n<!-- e.g. Ada --> [FIRST_NAME]\n",
            "Run `<!-- [YOUR_CITY] -->`",
            "[ROLE_TITLE]: Led the platform team\n\n[FIRST_NAME][LAST_NAME]\n\n[LAST_NAME](pronouns: she/her)",
            "See [run `$x`] and [a <b>x</b>] for [CITY].\n\n[run `$x`]: https://x.example\n[a <b>x</b>]: https://x.example",
            "Costs $5 [UNIT_NAME]$ a day",
            "Pay $5 for[x] and $[ROLE_NAME] now [CITY]",
            "![alt [CITY] $x](u.png) y$",
            "See [docs].\n\n<!-- old -->[docs]: https://evil.example\n\n[docs]: https://good.example",
            "## Goals\n<!-- What you want next -->\n\n<!--\n[GOAL_NAME]\n-->\n\n- [GOAL_TEXT]",
            "<div>\n[NAME_X]\n</div>\n\nAfter [AFTER_TEXT].",
        ];
        for render in [profiled as Render, unmarked] {
            for source in sources {
                let (text, blanks) = painted_text(render, source);
                for span in crate::shared_kernel::placeholders::placeholder_spans(source) {
                    assert!(text.contains(&source[span.clone()]), "{source}: {} is counted but not drawn in {text:?}", &source[span]);
                }
                assert_eq!(blanks, 0, "{source}: the body draws no blank");
            }
        }
    }

    /// A block of HTML that holds only comments shows nothing on the Profile paper. Other HTML shows as written, and
    /// `markdown` itself is unchanged.
    #[test]
    fn a_comment_block_stays_off_the_paper() {
        let source = "Before.\n\n<!-- a note on [GOAL_NAME] -->\n\n<!--\nsecond note\n-->\n\n<!-- old -->[docs]: https://evil.example\n\nAfter.";
        let (text, _) = painted_text(profiled, source);
        assert!(text.contains("Before.") && text.contains("After."), "{text:?}");
        assert!(!text.contains("a note on") && !text.contains("second note"), "{text:?}");
        assert!(text.contains("[docs]: https://evil.example"), "{text:?}");
        fn plain(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
            markdown(ui, cache, text);
        }
        assert!(painted_text(plain, source).0.contains("a note on"));
        // `<!-->` is an empty comment, so the text after it shows, and so does text after `--!>`.
        let (text, _) = painted_text(profiled, "Before.\n\n<!-->Open to relocation: no [CITY_NAME]\n\n<!-- a --!> Visible\n-->\n\nAfter.");
        assert!(text.contains("Open to relocation: no [CITY_NAME]") && text.contains("Visible"), "{text:?}");
    }

    #[test]
    fn a_profile_page_keeps_only_web_links() {
        let source = "[terminal](file:///Applications/Utilities/Terminal.app) and [posting](https://tessel.example/jobs/1) for [COMPANY]\n\n[script](javascript:alert(1)) and [[COMPANY_SITE]](file:///etc/hosts)";
        assert_eq!(opened_through(profiled, source, "terminal"), Vec::<String>::new());
        assert_eq!(opened_through(profiled, source, "script"), Vec::<String>::new());
        assert_eq!(opened_through(profiled, source, "[COMPANY_SITE]"), Vec::<String>::new());
        assert_eq!(opened_through(profiled, source, "posting"), vec!["https://tessel.example/jobs/1".to_owned()]);
    }

    /// A comment cut from the text could have made a definition of the line after it. The text is never cut, so the
    /// link keeps its target.
    #[test]
    fn a_comment_cannot_retarget_a_link() {
        let source = "See [docs].\n\n<!-- old -->[docs]: https://evil.example\n\n[docs]: https://good.example";
        assert_eq!(opened_through(profiled, source, "docs"), vec!["https://good.example".to_owned()]);
    }

    #[test]
    fn a_reference_link_with_a_dollar_sign_stays_a_link() {
        for source in [
            "See [Salary $100k].\n\n[Salary $100k]: https://x.example",
            "See [Salary $100k][].\n\n[Salary $100k]: https://x.example",
            "See [run `$x`].\n\n[run `$x`]: https://x.example",
        ] {
            let label = if source.contains("run") { "run $x" } else { "Salary $100k" };
            assert_eq!(opened_through(profiled, source, label), vec!["https://x.example".to_owned()], "{source}");
        }
        let source = "[FIRST_NAME] [LAST_NAME]\n\n[SECTOR_1]: [EXAMPLE_COMPANIES]\n[x]: https://x.example/$1$\n\nSee [x].";
        assert_eq!(opened_through(profiled, source, "x"), vec!["https://x.example/$1$".to_owned()]);
    }

    #[test]
    fn a_profile_page_shows_images_as_alt_text() {
        let source: &'static str = "Logo ![company logo](https://x.example/l.png) for [COMPANY].\n\n![alt [CITY] $x](u.png) y$";
        let (text, blanks) = painted_text(profiled, source);
        assert!(text.contains("company logo") && text.contains("alt [CITY] $x") && text.contains("y$"), "{text:?}");
        assert!(!text.contains("x.example") && !text.contains("u.png"), "{text:?}");
        assert_eq!(blanks, 0);
    }

    /// The pieces of a role's title, laid out: hidden characters dropped, each blank in the blank's ink, and nothing
    /// else in it.
    fn role_line(pieces: Vec<Piece>) -> (String, usize) {
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let found = Rc::new(RefCell::new((String::new(), 0)));
        let seen = Rc::clone(&found);
        let mut harness = Harness::builder().with_size([400.0, 100.0]).with_context_setup(setup).build_ui(move |ui| {
            let job = blank_line(ui, &pieces, Type::SECTION, Ink::Text);
            let ink = Palette::of(ui).interview;
            *seen.borrow_mut() = (job.text.clone(), job.sections.iter().filter(|section| section.format.color == ink).count());
        });
        harness.run();
        found.take()
    }

    #[test]
    fn a_role_line_drops_hidden_characters() {
        let pieces = vec![Piece::Text("Lead\u{202E}reenignE\u{200B} - ".to_owned()), Piece::Blank("to fill: company\u{200B}".to_owned())];
        assert_eq!(role_line(pieces), ("LeadreenignE - to fill: company".to_owned(), 1));
    }

    /// A role's title draws a blank exactly where the block's count found a token, and never for text of its own.
    #[test]
    fn a_role_line_draws_the_blanks_its_block_counts() {
        for (block, title, shown) in [
            ("### [JOB_TITLE] - Acme (2020)\nShipped.", "[JOB_TITLE]", "to fill: job title"),
            ("### Lead \\[TEAM_NAME] - Acme (2020)\nShipped.", "Lead \\[TEAM_NAME]", "Lead [TEAM_NAME]"),
            ("### Lead &#91;TEAM_NAME&#93; - Acme (2020)\nShipped.", "Lead &#91;TEAM_NAME&#93;", "Lead [TEAM_NAME]"),
            ("### Lead $\u{F0000}$ [ROLE_NAME] - Acme (2020)", "Lead $\u{F0000}$ [ROLE_NAME]", "Lead $\u{F0000}$ to fill: role name"),
            ("### Lead <b>[TEAM_NAME]</b> - Acme (2020)", "Lead <b>[TEAM_NAME]</b>", "Lead <b>to fill: team name</b>"),
        ] {
            let start = block.find(title).unwrap();
            let pieces = crate::shared_kernel::placeholders::shown_pieces(block, start..start + title.len());
            let counted = crate::shared_kernel::placeholders::placeholder_spans(block)
                .into_iter()
                .filter(|span| span.start >= start && span.end <= start + title.len())
                .count();
            let (text, blanks) = role_line(pieces);
            assert_eq!(text, shown, "{block}");
            assert_eq!(blanks, counted, "{block}");
        }
    }
}
