use std::ops::Range;
use std::sync::Arc;

use egui::layers::ShapeIdx;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Galley, Label, OutputCommand, Pos2, Rect, Response, RichText, Shape, Stroke, TextStyle, TextWrapMode, Ui, WidgetText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer, RenderHtmlFn};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

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

/// The room between a `name_line`'s name and its note.
const NAME_NOTE_GAP: f32 = 8.0;

/// A name in strong type with a `muted` note after it, such as a portal and its country, on one line cut at the right.
pub fn name_line(ui: &mut Ui, name: &str, note: Option<&str>) -> Response {
    let p = Palette::of(ui);
    let mut job = LayoutJob::default();
    job.append(name, 0.0, TextFormat::simple(Type::STRONG.font(), p.text));
    if let Some(note) = note {
        // A real space, so a screen reader hears the two apart, and the rest of the gap as room.
        job.append(&format!(" {note}"), NAME_NOTE_GAP / 2.0, TextFormat::simple(Type::NOTE.font(), p.muted));
    }
    add_job(ui, job, TextWrapMode::Truncate)
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
    let first = ui.painter().add(Shape::Noop);
    let viewer = CommonMarkViewer::new().render_html_fn(html);
    copyable(ui, |ui| viewer.show(ui, cache, &text));
    centre_bullets(ui, first);
    ui.ctx().output_mut(|output| keep_web_links(&mut output.commands, before));
}

/// Release notes from the update feed, rendered as docs/architecture/trust.md says: Markdown with no image loading and no
/// links. Hidden characters go before anything parses. A link shows as its words in body ink and never opens, an image
/// as its alt text in `muted`, raw HTML as its text, and every heading at the section size. Bold and lists stay.
/// Any url the viewer still asks to open is dropped.
pub fn release_markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    link_free_markdown(ui, cache, text, Type::BODY);
}

/// Claude's message in first-run setup, rendered by `release_markdown`'s rules, as docs/architecture/trust.md says:
/// hidden characters go before anything parses, a link shows as its words and never opens, an image as its alt text,
/// raw HTML as its text, and every heading at the section size. Its body reads at `Type::TAKE`, held to `DOC_MEASURE`,
/// and its words can be selected and copied.
pub fn claude_markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    ui.scope(|ui| {
        ui.set_max_width(ui.available_width().min(DOC_MEASURE));
        copyable(ui, |ui| link_free_markdown(ui, cache, text, Type::TAKE));
    });
}

/// `release_markdown` with its body in `body`.
fn link_free_markdown(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, body: Type) {
    // The rewrite runs once per distinct text, and is kept while the text stays on screen.
    let text = ui.ctx().memory_mut(|memory| memory.caches.cache::<LinkFreeCache>().get(text).clone());
    let before = ui.ctx().output(|output| output.commands.len());
    ui.scope(|ui| {
        let styles = &mut ui.style_mut().text_styles;
        styles.insert(TextStyle::Heading, Type::SECTION.font());
        styles.insert(TextStyle::Body, body.font());
        let first = ui.painter().add(Shape::Noop);
        // The viewer hands inline math to this, and the rewrite leaves no math but the alt text it marked.
        CommonMarkViewer::new().render_math_fn(Some(&alt_text)).show(ui, cache, &text);
        centre_bullets(ui, first);
    });
    ui.ctx().output_mut(|output| no_urls(&mut output.commands, before));
}

/// An image's alt text in release notes, where the image stood.
fn alt_text(ui: &mut Ui, alt: &str, _inline: bool) {
    // At the body size the scope set, so alt text reads level with the words around it.
    let font = TextStyle::Body.resolve(ui.style());
    ui.add(Label::new(RichText::new(alt).font(font).color(Palette::of(ui).muted)));
}

/// Drops every url opened after `from`.
fn no_urls(commands: &mut Vec<OutputCommand>, from: usize) {
    let mut index = 0;
    commands.retain(|command| {
        let kept = index < from || !matches!(command, OutputCommand::OpenUrl(_));
        index += 1;
        kept
    });
}

#[derive(Default)]
struct LinkFree;

impl egui::cache::ComputerMut<&str, Arc<str>> for LinkFree {
    fn compute(&mut self, text: &str) -> Arc<str> {
        link_free(text).into()
    }
}

type LinkFreeCache = egui::cache::FrameCache<Arc<str>, LinkFree>;

/// The parser's options for release notes: the viewer's own, with math, which the viewer turns on once it has a math
/// callback.
fn release_options() -> Options {
    markdown::options() | Options::ENABLE_MATH
}

/// Release notes as the viewer may show them: line ends made `\n`, hidden characters removed line by line, each link
/// swapped for its own words as written, each image for its alt text marked as inline math so `alt_text` paints it, any
/// math of the notes' own escaped to plain text, and every heading made a top-level one. A character reference such as
/// `&#x200B;` decodes only once parsed, so text that decodes to a hidden character is written out again without it.
/// The result is parsed again to confirm it holds no link, no image, no hidden character and no math but the marks.
/// Should it not, the alt text goes in plain, and failing that the whole text shows as plain characters.
fn link_free(text: &str) -> String {
    link_free_with(text, settled)
}

