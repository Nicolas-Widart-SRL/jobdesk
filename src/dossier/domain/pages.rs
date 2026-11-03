use std::path::Path;

/// One rendered page as straight RGBA bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct PageImage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// The pages a render gave, which may stop short of the end, and how many pages the document holds.
#[derive(Clone, PartialEq, Eq)]
pub struct RenderedPages {
    pub pages: Vec<PageImage>,
    pub total: usize,
}

pub trait PageRasterizer: Send + Sync {
    fn render(&self, path: &Path) -> Result<RenderedPages, String>;
}
