//! The Profile screen: a rail of topics beside one topic on paper, and the editor for one block of it.

mod editor;
mod rail;
mod screen;

pub use screen::{Moment, ProfileRequest, ProfileScreen, show};
