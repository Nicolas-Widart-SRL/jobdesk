use crate::shared_kernel::ApiKey;

const MASK: &str = "••••••••";
const PREFIX: &str = "sk-ant-";
/// The last four show only when the secret part of the key, after the public `sk-ant-` prefix, has this many characters.
/// So at most a third of the secret is ever on the screen.
const LONG_ENOUGH: usize = 12;

/// What the screen may show of a saved key: whether it has the usual prefix, and the last four characters of a long key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMask {
    prefixed: bool,
    last_four: Option<String>,
}

impl KeyMask {
    pub fn of(key: &ApiKey) -> Self {
        let text = key.expose();
        let prefixed = text.starts_with(PREFIX);
        let count = text.chars().count();
        let secret = if prefixed { count - PREFIX.len() } else { count };
        let last_four = (secret >= LONG_ENOUGH).then(|| text.chars().skip(count - 4).collect());
        Self { prefixed, last_four }
    }

    /// The dots, after `sk-ant-` when the key has that prefix.
    pub fn shown(&self) -> String {
        if self.prefixed { format!("{PREFIX}{MASK}") } else { MASK.to_owned() }
    }

    /// The last four characters, for a key long enough to show them.
    pub fn ending(&self) -> Option<&str> {
        self.last_four.as_deref()
    }

    /// The mask as one piece of text: the dots, then the last four when shown.
    pub fn text(&self) -> String {
        format!("{}{}", self.shown(), self.ending().unwrap_or_default())
    }
}

/// `text` with every occurrence of the key replaced by its mask.
pub fn redacted(text: &str, key: Option<&ApiKey>) -> String {
    match key {
        Some(key) => text.replace(key.expose(), &KeyMask::of(key).text()),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-ant-api03-FAKE-not-a-real-key-0000-R2kd";

    fn mask_of(text: &str) -> KeyMask {
        KeyMask::of(&ApiKey::parse(text).unwrap())
    }

    #[test]
    fn a_long_key_shows_only_its_last_four() {
        let mask = mask_of(FAKE);
        assert_eq!(mask.shown(), "sk-ant-••••••••");
        assert_eq!(mask.ending(), Some("R2kd"));
        assert_eq!(mask.text(), "sk-ant-••••••••R2kd");
        assert!(!format!("{mask:?}{}", mask.shown()).contains("api03"));
    }

    #[test]
    fn a_short_key_shows_no_characters() {
        let mask = mask_of("sk-ant-1234");
        assert_eq!(mask.shown(), "sk-ant-••••••••");
        assert_eq!(mask.ending(), None);
        assert_eq!(mask.text(), "sk-ant-••••••••");
        assert_eq!(mask_of("abc").ending(), None);
        assert!(!format!("{mask:?}").contains("1234"));
    }

    #[test]
    fn twelve_secret_characters_are_long_enough() {
        assert_eq!(mask_of("FAKE12345678").ending(), Some("5678"));
        assert_eq!(mask_of("FAKE1234567").ending(), None);
    }

    #[test]
    fn the_public_prefix_does_not_count_toward_the_secret() {
        assert_eq!(mask_of("sk-ant-12345").ending(), None, "five secret characters");
        assert_eq!(mask_of("sk-ant-12345678901").ending(), None, "eleven secret characters");
        assert_eq!(mask_of("sk-ant-123456789012").ending(), Some("9012"));
    }

    #[test]
    fn an_unprefixed_key_gets_only_dots() {
        let mask = mask_of("FAKE-not-a-real-key-0000-R2kd");
        assert_eq!(mask.shown(), "••••••••");
        assert_eq!(mask.ending(), Some("R2kd"));
    }

    #[test]
    fn every_occurrence_of_the_key_becomes_its_mask() {
        let key = ApiKey::parse(FAKE).unwrap();
        let text = redacted(&format!("bad key {FAKE}, again {FAKE}."), Some(&key));
        assert_eq!(text, "bad key sk-ant-••••••••R2kd, again sk-ant-••••••••R2kd.");
        assert_eq!(redacted("nothing here", Some(&key)), "nothing here");
        assert_eq!(redacted(FAKE, None), FAKE);
    }
}
