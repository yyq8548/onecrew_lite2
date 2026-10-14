use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use super::{
    ChannelsSearchInput, SEARCH_DEFAULT_COUNT, SEARCH_MAX_COUNT, SEARCH_MIN_COUNT,
    VideosSearchInput, YoutubeSearchOrder, channel, channels_search, commit_health,
    credential_read, credential_read_opened_by, videos_search,
};
use crate::{
    BoundSearchAuthorization, LocalPlatform, PlatformConnectionResolutionError,
    SearchPlanApprovalError, SearchPlanPolicy,
    credentials::services::{CredentialServices, YoutubeCredentialReadError},
    toolkits::{
        PlatformReadError,
        read::{BoundSearch, platform_native_data},
    },
};
use onecrew_creator_runtime::AccountRefId;
use onecrew_local_domain::{ServiceCredential, ServiceCredentialSecret, ServiceProvider};
use onecrew_local_vault::LocalVault;
use onecrew_youtube::{
    YoutubeApiFailureKind, YoutubeClientError, YoutubeCreator, YoutubePlatformError,
    YoutubeReadDepth, YoutubeRequestFailureKind, YoutubeSearchPage, YoutubeTransportError,
};
use serde_json::Value;

const CHANNEL_ID: &str = "UC_x5XG1OV2P6uZZ5FSM9Ttw";
const PROXY_URL: &str = "http://proxy_user:proxy_pass@gate.example:8080?key=SENTINEL";

fn creator() -> YoutubeCreator {
    YoutubeCreator {
        channel_id: CHANNEL_ID.into(),
        channel_url: format!("https://www.youtube.com/channel/{CHANNEL_ID}"),
        title: "Channel".into(),
        thumbnail_url: None,
        description: String::new(),
        description_source_url: None,
        custom_url: None,
        country: None,
        subscriber_count: Some(10),
        view_count: None,
        video_count: None,
        matched: None,
        recent_videos: Vec::new(),
        public_comments: Vec::new(),
        about_links: Vec::new(),
        unavailable_sections: Vec::new(),
        hidden_business_email: false,
        omitted_sections: Vec::new(),
    }
}

fn services(vault: LocalVault) -> CredentialServices {
    CredentialServices::new(
        vault,
        Arc::new(onecrew_email::ResendAdmission::new().unwrap()),
    )
}

fn credential(vault: &LocalVault) -> ServiceCredential {
    vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "read",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("AIza-test"),
        )
        .unwrap()
}

/// A stored YouTube credential that connection admission accepts. Opening it
/// builds a client and makes no request.
fn admitted_credential(platform: &LocalPlatform) -> ServiceCredential {
    let credential = credential(&platform.vault);
    platform.reconcile_vault_youtube_refs().unwrap();
    credential
}

fn stored(vault: &LocalVault) -> ServiceCredential {
    vault
        .list_service_credentials(ServiceProvider::Youtube, false)
        .unwrap()
        .pop()
        .unwrap()
}

fn direct_authorization<T: serde::Serialize + 'static>(input: &T) -> BoundSearchAuthorization {
    BoundSearchAuthorization::bind(input, SearchPlanPolicy::UserRequestedDirect, None, None)
        .unwrap()
}

fn channels_input(query: &str, count: u8) -> ChannelsSearchInput {
    ChannelsSearchInput {
        credential_id: "invalid-credential-id".into(),
        query: query.into(),
        count,
        order: YoutubeSearchOrder::Relevance,
        published_after: None,
        cursor: Some("opaque+/=cursor".into()),
        region_code: Some("TR".into()),
        language: Some("tr".into()),
    }
}

fn videos_input(query: &str, count: u8) -> VideosSearchInput {
    VideosSearchInput {
        credential_id: "invalid-credential-id".into(),
        query: query.into(),
        count,
        order: YoutubeSearchOrder::Relevance,
        published_after: None,
        cursor: Some("opaque+/=cursor".into()),
        region_code: Some("TR".into()),
        language: Some("tr".into()),
    }
}

