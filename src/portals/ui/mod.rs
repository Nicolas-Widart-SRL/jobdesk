//! The Portals screen: the installed portals on a slip, grouped On and Off, beside the picked one's detail on paper,
//! then the community list to browse and the form that has Claude build a portal.

mod add;
mod browse;
mod detail;
mod rail;
mod screen;

pub use browse::BrowseContext;
pub use screen::{BUSY, FOLDER_UPDATING, PortalsRequest, PortalsScreen, Segment, show};
