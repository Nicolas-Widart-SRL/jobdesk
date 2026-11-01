//! The Ledger design system: tokens and generic widgets. Colour is for job status, plus one stamp ink for what waits on the user. The frame is a fixed navy in both themes.
//! Every colour, measure and painted shape lives here. No widget knows about a feature.

mod controls;
mod fonts;
mod marks;
mod palette;
mod surfaces;
mod text;
pub mod tokens;
mod tray;

pub use controls::*;
pub use fonts::{Face, install_fonts};
pub use marks::*;
pub use palette::{Ink, Palette, Tone, install_theme};
pub use surfaces::*;
pub use text::*;
pub use tokens::Type;
pub use tray::*;