/// `link_free`, with `settled` judging each rewrite, so a test can reach each fallback.
fn link_free_with(text: &str, settled: fn(&str, &[String]) -> bool) -> String {
    let visible = visible(&text.replace("\r\n", "\n").replace('\r', "\n"));
    for mark in [true, false] {
        let (out, marks) = rewritten(&visible, mark);
        if settled(&out, &marks) {
            return out;
        }
    }
    // Every punctuation mark is escaped, `&` too, so nothing parses and no reference decodes.
    escaped(&visible)
}

/// `text` without hidden characters, line by line, so its line ends stay.
fn visible(text: &str) -> String {
    text.split('\n').map(|line| visible_chars(line).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// Whether `text` holds a character `visible` removes, other than a line end.
fn has_hidden(text: &str) -> bool {
    text.split('\n').any(|line| !visible_chars(line).eq(line.chars()))
}

/// The outermost link or image a rewrite is inside.
struct Outer {
    /// The whole link or image, as written.
    range: Range<usize>,
    image: bool,
    /// Its words, decoded, as plain text.
    words: String,
    /// Where its words are written, from the first thing inside it to the last.
    inner: Option<Range<usize>>,
    /// Whether it holds no image, no math and nothing that decodes to a hidden character, so its words can stand as
    /// written: a link holds no link.
    plain: bool,
    /// Whether a link holds an image, and words outside any image.
    holds_image: bool,
    loose_words: bool,
}

/// One rewrite of `text`, as `link_free` says. A link that holds an image or math, or words that decode to a hidden
/// character, becomes its escaped words instead, and a link that holds only images shows their alt text as an image
/// does. Returns it with the alt texts marked as math, in order, which only happens while `mark` holds.
fn rewritten(text: &str, mark: bool) -> (String, Vec<String>) {
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let (mut outer, mut depth, mut marks): (Option<Outer>, usize, Vec<String>) = (None, 0, Vec::new());
    for (event, range) in Parser::new_ext(text, release_options()).into_offset_iter() {
        if let Some(outer) = outer.as_mut().filter(|_| !matches!(event, Event::End(TagEnd::Link | TagEnd::Image)) || depth > 1) {
            outer.inner = Some(outer.inner.take().map_or(range.clone(), |inner| inner.start.min(range.start)..inner.end.max(range.end)));
            match &event {
                Event::Start(Tag::Image { .. }) => (outer.plain, outer.holds_image) = (false, true),
                Event::InlineMath(_) | Event::DisplayMath(_) => outer.plain = false,
                Event::Text(part) | Event::Code(part) => {
                    outer.plain &= !has_hidden(part);
                    // Inside a link, only an image's own words sit deeper.
                    outer.loose_words |= depth == 1;
                }
                _ => {}
            }
        }
        match event {
            Event::Start(tag @ (Tag::Link { .. } | Tag::Image { .. })) => {
                if depth == 0 {
                    let image = matches!(tag, Tag::Image { .. });
                    outer = Some(Outer { range, image, words: String::new(), inner: None, plain: true, holds_image: false, loose_words: false });
                }
                depth += 1;
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                depth -= 1;
                if depth == 0
                    && let Some(Outer { range, image, words, inner, plain, holds_image, loose_words }) = outer.take()
                {
                    let words = visible(&words).split_whitespace().collect::<Vec<_>>().join(" ");
                    let alt = image || holds_image && !loose_words;
                    let shown = if alt && mark && math_safe(&words) {
                        let shown = format!("${words}$");
                        marks.push(words);
                        shown
                    } else if !image && plain {
                        inner.map_or_else(String::new, |inner| text[inner].to_owned())
                    } else {
                        escaped(&words)
                    };
                    edits.push((range, shown));
                }
            }
            Event::Text(part) | Event::Code(part) | Event::InlineHtml(part) | Event::InlineMath(part) | Event::DisplayMath(part) if depth > 0 => {
                if let Some(outer) = outer.as_mut() {
                    outer.words.push_str(&part);
                }
            }
            Event::SoftBreak | Event::HardBreak if depth > 0 => {
                if let Some(outer) = outer.as_mut() {
                    outer.words.push(' ');
                }
            }
            // A decoded line end written back would start a new block, so it goes back as a space.
            Event::Text(part) if has_hidden(&part) => edits.push((range, escaped(&visible(&part.replace(['\n', '\r'], " "))))),
            Event::InlineMath(_) | Event::DisplayMath(_) => edits.push((range.clone(), escaped(&text[range]))),
            Event::Start(Tag::Heading { level, .. }) if level != HeadingLevel::H1 => edits.extend(top_heading(text, range)),
            _ => {}
        }
    }
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = String::new();
    let mut copied = 0;
    for (range, shown) in edits {
        if range.start < copied {
            continue;
        }
        out.push_str(&text[copied..range.start]);
        out.push_str(&shown);
        copied = range.end;
    }
    out.push_str(&text[copied..]);
    (out, marks)
}

/// Whether alt text can stand inside `$…$` as one inline math span: no space at either end, and no `$`, backslash,
/// brace, backtick, angle bracket, `&` or `|`, which could end the span, escape, decode or cut a table cell. Any other
/// character, such as curly quotes, dashes or `*`, is literal there, and `settled` confirms the span came out whole.
fn math_safe(words: &str) -> bool {
    !words.is_empty() && words.trim() == words && !words.chars().any(|c| "$\\{}`<>&|".contains(c) || c.is_control())
}

/// The edit that makes the heading at `range` a top-level one: an ATX heading's run of `#` becomes one, a setext
/// heading's underline of `-` becomes `=`.
fn top_heading(text: &str, range: Range<usize>) -> Option<(Range<usize>, String)> {
    let heading = &text[range.clone()];
    let indent = heading.len() - heading.trim_start_matches(' ').len();
    let hashes = heading[indent..].len() - heading[indent..].trim_start_matches('#').len();
    if hashes > 0 {
        let start = range.start + indent;
        return Some((start..start + hashes, "#".to_owned()));
    }
    let body = heading.trim_end_matches(['\n', '\r']);
    let underline = body.rfind('\n').map_or(0, |at| at + 1);
    let line = &body[underline..];
    let first = line.find('-')?;
    let last = line.rfind('-')? + 1;
    let start = range.start + underline + first;
    Some((start..start + (last - first), line[first..last].replace('-', "=")))
}

/// Whether a rewritten text holds no link, no image, no math but its `marks`, each as written, and no text that
/// decodes to a hidden character.
fn settled(text: &str, marks: &[String]) -> bool {
    let mut maths = Vec::new();
    for event in Parser::new_ext(text, release_options()) {
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. }) | Event::DisplayMath(_) => return false,
            Event::InlineMath(math) => maths.push(math.into_string()),
            Event::Text(part) | Event::Code(part) | Event::Html(part) | Event::InlineHtml(part) | Event::FootnoteReference(part) if has_hidden(&part) => {
                return false;
            }
            _ => {}
        }
    }
    maths == marks
}

