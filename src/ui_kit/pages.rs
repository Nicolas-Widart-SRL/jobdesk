use egui::{Align, Color32, Direction, Layout, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, UiBuilder, Vec2, pos2, vec2};

use crate::ui_kit::Palette;
use crate::ui_kit::palette::Ink;
use crate::ui_kit::surfaces::{card_skeleton, page_frame};
use crate::ui_kit::text::add_text;
use crate::ui_kit::tokens::{HAIRLINE, ITEM_GAP, PAGE_GAP, PAGE_PADDING, PAGE_SIZE, RADIUS_STAMP, Type};

/// The size of a page `aspect` wide for each unit of height, fitted to `room`: it takes the room's height less `caption`,
/// narrows so two pages sit side by side `PAGE_GAP` apart, and never draws smaller than `PAGE_SIZE`.
pub fn page_fit(room: Vec2, aspect: f32, caption: f32) -> Vec2 {
    let by_height = (room.y - caption) * aspect;
    let by_width = (room.x - PAGE_GAP) / 2.0;
    let least = PAGE_SIZE.0.min(PAGE_SIZE.1 * aspect);
    let width = by_height.min(by_width).max(least);
    vec2(width, width / aspect)
}

/// What a document's pages show: grey blocks for as many pages while they render, or the rendered pages, read as `alt`.
/// `of` is how many pages the document holds, which is more than `pages` when the render stopped short.
#[derive(Clone, Copy)]
pub enum PageSet<'a> {
    Loading(usize),
    Ready { pages: &'a [TextureHandle], of: usize, alt: &'a str },
}

/// A page's width for each unit of its height, when nothing rendered says otherwise.
const PAGE_ASPECT: f32 = PAGE_SIZE.0 / PAGE_SIZE.1;

/// How pages fit the room left in `ui`: the size inside `page_frame` of a page `aspect` wide for each unit of height,
/// with room kept under it for its caption and around it for the hairlines of two frames side by side.
fn fitter(ui: &Ui) -> impl Fn(f32) -> Vec2 + use<> {
    let room = ui.available_size() - vec2(4.0 * HAIRLINE, 2.0 * HAIRLINE);
    let caption = ITEM_GAP + ui.fonts_mut(|fonts| fonts.row_height(&Type::NOTE.font()));
    move |aspect| page_fit(room, aspect, caption)
}

/// A document's pages in `page_frame`s, fitted to the room left in `ui` as `page_fit` says, each over "Page N of M" in
/// `Type::NOTE` `faint`. Two sit side by side and a third wraps under them, so put them in a scroll area. While they
/// render, each is a grey block over a short one where its caption goes.
pub fn fitted_pages(ui: &mut Ui, set: PageSet) {
    let fit = fitter(ui);
    let (count, of) = match set {
        PageSet::Loading(count) => (count, count),
        PageSet::Ready { pages, of, .. } => (pages.len(), of.max(pages.len())),
    };
    let caption = |index: usize| format!("Page {} of {of}", index + 1);
    ui.spacing_mut().item_spacing = vec2(PAGE_GAP, PAGE_GAP);
    ui.horizontal_wrapped(|ui| {
        for index in 0..count {
            if index > 0 && index % 2 == 0 {
                ui.end_row();
            }
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = ITEM_GAP;
                match set {
                    // Pages sit on paper, where a plain `skeleton` takes the card's own fill in dark.
                    PageSet::Loading(_) => {
                        let size = fit(PAGE_ASPECT) + Vec2::splat(2.0 * HAIRLINE);
                        card_skeleton(ui, size.x, size.y);
                        let width = ui.fonts_mut(|fonts| fonts.layout_no_wrap(caption(index), Type::NOTE.font(), Color32::PLACEHOLDER).size().x);
                        card_skeleton(ui, width, Type::NOTE.size);
                    }
                    PageSet::Ready { pages, alt, .. } => {
                        let texture = &pages[index];
                        let rendered = texture.size_vec2();
                        let aspect = if rendered.x > 0.0 && rendered.y > 0.0 { rendered.x / rendered.y } else { PAGE_ASPECT };
                        page_frame(ui, |ui| {
                            ui.add(egui::Image::new((texture.id(), fit(aspect))).alt_text(format!("{alt} page {}", index + 1)));
                        });
                        add_text(ui, caption(index), Type::NOTE, Ink::Faint);
                    }
                }
            });
        }
    });
}

