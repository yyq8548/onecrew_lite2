use super::*;

#[test]
fn creator_save_preparation_returns_typed_row_errors_without_writes() {
    use crate::{CreatorSavePreparationErrorKind as Failure, CreatorTagsError};
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let input = entry(origin, "invalid-save-tags", "creator");
    let before = save_database_state(&repository);
    let mut too_many_tags = input.clone();
    too_many_tags.tags = Some(vec![
        "a".into(),
        "b".into(),
        "c".into(),
        "d".into(),
        "e".into(),
    ]);
    let mut invalid_fields = input.clone();
    invalid_fields.notes = Some("   ".into());
    let mut invalid_source = input.clone();
    invalid_source.provenance = crate::CreatorSaveProvenance::DirectSelection {
        source_ref: "source".into(),
        source_url: None,
        description: None,
        operator_account_ref_id: None,
    };
    let mut invalid_contact = input.clone();
    invalid_contact
        .contact_evidence
        .push(crate::CreatorSaveContactEvidence::Observation {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            value: "not-an-email".into(),
            source: SourceKind::ManualBusinessEmail,
            source_url: None,
        });
    for (invalid, kind) in [
        (too_many_tags, Failure::Tags(CreatorTagsError::TooMany)),
        (invalid_fields, Failure::InvalidFields),
        (invalid_source, Failure::InvalidSource),
        (invalid_contact, Failure::ContactNormalization),
    ] {
        let error = repository
            .prepare_creator_save_batch(vec![invalid], crate::CreatorSaveIntent::SaveOrUpdate)
            .unwrap_err();
        assert_eq!(error.row_id.as_deref(), Some(input.row_id.as_str()));
        assert_eq!(error.kind, kind);
        assert!(!error.to_string().contains(&input.row_id));
        assert_eq!(
            save_database_state(&repository),
            before,
            "failed preparation cannot write or reserve rows"
        );
    }
}

#[test]
fn identity_resolution_is_read_only_and_stable_ids_are_exact() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let candidate = entry(origin.clone(), "x-stable-1", "creator");
    assert_eq!(
        repository
            .resolve_creator_identity(&candidate.account)
            .expect("new identity"),
        IdentityResolution::New
    );
    let before = repository
        .lock()
        .expect("lock")
        .query_row(
            "SELECT
           (SELECT COUNT(*) FROM creators) +
           (SELECT COUNT(*) FROM platform_accounts) +
           (SELECT COUNT(*) FROM identity_conflicts)",
            [],
            |row| row.get::<_, u64>(0),
        )
        .expect("before");
    assert_eq!(before, 0);

    let saved = persist_entry(&repository, save_input(candidate, None)).expect("saved");
    assert!(saved.created);
    assert_eq!(
        repository.resolve_creator_identity(&saved.account).unwrap(),
        IdentityResolution::ExactExistingCreator {
            creator_id: saved.creator.creator.id.clone(),
        }
    );
    assert_eq!(saved.creator.account_links[0].state, LinkState::Confirmed);
    assert_eq!(saved.creator.platform_accounts[0].external_id, "x-stable-1");
    assert_eq!(
        repository
            .resolve_creator_identity(&PlatformAccountCandidate {
                platform: Platform::X,
                external_id: Some("x-stable-1".into()),
                target_kind: Some(TargetKind::User),
                handle: Some("renamed".into()),
                display_name: "Renamed".into(),
                profile_url: None,
            })
            .expect("exact identity"),
        IdentityResolution::ExactExistingCreator {
            creator_id: saved.creator.creator.id,
        }
    );
}

