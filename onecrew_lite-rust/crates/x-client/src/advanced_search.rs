use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_LIST_ITEMS: usize = 20;
const MAX_ATOM_CHARS: usize = 64;
const MAX_PHRASE_CHARS: usize = 256;
const MAX_COMPILED_QUERY_CHARS: usize = 512;

/// Smallest page size an advanced search accepts.
pub const ADVANCED_SEARCH_MIN_COUNT: u16 = 1;
/// Largest page size an advanced search accepts.
pub const ADVANCED_SEARCH_MAX_COUNT: u16 = 100;
/// Page size of an advanced search that names none.
pub const ADVANCED_SEARCH_DEFAULT_COUNT: u16 = 20;

/// Whether `count` is inside the accepted advanced-search page-size range.
pub fn accepts_advanced_search_count(count: u16) -> bool {
    (ADVANCED_SEARCH_MIN_COUNT..=ADVANCED_SEARCH_MAX_COUNT).contains(&count)
}

static HANDLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_]{1,15}$").expect("static handle regex"));
static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{L}\p{M}\p{N}_]+(?:[-'’][\p{L}\p{M}\p{N}_]+)*$")
        .expect("static Unicode word regex")
});
static HASHTAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\p{L}\p{M}\p{N}_]+$").expect("static Unicode hashtag regex"));
static LANGUAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-z]{2,3}(?:-[a-z0-9]{2,8})?$").expect("static language regex")
});

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancedSearchFilter {
    #[default]
    Any,
    Include,
    Exclude,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvancedSearchResultMode {
    #[default]
    Top,
    Latest,
}

impl AdvancedSearchResultMode {
    pub(crate) fn product(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Latest => "Latest",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdvancedSearchCriteria {
    pub all_words: Vec<String>,
    pub exact_phrase: Option<String>,
    pub any_words: Vec<String>,
    pub exclude_words: Vec<String>,
    pub hashtags: Vec<String>,
    pub from_accounts: Vec<String>,
    pub to_accounts: Vec<String>,
    pub mentioning_accounts: Vec<String>,
    pub language: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub min_replies: Option<u32>,
    pub min_likes: Option<u32>,
    pub min_reposts: Option<u32>,
    pub replies: AdvancedSearchFilter,
    pub links: AdvancedSearchFilter,
}

impl AdvancedSearchCriteria {
    /// Validate X's selectors, filters and date range without opening a session.
    ///
    /// # Errors
    /// Returns an input error if the criteria cannot form an X search query.
    pub fn validate(&self) -> Result<(), crate::XClientError> {
        compile_advanced_search(self)
            .map(|_| ())
            .map_err(|error| crate::XClientError::Input(error.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdvancedSearchPage {
    pub effective_query: String,
    pub page: crate::TimelinePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum AdvancedSearchValidationError {
    #[error("advanced search requires at least one positive selector")]
    MissingPositiveSelector,
    #[error("advanced search field `{field}` has too many values")]
    TooManyValues { field: &'static str },
    #[error("advanced search field `{field}` contains an invalid value")]
    InvalidValue { field: &'static str },
    #[error("advanced search date range is reversed")]
    ReversedDateRange,
    #[error("compiled advanced search query is too long")]
    QueryTooLong,
}

pub(crate) fn compile_advanced_search(
    criteria: &AdvancedSearchCriteria,
) -> Result<String, AdvancedSearchValidationError> {
    let mut terms = Vec::new();

    let all_words = checked_atoms("all_words", &criteria.all_words)?;
    terms.extend(all_words.iter().cloned());

    let exact_phrase = criteria
        .exact_phrase
        .as_deref()
        .map(|value| checked_phrase("exact_phrase", value))
        .transpose()?;
    if let Some(phrase) = &exact_phrase {
        terms.push(format!("\"{phrase}\""));
    }

    let any_words = checked_atoms("any_words", &criteria.any_words)?;
    push_or_group(&mut terms, any_words.iter().cloned());

    let exclude_words = checked_atoms("exclude_words", &criteria.exclude_words)?;
    terms.extend(exclude_words.iter().map(|value| format!("-{value}")));

    let hashtags = checked_prefixed_values("hashtags", &criteria.hashtags, '#', |value| {
        HASHTAG.is_match(value)
            && value
                .chars()
                .any(|character| character.is_alphanumeric() || character == '_')
    })?;
    push_or_group(&mut terms, hashtags.iter().map(|value| format!("#{value}")));

    let from_accounts = checked_handles("from_accounts", &criteria.from_accounts)?;
    push_or_group(
        &mut terms,
        from_accounts.iter().map(|value| format!("from:{value}")),
    );
    let to_accounts = checked_handles("to_accounts", &criteria.to_accounts)?;
    push_or_group(
        &mut terms,
        to_accounts.iter().map(|value| format!("to:{value}")),
    );
    let mentioning_accounts =
        checked_handles("mentioning_accounts", &criteria.mentioning_accounts)?;
    push_or_group(
        &mut terms,
        mentioning_accounts.iter().map(|value| format!("@{value}")),
    );

    let has_positive_selector = !all_words.is_empty()
        || exact_phrase.is_some()
        || !any_words.is_empty()
        || !hashtags.is_empty()
        || !from_accounts.is_empty()
        || !to_accounts.is_empty()
        || !mentioning_accounts.is_empty();
    if !has_positive_selector {
        return Err(AdvancedSearchValidationError::MissingPositiveSelector);
    }

    if let Some(language) = criteria.language.as_deref() {
        let language = language.trim();
        if language != criteria.language.as_deref().unwrap_or_default()
            || language != language.to_ascii_lowercase()
            || !LANGUAGE.is_match(language)
        {
            return Err(AdvancedSearchValidationError::InvalidValue { field: "language" });
        }
        terms.push(format!("lang:{language}"));
    }

    push_filter(&mut terms, "replies", criteria.replies);
    push_filter(&mut terms, "links", criteria.links);

    if let Some(value) = criteria.min_replies {
        terms.push(format!("min_replies:{value}"));
    }
    if let Some(value) = criteria.min_likes {
        terms.push(format!("min_faves:{value}"));
    }
    if let Some(value) = criteria.min_reposts {
        terms.push(format!("min_retweets:{value}"));
    }

    let since = criteria
        .since
        .as_deref()
        .map(|value| checked_date("since", value))
        .transpose()?;
    let until = criteria
        .until
        .as_deref()
        .map(|value| checked_date("until", value))
        .transpose()?;
    if since.zip(until).is_some_and(|(start, end)| start > end) {
        return Err(AdvancedSearchValidationError::ReversedDateRange);
    }
    if let Some(value) = criteria.since.as_deref() {
        terms.push(format!("since:{value}"));
    }
    if let Some(value) = criteria.until.as_deref() {
        terms.push(format!("until:{value}"));
    }

    let query = terms.join(" ");
    if query.chars().count() > MAX_COMPILED_QUERY_CHARS {
        return Err(AdvancedSearchValidationError::QueryTooLong);
    }
    Ok(query)
}

/// Checks word-list values. A bare value is one atom. A value of words joined
/// by single spaces compiles as a double-quoted phrase, where X reads every
/// word literally, so its words may be `and`/`or`; no word can carry an
/// operator, quote, or escape, so nothing reaches the query unquoted.
fn checked_atoms(
    field: &'static str,
    values: &[String],
) -> Result<Vec<String>, AdvancedSearchValidationError> {
    checked_list_len(field, values)?;
    values
        .iter()
        .map(|value| {
            let trimmed = value.trim();
            let phrase = trimmed.contains(' ');
            let words_valid = if phrase {
                trimmed.split(' ').all(is_phrase_word)
            } else {
                is_atom(trimmed)
            };
            if trimmed != value || trimmed.chars().count() > MAX_ATOM_CHARS || !words_valid {
                return Err(AdvancedSearchValidationError::InvalidValue { field });
            }
            if phrase {
                Ok(format!("\"{trimmed}\""))
            } else {
                Ok(trimmed.to_owned())
            }
        })
        .collect()
}

/// A bare term: a phrase word that X would not read as an operator.
fn is_atom(word: &str) -> bool {
    is_phrase_word(word)
        && !word.starts_with('-')
        && !word.eq_ignore_ascii_case("OR")
        && !word.eq_ignore_ascii_case("AND")
}

fn is_phrase_word(word: &str) -> bool {
    WORD.is_match(word)
        && word
            .chars()
            .any(|character| character.is_alphanumeric() || character == '_')
}

fn checked_phrase(
    field: &'static str,
    value: &str,
) -> Result<String, AdvancedSearchValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed != value
        || trimmed.chars().count() > MAX_PHRASE_CHARS
        || trimmed.chars().any(char::is_control)
        || trimmed
            .chars()
            .any(|character| matches!(character, '"' | '\\'))
    {
        return Err(AdvancedSearchValidationError::InvalidValue { field });
    }
    Ok(trimmed.to_owned())
}

fn checked_handles(
    field: &'static str,
    values: &[String],
) -> Result<Vec<String>, AdvancedSearchValidationError> {
    checked_prefixed_values(field, values, '@', |value| HANDLE.is_match(value))
}

fn checked_prefixed_values(
    field: &'static str,
    values: &[String],
    prefix: char,
    validate: impl Fn(&str) -> bool,
) -> Result<Vec<String>, AdvancedSearchValidationError> {
    checked_list_len(field, values)?;
    values
        .iter()
        .map(|value| {
            let trimmed = value.trim();
            if trimmed != value {
                return Err(AdvancedSearchValidationError::InvalidValue { field });
            }
            let normalized = trimmed.strip_prefix(prefix).unwrap_or(trimmed);
            if !validate(normalized) {
                return Err(AdvancedSearchValidationError::InvalidValue { field });
            }
            Ok(normalized.to_owned())
        })
        .collect()
}

fn checked_list_len<T>(
    field: &'static str,
    values: &[T],
) -> Result<(), AdvancedSearchValidationError> {
    if values.len() > MAX_LIST_ITEMS {
        return Err(AdvancedSearchValidationError::TooManyValues { field });
    }
    Ok(())
}

fn push_or_group(terms: &mut Vec<String>, values: impl IntoIterator<Item = String>) {
    let values = values.into_iter().collect::<Vec<_>>();
    match values.as_slice() {
        [] => {}
        [only] => terms.push(only.clone()),
        _ => terms.push(format!("({})", values.join(" OR "))),
    }
}

fn push_filter(terms: &mut Vec<String>, name: &str, filter: AdvancedSearchFilter) {
    match filter {
        AdvancedSearchFilter::Any => {}
        AdvancedSearchFilter::Include => terms.push(format!("filter:{name}")),
        AdvancedSearchFilter::Exclude => terms.push(format!("-filter:{name}")),
    }
}

fn checked_date(
    field: &'static str,
    value: &str,
) -> Result<(u16, u8, u8), AdvancedSearchValidationError> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(AdvancedSearchValidationError::InvalidValue { field });
    }
    let year = value[0..4]
        .parse::<u16>()
        .map_err(|_| AdvancedSearchValidationError::InvalidValue { field })?;
    let month = value[5..7]
        .parse::<u8>()
        .map_err(|_| AdvancedSearchValidationError::InvalidValue { field })?;
    let day = value[8..10]
        .parse::<u8>()
        .map_err(|_| AdvancedSearchValidationError::InvalidValue { field })?;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    };
    if year < 2006 || day == 0 || day > max_day {
        return Err(AdvancedSearchValidationError::InvalidValue { field });
    }
    Ok((year, month, day))
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_search_accepts_page_sizes_from_1_through_100_and_defaults_to_20() {
        assert_eq!(
            (
                ADVANCED_SEARCH_MIN_COUNT,
                ADVANCED_SEARCH_MAX_COUNT,
                ADVANCED_SEARCH_DEFAULT_COUNT
            ),
            (1, 100, 20)
        );
        for (count, accepted) in [(0, false), (1, true), (20, true), (100, true), (101, false)] {
            assert_eq!(accepts_advanced_search_count(count), accepted, "{count}");
        }
        assert!(accepts_advanced_search_count(ADVANCED_SEARCH_DEFAULT_COUNT));
    }

    fn complete_criteria() -> AdvancedSearchCriteria {
        AdvancedSearchCriteria {
            all_words: vec!["agent".into(), "platform".into()],
            exact_phrase: Some("local first".into()),
            any_words: vec!["rust".into(), "tauri".into()],
            exclude_words: vec!["hiring".into(), "giveaway".into()],
            hashtags: vec!["#AI".into(), "rustlang".into()],
            from_accounts: vec!["@OpenAI".into(), "github".into()],
            to_accounts: vec!["XDevelopers".into()],
            mentioning_accounts: vec!["rustlang".into()],
            language: Some("en".into()),
            since: Some("2026-08-01".into()),
            until: Some("2026-08-28".into()),
            min_replies: Some(1),
            min_likes: Some(10),
            min_reposts: Some(2),
            replies: AdvancedSearchFilter::Include,
            links: AdvancedSearchFilter::Exclude,
        }
    }

    #[test]
    fn compiler_golden_preserves_the_verified_x_web_dialect() {
        assert_eq!(
            compile_advanced_search(&complete_criteria()).unwrap(),
            "agent platform \"local first\" (rust OR tauri) -hiring -giveaway (#AI OR #rustlang) (from:OpenAI OR from:github) to:XDevelopers @rustlang lang:en filter:replies -filter:links min_replies:1 min_faves:10 min_retweets:2 since:2026-08-01 until:2026-08-28"
        );
    }

    #[test]
    fn compiler_golden_quotes_space_separated_word_values_as_phrases() {
        let live_criteria = AdvancedSearchCriteria {
            all_words: vec!["crypto".into()],
            any_words: vec![
                "RWA".into(),
                "tokenization".into(),
                "real-world assets".into(),
            ],
            exclude_words: vec!["giveaway".into(), "airdrop".into()],
            language: Some("tr".into()),
            since: Some("2026-08-01".into()),
            replies: AdvancedSearchFilter::Exclude,
            ..Default::default()
        };
        assert_eq!(
            compile_advanced_search(&live_criteria).unwrap(),
            "crypto (RWA OR tokenization OR \"real-world assets\") -giveaway -airdrop lang:tr -filter:replies since:2026-08-01"
        );

        let all_words_phrase = AdvancedSearchCriteria {
            all_words: vec!["security token".into(), "rwa".into()],
            ..Default::default()
        };
        assert_eq!(
            compile_advanced_search(&all_words_phrase).unwrap(),
            "\"security token\" rwa"
        );

        let exclude_phrase = AdvancedSearchCriteria {
            all_words: vec!["crypto".into()],
            exclude_words: vec!["real world".into()],
            ..Default::default()
        };
        assert_eq!(
            compile_advanced_search(&exclude_phrase).unwrap(),
            "crypto -\"real world\""
        );

        // Inside a quoted phrase X reads and/or literally.
        let connective_phrases = AdvancedSearchCriteria {
            all_words: vec!["research and development".into()],
            any_words: vec!["rock or roll".into(), "this OR that".into()],
            exclude_words: vec!["terms and conditions".into()],
            ..Default::default()
        };
        assert_eq!(
            compile_advanced_search(&connective_phrases).unwrap(),
            "\"research and development\" (\"rock or roll\" OR \"this OR that\") -\"terms and conditions\""
        );
    }

    #[test]
    fn filters_have_omit_include_and_exclude_forms() {
        for (replies, links, expected) in [
            (
                AdvancedSearchFilter::Any,
                AdvancedSearchFilter::Any,
                "topic",
            ),
            (
                AdvancedSearchFilter::Include,
                AdvancedSearchFilter::Include,
                "topic filter:replies filter:links",
            ),
            (
                AdvancedSearchFilter::Exclude,
                AdvancedSearchFilter::Exclude,
                "topic -filter:replies -filter:links",
            ),
        ] {
            let criteria = AdvancedSearchCriteria {
                all_words: vec!["topic".into()],
                replies,
                links,
                ..Default::default()
            };
            assert_eq!(compile_advanced_search(&criteria).unwrap(), expected);
        }
    }

    #[test]
    fn grammar_injection_and_invalid_identifiers_fail_closed() {
        for (field, value) in [
            ("all_words", "from:openai"),
            ("all_words", "two  words"),
            ("all_words", "tab\tseparated"),
            ("any_words", "OR"),
            ("any_words", "from:openai now"),
            ("any_words", "this  OR"),
            ("any_words", " or"),
            ("exclude_words", "keep -already"),
            ("exclude_words", "-already"),
            ("all_words", "quote\""),
            ("all_words", "slash\\"),
            ("all_words", "line\nfeed"),
        ] {
            let mut criteria = AdvancedSearchCriteria::default();
            match field {
                "all_words" => criteria.all_words.push(value.into()),
                "any_words" => criteria.any_words.push(value.into()),
                "exclude_words" => {
                    criteria.all_words.push("safe".into());
                    criteria.exclude_words.push(value.into());
                }
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    compile_advanced_search(&criteria),
                    Err(AdvancedSearchValidationError::InvalidValue { field: actual }) if actual == field
                ),
                "{field}: {value:?}"
            );
        }

        for criteria in [
            AdvancedSearchCriteria {
                from_accounts: vec!["not-a-handle".into()],
                ..Default::default()
            },
            AdvancedSearchCriteria {
                hashtags: vec!["two words".into()],
                ..Default::default()
            },
            AdvancedSearchCriteria {
                exact_phrase: Some(r#"unsafe " phrase"#.into()),
                ..Default::default()
            },
            AdvancedSearchCriteria {
                exact_phrase: Some(r"unsafe \ phrase".into()),
                ..Default::default()
            },
        ] {
            assert!(matches!(
                compile_advanced_search(&criteria),
                Err(AdvancedSearchValidationError::InvalidValue { .. })
            ));
        }
    }

    #[test]
    fn invalid_dates_language_empty_selector_and_bounds_are_rejected() {
        for (since, until) in [
            (Some("2026-02-30"), None),
            (Some("2026-08-29"), Some("2026-08-28")),
            (Some("2005-12-31"), None),
        ] {
            let criteria = AdvancedSearchCriteria {
                all_words: vec!["safe".into()],
                since: since.map(str::to_owned),
                until: until.map(str::to_owned),
                ..Default::default()
            };
            assert!(compile_advanced_search(&criteria).is_err());
        }

        let invalid_language = AdvancedSearchCriteria {
            all_words: vec!["safe".into()],
            language: Some("EN_us".into()),
            ..Default::default()
        };
        assert!(matches!(
            compile_advanced_search(&invalid_language),
            Err(AdvancedSearchValidationError::InvalidValue { field: "language" })
        ));
        for language in [" en ", "\ten", "en\n"] {
            let padded_language = AdvancedSearchCriteria {
                all_words: vec!["safe".into()],
                language: Some(language.into()),
                ..Default::default()
            };
            assert!(matches!(
                compile_advanced_search(&padded_language),
                Err(AdvancedSearchValidationError::InvalidValue { field: "language" })
            ));
        }

        assert_eq!(
            compile_advanced_search(&AdvancedSearchCriteria::default()),
            Err(AdvancedSearchValidationError::MissingPositiveSelector)
        );

        let too_many = AdvancedSearchCriteria {
            all_words: (0..=MAX_LIST_ITEMS)
                .map(|index| format!("w{index}"))
                .collect(),
            ..Default::default()
        };
        assert!(matches!(
            compile_advanced_search(&too_many),
            Err(AdvancedSearchValidationError::TooManyValues { field: "all_words" })
        ));

        let phrase_too_long = AdvancedSearchCriteria {
            exact_phrase: Some("x".repeat(MAX_PHRASE_CHARS + 1)),
            ..Default::default()
        };
        assert!(matches!(
            compile_advanced_search(&phrase_too_long),
            Err(AdvancedSearchValidationError::InvalidValue {
                field: "exact_phrase"
            })
        ));

        let too_long = AdvancedSearchCriteria {
            all_words: (0..MAX_LIST_ITEMS)
                .map(|index| format!("word{index:02}{}", "x".repeat(40)))
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            compile_advanced_search(&too_long),
            Err(AdvancedSearchValidationError::QueryTooLong)
        );
    }
}
