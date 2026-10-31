use chrono::NaiveDate;

/// Stored dates come from many portals, so anything but `YYYY-MM-DD` reads as absent.
pub fn parse_iso_date(raw: &str) -> Option<NaiveDate> {
    let raw = raw.trim();
    (raw.len() == 10).then(|| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()).flatten()
}

/// Every `YYYY-MM-DD` embedded in free text, in order of appearance.
pub fn iso_dates_in(text: &str) -> Vec<NaiveDate> {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(9))
        .filter(|&i| text.is_char_boundary(i) && text.is_char_boundary(i + 10))
        .filter(|&i| i == 0 || !bytes[i - 1].is_ascii_digit())
        .filter(|&i| bytes.get(i + 10).is_none_or(|b| !b.is_ascii_digit()))
        .filter_map(|i| parse_iso_date(&text[i..i + 10]))
        .collect()
}

pub fn days_from(earlier: NaiveDate, later: NaiveDate) -> i64 {
    (later - earlier).num_days()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shapes_portals_have_shipped() {
        for bad in ["ASAP", "31.10.2026", "2026-13-01", "2026-10-1", ""] {
            assert_eq!(parse_iso_date(bad), None, "{bad}");
        }
        assert!(parse_iso_date(" 2026-10-31 ").is_some());
    }

    #[test]
    fn finds_dates_in_notes() {
        let found = iso_dates_in("followed up 2026-10-01; é call 2026-10-09, id 12026-10-091");
        assert_eq!(found, vec![
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
        ]);
    }
}
