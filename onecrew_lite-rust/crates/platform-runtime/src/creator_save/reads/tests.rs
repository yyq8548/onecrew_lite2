use super::telegram as telegram_reads;
use super::*;
use onecrew_creator_runtime::{ContactChannel, CreatorSaveIntent, LocatorKind, SourceKind};

#[test]
fn a_reassigned_handle_cannot_replace_the_selected_stable_identity() {
    assert!(require_identity("123", "123").is_ok());
    assert!(matches!(
        require_identity("123", "999"),
        Err(CreatorSaveError::IdentityMismatch)
    ));
}

fn telegram_administrators_fixture(
    username: Option<&str>,
    is_bot: bool,
) -> crate::toolkits::telegram::TgPublicAdministrators {
    use crate::toolkits::telegram::{
        TgAdministratorRole, TgCommunityKind, TgPublicAdministrator, TgPublicAdministrators,
        TgPublicCommunity, TgPublicUser,
    };
    TgPublicAdministrators {
        community: TgPublicCommunity {
            external_id: "1688680661".into(),
            title: "Public community".into(),
            username: Some("public_community".into()),
            kind: TgCommunityKind::Supergroup,
        },
        administrators: vec![TgPublicAdministrator {
            role: TgAdministratorRole::Owner,
            user: TgPublicUser {
                id: "42".into(),
                username: username.map(str::to_owned),
                first_name: Some("Community".into()),
                last_name: Some("Owner".into()),
                is_bot,
                verified: true,
            },
        }],
        page_limit: 200,
    }
}

