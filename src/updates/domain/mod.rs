mod relaunch;
mod release;
mod state;
mod version;

pub use relaunch::relaunch_args;
pub use release::{
    DownloadEnd, FEED_URL, Failure, InstallBlock, Platform, REPOSITORY, Release, RestartPlan, ServiceEvent, UpdateService, asset_url, release_page,
};
pub use state::{CHECK_EVERY, Check, Download, UpdateState, due, percent};
pub use version::Version;