/// The first line of a text the viewer painted: where it starts, the rows it spans, its baseline, and its type.
struct Line {
    left: f32,
    rows: std::ops::RangeInclusive<f32>,
    baseline: f32,
    font: FontId,
}

/// The viewer centres a list's bullet in a line box of body type, which puts it above the middle of the small letters
/// beside it. Each bullet painted from `first` on moves onto the middle of the x-height of the text that starts its line.
/// The viewer paints circles only for bullets, and a bullet with no text beside it stays where it is.
fn centre_bullets(ui: &Ui, first: ShapeIdx) {
    let layer = ui.layer_id();
    let (bullets, lines) = ui.ctx().graphics(|layers| {
        let (mut bullets, mut lines) = (Vec::new(), Vec::new());
        for (index, clipped) in layers.get(layer).into_iter().flat_map(|list| list.all_entries().enumerate().skip(first.0)) {
            match &clipped.shape {
                Shape::Circle(circle) => bullets.push((ShapeIdx(index), circle.center)),
                Shape::Text(text) => {
                    let (Some(row), Some(section)) = (text.galley.rows.first(), text.galley.job.sections.first()) else { continue };
                    let Some(glyph) = row.glyphs.first() else { continue };
                    let rect = row.rect().translate(text.pos.to_vec2());
                    let baseline = text.pos.y + row.pos.y + glyph.pos.y;
                    lines.push(Line { left: rect.left(), rows: rect.top()..=rect.bottom(), baseline, font: section.format.font_id.clone() });
                }
                _ => {}
            }
        }
        (bullets, lines)
    });
    let moves: Vec<(ShapeIdx, f32)> = bullets
        .into_iter()
        .filter_map(|(index, centre)| {
            let line = lines.iter().filter(|line| line.left > centre.x && line.rows.contains(&centre.y)).min_by(|a, b| a.left.total_cmp(&b.left))?;
            Some((index, line.baseline + x_middle(ui, line.font.clone())))
        })
        .collect();
    if moves.is_empty() {
        return;
    }
    ui.ctx().graphics_mut(|layers| {
        let list = layers.entry(layer);
        for (index, y) in moves {
            list.mutate_shape(index, |clipped| {
                if let Shape::Circle(circle) = &mut clipped.shape {
                    circle.center.y = y;
                }
            });
        }
    });
}

/// How far the middle of the small letters of `font` sits from its baseline: negative, as it sits above.
fn x_middle(ui: &Ui, font: FontId) -> f32 {
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap("x".to_owned(), font, Color32::PLACEHOLDER));
    let baseline = galley.rows.first().and_then(|row| row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y)).unwrap_or_default();
    galley.mesh_bounds.center().y - baseline
}

/// A block of HTML on the Profile paper: nothing when it holds only comments, otherwise its text as written, as the
/// viewer shows HTML it is not handed off.
fn comments_hidden(ui: &mut Ui, html: &str) {
    if !only_comments(html) {
        ui.label(html.trim_end());
    }
}

/// A message as a document: one readable column that never grows past the space it has, with headings held to the
/// section size. A top-level table leaves the column and scrolls only past the whole width. A table of two or more columns
/// fills that width, and a table of a single column keeps the width of its cells.
/// Every part goes through `markdown`, so ARCHITECTURE.md#trust holds.
pub fn document(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    document_with(ui, cache, text, markdown);
}

/// Profile text as a document, laid out as `document` lays out a message, with each part rendered as `profile_markdown`
/// renders it: placeholders as written, and comment-only HTML not at all.
pub fn profile_document(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str) {
    document_with(ui, cache, text, profile_markdown);
}

fn document_with(ui: &mut Ui, cache: &mut CommonMarkCache, text: &str, render: fn(&mut Ui, &mut CommonMarkCache, &str)) {
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
                if part.table {
                    ui.set_max_width(width);
                    fill_width(ui, &part.cells, width);
                } else {
                    ui.set_max_width(width.min(DOC_MEASURE));
                }
                render(ui, cache, &part.text);
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
    /// A table's cells, each with its column. Prose has none.
    cells: Vec<Cell>,
}

