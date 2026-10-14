//! Inputs YouTube reads accept, published once for every caller.
//!
//! The client refuses anything else before a request. Callers above the
//! facade check their input with these functions and name page sizes with
//! these constants instead of keeping copies.

/// The smallest search page size YouTube accepts.
pub const SEARCH_MIN_COUNT: u8 = 1;
/// The largest search page size YouTube accepts.
pub const SEARCH_MAX_COUNT: u8 = 50;
/// The search page size used when a caller names none.
pub const SEARCH_DEFAULT_COUNT: u8 = 20;

/// Whether a search query names anything. Only the blank check trims; the
/// query is sent as given.
pub fn accepts_search_query(query: &str) -> bool {
    !query.trim().is_empty()
}

/// Whether YouTube accepts `count` as a search page size.
pub fn accepts_search_count(count: u8) -> bool {
    (SEARCH_MIN_COUNT..=SEARCH_MAX_COUNT).contains(&count)
}

/// The one spelling of a search publication bound this client sends: an
/// RFC 3339 UTC instant. YouTube's `publishedAfter` accepts other RFC 3339
/// forms; accepting only this one keeps a single spelling of the same bound,
/// so two equivalent requests cannot compile into two different confirmed
/// search-plan scopes.
pub const PUBLISHED_AFTER_FORMAT: &str = "YYYY-MM-DDTHH:MM:SSZ";

/// Whether `value` is a publication bound in [`PUBLISHED_AFTER_FORMAT`].
///
/// The calendar is checked, so `2026-02-30T00:00:00Z` is refused here rather
/// than by YouTube.
pub fn accepts_published_after(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 20 {
        return false;
    }
    let punctuation = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if punctuation
        .iter()
        .any(|(index, expected)| bytes[*index] != *expected)
    {
        return false;
    }
    if bytes[19] != b'Z' {
        return false;
    }
    if bytes.iter().enumerate().any(|(index, byte)| {
        !punctuation.iter().any(|(at, _)| *at == index) && index != 19 && !byte.is_ascii_digit()
    }) {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        value[0..4].parse::<u16>(),
        value[5..7].parse::<u8>(),
        value[8..10].parse::<u8>(),
    ) else {
        return false;
    };
    let (Ok(hour), Ok(minute), Ok(second)) = (
        value[11..13].parse::<u8>(),
        value[14..16].parse::<u8>(),
        value[17..19].parse::<u8>(),
    ) else {
        return false;
    };
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    // A leap second is the one second past 59 a real timestamp may name.
    (1..=days_in_month).contains(&day) && hour <= 23 && minute <= 59 && second <= 60
}

/// Whether `value` is a stable channel ID: `UC` and 22 more ASCII letters,
/// digits, `_` or `-`. Handles are presentation aliases, never channel IDs.
pub fn accepts_channel_id(value: &str) -> bool {
    value.starts_with("UC")
        && value.len() == 24
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_page_sizes_are_bounded_at_the_published_constants() {
        assert_eq!(
            (SEARCH_MIN_COUNT, SEARCH_MAX_COUNT, SEARCH_DEFAULT_COUNT),
            (1, 50, 20)
        );
        assert!(!accepts_search_count(SEARCH_MIN_COUNT - 1));
        assert!(accepts_search_count(SEARCH_MIN_COUNT));
        assert!(accepts_search_count(SEARCH_DEFAULT_COUNT));
        assert!(accepts_search_count(SEARCH_MAX_COUNT));
        assert!(!accepts_search_count(SEARCH_MAX_COUNT + 1));
        assert!(!accepts_search_count(u8::MAX));
    }

    #[test]
    fn a_search_query_is_refused_only_when_blank() {
        assert!(accepts_search_query("  rust  "));
        assert!(accepts_search_query("x"));
        for blank in ["", " ", " \n\t"] {
            assert!(!accepts_search_query(blank), "{blank:?}");
        }
    }

    #[test]
    fn a_publication_bound_is_one_rfc_3339_utc_instant_with_a_real_calendar_date() {
        assert_eq!(PUBLISHED_AFTER_FORMAT, "YYYY-MM-DDTHH:MM:SSZ");
        for accepted in [
            "2026-09-16T00:00:00Z",
            "2024-02-29T23:59:59Z",
            "1970-01-01T00:00:00Z",
            "2016-12-31T23:59:60Z",
        ] {
            assert!(accepts_published_after(accepted), "{accepted}");
        }
        for refused in [
            "",
            "2026-09-16",
            // Offsets and fractional seconds are RFC 3339 but not this form.
            "2026-09-16T00:00:00+03:00",
            "2026-09-16T00:00:00.000Z",
            "2026-09-16T00:00:00z",
            "2026-09-16 00:00:00Z",
            // A date the calendar does not have.
            "2026-02-30T00:00:00Z",
            "2025-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-01T00:00:00Z",
            "2026-09-00T00:00:00Z",
            "2026-09-16T24:00:00Z",
            "2026-09-16T00:60:00Z",
            "2026-09-16T00:00:61Z",
            " 2026-09-16T00:00:00Z",
            "2026-09-16T00:00:00Z ",
        ] {
            assert!(!accepts_published_after(refused), "{refused:?}");
        }
    }

    #[test]
    fn channel_id_lookup_accepts_only_stable_ids() {
        assert!(accepts_channel_id("UC_x5XG1OV2P6uZZ5FSM9Ttw"));
        assert!(accepts_channel_id("UC-x5XG1OV2P6uZZ5FSM9Ttw"));
        for invalid in [
            "@creator",
            "UC_too_short",
            "",
            "not-a-channel-id",
            "UC_x5XG1OV2P6uZZ5FSM9Ttwx",
            "uc_x5XG1OV2P6uZZ5FSM9Ttw",
            "UC_x5XG1OV2P6uZZ5FSM9Tt ",
        ] {
            assert!(!accepts_channel_id(invalid), "{invalid:?}");
        }
    }
}
