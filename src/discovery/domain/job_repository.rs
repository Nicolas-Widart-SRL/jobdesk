use crate::discovery::domain::Job;
use crate::shared_kernel::LoadError;

pub trait JobRepository {
    fn all(&self) -> Result<Vec<Job>, LoadError>;
}
