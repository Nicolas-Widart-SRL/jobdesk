use std::path::Path;

/// One rendered page as straight RGBA bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub trait PageRasterizer: Send + Sync {
    fn render(&self, path: &Path) -> Result<Vec<PageImage>, String>;
}
