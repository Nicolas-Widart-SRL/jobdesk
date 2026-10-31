use std::fmt;

/// Why a source file could not be read. Missing optional files are not errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    pub source: String,
    pub reason: String,
}

impl LoadError {
    pub fn new(source: impl Into<String>, reason: impl ToString) -> Self {
        Self { source: source.into(), reason: reason.to_string() }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source, self.reason)
    }
}
