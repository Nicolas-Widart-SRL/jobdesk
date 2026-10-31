use crate::shared_kernel::LoadError;
use crate::tracking::domain::TrackedApplication;

pub trait ApplicationRepository {
    fn all(&self) -> Result<Vec<TrackedApplication>, LoadError>;
}
