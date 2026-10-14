//! Offline Resend contract tests over the fixtures frozen by
//! `docs/plans/resend-bidirectional-mailbox-sync/evidence/provider-contract-2026-08-24.md`.
//! These fail when a required sent/received field, pagination semantic, or
//! rate-limit fact disappears from the modeled contract.

use onecrew_email_contracts::{
    ProviderAttachmentAccess, ProviderAttachmentMetadata, ProviderInboundEmail, ProviderSentEmail,
    ProviderSentEmailPage,
};
use onecrew_resend_platform::wire::{WireAttachment, WireAttachmentMetadata};
use onecrew_resend_platform::wire::{
    WireErrorBody, WireErrorClass, WireInboundEmail, WireInboundEmailSummary, WireList,
    WireSentEmail, WireSentEmailSummary, is_known_email_event, parse_rate_limit_headers,
};

const SENT_LIST_PAGE: &str = include_str!("fixtures/resend/sent_list_page.json");
const SENT_LIST_EMPTY: &str = include_str!("fixtures/resend/sent_list_empty.json");
const SENT_DETAIL: &str = include_str!("fixtures/resend/sent_detail.json");
const RECEIVED_LIST_PAGE: &str = include_str!("fixtures/resend/received_list_page.json");
const RECEIVED_LIST_EMPTY: &str = include_str!("fixtures/resend/received_list_empty.json");
const RECEIVED_DETAIL: &str = include_str!("fixtures/resend/received_detail.json");
const ATTACHMENT_GET: &str = include_str!("fixtures/resend/attachment_get.json");
const SENT_ATTACHMENTS_LIST: &str = include_str!("fixtures/resend/sent_attachments_list.json");

fn body_keys_absent(list_json: &str, forbidden: &[&str]) {
    let value: serde_json::Value = serde_json::from_str(list_json).unwrap();
    for item in value["data"].as_array().unwrap() {
        let object = item.as_object().unwrap();
        for key in forbidden {
            assert!(
                !object.contains_key(*key),
                "list item unexpectedly carries `{key}`; the list-vs-detail contract moved"
            );
        }
    }
}

#[test]
fn sent_list_carries_identity_and_status_but_no_body() {
    let page: WireList<WireSentEmailSummary> = serde_json::from_str(SENT_LIST_PAGE).unwrap();
    assert_eq!(page.object, "list");
    assert!(page.has_more);
    assert_eq!(page.data.len(), 3);

    let delivered = &page.data[0];
    assert_eq!(
        delivered.message_id.as_deref(),
        Some("<outreach-1@mail.example-team.com>")
    );
    assert_eq!(delivered.last_event, "delivered");
    assert!(is_known_email_event(&delivered.last_event));

    let scheduled = &page.data[1];
    assert!(scheduled.message_id.is_none());
    assert!(scheduled.scheduled_at.is_some());

    // Equal created_at values are a real provider case; ordering falls back
    // to the id cursor, never to timestamps alone.
    assert_eq!(page.data[1].created_at, page.data[2].created_at);

    body_keys_absent(SENT_LIST_PAGE, &["html", "text", "headers", "attachments"]);
}

#[test]
fn sent_detail_adds_the_body_but_never_reply_parentage() {
    let detail: WireSentEmail = serde_json::from_str(SENT_DETAIL).unwrap();
    assert_eq!(detail.id, "9a1c2b3d-0001-4a5b-8c6d-000000000001");
    let message_id = detail.message_id.as_deref().unwrap();
    assert!(message_id.starts_with('<') && message_id.contains('@') && message_id.ends_with('>'));
    assert!(detail.html.is_some());
    // Unknown additive fields (`tags`) are tolerated, not errors.

    // The frozen provider limitation: no sent endpoint exposes headers,
    // In-Reply-To, or References — historical outbound reply parentage is
    // unrecoverable from Resend.
    let value: serde_json::Value = serde_json::from_str(SENT_DETAIL).unwrap();
    for key in ["headers", "in_reply_to", "references"] {
        assert!(value.get(key).is_none());
    }
}

#[test]
fn received_list_carries_rfc_identity_and_attachment_metadata_but_no_body() {
    let page: WireList<WireInboundEmailSummary> = serde_json::from_str(RECEIVED_LIST_PAGE).unwrap();
    assert!(page.has_more);

    let reply = &page.data[0];
    assert_eq!(reply.message_id, "<reply-1@mailer.example.net>");
    assert_eq!(reply.received_for, vec!["hello@example-team.com"]);
    assert_eq!(reply.attachments.len(), 1);
    let attachment = &reply.attachments[0];
    assert_eq!(attachment.content_type, "application/pdf");
    assert!(attachment.content_id.is_none());
    assert_eq!(attachment.size, Some(482_133));

    // Team-scoped list: the second item belongs to another mailbox and must
    // remain filterable by recipient evidence.
    assert_eq!(
        page.data[1].received_for,
        vec!["someone-else@example-team.com"]
    );

    body_keys_absent(RECEIVED_LIST_PAGE, &["html", "text", "headers", "raw"]);
}