// This is an offline provider-fact transition, not a network-read substitute.
// It exercises the production list projection and the same canonical account
// mapping used after the fresh administrator read in telegram::resolve.
#[tokio::test]
async fn telegram_administrator_refresh_projection_matches_canonical_preparation() {
    use onecrew_creator_runtime::{
        AccountRefId, CanonicalCreatorSaveInput, CommunityAdministratorRole,
        CommunityAdministratorSource, CommunityReference, CommunityTargetKind,
        CreatorRepositoryCallError, CreatorSaveAdministratorEligibility as Eligibility,
        CreatorSaveAdministratorEligibilityReason as Reason, CreatorSavePreparationError,
        CreatorSavePreparationErrorKind, CreatorSaveProvenance, Platform,
    };

    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let creators = platform.creators().unwrap();
    let operator = creators
        .sync_account_ref(
            AccountRefId::new("telegram-transition-operator"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .unwrap();
    let original = telegram_reads::application_page(telegram_administrators_fixture(
        Some("community_owner"),
        false,
    ));
    assert_eq!(original.administrators[0].eligibility, Eligibility::Ready);

    for (username, is_bot, reason, expected_wire) in [
        (
            Some("community_owner"),
            false,
            None,
            serde_json::json!({"status":"ready"}),
        ),
        (
            None,
            false,
            Some(Reason::MissingPublicHandle),
            serde_json::json!({"status":"blocked","reason":"missing_public_handle"}),
        ),
        (
            Some("community_owner"),
            true,
            Some(Reason::Bot),
            serde_json::json!({"status":"blocked","reason":"bot"}),
        ),
        (
            Some("restored_owner"),
            false,
            None,
            serde_json::json!({"status":"ready"}),
        ),
    ] {
        let native = telegram_administrators_fixture(username, is_bot);
        let application = telegram_reads::application_page(native.clone());
        let expected = match reason {
            Some(reason) => Eligibility::Blocked { reason },
            None => Eligibility::Ready,
        };
        assert_eq!(application.administrators[0].eligibility, expected);
        assert_eq!(
            application.administrators[0].user,
            native.administrators[0].user
        );
        assert_eq!(
            application.administrators[0].role,
            native.administrators[0].role
        );

        // The HTTP handler serializes this application output directly. The
        // required eligibility tag and reason must survive that boundary.
        let wire = serde_json::to_value(&application).unwrap();
        assert_eq!(wire["administrators"][0]["eligibility"], expected_wire);
        assert_eq!(wire["administrators"][0]["user"]["id"], "42");
        assert_eq!(wire["administrators"][0]["role"], "owner");
        assert_eq!(wire["community"]["external_id"], "1688680661");
        assert_eq!(wire["page_limit"], 200);

        // Fresh facts are prepared even while the old page still says ready;
        // application eligibility is never an input to the canonical owner.
        assert_eq!(original.administrators[0].eligibility, Eligibility::Ready);
        let canonical = CanonicalCreatorSaveInput {
            row_id: "telegram-administrator-42".into(),
            account: telegram_reads::canonical_account(&native.administrators[0].user),
            profile: Default::default(),
            profile_observed_at: 1_800_000_000,
            provenance: CreatorSaveProvenance::AdministratorSelection {
                source_ref: "telegram:1688680661:42".into(),
                role: CommunityAdministratorRole::Owner,
                community: CommunityReference {
                    external_id: "1688680661".into(),
                    title: "Public community".into(),
                    handle: Some("public_community".into()),
                    target_kind: CommunityTargetKind::Supergroup,
                },
                source: CommunityAdministratorSource::OperatorAccount {
                    operator_account_ref_id: operator.id.clone(),
                },
            },
            contact_evidence: vec![],
            notes: None,
            tags: None,
            quality: Default::default(),
        };
        let prepared = creators
            .prepare_creator_save_batch_async(vec![canonical], CreatorSaveIntent::SaveOrUpdate)
            .await;
        match reason {
            Some(expected_reason) => match prepared.unwrap_err() {
                CreatorRepositoryCallError::Owner(CreatorSavePreparationError {
                    row_id,
                    kind: CreatorSavePreparationErrorKind::TelegramEligibility(reason),
                }) => {
                    assert_eq!(reason, expected_reason);
                    assert_eq!(row_id.as_deref(), Some("telegram-administrator-42"));
                }
                error => panic!("expected typed Telegram eligibility failure, got {error:?}"),
            },
            None => {
                let rows = prepared.unwrap().rows();
                assert_eq!(rows[0].account.external_id.as_deref(), Some("42"));
                assert_eq!(rows[0].account.display_name, "Community Owner");
                assert_eq!(rows[0].account.handle.as_deref(), username);
                assert!(rows[0].route_options.iter().any(|route| {
                    route.required
                        && route.channel == ContactChannel::Telegram
                        && Some(route.normalized_value.as_str()) == username
                }));
            }
        }
        assert!(
            creators
                .list_creators(Default::default())
                .unwrap()
                .items
                .is_empty(),
            "neither blocked nor ready preparation may save a Creator"
        );
    }
}

// youtube-client asserts this public snapshot against raw About HTML and
// playlistItems responses at creator_by_channel_id. Reusing it here connects
// the parser's output to the canonical save seam without injecting contacts.
fn youtube_fixture() -> onecrew_youtube::YoutubeCreator {
    serde_json::from_str(include_str!(
        "../../../../youtube-client/tests/fixtures/creator-profile.json"
    ))
    .unwrap()
}

fn canonical_youtube(
    channel: onecrew_youtube::YoutubeCreator,
) -> onecrew_creator_runtime::CanonicalCreatorSaveInput {
    let read = profiles::youtube(channel);
    let channel_url = read.account.profile_url.clone().unwrap();
    onecrew_creator_runtime::CanonicalCreatorSaveInput {
        row_id: "youtube-row".into(),
        account: read.account,
        profile: read.profile,
        profile_observed_at: 1_800_000_000,
        provenance: onecrew_creator_runtime::CreatorSaveProvenance::DirectSelection {
            source_ref: channel_url.clone(),
            source_url: Some(channel_url),
            description: None,
            operator_account_ref_id: None,
        },
        contact_evidence: read.evidence,
        notes: None,
        tags: None,
        quality: Default::default(),
    }
}

#[tokio::test]
async fn youtube_raw_profile_contacts_survive_canonical_save_update_and_readback() {
    use onecrew_creator_runtime::{CreatorSaveRouteSelection, CreatorSaveRowOutcome};
    use std::collections::BTreeSet;
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let creators = platform.creators().unwrap();
    let mut creator_id = None;
    let channel_url = "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw";
    let about_url = format!("{channel_url}/about");

    for pass in 0..2 {
        let prepared = creators
            .prepare_creator_save_batch_async(
                vec![canonical_youtube(youtube_fixture())],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .await
            .unwrap();
        let rows = prepared.rows();
        let actual: BTreeSet<_> = rows[0]
            .route_options
            .iter()
            .map(|route| route.normalized_value.as_str())
            .collect();
        assert_eq!(
            actual,
            BTreeSet::from([
                channel_url,
                "creator",
                "business@example.com",
                "https://channel.example.com/about",
                "https://www.tiktok.com/@creator",
                "https://www.tiktok.com/@creator.name",
            ]),
            "promo links, mentions, URL fragments and image filenames must not become routes"
        );
        for value in [
            "https://www.tiktok.com/@creator",
            "https://www.tiktok.com/@creator.name",
        ] {
            let route = rows[0]
                .route_options
                .iter()
                .find(|route| route.normalized_value == value)
                .unwrap();
            assert_eq!(route.channel, ContactChannel::Website);
            assert_eq!(route.kind, LocatorKind::Url);
        }
        let selected = prepared
            .select_routes(vec![CreatorSaveRouteSelection {
                row_id: "youtube-row".into(),
                route_ids: rows[0]
                    .route_options
                    .iter()
                    .map(|route| route.route_id.clone())
                    .collect(),
            }])
            .unwrap();
        let result = creators
            .execute_creator_save_batch_async(selected, format!("youtube-source-regression-{pass}"))
            .await
            .unwrap();
        let CreatorSaveRowOutcome::Saved {
            creator, created, ..
        } = &result.outcomes[0]
        else {
            panic!("saved row");
        };
        assert_eq!(*created, pass == 0);
        if let Some(expected) = &creator_id {
            assert_eq!(&creator.creator.id, expected);
        }
        creator_id = Some(creator.creator.id.clone());
        let record = creators.get_creator(&creator.creator.id).unwrap();
        let email = record
            .contact_routes
            .iter()
            .find(|route| route.locator.normalized_value == "business@example.com")
            .unwrap();
        let sources: BTreeSet<_> = email
            .observations
            .iter()
            .map(|observation| {
                (
                    observation.source.kind.as_str(),
                    observation.source.source_url.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            sources,
            BTreeSet::from([
                ("channel_description", Some(about_url.as_str())),
                ("profile_website", Some(about_url.as_str())),
                (
                    "video_description",
                    Some("https://www.youtube.com/watch?v=original")
                ),
                (
                    "video_description",
                    Some("https://www.youtube.com/watch?v=second")
                ),
            ])
        );
        if pass == 0 {
            assert_eq!(
                email.observations.len(),
                4,
                "one observation per source, even when the same address repeats in a description"
            );
        }
        for value in [
            "https://www.tiktok.com/@creator",
            "https://www.tiktok.com/@creator.name",
        ] {
            let route = record
                .contact_routes
                .iter()
                .find(|route| route.locator.normalized_value == value)
                .unwrap();
            assert!(
                route.observations.iter().all(|observation| observation
                    .source
                    .source_url
                    .as_deref()
                    == Some(about_url.as_str()))
            );
        }
    }
}

#[tokio::test]
async fn search_snippets_are_not_evidence_and_unknown_video_sources_stay_unknown() {
    use onecrew_creator_runtime::{
        CreatorSaveContactEvidence, CreatorSaveRouteSelection, CreatorSaveRowOutcome,
    };
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let creators = platform.creators().unwrap();
    let mut channel = youtube_fixture();
    // A search-result snippet is display-only provider data, never evidence.
    channel.matched = Some(onecrew_youtube::YoutubeSearchMatch {
        title: "Source not supplied".into(),
        description: "Business: snippet-only@example.com".into(),
        video_id: None,
        published_at: None,
    });
    let mut canonical = canonical_youtube(channel);
    canonical.profile.description = Some("display-only@example.com".into());
    canonical
        .profile
        .website_urls
        .push("https://display-only.example.com".into());
    canonical.profile.location = Some("location-only@example.com".into());
    canonical
        .contact_evidence
        .push(canonical.contact_evidence[0].clone());
    // A video description whose video is unknown keeps an unknown source.
    canonical
        .contact_evidence
        .push(CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::VideoDescription,
            source_url: None,
            text: "Business: unknown-source@example.com".into(),
        });
    let prepared = creators
        .prepare_creator_save_batch_async(vec![canonical], CreatorSaveIntent::SaveOrUpdate)
        .await
        .unwrap();
    let row = prepared.rows().remove(0);
    assert!(
        !row.route_options
            .iter()
            .any(|route| route.normalized_value.contains("only.example.com")
                || route.normalized_value.contains("only@example.com"))
    );
    let selected = prepared
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: row.row_id,
            route_ids: row
                .route_options
                .into_iter()
                .map(|route| route.route_id)
                .collect(),
        }])
        .unwrap();
    let result = creators
        .execute_creator_save_batch_async(selected, "youtube-unknown-source".into())
        .await
        .unwrap();
    let CreatorSaveRowOutcome::Saved { creator, .. } = &result.outcomes[0] else {
        panic!("saved row");
    };
    let record = creators.get_creator(&creator.creator.id).unwrap();
    let route = record
        .contact_routes
        .iter()
        .find(|route| route.locator.normalized_value == "unknown-source@example.com")
        .unwrap();
    assert_eq!(route.observations.len(), 1);
    assert_eq!(
        route.observations[0].source.kind,
        SourceKind::VideoDescription
    );
    assert_eq!(route.observations[0].source.source_url, None);
    let email = record
        .contact_routes
        .iter()
        .find(|route| route.locator.normalized_value == "business@example.com")
        .unwrap();
    assert_eq!(email.observations.len(), 4);
}

#[test]
fn wire_prepare_input_cannot_smuggle_context_or_prepared_payload() {
    for field in ["origin", "confirmed", "prepared", "operation_id"] {
        let value = serde_json::json!({ "candidates": [], field: "forged" });
        assert!(serde_json::from_value::<crate::PrepareCreatorSaveInput>(value).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn full_batch_profile_reads_use_four_slots_and_preserve_input_order() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let completed = Arc::new(AtomicUsize::new(0));
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let reads = (0..count).map(|index| {
        let (active, maximum, completed) = (active.clone(), maximum.clone(), completed.clone());
        async move {
            let running = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(running, Ordering::SeqCst);
            // Later entries often complete first, so ordering is observable.
            tokio::time::sleep(std::time::Duration::from_millis(4 - (index % 4) as u64)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            completed.fetch_add(1, Ordering::SeqCst);
            if index == count / 2 {
                Err(index)
            } else {
                Ok(index)
            }
        }
    });
    let results = collect_profile_reads(reads).await;
    assert_eq!(maximum.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(completed.load(Ordering::SeqCst), count);
    assert_eq!(results.len(), count);
    for (index, result) in results.into_iter().enumerate() {
        assert_eq!(result.unwrap_or_else(|failed| failed), index);
    }
}
