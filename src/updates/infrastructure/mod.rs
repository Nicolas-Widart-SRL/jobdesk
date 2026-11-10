pub mod cleanup;
mod download;
mod feed;
mod github;
#[cfg(target_os = "macos")]
pub mod install_macos;
pub mod install_windows;
mod scripted;
mod signature;

pub use github::{GithubSourcePins, GithubUpdates};
pub use scripted::{IdleUpdates, ScriptedUpdates};
pub use signature::UpdateKey;

use crate::shared_kernel::https::HttpsFailure;
use crate::updates::domain::Failure;

/// The update's word for each way a request can fail.
impl From<HttpsFailure> for Failure {
    fn from(failure: HttpsFailure) -> Failure {
        match failure {
            HttpsFailure::TimedOut => Failure::TimedOut,
            HttpsFailure::Unreachable => Failure::Unreachable,
            HttpsFailure::Refused(code) => Failure::Refused(code),
            HttpsFailure::Interrupted => Failure::Interrupted,
        }
    }
}
