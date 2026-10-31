use chrono::NaiveDate;

use crate::shared_kernel::dates::parse_iso_date;

/// The tracker's `date` column, which may hold a full date, a bare year or anything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedOn {
    Date(NaiveDate),
    Year(i32),
    Unknown(String),
}

impl AppliedOn {
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if let Some(date) = parse_iso_date(raw) {
            return Self::Date(date);
        }
        match raw.parse() {
            Ok(year) if raw.len() == 4 => Self::Year(year),
            _ => Self::Unknown(raw.to_owned()),
        }
    }

    pub fn date(&self) -> Option<NaiveDate> {
        match self {
            Self::Date(date) => Some(*date),
            _ => None,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Date(date) => date.to_string(),
            Self::Year(year) => year.to_string(),
            Self::Unknown(raw) => raw.clone(),
        }
    }
}