/// YouTube failure shapes, each carrying a provider body and a credentialed
/// proxy address wherever the shape has text.
fn youtube_errors() -> Vec<YoutubePlatformError> {
    let text = || format!("provider body via {PROXY_URL}");
    let api = |status, failure_kind, reason: Option<&str>| YoutubeTransportError::Api {
        status,
        failure_kind,
        reason: reason.map(str::to_owned),
    };
    vec![
        YoutubePlatformError::Transport(api(
            403,
            YoutubeApiFailureKind::QuotaExceeded,
            Some("quotaExceeded"),
        )),
        YoutubePlatformError::Client(YoutubeClientError::Transport(api(
            400,
            YoutubeApiFailureKind::CredentialRejected,
            Some("keyInvalid"),
        ))),
        YoutubePlatformError::Transport(api(503, YoutubeApiFailureKind::Server, None)),
        YoutubePlatformError::Transport(YoutubeTransportError::Request {
            operation: "sending API request",
            failure_kind: YoutubeRequestFailureKind::Timeout,
        }),
        YoutubePlatformError::Transport(YoutubeTransportError::Request {
            operation: "reading API response",
            failure_kind: YoutubeRequestFailureKind::Decode,
        }),
        YoutubePlatformError::Transport(YoutubeTransportError::BodyTooLarge { limit: 1024 }),
        YoutubePlatformError::Client(YoutubeClientError::ChannelNotFound),
        YoutubePlatformError::Client(YoutubeClientError::InvalidChannelId),
        YoutubePlatformError::Client(YoutubeClientError::Parse(text())),
    ]
}

#[test]
fn concrete_search_inputs_preserve_native_criteria_and_require_count() {
    let channel_input = channels_input("technology", 50);
    let encoded = serde_json::to_value(&channel_input).unwrap();
    assert_eq!(encoded["count"], 50);
    assert_eq!(encoded["cursor"], "opaque+/=cursor");
    assert_eq!(encoded["region_code"], "TR");
    assert_eq!(encoded["language"], "tr");
    let decoded: ChannelsSearchInput = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded.query, "technology");
    assert_eq!(decoded.cursor, channel_input.cursor);
    let video_input = videos_input("technology", 1);
    let encoded_video = serde_json::to_value(&video_input).unwrap();
    let decoded_video: VideosSearchInput = serde_json::from_value(encoded_video).unwrap();
    assert_eq!(decoded_video.count, 1);
    assert_eq!(decoded_video.region_code, video_input.region_code);
    let mut missing_count = encoded.clone();
    missing_count.as_object_mut().unwrap().remove("count");
    assert!(serde_json::from_value::<ChannelsSearchInput>(missing_count.clone()).is_err());
    assert!(serde_json::from_value::<VideosSearchInput>(missing_count).is_err());
    let mut unknown_criterion = encoded;
    unknown_criterion["kind"] = serde_json::json!("creator");
    assert!(serde_json::from_value::<ChannelsSearchInput>(unknown_criterion).is_err());
}

#[tokio::test]
async fn search_inputs_are_checked_with_youtube_constraints_before_credential_admission() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let count_refused =
        PlatformReadError::InputInvalid("YouTube search count must be between 1 and 50");
    let query_refused = PlatformReadError::InputInvalid("YouTube search query is required");
    // Accepted inputs reach connection admission; the deliberately bad
    // identifier proves they never need a provider connection.
    let admitted =
        PlatformReadError::Connection(PlatformConnectionResolutionError::InvalidIdentifier);
    for (query, count, expected) in [
        ("technology", SEARCH_MIN_COUNT - 1, &count_refused),
        ("technology", SEARCH_MIN_COUNT, &admitted),
        ("technology", SEARCH_DEFAULT_COUNT, &admitted),
        ("technology", SEARCH_MAX_COUNT, &admitted),
        ("technology", SEARCH_MAX_COUNT + 1, &count_refused),
        (" \n\t", 10, &query_refused),
    ] {
        let input = channels_input(query, count);
        let authorization = direct_authorization(&input);
        assert_eq!(
            &channels_search(&platform, input, authorization)
                .await
                .unwrap_err(),
            expected,
            "channels {query:?}/{count}"
        );
        let input = videos_input(query, count);
        let authorization = direct_authorization(&input);
        assert_eq!(
            &videos_search(&platform, input, authorization)
                .await
                .unwrap_err(),
            expected,
            "videos {query:?}/{count}"
        );
    }
}

