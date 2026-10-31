use chrono::NaiveDate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Strong,
    Good,
    Moderate,
    Weak,
    Poor,
}

impl Verdict {
    /// Bands from the evaluation framework in `04-job-evaluation.md`.
    pub fn from_score(score: u8) -> Self {
        match score {
            75.. => Self::Strong,
            60..=74 => Self::Good,
            45..=59 => Self::Moderate,
            30..=44 => Self::Weak,
            _ => Self::Poor,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "strong fit" => Some(Self::Strong),
            "good fit" => Some(Self::Good),
            "moderate fit" => Some(Self::Moderate),
            "weak fit" => Some(Self::Weak),
            "poor fit" => Some(Self::Poor),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Strong => "Strong Fit",
            Self::Good => "Good Fit",
            Self::Moderate => "Moderate Fit",
            Self::Weak => "Weak Fit",
            Self::Poor => "Poor Fit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Pass,
    Flag,
    Fail,
}

impl Gate {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "PASS" => Some(Self::Pass),
            "FLAG" => Some(Self::Flag),
            "FAIL" => Some(Self::Fail),
            _ => None,
        }
    }
}

/// What `/rank` concluded about a posting. Dimension scores are not persisted by `/rank`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankAssessment {
    pub score: u8,
    pub verdict: Verdict,
    pub location: Gate,
    pub language: Gate,
    pub language_note: Option<String>,
    pub strengths: Vec<String>,
    pub gaps: Vec<String>,
    pub ranked_on: Option<NaiveDate>,
}

impl RankAssessment {
    pub fn is_vetoed(&self) -> bool {
        self.location == Gate::Fail || self.language == Gate::Fail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_bands_match_the_framework() {
        let cases = [(75, Verdict::Strong), (74, Verdict::Good), (60, Verdict::Good), (59, Verdict::Moderate), (44, Verdict::Weak), (29, Verdict::Poor)];
        for (score, verdict) in cases {
            assert_eq!(Verdict::from_score(score), verdict);
        }
    }
}
