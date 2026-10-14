//! Frozen public failure presentations of the platform read bindings.
//!
//! Every row is keyed `operation/platform/variant`. Every platform's reads
//! report the runtime's [`PlatformReadError`] and are projected by the shared
//! read-failure projection: the variant names the error variant, and a
//! platform failure is keyed `Platform.<sample>` by one of the fixed protocol
//! evidence samples every platform shares. The expected wire JSON is frozen in
//! `fixtures/read_failure_presentations.json`: code, effect, retry safety,
//! recovery, provider status, provider evidence and the attached safe
//! message, where an `applied` effect marks a failure reported after a local
//! commit.
use std::collections::{BTreeMap, BTreeSet};

use onecrew_local_runtime::{
    DmDiagnostic, PlatformConnectionResolutionError, SearchPlanApprovalError,
    toolkits::PlatformReadError,
};
use serde_json::Value;

use super::{read_failure::read_failure_projection, telegram_gets::TELEGRAM_CONNECTION};
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, OperatorToolkitId, ServiceToolkitId},
    execution::{ClassifiedOwnerFailure, project_classified_owner_failure},
};

const FROZEN_ROWS: &str = include_str!("fixtures/read_failure_presentations.json");

const X_OPERATIONS: [&str; 4] = [
    "x.profile.get",
    "x.user_posts.list",
    "x.post_thread.get",
    "x.posts.search.advanced",
];
const INSTAGRAM_OPERATIONS: [&str; 2] = ["instagram.profile.get", "instagram.users.search"];
const YOUTUBE_OPERATIONS: [&str; 3] = [
    "youtube.channel.get",
    "youtube.channels.search",
    "youtube.videos.search",
];
const TELEGRAM_OPERATIONS: [&str; 3] = [
    "telegram.communities.search",
    "telegram.community.get",
    "telegram.community.administrators.list",
];

const CONNECTION_FAILURES: [PlatformConnectionResolutionError; 4] = [
    PlatformConnectionResolutionError::InvalidIdentifier,
    PlatformConnectionResolutionError::NotFoundOrMismatch,
    PlatformConnectionResolutionError::Unavailable,
    PlatformConnectionResolutionError::Internal,
];

fn refusal_name(reason: SearchPlanApprovalError) -> &'static str {
    match reason {
        SearchPlanApprovalError::ConfirmationRequired => "ConfirmationRequired",
        SearchPlanApprovalError::InvalidOrExpired => "InvalidOrExpired",
        SearchPlanApprovalError::ScopeMismatch => "ScopeMismatch",
        SearchPlanApprovalError::Unavailable => "Unavailable",
    }
}

fn connection_variant(error: PlatformConnectionResolutionError) -> String {
    let name = match error {
        PlatformConnectionResolutionError::InvalidIdentifier => "InvalidIdentifier",
        PlatformConnectionResolutionError::NotFoundOrMismatch => "NotFoundOrMismatch",
        PlatformConnectionResolutionError::Unavailable => "Unavailable",
        PlatformConnectionResolutionError::Internal => "Internal",
    };
    format!("Connection.{name}")
}

/// Fixed protocol evidence of a failed platform read, the same for every
/// platform: no platform response, HTTP 401, a 403 with a native code, 404,
/// 429, RPC 420, 503 and a local code. The projection reads only the origin,
/// HTTP status, RPC code and native code, so `platform` and `protocol` only
/// label the evidence.
fn evidence_samples(
    platform: &'static str,
    protocol: &'static str,
) -> Vec<(&'static str, DmDiagnostic)> {
    let http = |status, code: Option<&str>| DmDiagnostic {
        http_status: Some(status),
        code: code.map(str::to_owned),
        ..DmDiagnostic::provider(
            platform,
            protocol,
            "platform returned an unsuccessful response",
        )
    };
    vec![
        (
            "NoResponse",
            DmDiagnostic::provider(platform, protocol, "platform request produced no response"),
        ),
        ("Http@401", http(401, None)),
        ("Http@403.NativeCode", http(403, Some("native_code"))),
        ("Http@404", http(404, None)),
        ("Http@429", http(429, None)),
        (
            "Rpc@420",
            DmDiagnostic {
                rpc_code: Some(420),
                code: Some("FLOOD_WAIT".to_owned()),
                ..DmDiagnostic::provider(platform, protocol, "platform returned an RPC error")
            },
        ),
        ("Http@503", http(503, None)),
        (
            "Local",
            DmDiagnostic::local(
                platform,
                protocol,
                "invalid_response",
                "platform response could not be interpreted",
            ),
        ),
    ]
}

