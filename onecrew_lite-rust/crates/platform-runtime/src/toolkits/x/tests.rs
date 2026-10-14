use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use onecrew_creator_runtime::AccountRefId;
use onecrew_local_domain::{Platform, ProfileSecret};
use onecrew_x::{TransportError, XChatError, XClientError, XError};

use super::{
    ADVANCED_SEARCH_MAX_COUNT, ADVANCED_SEARCH_MIN_COUNT, AdvancedSearchInput,
    POST_THREAD_MAX_COUNT, POST_THREAD_MIN_COUNT, XToolkitWriteError, post_thread, profile,
    relationship_update, search_advanced, user_posts,
};
use crate::{
    BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform,
    PlatformConnectionResolutionError, SearchPlanApprovalError, SearchPlanPolicy,
    toolkits::{
        PlatformReadError,
        read::{BoundSearch, OperatorSession, operator_account_read},
    },
};

const PROXY_URL: &str = "http://proxy_user:proxy_pass@gate.example:8080?token=SENTINEL";

fn platform() -> (LocalPlatform, std::path::PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("onecrew-x-toolkit-test-{}", uuid::Uuid::new_v4()));
    (LocalPlatform::open(&directory).unwrap(), directory)
}

fn cleanup(platform: LocalPlatform, directory: std::path::PathBuf) {
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

fn search_input() -> AdvancedSearchInput {
    AdvancedSearchInput {
        operator_account_id: uuid::Uuid::new_v4().to_string(),
        criteria: onecrew_x::AdvancedSearchCriteria {
            all_words: vec!["rust".into()],
            ..Default::default()
        },
        result_mode: onecrew_x::AdvancedSearchResultMode::Latest,
        count: 20,
        cursor: Some("native-provider-cursor".into()),
    }
}

fn direct_authorization(input: &AdvancedSearchInput) -> BoundSearchAuthorization {
    BoundSearchAuthorization::bind(input, SearchPlanPolicy::UserRequestedDirect, None, None)
        .unwrap()
}

/// An X profile whose secret can be read but holds no session cookies:
/// admission accepts it, and opening its session fails before any network
/// call because the X session material is incomplete.
fn operator_account(platform: &LocalPlatform) -> AccountRefId {
    let profile = platform
        .vault
        .upsert(
            Platform::X,
            "gated_operator",
            &ProfileSecret {
                provider_account_id: "x-provider".into(),
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

/// A session description whose opening the test decides, so a read can
/// reach the platform call without a stored X session.
fn test_session<S>(
    operator_account_id: &str,
    open: fn(&onecrew_sessions::PlatformSessions, &str) -> Result<S, String>,
) -> OperatorSession<'_, S> {
    OperatorSession {
        platform: OperatorAccountPlatform::X,
        operator_account_id,
        open,
    }
}

#[tokio::test]
async fn search_validates_native_criteria_and_count_before_account_admission() {
    let (platform, directory) = platform();
    let mut invalid_inputs = Vec::new();
    for count in [0, 101] {
        invalid_inputs.push(AdvancedSearchInput {
            count,
            ..search_input()
        });
    }
    for criteria in [
        onecrew_x::AdvancedSearchCriteria::default(),
        onecrew_x::AdvancedSearchCriteria {
            all_words: vec!["from:injected".into()],
            ..Default::default()
        },
        onecrew_x::AdvancedSearchCriteria {
            all_words: vec!["rust".into()],
            since: Some("2026-09-14".into()),
            until: Some("2026-09-13".into()),
            ..Default::default()
        },
    ] {
        invalid_inputs.push(AdvancedSearchInput {
            criteria,
            ..search_input()
        });
    }
    for input in invalid_inputs {
        let authorization = direct_authorization(&input);
        assert!(matches!(
            search_advanced(&platform, input, authorization)
                .await
                .unwrap_err(),
            PlatformReadError::InputInvalid(_)
        ));
    }
    for count in [0, 101] {
        assert!(matches!(
            post_thread(&platform, "not-a-uuid", "500", count, Some("native-cursor"))
                .await
                .unwrap_err(),
            PlatformReadError::InputInvalid(_)
        ));
    }
    cleanup(platform, directory);
}

#[tokio::test]
async fn the_published_page_size_bounds_pass_validation_and_reach_admission() {
    let (platform, directory) = platform();
    let thread_refused =
        PlatformReadError::InputInvalid("X thread count must be between 1 and 100");
    let invalid_identifier =
        PlatformReadError::Connection(PlatformConnectionResolutionError::InvalidIdentifier);
    for (count, expected) in [
        (POST_THREAD_MIN_COUNT - 1, &thread_refused),
        (POST_THREAD_MIN_COUNT, &invalid_identifier),
        (POST_THREAD_MAX_COUNT, &invalid_identifier),
        (POST_THREAD_MAX_COUNT + 1, &thread_refused),
    ] {
        assert_eq!(
            &post_thread(&platform, "not-a-uuid", "500", count, None)
                .await
                .unwrap_err(),
            expected,
            "thread {count}"
        );
    }
    let search_refused =
        PlatformReadError::InputInvalid("X search count must be between 1 and 100");
    let not_found =
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch);
    for (count, expected) in [
        (ADVANCED_SEARCH_MIN_COUNT - 1, &search_refused),
        (ADVANCED_SEARCH_MIN_COUNT, &not_found),
        (ADVANCED_SEARCH_MAX_COUNT, &not_found),
        (ADVANCED_SEARCH_MAX_COUNT + 1, &search_refused),
    ] {
        let input = AdvancedSearchInput {
            count,
            ..search_input()
        };
        let authorization = direct_authorization(&input);
        assert_eq!(
            &search_advanced(&platform, input, authorization)
                .await
                .unwrap_err(),
            expected,
            "search {count}"
        );
    }
    cleanup(platform, directory);
}

#[tokio::test]
async fn search_rejects_changed_bound_fields_before_account_admission() {
    let (platform, directory) = platform();
    let original = search_input();
    let mut changes = Vec::new();
    let mut criteria = original.criteria.clone();
    criteria.all_words = vec!["changed".into()];
    changes.push(AdvancedSearchInput {
        criteria,
        ..original.clone()
    });
    changes.push(AdvancedSearchInput {
        count: 21,
        ..original.clone()
    });
    changes.push(AdvancedSearchInput {
        result_mode: onecrew_x::AdvancedSearchResultMode::Top,
        ..original.clone()
    });
    changes.push(AdvancedSearchInput {
        operator_account_id: uuid::Uuid::new_v4().to_string(),
        ..original.clone()
    });
    changes.push(AdvancedSearchInput {
        cursor: Some("changed-cursor".into()),
        ..original.clone()
    });
    for input in changes {
        assert_eq!(
            search_advanced(&platform, input, direct_authorization(&original))
                .await
                .unwrap_err(),
            PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::ScopeMismatch)
        );
    }
    assert_eq!(
        search_advanced(&platform, original.clone(), direct_authorization(&original))
            .await
            .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    cleanup(platform, directory);
}

#[tokio::test]
async fn a_bound_search_is_checked_before_admission_and_again_once_the_session_opens() {
    /// Sessions this test opened, as each search check observes them.
    static OPENED: AtomicUsize = AtomicUsize::new(0);
    fn open(_: &onecrew_sessions::PlatformSessions, _: &str) -> Result<(), String> {
        OPENED.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    let (platform, directory) = platform();
    let reads = AtomicUsize::new(0);

    // An unknown account would be a connection refusal, so a search refusal
    // proves the first check ran before admission.
    let failure = operator_account_read(
        &platform,
        test_session("not-a-uuid", open),
        Some(BoundSearch::from_check(|| {
            Err(SearchPlanApprovalError::InvalidOrExpired)
        })),
        |()| {
            reads.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), XError>(()) }
        },
        XError::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(OPENED.load(Ordering::SeqCst), 0);

    // The approval lapses between the checks. Each check records how many
    // sessions had opened: the first ran before, the second once the admitted
    // session opened, and the platform read never ran.
    let account_ref_id = operator_account(&platform);
    let opened_at_checks = Mutex::new(Vec::new());
    let failure = operator_account_read(
        &platform,
        test_session(account_ref_id.as_str(), open),
        Some(BoundSearch::from_check(|| {
            let mut opened_at_checks = opened_at_checks.lock().unwrap();
            opened_at_checks.push(OPENED.load(Ordering::SeqCst));
            if opened_at_checks.len() == 1 {
                Ok(())
            } else {
                Err(SearchPlanApprovalError::InvalidOrExpired)
            }
        })),
        |()| {
            reads.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), XError>(()) }
        },
        XError::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::InvalidOrExpired)
    );
    assert_eq!(*opened_at_checks.lock().unwrap(), [0, 1]);
    assert_eq!(reads.load(Ordering::SeqCst), 0);
    cleanup(platform, directory);
}