#[tokio::test]
async fn changed_search_requests_are_rejected_before_credential_admission() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let mut input = channels_input("technology", 10);
    let authorization = direct_authorization(&input);
    input.query = "different query".into();
    assert_eq!(
        channels_search(&platform, input, authorization)
            .await
            .unwrap_err(),
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::ScopeMismatch)
    );
    let mut input = videos_input("technology", 10);
    let authorization = direct_authorization(&input);
    input.region_code = Some("US".into());
    assert_eq!(
        videos_search(&platform, input, authorization)
            .await
            .unwrap_err(),
        PlatformReadError::SearchPlanRefused(SearchPlanApprovalError::ScopeMismatch)
    );
}

#[tokio::test]
async fn a_bound_search_is_checked_before_admission_and_again_once_the_credential_opens() {
    /// Credentials this test opened, as each search check observes them.
    static OPENED: AtomicUsize = AtomicUsize::new(0);
    fn open(
        services: &CredentialServices,
        credential_id: &str,
    ) -> Result<crate::credentials::services::YoutubeCredentialRead, YoutubeCredentialReadError>
    {
        let opened = services.youtube_platform_for_read(credential_id)?;
        OPENED.fetch_add(1, Ordering::SeqCst);
        Ok(opened)
    }
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let reads = AtomicUsize::new(0);

    // An invalid identifier would be a connection refusal, so a search
    // refusal proves the first check ran before admission.
    let failure = credential_read_opened_by(
        &platform,
        "not-a-uuid",
        open,
        Some(BoundSearch::from_check(|| {
            Err(SearchPlanApprovalError::InvalidOrExpired)
        })),
        |_| {
            reads.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), YoutubePlatformError>(()) }
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
    // credentials had opened: the first ran before, the second once the
    // admitted credential opened, YouTube was never called, and no health was
    // recorded.
    let credential = admitted_credential(&platform);
    let opened_at_checks = Mutex::new(Vec::new());
    let failure = credential_read_opened_by(
        &platform,
        &credential.id.to_string(),
        open,
        Some(BoundSearch::from_check(|| {
            let mut opened_at_checks = opened_at_checks.lock().unwrap();
            opened_at_checks.push(OPENED.load(Ordering::SeqCst));
            if opened_at_checks.len() == 1 {
                Ok(())
            } else {
                Err(SearchPlanApprovalError::InvalidOrExpired)
            }
        })),
        |_| {
            reads.fetch_add(1, Ordering::SeqCst);
            async { Ok::<(), YoutubePlatformError>(()) }
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
    let stored = stored(&platform.vault);
    assert!(stored.last_used_at.is_none());
    assert!(stored.last_error.is_none());
}

#[tokio::test]
async fn a_read_returns_its_outcome_only_after_the_credential_health_is_committed() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let credential = admitted_credential(&platform);
    let credential_id = credential.id.to_string();

    let returned = credential_read(&platform, &credential_id, None, |_| async {
        Ok::<_, YoutubePlatformError>(creator())
    })
    .await
    .unwrap();
    assert_eq!(returned.channel_id, CHANNEL_ID);
    let stored_after_success = stored(&platform.vault);
    assert!(stored_after_success.last_used_at.is_some());
    assert!(stored_after_success.last_error.is_none());

    let quota = || {
        YoutubePlatformError::Transport(YoutubeTransportError::Api {
            status: 403,
            failure_kind: YoutubeApiFailureKind::QuotaExceeded,
            reason: Some("quotaExceeded".to_owned()),
        })
    };
    let failure = credential_read(&platform, &credential_id, None, |_| async {
        Err::<YoutubeCreator, _>(quota())
    })
    .await
    .unwrap_err();
    assert_eq!(
        failure,
        PlatformReadError::Platform {
            evidence: quota().evidence(),
            after_local_commit: true,
        }
    );
    let stored = stored(&platform.vault);
    let last_error = stored.last_error.expect("the failure health is committed");
    assert_eq!(last_error, "YouTube API quota is exhausted");
    // The reason token stays in the evidence and never reaches the vault.
    assert!(!last_error.contains("quotaExceeded"));
    assert!(stored.cooldown_until.is_some());
    assert!(!stored.disabled);
}

#[test]
fn a_stale_read_health_result_cannot_replace_newer_credential_state() {
    let directory = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
    let credential = credential(&vault);
    let opened_revision = credential.updated_at.clone();
    let services = services(vault.clone());

    vault
        .mark_service_credential_failure(credential.id, "newer transient failure", false, None)
        .unwrap();
    let returned = commit_health::<YoutubeCreator>(
        &services,
        credential.id,
        &opened_revision,
        1,
        Ok(creator()),
    )
    .expect("provider data remains usable when its stale health result is ignored");
    assert_eq!(returned.channel_id, CHANNEL_ID);
    assert_eq!(
        stored(&vault).last_error.as_deref(),
        Some("newer transient failure")
    );

    let second = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "second-revision",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("AIza-second"),
        )
        .unwrap();
    let second_revision = second.updated_at.clone();
    vault.mark_service_credential_success(second.id).unwrap();
    let failure = commit_health::<YoutubeCreator>(
        &services,
        second.id,
        &second_revision,
        2,
        Err(YoutubePlatformError::Client(
            YoutubeClientError::ChannelNotFound,
        )),
    )
    .expect_err("a stale failure still reports the provider result");
    assert!(matches!(
        failure,
        PlatformReadError::Platform {
            after_local_commit: false,
            ..
        }
    ));
    assert!(stored(&vault).last_error.is_none());
}