#[test]
fn bot_candidate_is_rejected_before_any_creator_write() {
    let (_directory, repository) = repository();
    let mut bot = entry(platform_search_origin(&repository), "bot-42", "helper_bot");
    bot.account.platform = Platform::Telegram;
    bot.account.target_kind = Some(TargetKind::Bot);
    let input = save_input(bot, None);
    let row_id = input.row_id.clone();
    let error = repository
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .expect_err("Bot cannot be saved as a Creator");
    assert_eq!(
        error.kind,
        crate::CreatorSavePreparationErrorKind::TelegramEligibility(
            crate::CreatorSaveAdministratorEligibilityReason::Bot,
        )
    );
    assert_eq!(error.row_id.as_deref(), Some(row_id.as_str()));
    let count = repository
        .lock()
        .expect("lock")
        .query_row("SELECT COUNT(*) FROM creators", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("creator count");
    assert_eq!(count, 0);
}

#[test]
fn candidate_save_is_atomic_idempotent_and_projects_origin_observation() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let expected_account_ref_id = origin.account_ref_id().clone();
    let input = save_input(entry(origin, "x-stable-2", "atomic"), Some("phase 2"));
    let first = persist_entry(&repository, input.clone()).expect("first save");
    let second = persist_entry(&repository, input).expect("second save");
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(
        repository
            .resolve_creator_identity(&second.account)
            .unwrap(),
        IdentityResolution::ExactExistingCreator {
            creator_id: second.creator.creator.id.clone(),
        }
    );
    assert_eq!(first.creator.creator.id, second.creator.creator.id);
    assert_eq!(second.creator.platform_accounts.len(), 1);
    assert_eq!(second.creator.account_links.len(), 1);
    assert_eq!(second.creator.account_observations.len(), 1);
    assert_eq!(second.creator.observation_sources.len(), 2);
    assert_eq!(second.creator.origins.len(), 1);
    match &second.creator.origins[0] {
        CreatorOrigin::PlatformSearch {
            discovery_id,
            account_ref_id,
            creator_id,
            source_id,
            ..
        } => {
            assert_eq!(discovery_id.as_str(), "discovery-phase-2");
            assert_eq!(account_ref_id, &expected_account_ref_id);
            assert_eq!(creator_id, &second.creator.creator.id);
            let source = second
                .creator
                .observation_sources
                .iter()
                .find(|source| &source.id == source_id)
                .expect("search observation source");
            assert_eq!(source.kind, SourceKind::InitialPlatformAccount);
            assert_eq!(
                source.account_ref_id.as_ref(),
                Some(&expected_account_ref_id)
            );
            assert_eq!(
                source.metadata["account_ref_id"],
                expected_account_ref_id.as_str()
            );
            assert_eq!(source.metadata["query"], "crypto");
            assert_eq!(source.metadata["search_kind"], "users");
        }
        CreatorOrigin::CommunityAdministrator { .. }
        | CreatorOrigin::ManualEntry { .. }
        | CreatorOrigin::CreatorImport { .. } => {
            panic!("search candidate persisted the wrong origin kind")
        }
    }
    assert_eq!(second.creator.contact_routes.len(), 1);
    assert_eq!(second.creator.contact_routes[0].observations.len(), 1);
}

#[test]
fn candidate_save_cannot_modify_an_archived_creator() {
    let (_directory, repository) = repository();
    let input = save_input(
        entry(
            platform_search_origin(&repository),
            "x-archived",
            "archived",
        ),
        None,
    );
    let saved = persist_entry(&repository, input.clone()).expect("initial save");
    repository
        .archive_creator(&saved.creator.creator.id)
        .expect("archive Creator");
    let before = repository
        .get_creator(&saved.creator.creator.id)
        .expect("archived Creator projection");

    let row_id = input.row_id.clone();
    let error = repository
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .unwrap_err();
    assert_eq!(error.kind, crate::CreatorSavePreparationErrorKind::Identity);
    assert_eq!(error.row_id.as_deref(), Some(row_id.as_str()));

    let after = repository
        .get_creator(&saved.creator.creator.id)
        .expect("unchanged archived Creator projection");
    assert_eq!(
        after.account_observations.len(),
        before.account_observations.len()
    );
    assert_eq!(
        after.observation_sources.len(),
        before.observation_sources.len()
    );
    assert_eq!(after.origins.len(), before.origins.len());
    assert_eq!(after.contact_routes.len(), before.contact_routes.len());
    assert_eq!(after.relationship.stage, crate::CreatorStage::Archived);
}

