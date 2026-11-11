//! The Ledger design system: tokens and generic widgets. Colour is for job status, plus one stamp ink for what waits on the user. The window is a gridded desk in both themes, and anything read in rows sits on paper.
//! Every colour, measure and painted shape lives here. No widget knows about a feature.

mod controls;
mod fonts;
mod marks;
mod pages;
mod palette;
mod setup;
mod surfaces;
mod text;
pub mod tokens;
mod tray;
mod wizard;

pub use controls::*;
pub use fonts::{Face, install_fonts};
pub use marks::*;
pub use pages::*;
pub use palette::{Ink, Palette, Tone, install_theme};
pub use setup::*;
pub use surfaces::*;
pub use text::*;
pub use tokens::Type;
pub use tray::*;
pub use wizard::*;