/// X failure shapes, each carrying a provider body and a credentialed proxy
/// address wherever the shape has text.
fn x_errors() -> Vec<XError> {
    let text = || format!("provider body via {PROXY_URL}");
    let http = |status, code| TransportError::Http {
        status,
        code,
        message: text(),
    };
    vec![
        XError::Transport(TransportError::MissingCredential),
        XError::Transport(TransportError::InvalidHeader),
        XError::Transport(TransportError::Network(text())),
        XError::Transport(http(429, Some(88))),
        XError::Transport(TransportError::Api(text())),
        XError::Transport(TransportError::ApiResponse {
            status: 200,
            code: Some(214),
        }),
        XError::Client(XClientError::Transport(http(401, Some(32)))),
        XError::Client(XClientError::UnknownOperation(text())),
        XError::Client(XClientError::Url(text())),
        XError::Client(XClientError::Response(text())),
        XError::Client(XClientError::Input(text())),
        XError::Chat(XChatError::Transport(http(503, None))),
        XError::Chat(XChatError::Client(XClientError::Response(text()))),
        XError::Chat(XChatError::Protocol(text())),
        XError::Chat(XChatError::PremiumRequired),
    ]
}

#[tokio::test]
async fn every_x_failure_crosses_as_its_x_evidence_without_its_text() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);
    for (error, expected) in x_errors().into_iter().zip(x_errors()) {
        let failure = operator_account_read(
            &platform,
            test_session(account_ref_id.as_str(), |_, _| Ok(())),
            None,
            |()| async move { Err::<(), XError>(error) },
            XError::evidence,
        )
        .await
        .unwrap_err();
        assert_eq!(
            failure,
            PlatformReadError::Platform {
                evidence: expected.evidence(),
                after_local_commit: false,
            },
            "{expected:?}"
        );
        assert!(std::error::Error::source(&failure).is_none());
        for text in [format!("{failure:?}"), failure.to_string()] {
            for absent in [
                "provider body",
                "proxy_user",
                "proxy_pass",
                "SENTINEL",
                "token=",
            ] {
                assert!(!text.contains(absent), "{absent}: {text}");
            }
        }
    }
    cleanup(platform, directory);
}