#[test]
fn received_detail_exposes_headers_body_and_expiring_raw_access() {
    let detail: WireInboundEmail = serde_json::from_str(RECEIVED_DETAIL).unwrap();
    assert_eq!(detail.message_id, "<reply-1@mailer.example.net>");
    assert_eq!(
        detail.headers.get("in-reply-to").map(String::as_str),
        Some("<outreach-1@mail.example-team.com>")
    );
    assert!(detail.headers.contains_key("references"));
    assert_eq!(detail.html_format.as_deref(), Some("cid"));
    let raw = detail.raw.as_ref().unwrap();
    assert!(raw.download_url.starts_with("https://"));
    assert!(!raw.expires_at.is_empty());
}

#[test]
fn received_identity_is_required_not_defaulted() {
    let mut value: serde_json::Value = serde_json::from_str(RECEIVED_DETAIL).unwrap();
    value.as_object_mut().unwrap().remove("message_id");
    let missing_identity = serde_json::to_string(&value).unwrap();
    assert!(
        serde_json::from_str::<WireInboundEmail>(&missing_identity).is_err(),
        "a received email without its RFC Message-ID must fail loudly, not default"
    );

    let mut value: serde_json::Value = serde_json::from_str(RECEIVED_LIST_PAGE).unwrap();
    value["data"][0].as_object_mut().unwrap().remove("id");
    let missing_provider_id = serde_json::to_string(&value).unwrap();
    assert!(
        serde_json::from_str::<WireList<WireInboundEmailSummary>>(&missing_provider_id).is_err()
    );
}

#[test]
fn pagination_cursor_is_the_last_item_id_and_only_while_has_more() {
    let page: WireList<WireSentEmailSummary> = serde_json::from_str(SENT_LIST_PAGE).unwrap();
    assert_eq!(
        page.next_after(),
        Some("9a1c2b3d-0003-4a5b-8c6d-000000000003")
    );

    let empty: WireList<WireSentEmailSummary> = serde_json::from_str(SENT_LIST_EMPTY).unwrap();
    assert!(!empty.has_more);
    assert_eq!(empty.next_after(), None);

    let empty_received: WireList<WireInboundEmailSummary> =
        serde_json::from_str(RECEIVED_LIST_EMPTY).unwrap();
    assert_eq!(empty_received.next_after(), None);
}

#[test]
fn maximum_size_page_parses_and_cursors_from_its_hundredth_item() {
    let page: WireList<WireInboundEmailSummary> =
        serde_json::from_str(include_str!("fixtures/resend/received_list_full_page.json")).unwrap();
    assert_eq!(page.data.len(), 100);
    assert!(page.has_more);
    assert_eq!(
        page.next_after(),
        page.data.last().map(|item| item.id.as_str())
    );
}

#[test]
fn a_fresh_walk_overlaps_the_previous_newest_items_by_provider_id() {
    // Incremental sync restarts from the newest page instead of trusting a
    // stored `after` as a delta token; the overlap fixture re-lists an
    // already-imported provider id, which dedupe must recognize.
    let first: WireList<WireInboundEmailSummary> =
        serde_json::from_str(RECEIVED_LIST_PAGE).unwrap();
    let rewalk: WireList<WireInboundEmailSummary> = serde_json::from_str(include_str!(
        "fixtures/resend/received_list_overlap_page.json"
    ))
    .unwrap();

    let known: Vec<&str> = first.data.iter().map(|item| item.id.as_str()).collect();
    let overlap: Vec<&str> = rewalk
        .data
        .iter()
        .map(|item| item.id.as_str())
        .filter(|id| known.contains(id))
        .collect();
    assert_eq!(overlap, vec!["7b2d3e4f-1001-4c5d-9e6f-000000000001"]);
    assert!(
        rewalk.data.iter().any(|item| item.id.ends_with("0003")),
        "the rewalk also carries new mail"
    );
}

