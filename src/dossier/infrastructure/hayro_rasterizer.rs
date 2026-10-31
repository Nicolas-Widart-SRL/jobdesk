use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

use crate::dossier::domain::{PageImage, PageRasterizer};

const SCALE: f32 = 2.0;
const MAX_PAGES: usize = 6;

/// Pure Rust, so no PDF library ships beside the app.
pub struct HayroRasterizer;

impl PageRasterizer for HayroRasterizer {
    fn render(&self, path: &Path) -> Result<Vec<PageImage>, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        panic::catch_unwind(AssertUnwindSafe(|| render_pages(bytes))).unwrap_or_else(|_| Err("This PDF could not be rendered.".to_owned()))
    }
}

fn render_pages(bytes: Vec<u8>) -> Result<Vec<PageImage>, String> {
    let pdf = Pdf::new(Arc::new(bytes)).map_err(|_| "This PDF could not be read.".to_owned())?;
    let cache = RenderCache::new();
    let settings = InterpreterSettings::default();
    let pixmap_settings = PixmapSettings { x_scale: SCALE, y_scale: SCALE, bg_color: WHITE };
    let pages = pdf.pages();
    Ok(pages
        .iter()
        .take(MAX_PAGES)
        .map(|page| {
            let pixmap = render(page, &cache, &settings, &RenderSettings::default(), &pixmap_settings);
            PageImage { width: usize::from(pixmap.width()), height: usize::from(pixmap.height()), rgba: pixmap.data_as_u8_slice().to_vec() }
        })
        .collect())
}