/// One table cell: its column, and its text as the viewer lays it out, one label per run.
struct Cell {
    column: usize,
    runs: Vec<Run>,
}

/// A run of a cell's text, in body type or, for code, in monospace.
struct Run {
    text: String,
    code: bool,
}

/// Every cell of the first table in `text`, the head row's included.
/// No footnote marker is measured: a table's own part never holds a footnote's definition, so `parts` keeps a text whole
/// when a cell points to one, and a table in a whole text keeps to the column and is never widened.
fn table_cells(text: &str) -> Vec<Cell> {
    let mut cells: Vec<Cell> = Vec::new();
    let mut column = 0;
    for event in Parser::new_ext(text, markdown::options()) {
        let run = match event {
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                column = 0;
                continue;
            }
            Event::Start(Tag::TableCell) => {
                cells.push(Cell { column, runs: Vec::new() });
                column += 1;
                continue;
            }
            Event::End(TagEnd::Table) => break,
            Event::Text(text) | Event::InlineHtml(text) => Run { text: text.into_string(), code: false },
            Event::Code(text) => Run { text: text.into_string(), code: true },
            _ => continue,
        };
        if let Some(cell) = cells.last_mut() {
            cell.runs.push(run);
        }
    }
    cells
}

/// Widens the columns of a table about to render at `width`, so the table fills it when its cells fit, as a document's
/// table takes the whole width. Columns narrower than the rest widen alike, and the others keep their width.
/// A table whose cells need more room is left as it is, and scrolls.
///
/// The viewer lays a table out as a grid in a group frame and a sideways scroll area, and the grid widens each column
/// to the least cell width the style sets. Each cell after the first opens with two spaces, and the table ends where
/// the last column's widest cell ends, so only the columns before it can take the room.
/// A table of one column keeps its own width: a cell claims only its text, and the least cell width only moves the
/// next column along, so with no column after it nothing in the style can widen the grid or its scroll area.
fn fill_width(ui: &mut Ui, cells: &[Cell], width: f32) {
    let style = ui.style();
    let (body, code) = (TextStyle::Body.resolve(style), TextStyle::Monospace.resolve(style));
    let margin = style.spacing.scroll.content_margin;
    let frame = 2.0 * (6.0 + style.visuals.widgets.noninteractive.bg_stroke.width) + f32::from(margin.left) + f32::from(margin.right);
    let laid = |text: &str, font: &FontId| ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), Color32::PLACEHOLDER).size().x);
    let gap = laid("  ", &body);
    let mut columns: Vec<f32> = Vec::new();
    for cell in cells {
        let runs: f32 = cell.runs.iter().map(|run| laid(&run.text, if run.code { &code } else { &body })).sum();
        let size = runs + if cell.column > 0 { gap } else { 0.0 };
        if columns.len() <= cell.column {
            columns.resize(cell.column + 1, 0.0);
        }
        columns[cell.column] = columns[cell.column].max(size);
    }
    let Some((&last, before)) = columns.split_last() else { return };
    // A point to spare, so rounding never brings the scroll bar back, and room for a floating bar beside the table at rest.
    let spare = 1.0 + ui.spacing().scroll.floating_width;
    if let Some(floor) = column_floor(before, width - frame - last - spare) {
        ui.spacing_mut().interact_size.x = floor;
    }
}

/// The least width that, given to every column narrower than it, makes `columns` add up to `room`. Wider columns keep
/// their own width. `None` when the columns fill the room already.
fn column_floor(columns: &[f32], room: f32) -> Option<f32> {
    let mut sorted = columns.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut wider: f32 = sorted.iter().sum();
    if sorted.is_empty() || wider >= room {
        return None;
    }
    for (index, &width) in sorted.iter().enumerate() {
        wider -= width;
        let floor = (room - wider) / (index + 1) as f32;
        if sorted.get(index + 1).is_none_or(|&next| floor <= next) {
            return Some(floor);
        }
    }
    None
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
    let whole = || vec![Part { text: text.to_owned(), table: false, spaced: false, cells: Vec::new() }];
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
        let text = format!("{head}{}", &text[range]);
        let cells = if table { table_cells(&text) } else { Vec::new() };
        Part { text, table, spaced, cells }
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

/// A short run of Claude's Markdown, such as one numbered question or one proposed change, laid out as one wrapped text
/// by `release_markdown`'s rules, for a widget that shows it as a label or paints it inside a control of its own.
/// Hidden characters go before it parses and from every decoded piece. Bold takes the semibold face and code `Type::MONO`;
/// a link is its words in `ink` and opens nothing; an image is its alt text in `muted`; raw HTML and math are their text
/// as written. A heading reads as its words, at `style`. Blocks after the first start on a line of their own. `ink` may
/// be `Color32::PLACEHOLDER`, so a painter can choose the ink when it paints.
pub(in crate::ui_kit) fn inline_job(ui: &Ui, text: &str, style: Type, ink: Color32) -> LayoutJob {
    let muted = Palette::of(ui).muted;
    let clean = visible(&text.replace("\r\n", "\n").replace('\r', "\n"));
    let (mut strong, mut image) = (0_usize, 0_usize);
    let mut pieces: Vec<(String, FontId, Color32)> = Vec::new();
    let push = |pieces: &mut Vec<(String, FontId, Color32)>, piece: &str, font: FontId, color: Color32| {
        let piece: String = piece.split('\n').map(|line| visible_chars(line).collect::<String>()).collect::<Vec<_>>().join("\n");
        if !piece.is_empty() {
            pieces.push((piece, font, color));
        }
    };
    for event in Parser::new_ext(&clean, markdown::options()) {
        let font = |strong: usize| FontId::new(style.size, if strong > 0 { Type::STRONG.face.family() } else { style.face.family() });
        let color = if image > 0 { muted } else { ink };
        match event {
            Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::Item | Tag::BlockQuote(_) | Tag::CodeBlock(_)) => {
                if pieces.last().is_some_and(|(last, _, _)| !last.ends_with('\n')) {
                    push(&mut pieces, "\n", font(strong), color);
                }
            }
            Event::Start(Tag::Strong) => strong += 1,
            Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
            Event::Start(Tag::Image { .. }) => image += 1,
            Event::End(TagEnd::Image) => image = image.saturating_sub(1),
            Event::Text(part) | Event::Html(part) | Event::InlineHtml(part) | Event::InlineMath(part) | Event::DisplayMath(part) => {
                push(&mut pieces, &part, font(strong), color);
            }
            Event::Code(part) => push(&mut pieces, &part, Type::MONO.font(), color),
            Event::SoftBreak => push(&mut pieces, " ", font(strong), color),
            Event::HardBreak => push(&mut pieces, "\n", font(strong), color),
            _ => {}
        }
    }
    // A code block or a block of HTML ends on a line end, which would leave an empty line under the words.
    while let Some((last, _, _)) = pieces.last_mut() {
        let kept = last.trim_end_matches('\n').len();
        if kept > 0 {
            last.truncate(kept);
            break;
        }
        pieces.pop();
    }
    let mut job = LayoutJob::default();
    for (piece, font, color) in pieces {
        job.append(&piece, 0.0, TextFormat::simple(font, color));
    }
    job
}