#[test]
fn provider_neutral_mapping_keeps_the_two_identities_apart() {
    let page: WireList<WireSentEmailSummary> = serde_json::from_str(SENT_LIST_PAGE).unwrap();
    let neutral: ProviderSentEmailPage = page.into();

    assert_eq!(
        neutral.next_cursor.as_deref(),
        Some("9a1c2b3d-0003-4a5b-8c6d-000000000003")
    );
    let first = &neutral.messages[0];
    // Resend Email ID (API resource) and RFC Message-ID (mail ecosystem)
    // land in distinct fields and are never interchangeable.
    assert_eq!(
        first.provider_message_id,
        "9a1c2b3d-0001-4a5b-8c6d-000000000001"
    );
    assert_eq!(
        first.internet_message_id.as_deref(),
        Some("<outreach-1@mail.example-team.com>")
    );
    assert_ne!(
        first.provider_message_id,
        first.internet_message_id.clone().unwrap()
    );
    // Nullable list arrays normalize to empty, not to a parse failure.
    assert!(first.cc.is_empty() && first.bcc.is_empty() && first.reply_to.is_empty());
}

#[test]
fn detail_mappings_preserve_provider_neutral_identity_headers_and_bodies() {
    let sent: ProviderSentEmail = serde_json::from_str::<WireSentEmail>(SENT_DETAIL)
        .unwrap()
        .into();
    assert_eq!(
        sent.provider_message_id,
        "9a1c2b3d-0001-4a5b-8c6d-000000000001"
    );
    assert_eq!(
        sent.internet_message_id.as_deref(),
        Some("<outreach-1@mail.example-team.com>")
    );
    assert_eq!(
        sent.html.as_deref(),
        Some("Hi! We loved your <strong>latest video</strong>.")
    );
    assert!(sent.text.is_none());

    let received: ProviderInboundEmail = serde_json::from_str::<WireInboundEmail>(RECEIVED_DETAIL)
        .unwrap()
        .into();
    assert_eq!(
        received.provider_message_id,
        "7b2d3e4f-1001-4c5d-9e6f-000000000001"
    );
    assert_eq!(received.internet_message_id, "<reply-1@mailer.example.net>");
    assert_eq!(received.received_for, vec!["hello@example-team.com"]);
    assert_eq!(
        received.headers["in-reply-to"],
        "<outreach-1@mail.example-team.com>"
    );
    assert_eq!(
        received.text.as_deref(),
        Some("Sounds great, see attached kit.")
    );
    assert_eq!(
        received.attachments[0].provider_attachment_id,
        "2a0c9ce0-3112-4728-976e-000000000001"
    );
}

#[test]
fn attachment_capabilities_parse_and_never_leak_through_debug() {
    let fetched: WireAttachment = serde_json::from_str(ATTACHMENT_GET).unwrap();
    let access: ProviderAttachmentAccess = fetched.into();
    assert_eq!(
        access.metadata.provider_attachment_id,
        "2a0c9ce0-3112-4728-976e-000000000001"
    );
    assert!(access.download_url.contains("signature="));
    assert!(!access.expires_at.is_empty());

    let debug = format!("{access:?}");
    assert!(debug.contains("<redacted signed url>"));
    assert!(!debug.contains("signature="));

    let sent_attachments: WireList<WireAttachment> =
        serde_json::from_str(SENT_ATTACHMENTS_LIST).unwrap();
    assert_eq!(sent_attachments.data.len(), 1);
    let debug = format!("{:?}", sent_attachments.data[0]);
    assert!(!debug.contains("signature="));

    let received: WireInboundEmail = serde_json::from_str(RECEIVED_DETAIL).unwrap();
    let debug = format!("{:?}", received.raw.unwrap());
    assert!(debug.contains("<redacted signed url>") && !debug.contains("signature="));
}