#[test]
fn every_youtube_failure_crosses_as_its_evidence_after_the_local_commit() {
    let directory = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
    let credential = credential(&vault);
    let services = services(vault.clone());
    for (observation, (error, expected)) in youtube_errors()
        .into_iter()
        .zip(youtube_errors())
        .enumerate()
        .map(|(index, pair)| (index as u64 + 1, pair))
    {
        let revision = stored(&vault).updated_at;
        let failure = commit_health::<YoutubeCreator>(
            &services,
            credential.id,
            &revision,
            observation,
            Err(error),
        )
        .unwrap_err();
        assert_eq!(
            failure,
            PlatformReadError::Platform {
                evidence: expected.evidence(),
                after_local_commit: true,
            },
            "{expected:?}"
        );
        assert!(std::error::Error::source(&failure).is_none());
        let last_error = stored(&vault).last_error.expect("health is committed");
        for text in [
            format!("{failure:?}"),
            failure.to_string(),
            last_error.clone(),
        ] {
            for absent in [
                "provider body",
                "proxy_user",
                "proxy_pass",
                "gate.example",
                "SENTINEL",
                "key=",
            ] {
                assert!(!text.contains(absent), "{absent}: {text}");
            }
        }
        // The reason token is evidence only; the recorded health is static.
        for reason in ["keyInvalid", "quotaExceeded"] {
            assert!(!last_error.contains(reason), "{reason}: {last_error}");
        }
    }
}

#[test]
fn an_evidence_carries_only_its_reason_token_and_never_request_text() {
    let directory = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
    let credential = credential(&vault);
    let failure = commit_health::<YoutubeCreator>(
        &services(vault),
        credential.id,
        &credential.updated_at,
        1,
        Err(YoutubePlatformError::Transport(
            YoutubeTransportError::Api {
                status: 400,
                failure_kind: YoutubeApiFailureKind::CredentialRejected,
                reason: Some("keyInvalid".to_owned()),
            },
        )),
    )
    .unwrap_err();
    let PlatformReadError::Platform { evidence, .. } = &failure else {
        panic!("a YouTube failure crosses as its evidence: {failure:?}");
    };
    assert_eq!(
        (evidence.http_status, evidence.code.as_deref()),
        (Some(400), Some("keyInvalid"))
    );
    // Neither the stored API key nor any request URL, query or body crosses
    // with the token.
    for text in [
        serde_json::to_string(evidence).unwrap(),
        failure.safe_message().to_owned(),
        format!("{failure:?}"),
        failure.to_string(),
    ] {
        for absent in [
            "AIza-test",
            "key=",
            "googleapis",
            "youtube/v3",
            "https://",
            "http://",
            "?",
            "body",
        ] {
            assert!(!text.contains(absent), "{absent}: {text}");
        }
    }
}

