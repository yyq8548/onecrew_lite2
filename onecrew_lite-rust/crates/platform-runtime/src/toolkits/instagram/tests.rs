use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use onecrew_creator_runtime::AccountRefId;
use onecrew_ig::{IgError, IgUser};
use onecrew_local_domain::{Platform, ProfileSecret};
use serde_json::Value;

use super::{
    IgRelationshipAction, IgToolkitWriteError, USER_SEARCH_DEFAULT_COUNT, USER_SEARCH_MAX_COUNT,
    USER_SEARCH_MIN_COUNT, UsersSearchInput, instagram_read, profile, relationship_get,
    relationship_update, search_users, users_native_data,
};
use crate::{
    BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform,
    PlatformConnectionResolutionError, SearchPlanApprovalError, SearchPlanPolicy,
    toolkits::{
        PlatformReadError,
        read::{BoundSearch, OperatorSession, platform_native_data},
    },
};

const PROXY_URL: &str = "http://proxy_user:proxy_pass@gate.example:8080?token=SENTINEL";

fn platform() -> (LocalPlatform, std::path::PathBuf) {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-instagram-toolkit-test-{}",
        uuid::Uuid::new_v4()
    ));
    (LocalPlatform::open(&directory).unwrap(), directory)
}

fn cleanup(platform: LocalPlatform, directory: std::path::PathBuf) {
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

fn search_input() -> UsersSearchInput {
    UsersSearchInput {
        operator_account_id: uuid::Uuid::new_v4().to_string(),
        query: "rust".into(),
        count: 30,
    }
}

fn direct_authorization(input: &UsersSearchInput) -> BoundSearchAuthorization {
    BoundSearchAuthorization::bind(input, SearchPlanPolicy::UserRequestedDirect, None, None)
        .unwrap()
}

/// A stored operator profile of `platform` whose secret can be read but holds
/// no session material: admission accepts it for its own platform, and
/// opening its session fails before any network call.
fn operator_account(runtime: &LocalPlatform, platform: Platform) -> AccountRefId {
    let profile = runtime
        .vault
        .upsert(
            platform,
            "gated_operator",
            &ProfileSecret {
                provider_account_id: "provider-account".into(),
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
    runtime.sync_profile_account_ref(&profile).unwrap();
    AccountRefId::new(profile.id.to_string())
}

/// An Instagram session description whose opening the test decides, so a read
/// can reach the Instagram call without a stored session.
fn test_session<S>(
    operator_account_id: &str,
    open: fn(&onecrew_sessions::PlatformSessions, &str) -> Result<S, String>,
) -> OperatorSession<'_, S> {
    OperatorSession {
        platform: OperatorAccountPlatform::Instagram,
        operator_account_id,
        open,
    }
}

#[tokio::test]
async fn search_rejects_a_blank_query_and_an_out_of_range_page_size_before_account_admission() {
    let (platform, directory) = platform();
    let query_refused = PlatformReadError::InputInvalid("Instagram search query is required");
    let count_refused =
        PlatformReadError::InputInvalid("Instagram search count must be between 1 and 100");
    // The operator account is unknown, so an input that passes validation is
    // refused by admission instead.
    let not_found =
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch);
    for (query, count, expected) in [
        ("", USER_SEARCH_DEFAULT_COUNT, &query_refused),
        (" \t\n", USER_SEARCH_DEFAULT_COUNT, &query_refused),
        ("rust", USER_SEARCH_MIN_COUNT - 1, &count_refused),
        ("rust", USER_SEARCH_MIN_COUNT, &not_found),
        ("  rust  ", USER_SEARCH_DEFAULT_COUNT, &not_found),
        ("rust", USER_SEARCH_MAX_COUNT, &not_found),
        ("rust", USER_SEARCH_MAX_COUNT + 1, &count_refused),
    ] {
        let input = UsersSearchInput {
            query: query.into(),
            count,
            ..search_input()
        };
        let authorization = direct_authorization(&input);
        assert_eq!(
            &search_users(&platform, input, authorization)
                .await
                .unwrap_err(),
            expected,
            "{query:?} {count}"
        );
    }
    cleanup(platform, directory);
}

#[tokio::test]
async fn search_rejects_changed_bound_fields_before_account_admission() {
    let (platform, directory) = platform();
    let original = search_input();
    for input in [
        UsersSearchInput {
            query: "changed".into(),
            ..original.clone()
        },
        UsersSearchInput {
            count: 31,
            ..original.clone()
        },
        UsersSearchInput {
            operator_account_id: uuid::Uuid::new_v4().to_string(),
            ..original.clone()
        },
    ] {
        assert_eq!(
            search_users(&platform, input, direct_authorization(&original))
                .await
                .unwrap_err(),
            PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::ScopeMismatch)
        );
    }
    assert_eq!(
        search_users(&platform, original.clone(), direct_authorization(&original))
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
    let failure = instagram_read(
        &platform,
        test_session("not-a-uuid", open),
        Some(BoundSearch::from_check(|| {
            Err(SearchPlanApprovalError::InvalidOrExpired)
        })),
        |()| {
            reads.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), IgError>(()) }
        },
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
    // session opened, and the Instagram read never ran.
    let account_ref_id = operator_account(&platform, Platform::Instagram);
    let opened_at_checks = Mutex::new(Vec::new());
    let failure = instagram_read(
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
            async { Ok::<(), IgError>(()) }
        },
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

/// Every Instagram failure shape, each carrying a provider body, session
/// cookies and a credentialed proxy address wherever the shape has text.
fn instagram_errors() -> Vec<IgError> {
    let text = || format!("provider body; Cookie: sessionid=SID_SENTINEL; via {PROXY_URL}");
    vec![
        IgError::Session(text()),
        IgError::Network(text()),
        IgError::Api(text()),
        IgError::Challenge(text()),
        IgError::Response {
            status: 429,
            code: Some("rate_limit_error".into()),
        },
        IgError::Response {
            status: 404,
            code: None,
        },
        IgError::InvalidHeader,
    ]
}

#[tokio::test]
async fn every_instagram_failure_crosses_as_its_instagram_evidence_without_its_text() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform, Platform::Instagram);
    for (error, expected) in instagram_errors().into_iter().zip(instagram_errors()) {
        let failure = instagram_read(
            &platform,
            test_session(account_ref_id.as_str(), |_, _| Ok(())),
            None,
            |()| async move { Err::<(), IgError>(error) },
        )
        .await
        .unwrap_err();
        assert_eq!(
            failure,
            PlatformReadError::Platform {
                evidence: onecrew_ig::evidence(&expected),
                after_local_commit: false,
            },
            "{expected:?}"
        );
        assert!(std::error::Error::source(&failure).is_none());
        for text in [format!("{failure:?}"), failure.to_string()] {
            for absent in [
                "provider body",
                "sessionid",
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

#[test]
fn instagram_native_data_is_the_serde_serialization_of_each_instagram_user() {
    let creator = IgUser {
        id: "9007199254740993".into(),
        username: "creator".into(),
        full_name: "Creator".into(),
        is_private: Some(false),
        is_verified: Some(true),
        profile_pic_url: None,
        follower_count: Some(42),
        following_count: Some(7),
        media_count: None,
        biography: Some("Rust creator".into()),
        external_url: Some("https://creator.example".into()),
        bio_links: vec!["https://links.example/creator".into()],
        public_email: Some("creator@example.com".into()),
        city_name: None,
        address_street: None,
    };
    let other = IgUser {
        id: "42".into(),
        username: "other".into(),
        full_name: String::new(),
        is_private: None,
        is_verified: None,
        profile_pic_url: None,
        follower_count: None,
        following_count: None,
        media_count: None,
        biography: None,
        external_url: None,
        bio_links: Vec::new(),
        public_email: None,
        city_name: None,
        address_street: None,
    };
    assert_eq!(
        Value::Object(platform_native_data(&creator).unwrap()),
        serde_json::to_value(&creator).unwrap()
    );
    assert_eq!(
        users_native_data(&[creator.clone(), other.clone()])
            .unwrap()
            .into_iter()
            .map(Value::Object)
            .collect::<Vec<_>>(),
        vec![
            serde_json::to_value(&creator).unwrap(),
            serde_json::to_value(&other).unwrap()
        ]
    );
    assert!(users_native_data(&[]).unwrap().is_empty());
}

#[tokio::test]
async fn a_session_that_cannot_open_drops_its_diagnostic() {
    let (platform, directory) = platform();
    let account_ref_id = operator_account(&platform, Platform::Instagram);
    let failure = instagram_read(
        &platform,
        test_session::<()>(account_ref_id.as_str(), |_, _| {
            Err("Authorization: Bearer BEARER_SENTINEL; Cookie: sessionid=SID_SENTINEL; csrftoken=CSRF_SENTINEL; proxy http://proxy_user:proxy_pass@gate.example".to_owned())
        }),
        None,
        |()| async { Ok::<(), IgError>(()) },
    )
    .await
    .unwrap_err();
    assert_eq!(failure, PlatformReadError::SessionUnavailable);
    for text in [
        failure.safe_message().to_owned(),
        failure.to_string(),
        format!("{failure:?}"),
    ] {
        for absent in ["SENTINEL", "sessionid", "proxy_user", "proxy_pass"] {
            assert!(!text.contains(absent), "{absent}: {text}");
        }
    }

    // The stored Instagram profile holds no session, so its real session
    // cannot open: every read and write admission report it.
    let account = account_ref_id.as_str();
    assert_eq!(
        profile(&platform, account, "someone").await.unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    assert_eq!(
        relationship_get(&platform, account, "42")
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    let input = UsersSearchInput {
        operator_account_id: account.to_owned(),
        ..search_input()
    };
    let authorization = direct_authorization(&input);
    assert_eq!(
        search_users(&platform, input, authorization)
            .await
            .unwrap_err(),
        PlatformReadError::SessionUnavailable
    );
    assert!(matches!(
        relationship_update(&platform, account, IgRelationshipAction::Follow, "42")
            .await
            .unwrap_err(),
        IgToolkitWriteError::Admission(PlatformReadError::SessionUnavailable)
    ));
    cleanup(platform, directory);
}

#[tokio::test]
async fn an_x_account_is_not_an_instagram_connection_and_inputs_are_checked_first() {
    let (platform, directory) = platform();
    let x_account = operator_account(&platform, Platform::X);

    assert_eq!(
        profile(&platform, x_account.as_str(), "someone")
            .await
            .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    assert_eq!(
        profile(&platform, &uuid::Uuid::new_v4().to_string(), "someone")
            .await
            .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );
    assert_eq!(
        profile(&platform, "not-a-uuid", " @ ").await.unwrap_err(),
        PlatformReadError::InputInvalid("Instagram handle is required")
    );
    for stable_id in ["", "@creator", "4a2", " 42"] {
        assert_eq!(
            relationship_get(&platform, "not-a-uuid", stable_id)
                .await
                .unwrap_err(),
            PlatformReadError::InputInvalid("Instagram stable user ID is required"),
            "{stable_id:?}"
        );
    }
    assert_eq!(
        relationship_get(&platform, "not-a-uuid", "42")
            .await
            .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::InvalidIdentifier)
    );

    cleanup(platform, directory);
}
