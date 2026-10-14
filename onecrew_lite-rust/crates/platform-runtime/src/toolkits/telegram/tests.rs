//! Telegram reads through the shared operator-account read: input checks
//! before admission, the bound search checked again once the connection opens,
//! the connection lifecycle on every returned outcome, and Telegram's own
//! evidence.
use std::sync::atomic::{AtomicUsize, Ordering};

use onecrew_creator_runtime::AccountRefId;
use onecrew_dm_contracts::DmDiagnostic;
use onecrew_local_domain::{Platform, ProfileSecret};
use onecrew_tg::{TgChat, TgCommunityReadError, TgError};
use serde_json::Value;

use super::{
    BindSearch, COMMUNITY_SEARCH_MAX_COUNT, COMMUNITY_SEARCH_MIN_COUNT, CommunitiesSearchInput,
    TgAdministratorRole, TgCommunityKind, TgCommunityLocator, TgPublicAdministrator,
    TgPublicAdministrators, TgPublicCommunity, TgPublicUser, administrators, admins, community,
    connected_read, resolve_target, search_communities,
};
use crate::{
    BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform,
    PlatformConnectionResolutionError, SearchPlanApprovalError, SearchPlanPolicy,
    toolkits::{
        PlatformReadError,
        read::{BoundSearch, OperatorSession, platform_native_data},
    },
};

const PROXY_URL: &str = "socks5://proxy_user:proxy_pass@gate.example:1080";
const PHONE: &str = "+15551234567";

fn platform() -> (LocalPlatform, std::path::PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("onecrew-tg-toolkit-test-{}", uuid::Uuid::new_v4()));
    (LocalPlatform::open(&directory).unwrap(), directory)
}

fn cleanup(platform: LocalPlatform, directory: std::path::PathBuf) {
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

fn search_input() -> CommunitiesSearchInput {
    CommunitiesSearchInput {
        operator_account_id: uuid::Uuid::new_v4().to_string(),
        query: "rust".into(),
        count: 30,
    }
}

fn direct_authorization(input: &CommunitiesSearchInput) -> BoundSearchAuthorization {
    BoundSearchAuthorization::bind(input, SearchPlanPolicy::UserRequestedDirect, None, None)
        .unwrap()
}

fn community_locator() -> TgCommunityLocator {
    TgCommunityLocator::StableCommunity {
        external_id: "1688680661".into(),
        kind: TgCommunityKind::Supergroup,
    }
}

/// A Telegram profile whose secret can be read but holds no Telegram session
/// material: admission accepts it, and preparing its session fails before any
/// connection opens.
fn operator_account(platform: &LocalPlatform) -> AccountRefId {
    let profile = platform
        .vault
        .upsert(
            Platform::Telegram,
            "gated_operator",
            &ProfileSecret {
                provider_account_id: "telegram-provider".into(),
                cookies: Vec::new(),
                x_chat_passcode: None,
                instagram_session_settings: None,
                telegram_api_id: None,
                telegram_api_hash: None,
                telegram_session: None,
                profile_name: "Gated Operator".into(),
                browser_profile_dir: String::new(),
                fingerprint_seed: String::new(),
                proxy: None,
                proxy_egress: None,
            },
        )
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    AccountRefId::new(profile.id.to_string())
}

/// How often a test connection was opened, read and closed.
#[derive(Default)]
struct Connections {
    opened: AtomicUsize,
    reads: AtomicUsize,
    closed: AtomicUsize,
}

impl Connections {
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.opened.load(Ordering::SeqCst),
            self.reads.load(Ordering::SeqCst),
            self.closed.load(Ordering::SeqCst),
        )
    }
}

/// A bound search whose first `accepted` checks pass and whose later checks
/// find the plan expired, counting every check in `checks`.
fn lapsing_search(checks: &AtomicUsize, accepted: usize) -> BoundSearch<'_> {
    BoundSearch::from_check(move || {
        if checks.fetch_add(1, Ordering::SeqCst) < accepted {
            Ok(())
        } else {
            Err(SearchPlanApprovalError::InvalidOrExpired)
        }
    })
}