#[test]
fn community_administrator_origin_is_strict_and_persists_complete_provenance() {
    let (_directory, repository) = repository();
    let source_creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Source Community".into(),
            notes: None,
        })
        .expect("source creator");
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("telegram-community-operator"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .expect("telegram operator");
    let source_locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "sourcecommunity".into(),
            display_value: "@sourcecommunity".into(),
        })
        .expect("source locator");
    let source_route = repository
        .link_creator_contact_route(
            &source_creator.creator.id,
            &source_locator.id,
            Confidence::Exact,
        )
        .expect("source route");
    let resolved = repository
        .commit_contact_target_resolution(CommitContactTargetResolutionInput {
            creator_id: source_creator.creator.id.clone(),
            contact_route_id: source_route.id.clone(),
            locator_id: source_locator.id.clone(),
            account_ref_id: operator.id.clone(),
            platform: Platform::Telegram,
            target_kind: TargetKind::Supergroup,
            external_target_id: "telegram-supergroup-42".into(),
            attach_to_creator: false,
            metadata: serde_json::json!({
                "display_name": "Stale Source Community",
                "handle": "stale_sourcecommunity"
            }),
        })
        .expect("source target resolution");
    let CommitContactTargetResolutionResult::Active { value: resolved } = resolved else {
        panic!("new source community resolution must be active")
    };
    let discovery_id = DiscoveryId::new("telegram-admin-discovery-1");
    let origin = CreatorCandidateOrigin::CommunityAdministrator {
        discovery_id: discovery_id.clone(),
        role: CommunityAdministratorRole::Owner,
        community: CommunityReference {
            external_id: "telegram-supergroup-42".into(),
            title: "Provider Source Community".into(),
            handle: Some("provider_sourcecommunity".into()),
            target_kind: CommunityTargetKind::Supergroup,
        },
        source: CommunityAdministratorSource::CreatorContactRoute {
            source_creator_id: source_creator.creator.id.clone(),
            source_contact_route_id: Some(source_route.id.clone()),
            operator_account_ref_id: operator.id.clone(),
            source_resolved_target_id: resolved.target.id.clone(),
        },
    };
    let administrator = crate::CanonicalCreatorSaveInput {
        row_id: "telegram:user:administrator-42".into(),
        provenance: crate::CreatorSaveProvenance::Discovery {
            candidate_id: "telegram:user:administrator-42".into(),
            origin,
            selection_context: None,
        },
        account: PlatformAccountCandidate {
            platform: Platform::Telegram,
            external_id: Some("administrator-42".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("community_admin".into()),
            display_name: "Community Admin".into(),
            profile_url: Some("https://t.me/community_admin".into()),
        },
        profile: Default::default(),
        profile_observed_at: 1_700_000_000,
        contact_evidence: vec![],
        notes: None,
        tags: None,
        quality: Default::default(),
    };

    let mut input = save_input(administrator.clone(), None);
    let before_prepare = save_database_state(&repository);
    let prepared = repository
        .prepare_creator_save_batch(
            vec![(administrator).clone()],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("prepare administrator");
    assert_eq!(
        save_database_state(&repository),
        before_prepare,
        "administrator preparation writes no table"
    );
    let crate::CreatorSaveProvenance::Discovery { origin, .. } = &mut input.provenance else {
        panic!("fixture discovery provenance")
    };
    match origin {
        CreatorCandidateOrigin::CommunityAdministrator {
            role,
            community,
            source,
            ..
        } => {
            *role = CommunityAdministratorRole::Administrator;
            community.external_id = "attacker-community".into();
            community.title = "Attacker Community".into();
            community.handle = Some("attacker".into());
            community.target_kind = CommunityTargetKind::Channel;
            *source = CommunityAdministratorSource::CreatorContactRoute {
                source_creator_id: CreatorId::new("attacker-creator"),
                source_contact_route_id: Some(crate::CreatorContactRouteId::new("attacker-route")),
                operator_account_ref_id: AccountRefId::new("attacker-account"),
                source_resolved_target_id: crate::ResolvedTargetId::new("attacker-target"),
            };
        }
        CreatorCandidateOrigin::PlatformSearch { .. } => panic!("wrong test origin"),
    }
    assert!(
        repository
            .prepare_creator_save_batch(
                vec![(input).clone()],
                crate::CreatorSaveIntent::SaveOrUpdate
            )
            .is_err(),
        "tampered provenance cannot prepare"
    );
    assert_eq!(save_database_state(&repository), before_prepare);
    let selected = crate::test_support::select_required_routes(prepared).unwrap();
    let (mut result, changed) = repository
        .execute_creator_save_batch(selected, "administrator-provenance".into())
        .expect("save reviewed administrator");
    assert!(changed);
    let crate::CreatorSaveRowOutcome::Saved {
        creator, created, ..
    } = result.outcomes.remove(0)
    else {
        panic!("reviewed administrator must save")
    };
    let saved = SavedCreatorFixture {
        creator: *creator,
        account: administrator.account.clone(),
        created,
    };
    assert!(saved.created);
    assert_eq!(saved.creator.origins.len(), 1);
    match &saved.creator.origins[0] {
        CreatorOrigin::CommunityAdministrator {
            discovery_id: persisted_discovery_id,
            creator_id,
            source_id,
            role,
            community,
            source:
                CommunityAdministratorSource::CreatorContactRoute {
                    source_creator_id,
                    source_contact_route_id,
                    operator_account_ref_id,
                    source_resolved_target_id,
                },
            ..
        } => {
            assert_eq!(persisted_discovery_id, &discovery_id);
            assert_eq!(creator_id, &saved.creator.creator.id);
            assert_eq!(role, &CommunityAdministratorRole::Owner);
            assert_eq!(community.external_id, "telegram-supergroup-42");
            assert_eq!(community.title, "Provider Source Community");
            assert_eq!(
                community.handle.as_deref(),
                Some("provider_sourcecommunity")
            );
            assert_eq!(community.target_kind, CommunityTargetKind::Supergroup);
            assert_eq!(source_creator_id, &source_creator.creator.id);
            assert_eq!(source_contact_route_id.as_ref(), Some(&source_route.id));
            assert_eq!(operator_account_ref_id, &operator.id);
            assert_eq!(source_resolved_target_id, &resolved.target.id);
            let source = saved
                .creator
                .observation_sources
                .iter()
                .find(|source| &source.id == source_id)
                .expect("administrator observation source");
            assert_eq!(source.kind, SourceKind::CommunityAdministrator);
            assert_eq!(source.metadata["discovery_id"], discovery_id.as_str());
            assert_eq!(source.metadata["role"], "owner");
            assert_eq!(
                source.metadata["community"]["external_id"],
                "telegram-supergroup-42"
            );
            assert_eq!(source.metadata["community"]["target_kind"], "supergroup");
        }
        CreatorOrigin::PlatformSearch { .. }
        | CreatorOrigin::ManualEntry { .. }
        | CreatorOrigin::CreatorImport { .. }
        | CreatorOrigin::CommunityAdministrator {
            source: CommunityAdministratorSource::OperatorAccount { .. },
            ..
        } => {
            panic!("administrator candidate persisted the wrong origin kind")
        }
    }

    let source_admin_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Telegram,
            target_kind: TargetKind::User,
            external_id: "source-as-admin".into(),
        })
        .expect("source administrator identity");
    repository
        .link_creator_account(
            &source_creator.creator.id,
            &source_admin_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("source administrator account link");
    let mut source_identity_candidate = administrator.clone();
    source_identity_candidate.row_id = "telegram:user:source-as-admin".into();
    source_identity_candidate.account.external_id = Some("source-as-admin".into());
    source_identity_candidate.account.handle = Some("source_as_admin".into());
    source_identity_candidate.account.profile_url = Some("https://t.me/source_as_admin".into());
    let creator_count_before_source_merge = repository
        .list_creators(Default::default())
        .expect("creators")
        .items
        .len();
    let row_id = source_identity_candidate.row_id.clone();
    let source_merge_error = repository
        .prepare_creator_save_batch(
            vec![save_input(source_identity_candidate, None)],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("community administrator must not merge into its source Creator");
    assert_eq!(
        source_merge_error.kind,
        crate::CreatorSavePreparationErrorKind::Identity
    );
    assert_eq!(source_merge_error.row_id.as_deref(), Some(row_id.as_str()));
    assert_eq!(
        repository
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        creator_count_before_source_merge
    );
    assert!(
        repository
            .get_creator(&source_creator.creator.id)
            .expect("source Creator")
            .origins
            .is_empty()
    );

    let invalid_branch = repository.lock().expect("lock").execute(
        "INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, source_creator_id,
                source_contact_route_id, operator_account_ref_id,
                source_resolved_target_id, discovered_at)
             VALUES ('invalid-admin-origin', 'community_administrator', ?1, ?2, 'invalid-admin-discovery',
                     NULL, NULL, NULL, NULL, 1)",
        rusqlite::params![
            saved.creator.creator.id.as_str(),
            saved.creator.observation_sources[0].id.as_str(),
        ],
    );
    assert!(invalid_branch.is_err());
}

