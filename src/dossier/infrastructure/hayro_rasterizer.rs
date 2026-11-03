use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

use crate::dossier::domain::{PageImage, PageRasterizer, RenderedPages};

const SCALE: f32 = 2.0;
const MAX_PAGES: usize = 6;

/// Pure Rust, so no PDF library ships beside the app. Its reasons say what went wrong with the file, never only that
/// it could not be rendered, which the screen already says.
pub struct HayroRasterizer;

impl PageRasterizer for HayroRasterizer {
    fn render(&self, path: &Path) -> Result<RenderedPages, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        panic::catch_unwind(AssertUnwindSafe(|| render_pages(bytes))).unwrap_or_else(|_| Err("The renderer stopped partway through the file.".to_owned()))
    }
}

fn render_pages(bytes: Vec<u8>) -> Result<RenderedPages, String> {
    let pdf = Pdf::new(Arc::new(bytes)).map_err(|error| match error {
        LoadPdfError::Decryption(_) => "The PDF is encrypted, and the renderer cannot open it.".to_owned(),
        LoadPdfError::Invalid => "The file is not a PDF the renderer can read.".to_owned(),
    })?;
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let pixmap_settings = PixmapSettings { x_scale: SCALE, y_scale: SCALE, bg_color: WHITE };
    let pages = pdf.pages();
    let images = pages
        .iter()
        .take(MAX_PAGES)
        .map(|page| {
            let pixmap = render(page, &cache, &settings, &RenderSettings::default(), &pixmap_settings);
            PageImage { width: usize::from(pixmap.width()), height: usize::from(pixmap.height()), rgba: pixmap.data_as_u8_slice().to_vec() }
        })
        .collect();
    Ok(RenderedPages { pages: images, total: pages.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF of `pages` empty A4 pages, with its cross-reference offsets counted as it is written.
    fn pdf(pages: usize) -> Vec<u8> {
        let kids: Vec<String> = (0..pages).map(|page| format!("{} 0 R", 3 + page)).collect();
        let mut objects = vec!["<</Type/Catalog/Pages 2 0 R>>".to_owned(), format!("<</Type/Pages/Kids[{}]/Count {pages}>>", kids.join(" "))];
        objects.extend((0..pages).map(|_| "<</Type/Page/Parent 2 0 R/MediaBox[0 0 595 842]>>".to_owned()));
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
        out
    }

    #[test]
    fn a_long_pdf_renders_its_first_pages_and_counts_them_all() {
        let rendered = render_pages(pdf(8)).unwrap();
        assert_eq!((rendered.pages.len(), rendered.total), (MAX_PAGES, 8));
        let short = render_pages(pdf(2)).unwrap();
        assert_eq!((short.pages.len(), short.total), (2, 2));
    }

    #[test]
    fn a_reason_never_only_says_the_pdf_could_not_be_rendered() {
        let reason = render_pages(b"%PDF-1.4\nnothing else".to_vec()).err().unwrap();
        assert_eq!(reason, "The file is not a PDF the renderer can read.");
    }
}