/// One shared read over a counted test connection: the session opens without
/// stored material, `connects` decides whether the connection opens, and the
/// read returns `outcome`.
async fn counted_read<T, E>(
    platform: &LocalPlatform,
    operator_account_id: &str,
    connections: &Connections,
    search: Option<BindSearch<'_, '_>>,
    connects: bool,
    outcome: Result<T, E>,
    evidence: fn(&E) -> DmDiagnostic,
) -> Result<T, PlatformReadError> {
    connected_read(
        platform,
        OperatorSession {
            platform: OperatorAccountPlatform::Telegram,
            operator_account_id,
            open: |_, _| Ok(()),
        },
        search,
        async |()| {
            connections.opened.fetch_add(1, Ordering::SeqCst);
            if connects {
                Ok(connections)
            } else {
                Err(TgError::Network(format!("dial {PROXY_URL}")))
            }
        },
        async move |_: &&Connections| {
            connections.reads.fetch_add(1, Ordering::SeqCst);
            outcome
        },
        async |connection: &Connections| {
            connection.closed.fetch_add(1, Ordering::SeqCst);
        },
        evidence,
    )
    .await
}

/// Every Telegram failure shape, each carrying a credentialed proxy address
/// and a phone number wherever the shape has text.
fn telegram_errors() -> Vec<TgError> {
    let text = || format!("via {PROXY_URL} for {PHONE} SENTINEL");
    vec![
        TgError::Session(text()),
        TgError::Auth(text()),
        TgError::Permission(text()),
        TgError::Network(text()),
        TgError::Other(text()),
        TgError::Rpc {
            code: 420,
            name: Some("FLOOD_WAIT".into()),
        },
        TgError::Rpc {
            code: 401,
            name: None,
        },
    ]
}

/// Every community read failure: the client's local conclusions, then each
/// Telegram failure shape.
fn community_read_errors() -> Vec<TgCommunityReadError> {
    let mut errors = vec![
        TgCommunityReadError::InvalidLocator,
        TgCommunityReadError::NotFound,
        TgCommunityReadError::UnsupportedKind,
        TgCommunityReadError::IdentityMismatch,
        TgCommunityReadError::PeerUnavailable,
    ];
    errors.extend(
        telegram_errors()
            .into_iter()
            .map(TgCommunityReadError::Provider),
    );
    errors
}

fn assert_text_free(failure: &PlatformReadError) {
    assert!(std::error::Error::source(failure).is_none());
    for text in [format!("{failure:?}"), failure.to_string()] {
        for absent in [
            "proxy_user",
            "proxy_pass",
            "gate.example",
            PHONE,
            "SENTINEL",
        ] {
            assert!(!text.contains(absent), "{absent}: {text}");
        }
    }
}

#[tokio::test]
async fn search_checks_input_and_bound_request_before_account_admission() {
    let (platform, directory) = platform();
    let count_refused =
        PlatformReadError::InputInvalid("Telegram search count must be between 1 and 100");
    let query_refused = PlatformReadError::InputInvalid("Telegram search query is required");
    for (query, count, expected) in [
        ("rust", COMMUNITY_SEARCH_MIN_COUNT - 1, &count_refused),
        ("rust", COMMUNITY_SEARCH_MAX_COUNT + 1, &count_refused),
        (" \t", 10, &query_refused),
    ] {
        let input = CommunitiesSearchInput {
            query: query.into(),
            count,
            ..search_input()
        };
        let authorization = direct_authorization(&input);
        assert_eq!(
            &search_communities(&platform, input, authorization)
                .await
                .unwrap_err(),
            expected,
            "{query:?} {count}"
        );
    }
    let original = search_input();
    for input in [
        CommunitiesSearchInput {
            query: "changed".into(),
            ..original.clone()
        },
        CommunitiesSearchInput {
            count: 31,
            ..original.clone()
        },
        CommunitiesSearchInput {
            operator_account_id: uuid::Uuid::new_v4().to_string(),
            ..original.clone()
        },
    ] {
        assert_eq!(
            search_communities(&platform, input, direct_authorization(&original))
                .await
                .unwrap_err(),
            PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::ScopeMismatch)
        );
    }
    for count in [COMMUNITY_SEARCH_MIN_COUNT, COMMUNITY_SEARCH_MAX_COUNT] {
        let input = CommunitiesSearchInput {
            count,
            ..original.clone()
        };
        let authorization = direct_authorization(&input);
        assert_eq!(
            search_communities(&platform, input, authorization)
                .await
                .unwrap_err(),
            PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch),
            "{count}"
        );
    }
    cleanup(platform, directory);
}

