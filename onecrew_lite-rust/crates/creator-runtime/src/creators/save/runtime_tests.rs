use crate::test_support::{save_fixture, x_save_entry};
use crate::{
    AccountRefId, CommunityAdministratorRole, CommunityTargetKind, Confidence, ContactChannel,
    ContactLocatorInput, CreateCreatorInput, CreatorCandidateOrigin, CreatorChangeScope,
    CreatorGateways, CreatorRuntime, CreatorSaveRouteSelection, CreatorSaveRowOutcome, LinkState,
    LocatorKind, Platform, ResolveContactTargetInput, TargetKind, TargetResolutionStatus,
    UpsertPlatformAccountInput,
};

#[test]
fn exact_existing_creator_owns_required_route_without_a_collision() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("self-owned-route"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let input = x_save_entry(account_ref.id, false);
    let first = save_fixture(&runtime, input.clone(), "self-owned-initial")
        .expect("the initial Creator save should succeed");
    assert!(matches!(
        first,
        CreatorSaveRowOutcome::Saved { created: true, .. }
    ));

    let prepared = runtime
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .expect("an exact existing Creator should remain reviewable");
    let row = prepared.rows().into_iter().next().expect("prepared row");
    assert!(matches!(
        row.identity_resolution,
        crate::IdentityResolution::ExactExistingCreator { .. }
    ));
    let required = row
        .route_options
        .iter()
        .find(|route| route.required)
        .expect("initial platform route");
    assert_eq!(
        required.collision,
        crate::ContactRouteCollision::None,
        "the Creator's own route is not an external collision"
    );
}

#[test]
fn add_new_skips_an_exact_existing_creator_while_update_intent_still_updates_it() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("explicit-save-intent"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let input = x_save_entry(account_ref.id, false);
    let first = save_fixture(&runtime, input.clone(), "intent-initial")
        .expect("the initial Creator save should succeed");
    let first_id = match first {
        CreatorSaveRowOutcome::Saved {
            creator, created, ..
        } => {
            assert!(created);
            creator.creator.id
        }
        _ => panic!("initial save should create a Creator"),
    };

    let add_new = runtime
        .prepare_creator_save_batch(vec![input.clone()], crate::CreatorSaveIntent::AddNew)
        .expect("add-new review");
    assert_eq!(
        add_new.rows()[0].status,
        crate::CreatorSavePreparedStatus::ExistingSkipped
    );
    assert!(matches!(
        add_new.select_routes(vec![crate::CreatorSaveRouteSelection {
            row_id: input.row_id.clone(),
            route_ids: vec!["route-0".into()],
        }]),
        Err(crate::CreatorSaveSelectionError::RowNotEligible)
    ));

    let update = runtime
        .prepare_creator_save_batch(vec![input.clone()], crate::CreatorSaveIntent::SaveOrUpdate)
        .expect("update review");
    assert_eq!(
        update.rows()[0].status,
        crate::CreatorSavePreparedStatus::EligibleUpdate
    );
    let selected = crate::test_support::select_required_routes(update).expect("update routes");
    let outcome = runtime
        .execute_creator_save_batch(selected, "intent-update".into())
        .expect("update confirmation")
        .outcomes
        .into_iter()
        .next()
        .expect("update outcome");
    let CreatorSaveRowOutcome::Saved {
        creator, created, ..
    } = outcome
    else {
        panic!("save-or-update should update an exact existing Creator");
    };
    assert!(!created);
    assert_eq!(creator.creator.id, first_id);
}

