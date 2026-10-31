/// The tracker vocabulary from `/outcome`, including its legacy space spellings on read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationStatus {
    Drafted,
    Applied,
    Interview,
    Offer,
    Hired,
    Rejected,
    NoResponse,
    OfferDeclined,
    Withdrawn,
    Other(String),
}

impl ApplicationStatus {
    pub fn parse(raw: &str) -> Self {
        let normal = raw.trim().to_lowercase().replace(' ', "_");
        match normal.as_str() {
            "drafted" => Self::Drafted,
            "applied" => Self::Applied,
            "interview" => Self::Interview,
            "offer" => Self::Offer,
            "hired" => Self::Hired,
            "rejected" => Self::Rejected,
            "no_response" => Self::NoResponse,
            "offer_declined" => Self::OfferDeclined,
            "withdrawn" => Self::Withdrawn,
            _ => Self::Other(raw.trim().to_owned()),
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(self, Self::Hired | Self::Rejected | Self::NoResponse | Self::OfferDeclined | Self::Withdrawn)
    }

    /// `offer_declined` and `withdrawn` were the candidate's choice, so they are not rejections.
    pub fn is_rejection(&self) -> bool {
        matches!(self, Self::Rejected | Self::NoResponse)
    }

    pub fn bucket(&self) -> StatusBucket {
        match self {
            Self::Drafted => StatusBucket::Drafted,
            Self::Applied => StatusBucket::Active,
            Self::Interview => StatusBucket::Interview,
            Self::Offer => StatusBucket::Offer,
            Self::Hired => StatusBucket::Hired,
            Self::Rejected | Self::NoResponse | Self::OfferDeclined | Self::Withdrawn | Self::Other(_) => StatusBucket::Closed,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Drafted => "Drafted",
            Self::Applied => "Applied",
            Self::Interview => "Interview",
            Self::Offer => "Offer",
            Self::Hired => "Hired",
            Self::Rejected => "Rejected",
            Self::NoResponse => "No response",
            Self::OfferDeclined => "Offer declined",
            Self::Withdrawn => "Withdrawn",
            Self::Other(raw) => raw,
        }
    }
}

/// The six groups `/html-report` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatusBucket {
    Drafted,
    Active,
    Interview,
    Offer,
    Hired,
    Closed,
}

impl StatusBucket {
    pub const ALL: [Self; 6] = [Self::Drafted, Self::Active, Self::Interview, Self::Offer, Self::Hired, Self::Closed];

    pub fn label(self) -> &'static str {
        match self {
            Self::Drafted => "Drafted",
            Self::Active => "Applied",
            Self::Interview => "Interview",
            Self::Offer => "Offer",
            Self::Hired => "Hired",
            Self::Closed => "Closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_legacy_spellings_and_case() {
        assert_eq!(ApplicationStatus::parse("No Response"), ApplicationStatus::NoResponse);
        assert_eq!(ApplicationStatus::parse("offer declined"), ApplicationStatus::OfferDeclined);
        assert!(ApplicationStatus::parse("no response").is_final());
    }

    #[test]
    fn unknown_status_counts_as_closed_like_the_report() {
        let status = ApplicationStatus::parse("ghosted");
        assert_eq!(status.bucket(), StatusBucket::Closed);
        assert!(!status.is_final());
        assert_eq!(status.label(), "ghosted");
    }

    #[test]
    fn declined_offer_is_not_a_rejection() {
        assert!(!ApplicationStatus::OfferDeclined.is_rejection());
        assert!(!ApplicationStatus::Withdrawn.is_rejection());
        assert!(ApplicationStatus::NoResponse.is_rejection());
    }
}