/// Every [`PlatformReadError`] variant keyed by its name, with each platform
/// failure keyed by its evidence sample, and the 503 sample once more after a
/// local commit.
fn read_error_samples(
    platform: &'static str,
    protocol: &'static str,
) -> Vec<(String, PlatformReadError)> {
    let mut samples = vec![(
        "InputInvalid".to_owned(),
        PlatformReadError::InputInvalid("platform read input is invalid"),
    )];
    samples.extend(SearchPlanApprovalError::ALL.map(|reason| {
        (
            format!("SearchPlanRefused.{}", refusal_name(reason)),
            PlatformReadError::SearchPlanRefused(reason),
        )
    }));
    samples.extend(CONNECTION_FAILURES.map(|error| {
        (
            connection_variant(error),
            PlatformReadError::Connection(error),
        )
    }));
    samples.extend(
        [
            ("SessionUnavailable", PlatformReadError::SessionUnavailable),
            (
                "CredentialUnavailable",
                PlatformReadError::CredentialUnavailable,
            ),
            (
                "CredentialHealthCommitUnknown",
                PlatformReadError::CredentialHealthCommitUnknown,
            ),
            ("Interrupted", PlatformReadError::Interrupted),
            (
                "NativeDataNotObject",
                PlatformReadError::NativeDataNotObject,
            ),
        ]
        .map(|(name, error)| (name.to_owned(), error)),
    );
    let failure = |evidence, after_local_commit| PlatformReadError::Platform {
        evidence,
        after_local_commit,
    };
    for (name, evidence) in evidence_samples(platform, protocol) {
        if name == "Http@503" {
            samples.push((
                format!("Platform.{name}.AfterLocalCommit"),
                failure(evidence.clone(), true),
            ));
        }
        samples.push((format!("Platform.{name}"), failure(evidence, false)));
    }
    samples
}

/// Projects every sample through the binding failure projection of each
/// operation.
fn rows<E>(
    platform: &str,
    operations: &[&str],
    samples: impl Fn() -> Vec<(String, E)>,
    classify: impl Fn(E) -> ClassifiedOwnerFailure,
) -> BTreeMap<String, Value> {
    let mut rows = BTreeMap::new();
    for operation in operations {
        for (variant, error) in samples() {
            let key = format!("{operation}/{platform}/{variant}");
            let wire = serde_json::to_value(project_classified_owner_failure(classify(error)))
                .expect("a failure presentation serializes");
            assert!(
                rows.insert(key.clone(), wire).is_none(),
                "duplicate row {key}"
            );
        }
    }
    rows
}

fn frozen_rows() -> BTreeMap<String, Value> {
    serde_json::from_str(FROZEN_ROWS).expect("frozen failure presentations are JSON")
}

fn assert_frozen(platform: &str, actual: BTreeMap<String, Value>) {
    let expected = frozen_rows()
        .into_iter()
        .filter(|(key, _)| key.split('/').nth(1) == Some(platform))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actual.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>(),
        "{platform}: frozen row keys"
    );
    for (key, wire) in &actual {
        assert_eq!(wire, &expected[key], "{key}");
    }
}

/// Every X read binding declares an X operator account connection, so the
/// shared projection recovers a response-less failure through X's self-check.
#[test]
fn x_read_failures_project_to_their_frozen_presentations() {
    let connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::X,
    };
    assert_frozen(
        "x",
        rows(
            "x",
            &X_OPERATIONS,
            || read_error_samples("x", "x_web"),
            read_failure_projection(&connection),
        ),
    );
}

/// Both Instagram read bindings declare an Instagram operator account
/// connection, so the shared projection recovers a response-less failure
/// through Instagram's self-check.
#[test]
fn instagram_read_failures_project_to_their_frozen_presentations() {
    let connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::Instagram,
    };
    assert_frozen(
        "instagram",
        rows(
            "instagram",
            &INSTAGRAM_OPERATIONS,
            || read_error_samples("instagram", "instagram_private_api"),
            read_failure_projection(&connection),
        ),
    );
}

/// Every YouTube read binding declares a YouTube service credential, so the
/// shared projection recovers a response-less failure and an unknown health
/// commit through YouTube's self-check.
#[test]
fn youtube_read_failures_project_to_their_frozen_presentations() {
    let connection = ConnectionRequirement::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
    };
    assert_frozen(
        "youtube",
        rows(
            "youtube",
            &YOUTUBE_OPERATIONS,
            || read_error_samples("youtube", "youtube_data_api_v3"),
            read_failure_projection(&connection),
        ),
    );
}

/// Every Telegram read binding declares the same Telegram operator account
/// connection, so the shared projection recovers a response-less failure
/// through Telegram's self-check.
#[test]
fn telegram_read_failures_project_to_their_frozen_presentations() {
    assert_frozen(
        "telegram",
        rows(
            "telegram",
            &TELEGRAM_OPERATIONS,
            || read_error_samples("telegram", "mtproto"),
            read_failure_projection(&TELEGRAM_CONNECTION),
        ),
    );
}

#[test]
fn frozen_presentations_cover_exactly_the_platform_read_operations() {
    let operations = [
        ("x", &X_OPERATIONS[..]),
        ("instagram", &INSTAGRAM_OPERATIONS[..]),
        ("youtube", &YOUTUBE_OPERATIONS[..]),
        ("telegram", &TELEGRAM_OPERATIONS[..]),
    ]
    .into_iter()
    .flat_map(|(platform, operations)| {
        operations
            .iter()
            .map(move |operation| format!("{operation}/{platform}"))
    })
    .collect::<BTreeSet<_>>();
    assert_eq!(operations.len(), 12);
    let named = frozen_rows()
        .keys()
        .map(|key| key.rsplit_once('/').expect("a keyed row").0.to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(named, operations);
}
