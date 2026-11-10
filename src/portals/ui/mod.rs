//! The Portals screen: the installed portals on a slip, grouped On and Off, beside the picked one's detail on paper.

mod detail;
mod rail;
mod screen;

pub use screen::{BUSY, PortalsRequest, PortalsScreen, show};
