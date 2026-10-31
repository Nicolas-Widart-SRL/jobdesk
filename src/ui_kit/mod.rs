//! Design tokens and generic widgets. Colour is reserved for data; chrome stays neutral.

mod fonts;
mod palette;
mod widgets;

pub use fonts::{install_fonts, semibold};
pub use palette::{Palette, install_theme};
pub use widgets::*;