#[test]
fn possible_handle_match_blocks_save_without_partial_rows() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let first = persist_entry(
        &repository,
        save_input(
            entry(origin.clone(), "x-stable-existing", "same_handle"),
            None,
        ),
    )
    .expect("first creator");
    let possible = entry(origin, "x-stable-new", "@same_handle");
    assert_eq!(
        repository
            .resolve_creator_identity(&possible.account)
            .expect("possible match"),
        IdentityResolution::PossibleMatch {
            creator_ids: vec![first.creator.creator.id],
        }
    );
    let row_id = possible.row_id.clone();
    let prepared = repository
        .prepare_creator_save_batch(
            vec![save_input(possible, None)],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("possible match remains visible as a blocked review row");
    let row = prepared
        .rows()
        .into_iter()
        .next()
        .expect("blocked review row");
    assert_eq!(row.row_id, row_id);
    assert_eq!(
        row.status,
        crate::CreatorSavePreparedStatus::ContactConflict
    );
    assert_eq!(
        row.reason,
        Some(crate::CreatorSavePreparedReason::RequiredContactRouteConflict)
    );
    let selection = crate::CreatorSaveRouteSelection {
        row_id: row.row_id,
        route_ids: row
            .route_options
            .into_iter()
            .map(|route| route.route_id)
            .collect(),
    };
    assert!(matches!(
        prepared.select_routes(vec![selection]),
        Err(crate::CreatorSaveSelectionError::RowNotEligible)
    ));
    assert_eq!(
        repository
            .list_creators(Default::default())
            .expect("creators")
            .items
            .len(),
        1
    );
    let account_count = repository
        .lock()
        .expect("lock")
        .query_row("SELECT COUNT(*) FROM platform_accounts", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("account count");
    assert_eq!(account_count, 1);
}

#[test]
fn ambiguous_handle_match_returns_conflict_without_writing_conflict_rows() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let first = persist_entry(
        &repository,
        save_input(
            entry(origin.clone(), "x-conflict-first", "shared_handle"),
            None,
        ),
    )
    .expect("first creator");
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second Creator".into(),
            notes: None,
        })
        .expect("second creator");
    let second_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-conflict-second".into(),
        })
        .expect("second account");
    repository
        .link_creator_account(
            &second.creator.id,
            &second_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("second link");
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::ProfileBio,
            platform: Some(Platform::X),
            source_url: None,
            external_ref: Some("conflict-source".into()),
            account_ref_id: Some(origin.account_ref_id().clone()),
            metadata: serde_json::json!({}),
        })
        .expect("source");
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: second_account.id,
            source_id: source.id,
            handle: Some("@shared_handle".into()),
            display_name: Some("Second Creator".into()),
            profile_url: None,
            public_data: serde_json::json!({}),
        })
        .expect("second observation");

    let conflict = repository
        .resolve_creator_identity(&entry(origin, "x-conflict-third", "shared_handle").account)
        .expect("conflict");
    let IdentityResolution::ConflictingMatches { creator_ids } = conflict else {
        panic!("expected conflicting matches");
    };
    assert_eq!(creator_ids.len(), 2);
    assert!(creator_ids.contains(&first.creator.creator.id));
    assert!(creator_ids.contains(&second.creator.id));
    let conflict_rows = repository
        .lock()
        .expect("lock")
        .query_row("SELECT COUNT(*) FROM identity_conflicts", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect("conflict rows");
    assert_eq!(conflict_rows, 0);
}