#[test]
fn batch_identity_duplicates_are_skipped_and_conflicting_updates_wait_for_review() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("duplicate-intent"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let first = x_save_entry(account_ref.id.clone(), false);
    let mut second = first.clone();
    second.row_id = "x-profile-42-second".into();

    let prepared = runtime
        .prepare_creator_save_batch(
            vec![first.clone(), second.clone()],
            crate::CreatorSaveIntent::AddNew,
        )
        .expect("duplicate add-new review");
    let rows = prepared.rows();
    assert_eq!(
        rows[0].status,
        crate::CreatorSavePreparedStatus::EligibleNew
    );
    assert_eq!(
        rows[1].status,
        crate::CreatorSavePreparedStatus::DuplicateSkipped
    );
    assert_eq!(
        prepared.summary().duplicate_skipped,
        1,
        "duplicate rows are counted independently of eligible rows"
    );

    let mut conflicting = second;
    conflicting.notes = Some("different update".into());
    let prepared = runtime
        .prepare_creator_save_batch(
            vec![first, conflicting],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("conflicting update review");
    assert!(prepared.rows().iter().all(|row| {
        row.status == crate::CreatorSavePreparedStatus::IdentityPending
            && row.reason == Some(crate::CreatorSavePreparedReason::ConflictingUpdate)
    }));
}

#[test]
fn add_new_confirmation_race_skips_the_newly_existing_creator_without_updating_it() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("add-new-race"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let input = x_save_entry(account_ref.id, false);
    let prepared = runtime
        .prepare_creator_save_batch(vec![input.clone()], crate::CreatorSaveIntent::AddNew)
        .expect("prepare add-new review");
    let selected = crate::test_support::select_required_routes(prepared)
        .expect("select add-new route before the race");

    let concurrent = save_fixture(&runtime, input.clone(), "concurrent-creator")
        .expect("the concurrent operation creates the Creator");
    let concurrent_id = match concurrent {
        CreatorSaveRowOutcome::Saved {
            creator, created, ..
        } => {
            assert!(created);
            creator.creator.id
        }
        _ => panic!("the concurrent operation must create a Creator"),
    };
    let mut changes = runtime.subscribe_changes();
    let raced = runtime
        .execute_creator_save_batch(selected, "add-new-race-confirm".into())
        .expect("the race has a deterministic skip outcome")
        .outcomes
        .into_iter()
        .next()
        .expect("raced outcome");
    assert!(matches!(
        raced,
        CreatorSaveRowOutcome::Skipped {
            reason: crate::CreatorSaveSkipReason::ExistingCreator,
            ..
        }
    ));
    assert!(
        changes.try_recv().is_err(),
        "a skipped add must not publish a change"
    );
    let creator = runtime
        .get_creator(&concurrent_id)
        .expect("existing Creator");
    assert_eq!(creator.creator.id, concurrent_id);
}

#[test]
fn telegram_administrator_eligibility_is_typed_and_matches_initial_route_rules() {
    let mut account = crate::PlatformAccountCandidate {
        platform: Platform::Telegram,
        external_id: Some("tg-user".into()),
        target_kind: Some(TargetKind::User),
        handle: Some("public_user".into()),
        display_name: "Public User".into(),
        profile_url: Some("https://t.me/public_user".into()),
    };
    assert_eq!(
        crate::assess_telegram_administrator_eligibility(&account),
        crate::CreatorSaveAdministratorEligibility::Ready
    );
    account.target_kind = Some(TargetKind::Bot);
    assert_eq!(
        crate::assess_telegram_administrator_eligibility(&account),
        crate::CreatorSaveAdministratorEligibility::Blocked {
            reason: crate::CreatorSaveAdministratorEligibilityReason::Bot
        }
    );
    account.target_kind = Some(TargetKind::User);
    account.handle = None;
    assert_eq!(
        crate::require_telegram_administrator_eligibility(&account),
        Err(crate::CreatorSaveAdministratorEligibilityReason::MissingPublicHandle)
    );
}

#[tokio::test]
async fn creator_save_preparation_async_preserves_tag_error_and_row_context() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("tag-validation"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let mut input = x_save_entry(account_ref.id, false);
    input.tags = Some(vec!["rust".into(), " rust ".into()]);
    let row_id = input.row_id.clone();
    let error = runtime
        .prepare_creator_save_batch_async(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .await
        .unwrap_err();
    assert!(
        matches!(error, crate::CreatorRepositoryCallError::Owner(crate::CreatorSavePreparationError {
        row_id: Some(actual_row_id),
        kind: crate::CreatorSavePreparationErrorKind::Tags(crate::CreatorTagsError::Duplicate),
    }) if actual_row_id == row_id)
    );
    assert!(
        runtime
            .list_creators(crate::ListCreatorsInput::default())
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn administrator_prepare_preserves_public_sources_and_rejects_invalid_routes() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("telegram-operator"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .expect("account reference");

    let owner = administrator_entry(
        crate::CommunityReference {
            external_id: "1688680661".into(),
            title: "Provider Crypto Community".into(),
            handle: Some("provider_crypto_community".into()),
            target_kind: CommunityTargetKind::Supergroup,
        },
        crate::CommunityAdministratorSource::OperatorAccount {
            operator_account_ref_id: account_ref.id.clone(),
        },
    );
    let prepared = runtime
        .prepare_creator_save_batch(vec![owner.clone()], crate::CreatorSaveIntent::SaveOrUpdate)
        .expect("read-only administrator prepare");
    let preview = prepared.rows().remove(0);
    assert_eq!(
        serde_json::to_value(&preview.provenance).unwrap(),
        serde_json::to_value(&owner.clone().provenance).unwrap()
    );
    assert!(preview.route_options.iter().any(|route| route.required
        && route.channel == ContactChannel::Telegram
        && route.normalized_value == "community_owner"));
    assert!(
        runtime
            .list_creators(Default::default())
            .unwrap()
            .items
            .is_empty(),
        "administrator preparation must not create a Creator"
    );
    let mut bot = owner.clone();
    bot.account.external_id = Some("telegram-bot-42".into());
    bot.account.target_kind = Some(TargetKind::Bot);
    assert_eq!(
        runtime
            .prepare_creator_save_batch(vec![bot.clone()], crate::CreatorSaveIntent::SaveOrUpdate)
            .unwrap_err()
            .kind,
        crate::CreatorSavePreparationErrorKind::TelegramEligibility(
            crate::CreatorSaveAdministratorEligibilityReason::Bot,
        )
    );
    let collision_owner = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Existing route owner".into(),
            notes: None,
        })
        .expect("collision route owner");
    let collision_locator = runtime
        .repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "shared_admin".into(),
            display_value: "@shared_admin".into(),
        })
        .expect("collision locator");
    runtime
        .repository
        .link_creator_contact_route(
            &collision_owner.creator.id,
            &collision_locator.id,
            Confidence::Exact,
        )
        .expect("collision route");
    let mut collision_candidate = owner.clone();
    collision_candidate.row_id = "telegram:user:collision-admin".into();
    if let crate::CreatorSaveProvenance::Discovery { candidate_id, .. } =
        &mut collision_candidate.provenance
    {
        *candidate_id = "telegram:user:collision-admin".into();
    }
    collision_candidate.account.external_id = Some("collision-admin".into());
    collision_candidate.account.handle = Some("shared_admin".into());
    collision_candidate.account.profile_url = Some("https://t.me/shared_admin".into());
    let mut creator_changes = runtime.subscribe_changes();
    let creator_count_before_collision = runtime
        .list_creators(Default::default())
        .expect("creators")
        .items
        .len();
    let collision_prepared = runtime
        .prepare_creator_save_batch(
            vec![{
                let mut input = collision_candidate.clone();
                input.notes = None;
                input
            }],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("a route conflict remains visible in preparation");
    let collision_row = collision_prepared.rows().into_iter().next().unwrap();
    assert_eq!(
        collision_row.status,
        crate::CreatorSavePreparedStatus::ContactConflict
    );
    assert_eq!(
        collision_row.reason,
        Some(crate::CreatorSavePreparedReason::RequiredContactRouteConflict)
    );
    assert!(matches!(
        collision_prepared.select_routes(vec![crate::CreatorSaveRouteSelection {
            row_id: collision_row.row_id,
            route_ids: collision_row
                .route_options
                .iter()
                .filter(|route| route.required)
                .map(|route| route.route_id.clone())
                .collect(),
        }]),
        Err(crate::CreatorSaveSelectionError::RowNotEligible)
    ));
    assert!(creator_changes.try_recv().is_err());
    assert_eq!(
        runtime
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        creator_count_before_collision
    );
    assert!(
        runtime
            .get_creator(&collision_owner.creator.id)
            .expect("collision owner")
            .origins
            .is_empty()
    );

    let mut no_handle_candidate = owner.clone();
    no_handle_candidate.row_id = "telegram:user:no-public-handle".into();
    if let crate::CreatorSaveProvenance::Discovery { candidate_id, .. } =
        &mut no_handle_candidate.provenance
    {
        *candidate_id = "telegram:user:no-public-handle".into();
    }
    no_handle_candidate.account.external_id = Some("no-public-handle".into());
    no_handle_candidate.account.handle = None;
    no_handle_candidate.account.profile_url = None;
    no_handle_candidate.contact_evidence.clear();
    let creator_count_before_no_handle = runtime
        .list_creators(Default::default())
        .expect("creators")
        .items
        .len();
    let no_handle_error = runtime
        .prepare_creator_save_batch(
            vec![{
                let mut input = no_handle_candidate.clone();
                input.notes = None;
                input
            }],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("administrator without a public handle must not prepare for saving");
    assert_eq!(
        no_handle_error.kind,
        crate::CreatorSavePreparationErrorKind::TelegramEligibility(
            crate::CreatorSaveAdministratorEligibilityReason::MissingPublicHandle,
        )
    );
    assert_eq!(
        no_handle_error.row_id.as_deref(),
        Some(no_handle_candidate.row_id.as_str())
    );
    assert!(creator_changes.try_recv().is_err());
    assert_eq!(
        runtime
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        creator_count_before_no_handle
    );

    let mut owner_input = owner.clone();
    owner_input.notes = Some("community owner".into());
    let owner_selected = crate::test_support::select_required_routes(
        runtime
            .prepare_creator_save_batch(vec![owner_input], crate::CreatorSaveIntent::SaveOrUpdate)
            .expect("prepare owner once"),
    )
    .expect("select owner routes");
    let saved = runtime
        .execute_creator_save_batch(owner_selected.clone(), "owner-save".into())
        .expect("save human community administrator")
        .outcomes
        .remove(0);
    let CreatorSaveRowOutcome::Saved {
        creator,
        created,
        outreach_context,
        ..
    } = saved
    else {
        panic!("community administrator must save after route selection")
    };
    assert_eq!(
        creator_changes
            .try_recv()
            .expect("community save change")
            .scopes,
        vec![CreatorChangeScope::Creators]
    );
    let outreach_context = outreach_context.expect("administrator outreach context");
    assert!(created);
    assert_eq!(creator.creator.notes.as_deref(), Some("community owner"));
    let telegram_route = creator
        .contact_routes
        .iter()
        .find(|route| route.locator.channel == ContactChannel::Telegram)
        .expect("confirmed Telegram route");
    assert_eq!(telegram_route.locator.normalized_value, "community_owner");
    assert_eq!(telegram_route.route.confidence, Confidence::Exact);
    assert_eq!(outreach_context.creator_id, creator.creator.id);
    assert_eq!(outreach_context.contact_route_id, telegram_route.route.id);
    assert_eq!(outreach_context.operator_account_ref_id, account_ref.id);
    assert!(matches!(
        creator.origins.as_slice(),
        [crate::CreatorOrigin::CommunityAdministrator { .. }]
    ));

    let mut second_human = owner.clone();
    second_human.row_id = "telegram:user:second-human-admin".into();
    if let crate::CreatorSaveProvenance::Discovery { candidate_id, .. } =
        &mut second_human.provenance
    {
        *candidate_id = "telegram:user:second-human-admin".into();
    }
    second_human.account.external_id = Some("second-human-admin".into());
    second_human.account.handle = Some("second_human_admin".into());
    second_human.account.display_name = "Second Human Admin".into();
    second_human.account.profile_url = Some("https://t.me/second_human_admin".into());
    let mut second_input = second_human.clone();
    second_input.notes = Some("second administrator".into());
    let second_selected = crate::test_support::select_required_routes(
        runtime
            .prepare_creator_save_batch(vec![second_input], crate::CreatorSaveIntent::SaveOrUpdate)
            .expect("prepare second owner once"),
    )
    .expect("select second owner routes");
    let second_saved = runtime
        .execute_creator_save_batch(second_selected.clone(), "second_human-save".into())
        .expect("second human from same discovery saves independently")
        .outcomes
        .remove(0);
    let CreatorSaveRowOutcome::Saved {
        creator: second_creator,
        created: second_created,
        ..
    } = second_saved
    else {
        panic!("second administrator must save after route selection")
    };
    assert!(second_created);
    assert_ne!(second_creator.creator.id, creator.creator.id);
    assert_eq!(
        creator_changes.try_recv().unwrap().scopes,
        vec![CreatorChangeScope::Creators]
    );

    std::thread::sleep(std::time::Duration::from_millis(1_100));
    let replay = runtime
        .execute_creator_save_batch(owner_selected.clone(), "owner-save".into())
        .expect("replay the exact selected owner payload")
        .outcomes
        .remove(0);
    let CreatorSaveRowOutcome::Saved {
        creator: replay_creator,
        created: replay_created,
        outreach_context: replay_outreach_context,
        ..
    } = replay
    else {
        panic!("administrator replay must not require confirmation")
    };
    assert!(
        replay_created,
        "receipt replay returns the original outcome"
    );
    assert!(replay_outreach_context.is_some());
    assert_eq!(replay_creator.creator.id, creator.creator.id);
    assert_eq!(
        replay_creator.creator.updated_at, creator.creator.updated_at,
        "a replay in a later timestamp second must not touch the Creator"
    );
    assert!(
        creator_changes.try_recv().is_err(),
        "save replay is a no-op"
    );

    let second_replay = runtime
        .execute_creator_save_batch(second_selected.clone(), "second_human-save".into())
        .expect("replay the exact selected second-owner payload")
        .outcomes
        .remove(0);
    let CreatorSaveRowOutcome::Saved {
        creator: second_replay_creator,
        created: second_replay_created,
        ..
    } = second_replay
    else {
        panic!("second administrator replay must remain saved")
    };
    assert!(
        second_replay_created,
        "receipt replay returns the original outcome"
    );
    assert_eq!(second_replay_creator.creator.id, second_creator.creator.id);
    assert_eq!(
        second_replay_creator.creator.updated_at,
        second_creator.creator.updated_at
    );
    assert!(creator_changes.try_recv().is_err());

    let concurrent_inputs = [
        (owner_selected, "owner-save"),
        (second_selected, "second_human-save"),
    ];
    std::thread::scope(|scope| {
        for (selected, execution_key) in concurrent_inputs {
            let runtime = runtime.clone();
            scope.spawn(move || {
                runtime
                    .execute_creator_save_batch(selected, execution_key.into())
                    .expect("concurrent exact-payload replay");
            });
        }
    });
    assert!(
        creator_changes.try_recv().is_err(),
        "concurrent replays are no-ops"
    );

    let mut fresh_input = owner.clone();
    fresh_input.notes = Some("community owner".into());
    let fresh = save_fixture(&runtime, fresh_input, "owner-fresh-prepare")
        .expect("a separately prepared request uses its own execution key");
    let CreatorSaveRowOutcome::Saved {
        creator: fresh_creator,
        created: false,
        ..
    } = fresh
    else {
        panic!("new preparation merges the existing Creator")
    };
    assert_eq!(fresh_creator.creator.id, creator.creator.id);
    assert_eq!(fresh_creator.origins.len(), creator.origins.len());
    assert_eq!(
        fresh_creator.observation_sources.len(),
        creator.observation_sources.len()
    );
    assert_eq!(
        fresh_creator.account_observations.len(),
        creator.account_observations.len()
    );
    assert_eq!(
        creator_changes.try_recv().unwrap().scopes,
        vec![CreatorChangeScope::Creators]
    );

    let bot_error = runtime
        .prepare_creator_save_batch(
            vec![{
                let mut input = bot.clone();
                input.notes = None;
                input
            }],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("bot administrator must remain read-only");
    assert_eq!(
        bot_error.kind,
        crate::CreatorSavePreparationErrorKind::TelegramEligibility(
            crate::CreatorSaveAdministratorEligibilityReason::Bot,
        )
    );
    assert_eq!(bot_error.row_id.as_deref(), Some(bot.row_id.as_str()));
}

#[tokio::test]
async fn saved_community_context_preserves_source_and_rejects_self_merge() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let discovery_operator = runtime
        .sync_account_ref(
            AccountRefId::new("phase-2-operator"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("discovery account reference");
    let telegram_operator = runtime
        .sync_account_ref(
            AccountRefId::new("telegram-operator"),
            Platform::Telegram,
            "profile",
            "@telegram_operator",
        )
        .expect("Telegram account reference");
    let candidate = x_save_entry(discovery_operator.id.clone(), true);
    let mut creator_changes = runtime.subscribe_changes();
    let mut input = candidate.clone();
    input.quality.score = Some(88);
    input.quality.assessment = Some("  qualified crypto KOL with a live Telegram route  ".into());
    let prepared = runtime
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .expect("prepare Creator save");
    let rows = prepared.rows();
    let route_options = &rows[0].route_options;
    assert!(
        creator_changes.try_recv().is_err(),
        "confirmation_required must not publish"
    );
    let confirmed_route_ids = route_options
        .iter()
        .filter(|route| {
            matches!(
                route.channel,
                ContactChannel::X | ContactChannel::Telegram | ContactChannel::LinkHub
            )
        })
        .map(|route| route.route_id.clone())
        .collect();
    let selected = prepared
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: rows[0].row_id.clone(),
            route_ids: confirmed_route_ids,
        }])
        .expect("select reviewed routes");
    let saved = runtime
        .execute_creator_save_batch(selected, "source-creator-save".into())
        .expect("confirm Creator save")
        .outcomes
        .remove(0);
    assert_eq!(
        creator_changes
            .try_recv()
            .expect("search save change")
            .scopes,
        vec![CreatorChangeScope::Creators]
    );
    let CreatorSaveRowOutcome::Saved {
        creator: saved_creator,
        outreach_context,
        ..
    } = saved
    else {
        panic!("confirmed search candidate must be saved")
    };
    assert!(outreach_context.is_none());
    assert_eq!(
        saved_creator.creator.score,
        Some(88),
        "agent score must survive the confirmation flow"
    );
    assert_eq!(
        saved_creator.creator.assessment.as_deref(),
        Some("qualified crypto KOL with a live Telegram route"),
        "assessment is trimmed and persisted"
    );
    // The canonical input carries the count read by the caller; preparation
    // and execution preserve it independently of the discovery snapshot.
    assert!(
        candidate.profile.follower_count.is_some(),
        "the X discovery fixture must publish a count"
    );
    assert_eq!(
        saved_creator
            .creator
            .follower_counts
            .get(&crate::Platform::X),
        candidate.profile.follower_count.as_ref()
    );
    let telegram_route = saved_creator
        .contact_routes
        .iter()
        .find(|route| route.locator.channel == ContactChannel::Telegram)
        .expect("saved Telegram route");
    let source_creator_id = saved_creator.creator.id.clone();

    let preparation = runtime
        .prepare_contact_target_resolution(ResolveContactTargetInput {
            creator_id: source_creator_id.clone(),
            contact_route_id: telegram_route.route.id.clone(),
            operator_account_ref_id: telegram_operator.id.clone(),
            refresh: false,
        })
        .await
        .expect("prepare Telegram target");
    let crate::CreatorContactTargetPreparation::Read(context) = preparation else {
        panic!("Telegram provider identity read")
    };
    let resolution = runtime
        .finish_contact_target_resolution(
            context,
            crate::PlatformTargetResolution::Resolved {
                target: crate::PlatformResolvedTarget {
                    platform: Platform::Telegram,
                    target_kind: TargetKind::Channel,
                    external_target_id: "1399396050".into(),
                    handle: Some("kriptomessitg".into()),
                    display_name: Some("Kripto Messi".into()),
                    canonical_url: Some("https://t.me/kriptomessitg".into()),
                },
            },
        )
        .await
        .expect("record Telegram stable identity");
    assert_eq!(resolution.status, TargetResolutionStatus::Resolved);

    let source_admin_account = runtime
        .repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Telegram,
            target_kind: TargetKind::User,
            external_id: "telegram-owner-42".into(),
        })
        .expect("source Telegram identity");
    runtime
        .repository
        .link_creator_account(
            &source_creator_id,
            &source_admin_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("source Telegram account link");
    let source_origin_count = runtime
        .get_creator(&source_creator_id)
        .expect("source Creator")
        .origins
        .len();
    let creator_count_before_admin_save = runtime
        .list_creators(Default::default())
        .expect("creators")
        .items
        .len();

    let context = runtime
        .telegram_community_route_context(
            source_creator_id.clone(),
            telegram_route.route.id.clone(),
            telegram_operator.id.clone(),
        )
        .await
        .expect("saved community context");
    assert_eq!(context.external_id, "1399396050");
    assert_eq!(context.handle.as_deref(), Some("kriptomessitg"));
    assert_eq!(context.target_kind, CommunityTargetKind::Channel);
    let administrator = administrator_entry(
        crate::CommunityReference {
            external_id: context.external_id.clone(),
            title: "Provider Kripto Messi".into(),
            handle: Some("provider_kriptomessi".into()),
            target_kind: context.target_kind,
        },
        context.source,
    );
    let crate::CreatorSaveProvenance::Discovery {
        origin:
            crate::CreatorCandidateOrigin::CommunityAdministrator {
                source:
                    crate::CommunityAdministratorSource::CreatorContactRoute {
                        source_creator_id: recorded_creator,
                        source_contact_route_id: Some(recorded_route),
                        operator_account_ref_id: recorded_operator,
                        source_resolved_target_id,
                    },
                ..
            },
        ..
    } = &administrator.provenance
    else {
        panic!("saved-route provenance must preserve its resolved parent")
    };
    assert_eq!(recorded_creator, &source_creator_id);
    assert_eq!(recorded_route, &telegram_route.route.id);
    assert_eq!(recorded_operator, &telegram_operator.id);
    assert!(!source_resolved_target_id.as_str().is_empty());
    let source_merge_error = runtime
        .prepare_creator_save_batch(
            vec![{
                let mut input = administrator.clone();
                input.notes = None;
                input
            }],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("administrator must remain independent from the source Creator");
    assert_eq!(
        source_merge_error.kind,
        crate::CreatorSavePreparationErrorKind::Identity
    );
    assert_eq!(
        source_merge_error.row_id.as_deref(),
        Some(administrator.row_id.as_str())
    );
    assert_eq!(
        runtime
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        creator_count_before_admin_save
    );
    assert_eq!(
        runtime
            .get_creator(&source_creator_id)
            .expect("source Creator")
            .origins
            .len(),
        source_origin_count
    );
}

fn administrator_entry(
    community: crate::CommunityReference,
    source: crate::CommunityAdministratorSource,
) -> crate::CanonicalCreatorSaveInput {
    crate::CanonicalCreatorSaveInput {
        row_id: "telegram:user:telegram-owner-42".into(),
        provenance: crate::CreatorSaveProvenance::Discovery {
            candidate_id: "telegram:user:telegram-owner-42".into(),
            origin: CreatorCandidateOrigin::CommunityAdministrator {
                discovery_id: crate::DiscoveryId::new("historical-admin-fixture"),
                role: CommunityAdministratorRole::Owner,
                community,
                source,
            },
            selection_context: None,
        },
        account: crate::PlatformAccountCandidate {
            platform: Platform::Telegram,
            external_id: Some("telegram-owner-42".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("community_owner".into()),
            display_name: "Community Owner".into(),
            profile_url: Some("https://t.me/community_owner".into()),
        },
        profile: Default::default(),
        profile_observed_at: 1_700_000_000,
        contact_evidence: vec![],
        notes: None,
        tags: None,
        quality: Default::default(),
    }
}
