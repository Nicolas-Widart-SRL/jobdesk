pub mod cleanup;
mod download;
mod feed;
mod github;
mod http;
#[cfg(target_os = "macos")]
pub mod install_macos;
pub mod install_windows;
mod scripted;
mod signature;

pub use github::GithubUpdates;
pub use http::HttpsTransport;
pub use scripted::{IdleUpdates, ScriptedUpdates};
pub use signature::UpdateKey;