#[test]
fn candidate_platform_must_match_the_bound_account_reference() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let mut mismatched = entry(origin, "instagram-42", "mismatch");
    mismatched.account.platform = Platform::Instagram;
    let row_id = mismatched.row_id.clone();
    let error = repository
        .prepare_creator_save_batch(
            vec![save_input(mismatched, None)],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("platform mismatch");
    assert_eq!(
        error.kind,
        crate::CreatorSavePreparationErrorKind::InvalidSource
    );
    assert_eq!(error.row_id.as_deref(), Some(row_id.as_str()));
    assert!(
        repository
            .list_creators(Default::default())
            .expect("creators")
            .items
            .is_empty()
    );
}

#[test]
fn one_creator_can_own_multiple_platform_accounts() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator A".into(),
            notes: None,
        })
        .expect("creator");
    let x = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-100".into(),
        })
        .expect("x account");
    let telegram = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Telegram,
            target_kind: TargetKind::User,
            external_id: "tg-200".into(),
        })
        .expect("telegram account");
    repository
        .link_creator_account(
            &creator.creator.id,
            &x.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("link x");
    repository
        .link_creator_account(
            &creator.creator.id,
            &telegram.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("link telegram");

    let loaded = repository
        .get_creator(&creator.creator.id)
        .expect("loaded creator");
    assert_eq!(loaded.platform_accounts.len(), 2);
}

#[test]
fn one_stable_account_cannot_be_confirmed_for_two_creators() {
    let (_directory, repository) = repository();
    let first = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "First".into(),
            notes: None,
        })
        .expect("first creator");
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second".into(),
            notes: None,
        })
        .expect("second creator");
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Instagram,
            target_kind: TargetKind::Creator,
            external_id: "ig-42".into(),
        })
        .expect("account");
    repository
        .link_creator_account(
            &first.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("first link");
    let error = repository
        .link_creator_account(
            &second.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect_err("second confirmed link must fail");
    assert!(error.contains("UNIQUE constraint failed"));
}

#[test]
fn creator_list_rows_carry_identity_presentation_facts() {
    let (_directory, repository) = repository();
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Harry".into(),
            notes: None,
        })
        .expect("creator");
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/harry".into()),
            external_ref: None,
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .expect("source");

    // First-captured X account with a stale and a latest observation.
    let x_account = repository
        .upsert_platform_account(crate::UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::Creator,
            external_id: "x-harry".into(),
        })
        .expect("x account");
    repository
        .link_creator_account(
            &creator.creator.id,
            &x_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .expect("x link");
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: x_account.id.clone(),
            source_id: source.id.clone(),
            handle: Some("old_harry".into()),
            display_name: Some("Harry".into()),
            profile_url: None,
            public_data: serde_json::json!({"profile_image_url": "https://img/old.png"}),
        })
        .expect("stale observation");
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: x_account.id.clone(),
            source_id: source.id.clone(),
            handle: Some("Harry_DgFex".into()),
            display_name: Some("Harry".into()),
            profile_url: None,
            public_data: serde_json::json!({"profile_image_url": "https://img/new.png"}),
        })
        .expect("latest observation");

    // A later Telegram account plus a rejected Instagram link that must not
    // count or surface anywhere.
    let telegram_account = repository
        .upsert_platform_account(crate::UpsertPlatformAccountInput {
            platform: Platform::Telegram,
            target_kind: TargetKind::User,
            external_id: "tg-harry".into(),
        })
        .expect("telegram account");
    repository
        .link_creator_account(
            &creator.creator.id,
            &telegram_account.id,
            LinkState::Claimed,
            Confidence::Asserted,
        )
        .expect("telegram link");
    let rejected_account = repository
        .upsert_platform_account(crate::UpsertPlatformAccountInput {
            platform: Platform::Instagram,
            target_kind: TargetKind::Creator,
            external_id: "ig-harry".into(),
        })
        .expect("instagram account");
    repository
        .link_creator_account(
            &creator.creator.id,
            &rejected_account.id,
            LinkState::Rejected,
            Confidence::Possible,
        )
        .expect("rejected link");
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: rejected_account.id,
            source_id: source.id.clone(),
            handle: Some("rejected_harry".into()),
            display_name: Some("Rejected Harry".into()),
            profile_url: None,
            public_data: serde_json::json!({}),
        })
        .expect("rejected account observation");

    // Two routes on distinct channels plus a second email route (distinct set).
    for (channel, kind, value) in [
        (ContactChannel::X, LocatorKind::Handle, "Harry_DgFex"),
        (
            ContactChannel::Email,
            LocatorKind::EmailAddress,
            "harry@example.com",
        ),
        (
            ContactChannel::Email,
            LocatorKind::EmailAddress,
            "harry2@example.com",
        ),
    ] {
        let locator = repository
            .upsert_contact_locator(ContactLocatorInput {
                channel,
                kind,
                normalized_value: value.to_lowercase(),
                display_value: value.into(),
            })
            .expect("locator");
        repository
            .link_creator_contact_route(&creator.creator.id, &locator.id, Confidence::Exact)
            .expect("route");
    }

    let listed = repository
        .list_creators(Default::default())
        .expect("creators");
    let row = listed
        .items
        .iter()
        .find(|row| row.id == creator.creator.id)
        .expect("row");
    assert_eq!(row.primary_platform, Some(Platform::X));
    assert_eq!(row.primary_handle.as_deref(), Some("Harry_DgFex"));
    assert_eq!(row.avatar_url.as_deref(), Some("https://img/new.png"));
    assert_eq!(row.platforms, vec![Platform::X, Platform::Telegram]);
    assert_eq!(row.channels, vec![ContactChannel::X, ContactChannel::Email]);
    assert!(!row.has_manual_origin);
    assert_eq!(
        row.platform_account_count, 2,
        "rejected link must not count"
    );
    assert_eq!(row.contact_route_count, 3);

    // Identity and every saved contact-route field answer the same backend
    // query, rather than filtering only the currently loaded UI page.
    let by_handle = repository
        .list_creators(crate::ListCreatorsInput {
            query: Some("harry_dgfex".into()),
            ..Default::default()
        })
        .expect("query by handle");
    assert!(
        by_handle
            .items
            .iter()
            .any(|row| row.id == creator.creator.id)
    );
    for query in ["EMAIL", "email_address", "harry2@example.com"] {
        let by_contact_route = repository
            .list_creators(crate::ListCreatorsInput {
                query: Some(query.into()),
                ..Default::default()
            })
            .expect("query by contact route");
        assert!(
            by_contact_route
                .items
                .iter()
                .any(|row| row.id == creator.creator.id),
            "saved contact route field {query:?} must be searchable"
        );
    }
    for query in ["old_harry", "rejected_harry"] {
        let by_inactive_handle = repository
            .list_creators(crate::ListCreatorsInput {
                query: Some(query.into()),
                ..Default::default()
            })
            .expect("query by inactive handle");
        assert!(
            by_inactive_handle
                .items
                .iter()
                .all(|row| row.id != creator.creator.id),
            "stale or rejected handle {query:?} must not be searchable"
        );
    }
    let by_other = repository
        .list_creators(crate::ListCreatorsInput {
            query: Some("no-such-name".into()),
            ..Default::default()
        })
        .expect("query miss");
    assert!(
        by_other
            .items
            .iter()
            .all(|row| row.id != creator.creator.id)
    );
}

