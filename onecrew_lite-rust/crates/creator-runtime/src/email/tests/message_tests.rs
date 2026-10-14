use super::*;

#[test]
fn normalizes_named_email_addresses() {
    assert_eq!(
        normalize_email_address("Creator <HELLO@Example.com>").unwrap(),
        "hello@example.com"
    );
    assert!(normalize_email_address("not-an-email").is_err());
}

#[test]
fn parses_documented_resend_timestamp_variants_strictly() {
    let expected = parse_provider_time("2026-08-05T10:00:00Z").unwrap();
    assert_eq!(
        parse_provider_time("2026-08-05T10:00:00+00:00").unwrap(),
        expected
    );
    assert_eq!(
        parse_provider_time("2026-08-05 10:00:00.951732+00").unwrap(),
        expected
    );
    assert_eq!(
        parse_provider_time("2026-08-05 10:00:00+00").unwrap(),
        expected
    );
    assert!(parse_provider_time("").is_err());
    assert!(parse_provider_time("   ").is_err());
    assert!(parse_provider_time("not-a-timestamp").is_err());
    assert!(parse_provider_time("2026-08-05 10:00:00").is_err());
}

#[test]
fn extracts_ordered_message_ids_from_headers() {
    let headers = serde_json::json!({
        "References": "<root@example.com> <reply@example.com>",
        "In-Reply-To": "<reply@example.com>"
    });
    assert_eq!(
        header_message_ids(&headers, "references"),
        vec!["<root@example.com>", "<reply@example.com>"]
    );
}