#[test]
fn error_registry_distinguishes_quota_from_transient_rate_limits() {
    let cases = [
        (
            "fixtures 401",
            include_str!("fixtures/resend/error_401.json"),
            WireErrorClass::Auth,
            false,
        ),
        (
            "fixtures 429 rate",
            include_str!("fixtures/resend/error_429_rate.json"),
            WireErrorClass::RateLimited,
            true,
        ),
        (
            "fixtures 429 daily quota",
            include_str!("fixtures/resend/error_429_daily_quota.json"),
            WireErrorClass::DailyQuotaExceeded,
            false,
        ),
        (
            "fixtures 429 monthly quota",
            include_str!("fixtures/resend/error_429_monthly_quota.json"),
            WireErrorClass::MonthlyQuotaExceeded,
            false,
        ),
        (
            "fixtures 500",
            include_str!("fixtures/resend/error_500.json"),
            WireErrorClass::ProviderFault,
            true,
        ),
        (
            "fixtures 400 malformed cursor",
            include_str!("fixtures/resend/error_400_malformed_cursor.json"),
            WireErrorClass::Validation,
            false,
        ),
    ];
    for (label, fixture, expected_class, expected_transient) in cases {
        let body: WireErrorBody = serde_json::from_str(fixture).unwrap();
        assert_eq!(body.classify(), expected_class, "{label}");
        assert_eq!(
            body.classify().is_transient(),
            expected_transient,
            "{label}"
        );
    }

    // A bodyless 429 stays a transient rate limit; a bodyless quota signal
    // does not exist, so status-only classification can never invent one.
    assert_eq!(
        WireErrorBody::classify_status_only(429),
        WireErrorClass::RateLimited
    );
    assert_eq!(
        WireErrorBody::classify_status_only(503),
        WireErrorClass::ProviderFault
    );
    assert_eq!(
        WireErrorBody::classify_status_only(401),
        WireErrorClass::Auth
    );

    let unknown: WireErrorBody =
        serde_json::from_str(r#"{"statusCode":429,"message":"?","name":"brand_new_error"}"#)
            .unwrap();
    assert_eq!(unknown.classify(), WireErrorClass::Unrecognized);
    assert!(!unknown.classify().is_transient());
}

#[test]
fn rate_limit_headers_parse_case_insensitively_and_reject_bad_metadata() {
    let status = parse_rate_limit_headers([
        ("RateLimit-Limit", "10"),
        ("ratelimit-remaining", "0"),
        ("RATELIMIT-RESET", "1"),
        ("Retry-After", "2"),
        ("content-type", "application/json"),
    ])
    .unwrap();
    assert_eq!(status.limit, Some(10));
    assert_eq!(status.remaining, Some(0));
    assert_eq!(status.reset_seconds, Some(1));
    assert_eq!(status.retry_after_seconds, Some(2));

    let absent = parse_rate_limit_headers([("content-type", "application/json")]);
    assert!(absent.unwrap_err().contains("missing"));

    let garbage = parse_rate_limit_headers([("ratelimit-reset", "soon"), ("retry-after", "-1")]);
    assert!(garbage.unwrap_err().contains("invalid"));

    let boundary = parse_rate_limit_headers([
        ("ratelimit-limit", "10"),
        ("ratelimit-remaining", "0"),
        ("ratelimit-reset", "0"),
        ("retry-after", "not-applicable-on-2xx"),
    ])
    .unwrap();
    assert_eq!(boundary.reset_seconds, Some(0));
    assert!(boundary.retry_after_seconds.is_none());
}

#[test]
fn unknown_email_events_degrade_to_unknown_not_to_a_failed_page() {
    let mut value: serde_json::Value = serde_json::from_str(SENT_LIST_PAGE).unwrap();
    value["data"][0]["last_event"] = serde_json::Value::String("brand_new_event".into());
    let page: WireList<WireSentEmailSummary> =
        serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
    assert!(!is_known_email_event(&page.data[0].last_event));
}

#[test]
fn minimal_live_wire_items_parse_with_absent_optional_lists() {
    // Live acceptance finding (2026-08-24): the real Receiving list omits
    // empty recipient lists, received_for, headers, and attachments
    // entirely. Absent means empty; only true identity fields are required.
    let minimal_summary = r#"{
        "id": "7b2d3e4f-3001-4c5d-9e6f-000000000001",
        "to": ["hello@example-team.com"],
        "from": "Creator <creator@example.net>",
        "created_at": "2026-08-24 08:00:00.000000+00",
        "subject": "Minimal",
        "message_id": "<minimal-1@mailer.example.net>"
    }"#;
    let summary: WireInboundEmailSummary = serde_json::from_str(minimal_summary).unwrap();
    assert!(summary.cc.is_empty() && summary.received_for.is_empty());
    assert!(summary.attachments.is_empty());

    let minimal_detail = r#"{
        "id": "7b2d3e4f-3001-4c5d-9e6f-000000000001",
        "to": ["hello@example-team.com"],
        "from": "Creator <creator@example.net>",
        "created_at": "2026-08-24 08:00:00.000000+00",
        "subject": "Minimal",
        "message_id": "<minimal-1@mailer.example.net>",
        "html": null,
        "text": "hi",
        "raw": null
    }"#;
    let detail: WireInboundEmail = serde_json::from_str(minimal_detail).unwrap();
    assert!(detail.headers.is_empty());
    assert!(detail.attachments.is_empty());

    // Unnamed inline attachments carry no filename; the durable-safe model
    // normalizes that to an empty name rather than failing the message.
    let unnamed = r#"{
        "id": "2a0c9ce0-3112-4728-976e-000000000009",
        "content_type": "image/png"
    }"#;
    let metadata: WireAttachmentMetadata = serde_json::from_str(unnamed).unwrap();
    let neutral: ProviderAttachmentMetadata = metadata.into();
    assert_eq!(neutral.filename, "");
}