/// A second operator account searching the same platform produces its own
/// platform-search origin for the same Creator: provenance is per
/// (operator, discovery, Creator), and the read mapping restores it exactly.
#[test]
fn platform_search_candidate_save_persists_one_origin_per_operator_and_discovery() {
    let (_directory, repository) = repository();
    let first = platform_search_origin(&repository);
    let saved = persist_entry(
        &repository,
        save_input(entry(first.clone(), "x-777", "kripto_messi"), None),
    )
    .expect("search candidate saves");
    assert!(saved.created);
    let second_operator = repository
        .upsert_account_ref(
            AccountRefId::new("x-profile-second"),
            Platform::X,
            "profile",
            "@second",
        )
        .expect("second account ref");
    let second = CreatorCandidateOrigin::PlatformSearch {
        discovery_id: DiscoveryId::new("discovery-second-search"),
        account_ref_id: second_operator.id.clone(),
        query: "kripto messi".into(),
        search_kind: "people".into(),
    };
    let merged = persist_entry(
        &repository,
        save_input(entry(second, "x-777", "kripto_messi"), None),
    )
    .expect("second operator search merges into the same Creator");
    assert!(!merged.created);
    assert_eq!(merged.creator.creator.id, saved.creator.creator.id);
    let replay = persist_entry(
        &repository,
        save_input(entry(first.clone(), "x-777", "kripto_messi"), None),
    )
    .expect("repeating the first search is idempotent");
    assert!(!replay.created);

    let reread = repository
        .get_creator(&saved.creator.creator.id)
        .expect("creator reread");
    let mut origins = reread
        .origins
        .iter()
        .map(|origin| match origin {
            CreatorOrigin::PlatformSearch {
                discovery_id,
                account_ref_id,
                creator_id,
                ..
            } => {
                assert_eq!(creator_id, &reread.creator.id);
                (
                    discovery_id.as_str().to_owned(),
                    account_ref_id.as_str().to_owned(),
                )
            }
            other => panic!("expected platform search origins, got {other:?}"),
        })
        .collect::<Vec<_>>();
    origins.sort();
    assert_eq!(
        origins,
        vec![
            (
                "discovery-phase-2".to_string(),
                first.account_ref_id().as_str().to_owned()
            ),
            (
                "discovery-second-search".to_string(),
                second_operator.id.as_str().to_owned()
            ),
        ]
    );
}