#[tokio::test]
async fn a_session_that_cannot_open_drops_its_diagnostic() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);
    let failure = operator_account_read(
        &platform,
        test_session::<()>(account_ref_id.as_str(), |_, _| {
            Err("Authorization: Bearer BEARER_SENTINEL; Cookie: sid=SID_SENTINEL; csrftoken=CSRF_SENTINEL; proxy http://proxy_user:proxy_pass@gate.example".to_owned())
        }),
        None,
        |()| async { Ok::<(), XError>(()) },
        XError::evidence,
    )
    .await
    .unwrap_err();
    assert_eq!(failure, PlatformReadError::SessionUnavailable);
    for text in [
        failure.safe_message().to_owned(),
        failure.to_string(),
        format!("{failure:?}"),
    ] {
        for absent in ["SENTINEL", "proxy_user", "proxy_pass"] {
            assert!(!text.contains(absent), "{absent}: {text}");
        }
    }

    // The stored X profile holds no session cookies, so its real session
    // cannot open: reads and write admission both report it.
    assert_eq!(
        profile(&platform, account_ref_id.as_str(), "someone")
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    assert!(matches!(
        relationship_update(
            &platform,
            account_ref_id.as_str(),
            onecrew_x::RelationshipAction::Follow,
            "42"
        )
        .await
        .unwrap_err(),
        XToolkitWriteError::Admission(PlatformReadError::SessionUnavailable)
    ));
    cleanup(platform, directory);
}

#[tokio::test]
async fn a_read_after_a_failed_vault_delete_is_refused_by_the_gate() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform);

    // The disconnect claim clears `active` durably and moves to the
    // remote step; the vault delete then fails, leaving the claim open
    // and the vault row in place. Exactly that state must read as
    // refused, not as a still-usable connection.
    platform
        .creators
        .begin_operator_account_disconnect("disconnect-1", &account_ref_id)
        .unwrap();
    platform
        .creators
        .mark_operator_account_disconnect_remote_started("disconnect-1", &account_ref_id)
        .unwrap();
    assert!(
        platform
            .vault
            .get(account_ref_id.as_str().parse().unwrap())
            .is_ok()
    );

    let error = profile(&platform, account_ref_id.as_str(), "someone")
        .await
        .unwrap_err();
    assert_eq!(
        error,
        PlatformReadError::Connection(PlatformConnectionResolutionError::Unavailable)
    );

    cleanup(platform, directory);
}

#[tokio::test]
async fn an_unknown_operator_account_is_not_found_and_empty_inputs_are_invalid() {
    let (platform, directory) = platform();

    assert_eq!(
        profile(&platform, &uuid::Uuid::new_v4().to_string(), "someone")
            .await
            .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    assert_eq!(
        profile(&platform, "not-a-uuid", " @ ").await.unwrap_err(),
        PlatformReadError::InputInvalid("X handle is required")
    );
    assert_eq!(
        user_posts(&platform, "not-a-uuid", "  ", None)
            .await
            .unwrap_err(),
        PlatformReadError::InputInvalid("X user id is required")
    );

    cleanup(platform, directory);
}