#[test]
fn a_search_page_is_returned_unchanged_only_after_its_health_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent-platform.db");
    let vault = LocalVault::open(&path).unwrap();
    let credential = credential(&vault);
    let page = YoutubeSearchPage {
        items: Vec::new(),
        channels: BTreeMap::new(),
        next_page_token: Some("opaque+/=cursor".into()),
    };
    let returned = commit_health(
        &services(vault.clone()),
        credential.id,
        &credential.updated_at,
        1,
        Ok(page.clone()),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(returned).unwrap(),
        serde_json::to_value(&page).unwrap()
    );
    assert!(stored(&vault).last_used_at.is_some());
    let read_only = services(LocalVault::open_existing_read_only(&path).unwrap());
    let stale_success = commit_health(
        &read_only,
        credential.id,
        &credential.updated_at,
        2,
        Ok(page.clone()),
    )
    .expect("an old successful read remains usable when its health mark is superseded");
    assert_eq!(
        serde_json::to_value(stale_success).unwrap(),
        serde_json::to_value(&page).unwrap()
    );
    let current_revision = stored(&vault).updated_at;
    assert_eq!(
        commit_health::<YoutubeSearchPage>(
            &read_only,
            credential.id,
            &current_revision,
            3,
            Err(YoutubePlatformError::Client(
                YoutubeClientError::ChannelNotFound
            ))
        )
        .unwrap_err(),
        PlatformReadError::CredentialHealthCommitUnknown
    );
}

#[test]
fn an_uncommittable_health_mark_leaves_the_outcome_unknown_for_success_and_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("agent-platform.db");
    let vault = LocalVault::open(&path).unwrap();
    let credential = credential(&vault);
    // A read-only vault accepts no health write.
    let read_only = services(LocalVault::open_existing_read_only(&path).unwrap());

    assert_eq!(
        commit_health(
            &read_only,
            credential.id,
            &credential.updated_at,
            1,
            Ok(creator())
        )
        .map(|creator| creator.channel_id)
        .unwrap_err(),
        PlatformReadError::CredentialHealthCommitUnknown
    );
    assert_eq!(
        commit_health::<YoutubeCreator>(
            &read_only,
            credential.id,
            &credential.updated_at,
            2,
            Err(YoutubePlatformError::Client(
                YoutubeClientError::ChannelNotFound
            )),
        )
        .map(|creator| creator.channel_id)
        .unwrap_err(),
        PlatformReadError::CredentialHealthCommitUnknown
    );
    let stored = stored(&vault);
    assert!(stored.last_used_at.is_none());
    assert!(stored.last_error.is_none());
}

#[test]
fn platform_native_data_is_the_serde_serialization_of_the_youtube_models() {
    let creator = creator();
    assert_eq!(
        Value::Object(platform_native_data(&creator).unwrap()),
        serde_json::to_value(&creator).unwrap()
    );
    let page = YoutubeSearchPage {
        items: Vec::new(),
        channels: BTreeMap::new(),
        next_page_token: Some("provider+/cursor==".into()),
    };
    assert_eq!(
        Value::Object(platform_native_data(&page).unwrap()),
        serde_json::to_value(&page).unwrap()
    );

    // A success is committed before its data is serialized, and data that is
    // not an object is still that local outcome, not a failure after the
    // commit.
    let directory = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
    let credential = credential(&vault);
    let list = commit_health(
        &services(vault.clone()),
        credential.id,
        &credential.updated_at,
        1,
        Ok(vec![creator.clone()]),
    )
    .unwrap();
    assert!(stored(&vault).last_used_at.is_some());
    assert_eq!(
        platform_native_data(&list).unwrap_err(),
        PlatformReadError::NativeDataNotObject
    );
}

#[tokio::test]
async fn a_read_after_a_failed_vault_delete_is_refused_by_the_gate() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let credential = admitted_credential(&platform);
    let account_ref_id = AccountRefId::new(credential.id.to_string());

    // The delete claim clears `active` durably; the vault delete then
    // fails and the row stays. The gate refuses the read.
    platform
        .creators
        .begin_service_credential_delete("delete-1", &account_ref_id)
        .unwrap();
    assert!(platform.vault.get_service_credential(credential.id).is_ok());

    assert_eq!(
        channel(
            &platform,
            account_ref_id.as_str(),
            CHANNEL_ID,
            YoutubeReadDepth::Full,
        )
        .await
        .unwrap_err(),
        PlatformReadError::Connection(PlatformConnectionResolutionError::Unavailable)
    );
    assert_eq!(
        channel(
            &platform,
            account_ref_id.as_str(),
            "not-a-channel",
            YoutubeReadDepth::Full,
        )
        .await
        .unwrap_err(),
        PlatformReadError::InputInvalid(
            "YouTube channel id must be a stable 24-character UC identifier"
        )
    );
}