/// Observe every current table and the write connection's mutation counter.
/// Preparation must not insert, update, delete, or reserve on any table.
fn save_database_state(
    repository: &SqliteRepository,
) -> (u64, std::collections::BTreeMap<String, u64>) {
    let connection = repository.lock().unwrap();
    let tables = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let rows = tables
        .into_iter()
        .map(|table| {
            let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
            let count = connection
                .query_row(&sql, [], |row| row.get::<_, u64>(0))
                .unwrap();
            (table, count)
        })
        .collect();
    let changes = connection
        .query_row("SELECT total_changes()", [], |row| row.get::<_, u64>(0))
        .unwrap();
    (changes, rows)
}

#[test]
fn prepared_creator_save_owns_reviewed_values_after_snapshot_changes_and_removal() {
    let (_directory, repository) = repository();
    let mut candidate = entry(
        platform_search_origin(&repository),
        "review-candidate",
        "reviewcreator",
    );
    let mut input = (candidate).clone();
    input.notes = Some("Reviewed".into());
    let before = save_database_state(&repository);
    let prepared = repository
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .unwrap();
    assert_eq!(
        save_database_state(&repository),
        before,
        "prepare must not reserve or write any table"
    );
    let rows = prepared.rows();
    let required = rows[0]
        .route_options
        .iter()
        .find(|route| route.required)
        .unwrap()
        .route_id
        .clone();
    let duplicate = prepared
        .clone()
        .select_routes(vec![crate::CreatorSaveRouteSelection {
            row_id: rows[0].row_id.clone(),
            route_ids: vec![required.clone(), required],
        }])
        .expect_err("duplicate routes cannot be selected");
    assert_eq!(duplicate, crate::CreatorSaveSelectionError::DuplicateRoute);
    assert_eq!(save_database_state(&repository), before);

    candidate.profile.follower_count = Some(99);
    assert_eq!(
        candidate.profile.follower_count,
        Some(99),
        "the caller now holds different profile facts"
    );
    let selected = crate::test_support::select_required_routes(prepared).unwrap();
    let (mut result, changed) = repository
        .execute_creator_save_batch(selected.clone(), "review-owned".into())
        .unwrap();
    assert!(changed);
    let crate::CreatorSaveRowOutcome::Saved {
        creator, row_id, ..
    } = result.outcomes.remove(0)
    else {
        panic!("reviewed row saves")
    };
    assert_eq!(row_id, "x:user:review-candidate");
    assert_eq!(creator.creator.notes.as_deref(), Some("Reviewed"));
    assert_eq!(creator.creator.follower_counts.get(&Platform::X), Some(&42));
    let committed = save_database_state(&repository);
    let (_, replay_changed) = repository
        .execute_creator_save_batch(selected, "review-owned".into())
        .unwrap();
    assert!(!replay_changed);
    assert_eq!(
        save_database_state(&repository),
        committed,
        "receipt replay must be a no-op"
    );
}

