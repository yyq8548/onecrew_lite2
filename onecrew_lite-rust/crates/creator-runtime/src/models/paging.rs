use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Bounds for one local list read: the limit applied when the caller sends
/// none, and the ceiling a caller cannot exceed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLimits {
    pub default: u32,
    pub max: u32,
}

impl PageLimits {
    pub const fn new(default: u32, max: u32) -> Self {
        assert!(default >= 1);
        assert!(default <= max);
        Self { default, max }
    }

    /// Resolve a caller-requested limit into the bounded effective limit,
    /// following the mailbox-sync clamp precedent instead of rejecting.
    pub fn effective(&self, requested: Option<u32>) -> u32 {
        match requested {
            None => self.default,
            Some(requested) => requested.clamp(1, self.max),
        }
    }
}

/// One bounded page of summary items from a local list read.
///
/// `next_cursor` is present only when more items exist; it is opaque and
/// bound to the issuing query context, so it cannot be replayed against a
/// different filter set or resource.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Page<T> {
    pub items: Vec<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

const PAGE_CURSOR_VERSION: u64 = 1;
const INVALID_PAGE_CURSOR: &str = "page cursor is not valid for this query";

#[derive(Serialize, Deserialize)]
struct PageCursorEnvelope {
    v: u64,
    context: String,
    position: String,
}

/// Encode a keyset position into an opaque cursor bound to `context`.
///
/// `context` is the owning use case's canonical description of the query
/// (resource plus normalized filters). Every list read composes it the same
/// way for encode and decode, so a cursor from one query cannot continue a
/// different one.
pub fn encode_page_cursor(context: &str, position: &str) -> String {
    let envelope = PageCursorEnvelope {
        v: PAGE_CURSOR_VERSION,
        context: context.to_string(),
        position: position.to_string(),
    };
    let json = serde_json::to_string(&envelope).expect("page cursor envelope serializes");
    URL_SAFE_NO_PAD.encode(json)
}

/// Decode an opaque cursor, returning its keyset position only when it was
/// issued for the same query context. Every failure maps to one generic
/// message so internals and other queries' contexts never leak.
pub fn decode_page_cursor(cursor: &str, expected_context: &str) -> Result<String, String> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.trim())
        .map_err(|_| INVALID_PAGE_CURSOR.to_string())?;
    let envelope: PageCursorEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| INVALID_PAGE_CURSOR.to_string())?;
    if envelope.v != PAGE_CURSOR_VERSION || envelope.context != expected_context {
        return Err(INVALID_PAGE_CURSOR.to_string());
    }
    Ok(envelope.position)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: PageLimits = PageLimits::new(25, 100);

    #[test]
    fn effective_limit_defaults_clamps_and_floors() {
        assert_eq!(LIMITS.effective(None), 25);
        assert_eq!(LIMITS.effective(Some(40)), 40);
        assert_eq!(LIMITS.effective(Some(0)), 1);
        assert_eq!(LIMITS.effective(Some(100)), 100);
        assert_eq!(LIMITS.effective(Some(5_000)), 100);
    }

    #[test]
    fn cursor_round_trips_within_one_query_context() {
        let cursor = encode_page_cursor("creator_list|stage=engaged", "1699999999|abc");
        assert_eq!(
            decode_page_cursor(&cursor, "creator_list|stage=engaged").as_deref(),
            Ok("1699999999|abc")
        );
    }

    #[test]
    fn cursor_is_rejected_for_a_different_query_context() {
        let cursor = encode_page_cursor("creator_list|stage=engaged", "1699999999|abc");
        let error = decode_page_cursor(&cursor, "creator_list|stage=prospect")
            .expect_err("cross-context cursor must be rejected");
        assert_eq!(error, INVALID_PAGE_CURSOR);
        assert!(!error.contains("active"), "context must not leak: {error}");
    }

    #[test]
    fn malformed_and_foreign_cursors_are_rejected_generically() {
        for garbage in [
            "",
            "not-base64!",
            "aGVsbG8",
            &URL_SAFE_NO_PAD.encode("{\"v\":99}"),
        ] {
            let error =
                decode_page_cursor(garbage, "any").expect_err("malformed cursor must be rejected");
            assert_eq!(error, INVALID_PAGE_CURSOR);
        }
    }

    #[test]
    fn cursor_is_opaque_on_the_wire() {
        let cursor = encode_page_cursor("creator_list", "position-key");
        assert!(!cursor.contains("creator_list"));
        assert!(!cursor.contains("position-key"));
    }

    #[test]
    fn page_serialization_omits_an_absent_next_cursor() {
        let page = Page::new(vec![1_u32, 2], None);
        let json = serde_json::to_value(&page).expect("page serializes");
        assert_eq!(json, serde_json::json!({ "items": [1, 2] }));
        let last = Page::new(Vec::<u32>::new(), Some("next".into()));
        let json = serde_json::to_value(&last).expect("page serializes");
        assert_eq!(json["next_cursor"], "next");
    }
}
