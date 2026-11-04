use std::fmt;

/// A release number: exactly three whole parts, so the order of two releases is never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Reads `0.2.0` or `v0.2.0`. A suffix such as `-beta`, a missing part or a fourth part gives `None`.
    pub fn parse(text: &str) -> Option<Version> {
        let digits = text.strip_prefix('v').unwrap_or(text);
        let mut parts = digits.split('.');
        let mut part = || {
            let part = parts.next()?;
            (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())).then(|| part.parse::<u32>().ok()).flatten()
        };
        let version = Version { major: part()?, minor: part()?, patch: part()? };
        parts.next().is_none().then_some(version)
    }

    /// The git tag of this release: `v0.2.0`.
    pub fn tag(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u32, minor: u32, patch: u32) -> Version {
        Version { major, minor, patch }
    }

    #[test]
    fn a_version_reads_with_or_without_the_v() {
        assert_eq!(Version::parse("0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(Version::parse("v0.2.0"), Some(v(0, 2, 0)));
        assert_eq!(Version::parse("v10.20.30"), Some(v(10, 20, 30)));
    }

    #[test]
    fn a_version_that_is_not_three_plain_numbers_is_refused() {
        for bad in [
            "",
            "v",
            "v1.2",
            "1.2",
            "1.2.3.4",
            "1.2.3-beta",
            "01.2.3x",
            "1.2.x",
            "1..3",
            "+1.2.3",
            "-1.2.3",
            "V1.2.3",
            "vv1.2.3",
            " 1.2.3",
            "1.2.3 ",
            "1.2.99999999999",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn versions_order_by_number_not_by_text() {
        assert!(v(0, 10, 0) > v(0, 9, 9));
        assert!(v(1, 0, 0) > v(0, 99, 99));
        assert!(v(0, 2, 1) > v(0, 2, 0));
        assert_eq!(v(0, 2, 0), Version::parse("v0.2.0").unwrap());
    }

    #[test]
    fn a_version_shows_and_tags_itself() {
        assert_eq!(v(0, 2, 0).to_string(), "0.2.0");
        assert_eq!(v(0, 2, 0).tag(), "v0.2.0");
    }
}
