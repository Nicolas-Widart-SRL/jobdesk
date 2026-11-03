//! The Profile screen: a rail of topics beside one topic on paper, and the editor for one block of it. Under the topics,
//! the CV documents built from the profile.

mod cv;
mod editor;
mod rail;
mod screen;

pub use screen::{Away, Moment, ProfileRequest, ProfileScreen, show};