#[tokio::test]
async fn an_invalid_locator_is_invalid_input_before_account_admission() {
    let (platform, directory) = platform();
    let refused = PlatformReadError::InputInvalid("Telegram community locator is invalid");
    for locator in [
        TgCommunityLocator::PublicUsername {
            username: "https://t.me/name/123".into(),
        },
        TgCommunityLocator::StableCommunity {
            external_id: "00123".into(),
            kind: TgCommunityKind::Channel,
        },
    ] {
        // An identifier that is not a UUID would be a connection refusal, so
        // invalid input proves the locator was checked first.
        assert_eq!(
            community(&platform, "not-a-uuid", &locator)
                .await
                .unwrap_err(),
            refused
        );
        assert_eq!(
            admins(&platform, "not-a-uuid", &locator).await.unwrap_err(),
            refused
        );
        assert_eq!(
            administrators(&platform, "not-a-uuid", &locator)
                .await
                .unwrap_err(),
            refused
        );
    }
    assert_eq!(
        admins(
            &platform,
            &uuid::Uuid::new_v4().to_string(),
            &community_locator()
        )
        .await
        .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    cleanup(platform, directory);
}

#[tokio::test]
async fn a_session_without_stored_material_is_unavailable_before_any_connection() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);
    let account = account_ref_id.as_str();
    assert_eq!(
        community(&platform, account, &community_locator())
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    assert_eq!(
        admins(&platform, account, &community_locator())
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    let input = CommunitiesSearchInput {
        operator_account_id: account.to_owned(),
        ..search_input()
    };
    let authorization = direct_authorization(&input);
    assert_eq!(
        search_communities(&platform, input, authorization)
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    assert_eq!(
        resolve_target(&platform, account, "https://t.me/rust")
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    cleanup(platform, directory);
}

/// Covers every outcome the read returns. A read that panics or is cancelled
/// never reaches `close`; there the Telegram transport connection's `Drop`
/// quits it and aborts its pool task, which these counted connections do not
/// model.
#[tokio::test]
async fn an_opened_connection_is_closed_on_every_returned_outcome() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);
    let account = account_ref_id.as_str();

    // The read succeeds.
    let connections = Connections::default();
    let read = counted_read(
        &platform,
        account,
        &connections,
        None,
        true,
        Ok::<_, TgError>("page"),
        onecrew_tg::evidence,
    )
    .await;
    assert_eq!(read, Ok("page"));
    assert_eq!(connections.counts(), (1, 1, 1));

    // Telegram fails the read.
    let connections = Connections::default();
    let flood = || TgError::Rpc {
        code: 420,
        name: Some("FLOOD_WAIT".into()),
    };
    let failure = counted_read(
        &platform,
        account,
        &connections,
        None,
        true,
        Err::<(), _>(flood()),
        onecrew_tg::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::Platform {
            evidence: onecrew_tg::evidence(&flood()),
            after_local_commit: false,
        }
    );
    assert_eq!(connections.counts(), (1, 1, 1));

    // The client concludes the community read locally.
    let connections = Connections::default();
    let failure = counted_read(
        &platform,
        account,
        &connections,
        None,
        true,
        Err::<(), _>(TgCommunityReadError::PeerUnavailable),
        onecrew_tg::community_read_evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::Platform {
            evidence: onecrew_tg::community_read_evidence(&TgCommunityReadError::PeerUnavailable),
            after_local_commit: false,
        }
    );
    assert_eq!(connections.counts(), (1, 1, 1));

    // The connection cannot open: its failed connect left nothing open, the
    // read never ran, and the connect diagnostic is gone.
    let connections = Connections::default();
    let failure = counted_read(
        &platform,
        account,
        &connections,
        None,
        false,
        Ok::<(), TgError>(()),
        onecrew_tg::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(failure, PlatformReadError::SessionUnavailable);
    assert_text_free(&failure);
    assert_eq!(connections.counts(), (1, 0, 0));

    // A live bound search is checked before admission, once the session is
    // open and once the connection is open, then read.
    let connections = Connections::default();
    let checks = AtomicUsize::new(0);
    let read = counted_read(
        &platform,
        account,
        &connections,
        Some(&|| lapsing_search(&checks, usize::MAX)),
        true,
        Ok::<_, TgError>("page"),
        onecrew_tg::evidence,
    )
    .await;
    assert_eq!(read, Ok("page"));
    assert_eq!(checks.load(Ordering::SeqCst), 3);
    assert_eq!(connections.counts(), (1, 1, 1));

    // The bound search lapses once the session is open: no connection opens.
    let connections = Connections::default();
    let checks = AtomicUsize::new(0);
    let failure = counted_read(
        &platform,
        account,
        &connections,
        Some(&|| lapsing_search(&checks, 1)),
        true,
        Ok::<(), TgError>(()),
        onecrew_tg::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(checks.load(Ordering::SeqCst), 2);
    assert_eq!(connections.counts(), (0, 0, 0));

    // The bound search lapses while the connection opens: Telegram is not
    // called, and the opened connection is closed.
    let connections = Connections::default();
    let checks = AtomicUsize::new(0);
    let failure = counted_read(
        &platform,
        account,
        &connections,
        Some(&|| lapsing_search(&checks, 2)),
        true,
        Ok::<(), TgError>(()),
        onecrew_tg::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(checks.load(Ordering::SeqCst), 3);
    assert_eq!(connections.counts(), (1, 0, 1));

    // Admission refuses the account: no connection opens.
    let connections = Connections::default();
    let failure = counted_read(
        &platform,
        &uuid::Uuid::new_v4().to_string(),
        &connections,
        None,
        true,
        Ok::<(), TgError>(()),
        onecrew_tg::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    assert_eq!(connections.counts(), (0, 0, 0));
    cleanup(platform, directory);
}

#[tokio::test]
async fn every_telegram_failure_crosses_as_its_telegram_evidence_without_its_text() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);
    let connections = Connections::default();

    let community_errors = community_read_errors();
    let mut reads = community_errors.len();
    for (error, expected) in community_errors.into_iter().zip(community_read_errors()) {
        let failure = counted_read(
            &platform,
            account_ref_id.as_str(),
            &connections,
            None,
            true,
            Err::<(), _>(error),
            onecrew_tg::community_read_evidence,
        )
        .await
        .unwrap_err();
        assert_eq!(
            failure,
            PlatformReadError::Platform {
                evidence: onecrew_tg::community_read_evidence(&expected),
                after_local_commit: false,
            },
            "{expected:?}"
        );
        assert_text_free(&failure);
    }

    // Searches and target resolutions fail with Telegram's own error.
    let search_errors = telegram_errors();
    reads += search_errors.len();
    for (error, expected) in search_errors.into_iter().zip(telegram_errors()) {
        let failure = counted_read(
            &platform,
            account_ref_id.as_str(),
            &connections,
            None,
            true,
            Err::<(), _>(error),
            onecrew_tg::evidence,
        )
        .await
        .unwrap_err();
        assert_eq!(
            failure,
            PlatformReadError::Platform {
                evidence: onecrew_tg::evidence(&expected),
                after_local_commit: false,
            },
            "{expected:?}"
        );
        assert_text_free(&failure);
    }
    assert_eq!(connections.counts(), (reads, reads, reads));
    cleanup(platform, directory);
}

#[test]
fn platform_native_data_is_the_serde_serialization_of_the_telegram_models() {
    let community = TgPublicCommunity {
        external_id: "9007199254740993".into(),
        title: "Rust community".into(),
        username: Some("rust".into()),
        kind: TgCommunityKind::Supergroup,
    };
    let administrators = TgPublicAdministrators {
        community: community.clone(),
        administrators: vec![TgPublicAdministrator {
            role: TgAdministratorRole::Owner,
            user: TgPublicUser {
                id: "9007199254740995".into(),
                username: Some("ferris".into()),
                first_name: Some("Ferris".into()),
                last_name: None,
                is_bot: false,
                verified: false,
            },
        }],
        page_limit: 200,
    };
    let chat = TgChat {
        id: "9007199254740993".into(),
        title: "Rust community".into(),
        username: None,
        kind: "supergroup".into(),
        participant_count: Some(42),
        verified: false,
        scam: false,
    };
    assert_eq!(
        Value::Object(platform_native_data(&community).unwrap()),
        serde_json::to_value(&community).unwrap()
    );
    assert_eq!(
        Value::Object(platform_native_data(&administrators).unwrap()),
        serde_json::to_value(&administrators).unwrap()
    );
    assert_eq!(
        Value::Object(platform_native_data(&chat).unwrap()),
        serde_json::to_value(&chat).unwrap()
    );
}