/// A name from outside Jobdesk, such as a file's, as plain text on one line: hidden characters and line breaks removed.
pub(in crate::ui_kit) fn plain_name(name: &str) -> String {
    visible_chars(&name.replace(['\n', '\r'], " ")).collect()
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
    use egui_kittest::kittest::{NodeT, Queryable};

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

    /// A table narrower than the room widens to fill it, at any width, and its last cell stays in sight. It stops a
    /// floating bar's width short, so a bar beside it at rest never touches its frame.
    #[test]
    fn a_narrow_table_fills_the_width() {
        let source: &'static str = "Spoken at work.\n\n| Language | Level | Notes |\n|---|---|---|\n| French | Native | |\n| English | C1 | Daily at work since `2014` |\n| Dutch | B1 | Meetings, not contracts |";
        for size in [700.0, 1000.0, 1400.0] {
            let found = Rc::new(RefCell::new((0.0, 0.0, Rect::NOTHING)));
            let seen = Rc::clone(&found);
            let mut cache = CommonMarkCache::default();
            let setup = |ctx: &egui::Context| {
                crate::ui_kit::install_fonts(ctx);
                crate::ui_kit::install_theme(ctx);
            };
            let mut harness = Harness::builder().with_size([size, 600.0]).with_context_setup(setup).build_ui(move |ui| {
                let (room, bar) = (ui.available_width(), ui.spacing().scroll.floating_width);
                *seen.borrow_mut() = (room, bar, ui.scope(|ui| document(ui, &mut cache, source)).response.rect);
            });
            harness.run();
            let (room, bar, area) = *found.borrow();
            // The widest cell ends in code, so the frame and the cell's two runs all count.
            let last = harness.get_by_label("2014").rect();
            assert!(area.width() >= room - bar - 2.0 && area.width() <= room - bar, "in {room} px the document is {} px", area.width());
            assert!(last.right() <= area.right() - 6.0, "the last cell ends at {}, the table at {}", last.right(), area.right());
        }
    }

    /// A table of one column has no column before its last to widen, so it keeps the width of its cells, whole and with
    /// no scroll bar, at any width.
    #[test]
    fn a_one_column_table_keeps_its_width() {
        let source: &'static str = "| Step |\n|---|\n| Ask about the salary band |\n| Send the portfolio |";
        for size in [400.0, 1000.0] {
            let area = Rc::new(RefCell::new(Rect::NOTHING));
            let seen = Rc::clone(&area);
            let mut cache = CommonMarkCache::default();
            let mut harness = Harness::builder().with_size([size, 400.0]).build_ui(move |ui| {
                *seen.borrow_mut() = ui.scope(|ui| document(ui, &mut cache, source)).response.rect;
            });
            harness.run();
            let (area, cell) = (*area.borrow(), harness.get_by_label("Ask about the salary band").rect());
            assert!(area.width() < 300.0, "in {size} px the table is {} px", area.width());
            assert!(cell.right() <= area.right() - 6.0, "the cell ends at {}, the table at {}", cell.right(), area.right());
        }
    }

    /// A footnote marker in a cell keeps the text whole, so its table is never widened past what its cells measure, and
    /// the marker shows inside the table with no scroll bar.
    #[test]
    fn a_footnote_in_a_cell_keeps_the_table_unwidened() {
        let source: &'static str =
            "Gates.\n\n| Gate | Result |\n|---|---|\n| Salary | Checked against the band[^1] |\n\nAfter[^1].\n\n[^1]: Ask on the first call.";
        assert_eq!(parts(source).len(), 1);
        let area = Rc::new(RefCell::new(Rect::NOTHING));
        let seen = Rc::clone(&area);
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([1000.0, 600.0]).build_ui(move |ui| {
            *seen.borrow_mut() = ui.scope(|ui| document(ui, &mut cache, source)).response.rect;
        });
        harness.run();
        let area = *area.borrow();
        let marker = harness.query_all_by_label("1").map(|node| node.rect()).max_by(|a, b| a.right().total_cmp(&b.right())).expect("the marker shows");
        assert!(area.width() <= DOC_MEASURE, "the document is {} px", area.width());
        assert!(marker.right() <= area.right() - 6.0, "the marker ends at {}, the table at {}", marker.right(), area.right());
    }

    #[test]
    fn narrow_columns_widen_alike() {
        assert_eq!(column_floor(&[100.0, 50.0, 300.0], 600.0), Some(150.0));
        assert_eq!(column_floor(&[100.0, 50.0, 300.0], 900.0), Some(300.0));
        assert_eq!(column_floor(&[100.0, 50.0, 300.0], 1200.0), Some(400.0));
        assert_eq!(column_floor(&[100.0, 50.0, 300.0], 450.0), None);
        assert_eq!(column_floor(&[], 450.0), None);
    }

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

    /// Each bullet's centre, and the middle of the small letters of the text beside it, as `render` paints `source`.
    /// Every item is written in letters without ascenders or descenders, so the ink of its text is its x-height.
    fn bullets_beside_text(render: Render, source: &'static str) -> Vec<(f32, f32)> {
        let found = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&found);
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut cache = CommonMarkCache::default();
        let mut harness = Harness::builder().with_size([800.0, 400.0]).with_context_setup(setup).build_ui(move |ui| {
            render(ui, &mut cache, source);
            let layer = ui.layer_id();
            *seen.borrow_mut() = ui.ctx().graphics(|layers| {
                let shapes: Vec<&Shape> = layers.get(layer).map(|list| list.all_entries().map(|clipped| &clipped.shape).collect()).unwrap_or_default();
                let texts: Vec<_> = shapes.iter().filter_map(|shape| if let Shape::Text(text) = shape { Some(text) } else { None }).collect();
                let circles = shapes.iter().filter_map(|shape| if let Shape::Circle(circle) = shape { Some(circle.center) } else { None });
                circles
                    .map(|centre| {
                        let beside = texts.iter().filter(|text| text.pos.x > centre.x && !text.galley.is_empty()).min_by(|a, b| {
                            let gap = |text: &&&egui::epaint::TextShape| (text.pos.y + text.galley.mesh_bounds.center().y - centre.y).abs();
                            gap(a).total_cmp(&gap(b))
                        });
                        let middle = beside.map_or(f32::NAN, |text| text.pos.y + text.galley.mesh_bounds.center().y);
                        (centre.y, middle)
                    })
                    .collect()
            });
        });
        harness.run();
        found.take()
    }

    #[test]
    fn a_bullet_sits_on_the_middle_of_its_line() {
        fn plain(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
            markdown(ui, cache, text);
        }
        let source = "- one\n- a manner\n  - nos one\n- xoxo\n\nsome more\n\n- nurse uwu";
        for render in [plain as Render, unmarked, profiled] {
            let bullets = bullets_beside_text(render, source);
            assert_eq!(bullets.len(), 5, "{bullets:?}");
            for (bullet, middle) in bullets {
                assert!((bullet - middle).abs() <= 0.75, "a bullet sits at {bullet}, the middle of its text at {middle}");
            }
        }
    }

    /// A bullet beside a task's box sits on the middle of the task's text.
    #[test]
    fn a_task_bullet_sits_on_the_middle_of_its_line() {
        let bullets = bullets_beside_text(unmarked, "- [ ] a new resume\n- [x] warm cover");
        assert_eq!(bullets.len(), 2, "{bullets:?}");
        for (bullet, middle) in bullets {
            assert!((bullet - middle).abs() <= 0.75, "a bullet sits at {bullet}, the middle of its text at {middle}");
        }
    }

    /// A bullet with no text on its line stays where the viewer put it, and the bullet after it still moves.
    #[test]
    fn a_bullet_with_no_text_stays_put() {
        fn plain(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
            markdown(ui, cache, text);
        }
        fn viewer(ui: &mut Ui, cache: &mut CommonMarkCache, text: &'static str) {
            CommonMarkViewer::new().show(ui, cache, text);
        }
        let source = "-\n- one";
        let (moved, kept) = (bullets_beside_text(plain, source), bullets_beside_text(viewer, source));
        assert_eq!((moved.len(), kept.len()), (2, 2), "{moved:?} {kept:?}");
        assert_eq!(moved[0].0, kept[0].0, "the bare bullet moved");
        assert!((moved[1].0 - moved[1].1).abs() <= 0.75, "the bullet beside one sits at {}, its text at {}", moved[1].0, moved[1].1);
    }

    /// Release notes that try every way out: a link, an autolink, an image, and the same two in raw HTML.
    const RELEASE_TRAPS: &str = "### Features\n\n- **settings**: see [the pull request](https://x.example/pull/41)\n- mail <https://x.example/inbox>\n\n![The new Data page](https://x.example/img.png)\n\nRaw <a href=\"https://x.example/a\">anchor words</a> here.\n\n<img src=\"https://x.example/b.png\" alt=\"html image\">\n";

    /// Renders release notes, clicks the middle of every node a screen reader finds, and returns every url a frame asked
    /// to open, with the harness to look in.
    fn release_clicks(source: &'static str) -> (Vec<String>, Harness<'static>) {
        let opened = Rc::new(RefCell::new(Vec::new()));
        let seen = Rc::clone(&opened);
        let mut cache = CommonMarkCache::default();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([600.0, 500.0]).with_context_setup(setup).build_ui(move |ui| {
            release_markdown(ui, &mut cache, source);
            let urls: Vec<String> = ui.ctx().output(|output| {
                output.commands.iter().filter_map(|command| if let OutputCommand::OpenUrl(open) = command { Some(open.url.clone()) } else { None }).collect()
            });
            seen.borrow_mut().extend(urls);
        });
        harness.run();
        // The harness draws at one point per pixel, so a node's bounds are in points.
        let spots: Vec<Pos2> = harness
            .query_all_by(|_| true)
            .filter_map(|node| node.accesskit_node().bounding_box())
            .map(|bounds| egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32))
            .collect();
        assert!(spots.len() > 5, "the notes show {} nodes", spots.len());
        for at in spots {
            for pressed in [true, false] {
                harness.input_mut().events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run();
            }
        }
        (opened.take(), harness)
    }

    #[test]
    fn release_notes_open_nothing() {
        let (opened, harness) = release_clicks(RELEASE_TRAPS);
        assert_eq!(opened, Vec::<String>::new());
        assert!(harness.query_by_role(egui::accesskit::Role::Link).is_none(), "the notes hold a link");
        for words in ["the pull request", "https://x.example/inbox", "The new Data page", "anchor words", "Features"] {
            assert!(harness.query_by_label_contains(words).is_some(), "{words} is not shown");
        }
        assert!(harness.query_by_label_contains("x.example/img.png").is_none(), "an image's address shows");
    }

    #[test]
    fn release_notes_drop_hidden_characters() {
        let hidden = ['\u{200B}', '\u{200D}', '\u{FEFF}', '\u{202E}', '\u{2066}', '\u{2069}'];
        let source = "## Fi\u{200B}xes\n\n- **to\u{202E}day**: a sk\u{2066}ipped\u{2069} card [st\u{200D}ays](https://x.example) put\u{FEFF}\n\n![lo\u{200B}go](https://x.example/l.png)\n";
        let (_, harness) = release_clicks(source);
        // A label keeps its words as its value.
        let labels: Vec<String> = harness.query_all_by(|_| true).filter_map(|node| node.accesskit_node().value().or(node.accesskit_node().label())).collect();
        for words in ["Fixes", "today", "skipped", "stays", "logo"] {
            assert!(labels.iter().any(|label| label.contains(words)), "{words} is not in {labels:?}");
        }
        for label in &labels {
            assert!(!label.chars().any(|c| hidden.contains(&c)), "{label:?} keeps a hidden character");
        }
    }

    /// Character references, decimal, hex and named, that decode to hidden or reordering characters, in text, a
    /// heading, link words, alt text and code.
    const HIDDEN_REFERENCES: &str = "## Fi&#x200B;xes\n\n- Fixed&#8203;the &#x202E;gpj.exe step\n- a&ZeroWidthSpace;b&zwj;c&lrm;d&rlm;e&shy;f\n- [li&#x202E;nk words](https://x.example) and `co&#x200B;de`\n- **to&#8238;day** and _we&#x2066;ek_\n\n![al&#x202E;t text](https://x.example/i.png)\n\n| A&#x200B;B | C |\n|---|---|\n| d&#8203;e | f |\n";

    #[test]
    fn release_notes_drop_hidden_characters_written_as_references() {
        let hidden = ['\u{200B}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{00AD}', '\u{202E}', '\u{2066}'];
        let out = link_free(HIDDEN_REFERENCES);
        assert!(settled(&out, &["alt text".to_owned()]), "{out}");
        for event in Parser::new_ext(&out, release_options()) {
            if let Event::Text(part) | Event::Code(part) | Event::InlineMath(part) = event {
                assert!(!part.chars().any(|c| hidden.contains(&c)), "{part:?} decodes to a hidden character");
            }
        }
        let (_, harness) = release_clicks(HIDDEN_REFERENCES);
        let labels: Vec<String> = harness.query_all_by(|_| true).filter_map(|node| node.accesskit_node().value().or(node.accesskit_node().label())).collect();
        for words in ["Fixes", "gpj.exe", "link words", "alt text", "co&#x200B;de", "today", "week", "AB", "de"] {
            assert!(labels.iter().any(|label| label.contains(words)), "{words} is not in {labels:?}");
        }
        for label in &labels {
            assert!(!label.chars().any(|c| hidden.contains(&c)), "{label:?} keeps a hidden character");
        }
    }

    #[test]
    fn release_notes_keep_their_line_ends() {
        assert_eq!(link_free("- a\r- b\r\n- c"), "- a\n- b\n- c");
    }

    /// The blocks of `text`, by kind, in order.
    fn blocks(text: &str) -> Vec<String> {
        Parser::new_ext(text, release_options())
            .filter_map(|event| match event {
                Event::Start(tag @ (Tag::Heading { .. } | Tag::Paragraph | Tag::Table(_) | Tag::TableRow)) => {
                    Some(format!("{tag:?}").chars().take(5).collect())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_decoded_line_end_stays_inside_its_block() {
        for source in
            ["## a&#10;b&#x200B;", "## a&#x0D;b&#8203;", "| a&#10;b&#x200B; | c |\n|---|---|\n| d | e |", "| a&#13;b&#x200B; | c |\n|---|---|\n| d | e |"]
        {
            let out = link_free(source);
            assert_eq!(blocks(&out), blocks(source), "{source:?} became {out:?}");
            assert!(settled(&out, &[]), "{out:?}");
        }
        // The parser gives each reference a text of its own: a line feed's is left as written, and a carriage return,
        // which counts as hidden, goes back as a space between the words it parted.
        assert_eq!(link_free("## a&#10;b&#x200B;"), "# a&#10;b");
        assert_eq!(link_free("## a&#x0D;b&#8203;"), "# a b");
        assert_eq!(link_free("| a&#13;b | c |\n|---|---|\n| d | e |"), "| a b | c |\n|---|---|\n| d | e |");
    }

    /// Whether `text` parses with no link and no image.
    fn holds_no_link(text: &str) -> bool {
        !Parser::new_ext(text, release_options()).any(|event| matches!(event, Event::Start(Tag::Link { .. } | Tag::Image { .. })))
    }

    #[test]
    fn release_notes_fall_back_to_plain_alt_text_then_to_plain_characters() {
        let source = "A [link](https://x.example) and ![logo](https://x.example/l.png).";
        let marked = link_free_with(source, |_, _| true);
        assert_eq!(marked, "A link and $logo$.");
        // A rewrite whose marks do not settle shows the alt text in plain.
        let plain = link_free_with(source, |text, marks| marks.is_empty() && settled(text, marks));
        assert_eq!(plain, "A link and logo.");
        // One that never settles shows every character as written.
        let whole = link_free_with(source, |_, _| false);
        assert_eq!(whole, escaped(source));
        for out in [&marked, &plain, &whole] {
            assert!(holds_no_link(out), "{out}");
        }
        // Unwrapping the outer brackets would make a new link, so these notes show as written.
        let nested = "[[x]](https://a.example)\n\n[x]: https://b.example";
        assert_eq!(link_free(nested), escaped(nested));
        assert!(holds_no_link(&link_free(nested)));
        let (opened, harness) = release_clicks("[[x]](https://a.example)\n\n[x]: https://b.example");
        assert_eq!(opened, Vec::<String>::new());
        assert!(harness.query_by_role(egui::accesskit::Role::Link).is_none());
    }

    #[test]
    fn release_notes_swap_links_and_images_before_the_viewer() {
        assert_eq!(link_free("A [link **here**](https://x.example) now."), "A link **here** now.");
        assert_eq!(link_free("<https://x.example/a>"), "https://x.example/a");
        assert_eq!(link_free("[two\nlines](https://x.example)"), "two\nlines");
        assert_eq!(link_free("[](https://x.example) gone"), " gone");
        assert_eq!(link_free("[sum $x$](https://x.example)"), "sum x");
        assert_eq!(link_free("![The new page](https://x.example/p.png)"), "$The new page$");
        assert_eq!(link_free("[![badge](https://x.example/b.svg)](https://x.example)"), "$badge$", "a link that holds only an image shows its alt text");
        assert_eq!(link_free("[see ![badge](https://x.example/b.svg)](https://x.example)"), "see badge");
        assert_eq!(link_free("![Jobdesk’s “new” page — 2… _a_ *b* ~c~](u)"), "$Jobdesk’s “new” page — 2… a b c$");
        assert_eq!(link_free("![a & b](u)"), "a \\& b");
        assert_eq!(link_free("![a {brace}](https://x.example/p.png)"), "a \\{brace\\}");
        assert_eq!(link_free("![first](a)![second](b)"), "$first$$second$");
        assert_eq!(link_free("Sum $x+1$ ![logo](l)"), "Sum \\$x\\+1\\$ $logo$");
        assert_eq!(link_free("[ref][r]\n\n[r]: https://x.example"), "ref\n\n[r]: https://x.example");
        assert_eq!(link_free("`[code](https://x.example)`"), "`[code](https://x.example)`");
    }

    #[test]
    fn release_notes_hold_every_heading_to_the_section_size() {
        assert_eq!(link_free("### Features\n\nText"), "# Features\n\nText");
        assert_eq!(link_free("  ## Fixes ##\n"), "  # Fixes ##\n");
        assert_eq!(link_free("Fixes\n---\n"), "Fixes\n===\n");
        assert_eq!(link_free("- ###### Deep\n"), "- # Deep\n");
        assert_eq!(link_free("# Top\n"), "# Top\n");
        let line = Rc::new(RefCell::new(0.0));
        let seen = Rc::clone(&line);
        let mut cache = CommonMarkCache::default();
        let setup = |ctx: &egui::Context| {
            crate::ui_kit::install_fonts(ctx);
            crate::ui_kit::install_theme(ctx);
        };
        let mut harness = Harness::builder().with_size([600.0, 300.0]).with_context_setup(setup).build_ui(move |ui| {
            *seen.borrow_mut() = wrapped_height(ui, "Fit", Type::SECTION, f32::INFINITY);
            release_markdown(ui, &mut cache, "Fit\n===\n\n### Gap\n\nBody text.");
        });
        harness.run();
        for heading in ["Fit", "Gap"] {
            let height = harness.get_by_label(heading).rect().height();
            assert!((height - *line.borrow()).abs() <= 2.0, "{heading} is {height} px, one section line is {}", line.borrow());
        }
    }
}