#[test]
fn creator_save_batch_prepares_all_rows_without_writes_then_executes_selected_subsets() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let inputs = (0..3)
        .map(|index| {
            (entry(
                origin.clone(),
                &format!("batch-{index}"),
                &format!("creator{index}"),
            ))
            .clone()
        })
        .collect::<Vec<_>>();
    let before = save_database_state(&repository);
    let mut invalid = inputs.clone();
    invalid[2].row_id = invalid[0].row_id.clone();
    assert_eq!(
        repository
            .prepare_creator_save_batch(invalid, crate::CreatorSaveIntent::SaveOrUpdate)
            .unwrap_err()
            .kind,
        crate::CreatorSavePreparationErrorKind::InvalidFields
    );
    assert_eq!(
        save_database_state(&repository),
        before,
        "invalid preparation must not write or reserve"
    );
    let prepared = repository
        .prepare_creator_save_batch(inputs, crate::CreatorSaveIntent::SaveOrUpdate)
        .unwrap();
    let rows = prepared.rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| &row.row_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        3
    );
    assert_eq!(
        save_database_state(&repository),
        before,
        "preparation must not write or reserve"
    );
    let selection = |index: usize| crate::CreatorSaveRouteSelection {
        row_id: rows[index].row_id.clone(),
        route_ids: rows[index]
            .route_options
            .iter()
            .filter(|route| route.required)
            .map(|route| route.route_id.clone())
            .collect(),
    };
    let selected = prepared.clone().select_routes(vec![selection(1)]).unwrap();
    let (mut single, changed) = repository
        .execute_creator_save_batch(selected, "batch-middle".into())
        .unwrap();
    assert!(changed);
    assert!(
        matches!(single.outcomes.remove(0), crate::CreatorSaveRowOutcome::Saved { row_id, .. } if row_id == "x:user:batch-1")
    );
    assert_eq!(
        repository
            .list_creators(Default::default())
            .unwrap()
            .items
            .len(),
        1
    );
    let selected = prepared
        .select_routes(vec![selection(2), selection(0)])
        .unwrap();
    let (result, changed) = repository
        .execute_creator_save_batch(selected, "batch-remaining".into())
        .unwrap();
    assert!(changed);
    assert_eq!(result.outcomes.len(), 2);
    let saved_ids = result
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            crate::CreatorSaveRowOutcome::Saved { row_id, .. } => row_id.as_str(),
            other => panic!("batch row failed: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        saved_ids,
        ["x:user:batch-0", "x:user:batch-2"],
        "execution follows preparation order"
    );
    assert_eq!(
        repository
            .list_creators(Default::default())
            .unwrap()
            .items
            .len(),
        3
    );
}

#[test]
fn creator_save_batch_rejects_more_than_business_limit_without_writes() {
    let (_directory, repository) = repository();
    let input = (entry(
        platform_search_origin(&repository),
        "batch-limit",
        "limit_creator",
    ))
    .clone();
    let before = save_database_state(&repository);
    let error = repository
        .prepare_creator_save_batch(
            vec![input; crate::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS + 1],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap_err();
    assert_eq!(error.row_id, None);
    assert_eq!(
        error.kind,
        crate::CreatorSavePreparationErrorKind::InvalidFields
    );
    assert_eq!(
        save_database_state(&repository),
        before,
        "oversized batches must not mutate any table"
    );
}

#[test]
fn creator_save_batch_accepts_exactly_business_limit_and_cancellation_leaves_no_rows() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    let inputs = (0..crate::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS)
        .map(|index| {
            (entry(
                origin.clone(),
                &format!("batch-limit-{index}"),
                &format!("creator_{index}"),
            ))
            .clone()
        })
        .collect();
    let before = save_database_state(&repository);
    let prepared = repository
        .prepare_creator_save_batch(inputs, crate::CreatorSaveIntent::SaveOrUpdate)
        .expect("exact business limit is a valid batch");
    assert_eq!(
        prepared.rows().len(),
        crate::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS
    );
    assert_eq!(save_database_state(&repository), before);
    drop(prepared);
    assert_eq!(
        save_database_state(&repository),
        before,
        "cancelled preparation leaves no Creator, origin or reservation"
    );
}

#[test]
fn administrator_prepare_and_cancellation_are_read_only_across_every_table() {
    let (_directory, repository) = repository();
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("telegram-readonly"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .unwrap();
    let input = crate::CanonicalCreatorSaveInput {
        profile_observed_at: 1_700_000_000,
        row_id: "administrator-row".into(),
        account: PlatformAccountCandidate {
            platform: Platform::Telegram,
            external_id: Some("admin-42".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("community_admin".into()),
            display_name: "Community Admin".into(),
            profile_url: Some("https://t.me/community_admin".into()),
        },
        profile: Default::default(),
        provenance: crate::CreatorSaveProvenance::AdministratorSelection {
            source_ref: "community-42/admin-42".into(),
            role: CommunityAdministratorRole::Owner,
            community: CommunityReference {
                external_id: "community-42".into(),
                title: "Community".into(),
                handle: Some("community".into()),
                target_kind: CommunityTargetKind::Supergroup,
            },
            source: CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: operator.id,
            },
        },
        contact_evidence: Vec::new(),
        notes: None,
        tags: None,
        quality: Default::default(),
    };
    let before = save_database_state(&repository);
    let prepared = repository
        .prepare_creator_save_batch(vec![input], crate::CreatorSaveIntent::SaveOrUpdate)
        .unwrap();
    assert_eq!(prepared.rows()[0].account.platform, Platform::Telegram);
    assert!(
        prepared.rows()[0]
            .route_options
            .iter()
            .any(|route| route.required && route.channel == ContactChannel::Telegram)
    );
    assert_eq!(save_database_state(&repository), before);
    drop(prepared);
    assert_eq!(save_database_state(&repository), before);
}