/// An empty page where a document's pages would show: a `page_frame` filled `card`, sized as `fitted_pages` sizes a
/// page, with `text` centred in it in `muted`.
pub fn blank_page(ui: &mut Ui, text: &str) {
    let size = fitter(ui)(PAGE_ASPECT);
    let card = Palette::of(ui).card;
    page_frame(ui, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, 0.0, card);
        let inner = rect.shrink2(vec2(PAGE_PADDING.0, 0.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::centered_and_justified(Direction::TopDown)), |ui| {
            add_text(ui, text, Type::BODY, Ink::Muted);
        });
    });
}

/// The lines on a mini page, as shares of its text width: the head, then the lines under it. A small page shows the
/// first six.
const MINI_PAGE_LINES: [f32; 9] = [0.6, 1.0, 0.7, 1.0, 1.0, 0.7, 1.0, 0.7, 1.0];

/// A page drawn small, as a mark for a document rather than its image: `page` paper in a `line` hairline, a `page_head`
/// bar, then grey `page_line` lines. `small` draws the one at the start of a file list's row, with fewer lines.
pub(in crate::ui_kit) fn paint_mini_page(ui: &Ui, rect: Rect, small: bool) {
    let p = Palette::of(ui);
    let painter = ui.painter();
    painter.rect(rect, RADIUS_STAMP, p.page, Stroke::new(HAIRLINE, p.line), StrokeKind::Inside);
    // Inset, line height, head height, the room between lines, and the extra room under the head.
    let (inset, line, head, gap, under_head) = if small { (vec2(3.0, 4.0), 2.0, 3.0, 2.0, 1.0) } else { (vec2(7.0, 8.0), 3.0, 6.0, 4.0, 3.0) };
    let text = rect.shrink2(inset);
    let mut top = text.top();
    for (index, share) in MINI_PAGE_LINES.iter().take(if small { 6 } else { 9 }).enumerate() {
        let (height, ink) = if index == 0 { (head, p.page_head) } else { (line, p.page_line) };
        if top + height > text.bottom() {
            break;
        }
        painter.rect_filled(Rect::from_min_size(pos2(text.left(), top), vec2(text.width() * share, height)), 0.0, ink);
        top += height + gap + if index == 0 { under_head } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPTION: f32 = 20.0;

    #[test]
    fn a_tall_room_fits_the_page_to_its_height() {
        let page = page_fit(vec2(2000.0, 600.0), PAGE_ASPECT, CAPTION);
        assert!((page.y - (600.0 - CAPTION)).abs() < 0.01, "{page:?}");
        assert!((page.x / page.y - PAGE_ASPECT).abs() < 0.001, "{page:?}");
    }

    #[test]
    fn a_narrow_room_fits_two_pages_side_by_side() {
        let page = page_fit(vec2(500.0, 900.0), PAGE_ASPECT, CAPTION);
        assert!((2.0 * page.x + PAGE_GAP - 500.0).abs() < 0.01, "{page:?}");
        assert!((page.x / page.y - PAGE_ASPECT).abs() < 0.001, "{page:?}");
    }

    #[test]
    fn a_tiny_room_keeps_the_page_size() {
        let page = page_fit(vec2(100.0, 100.0), PAGE_ASPECT, CAPTION);
        assert!((page - vec2(PAGE_SIZE.0, PAGE_SIZE.1)).length() < 0.01, "{page:?}");
    }

    #[test]
    fn a_wide_page_keeps_within_the_page_size() {
        let page = page_fit(Vec2::ZERO, 2.0, CAPTION);
        assert!((page - vec2(PAGE_SIZE.0, PAGE_SIZE.0 / 2.0)).length() < 0.01, "{page:?}");
    }
}
