use super::{execute_prepared, prepare_entries, provenance};
use crate::repository::ContactRepository;
use crate::repository::ConversationRepository;
use crate::repository::CreatorRepository;
use crate::repository::TargetRepository;
use crate::repository::sqlite::SqliteRepository;
use crate::*;
use rusqlite::params;

fn repository() -> (tempfile::TempDir, SqliteRepository) {
    let directory = tempfile::tempdir().unwrap();
    let repository = SqliteRepository::open(directory.path().join("creator.db")).unwrap();
    repository
        .upsert_account_ref(
            AccountRefId::new("operator-x"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    repository
        .upsert_account_ref(
            AccountRefId::new("operator-tg"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .unwrap();
    (directory, repository)
}

fn input(row_id: &str, stable_id: &str) -> CanonicalCreatorSaveInput {
    CanonicalCreatorSaveInput {
        row_id: row_id.into(),
        account: PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some(stable_id.into()),
            target_kind: Some(TargetKind::User),
            handle: Some(format!("creator_{stable_id}")),
            display_name: "Creator".into(),
            profile_url: Some(format!("https://x.com/creator_{stable_id}")),
        },
        profile: CreatorSaveProfile {
            description: Some("Public creator bio".into()),
            follower_count: Some(42),
            ..Default::default()
        },
        provenance: CreatorSaveProvenance::DirectSelection {
            source_ref: format!("source-{row_id}"),
            source_url: Some("https://example.com/list".into()),
            description: Some("User selected this public profile".into()),
            operator_account_ref_id: Some(AccountRefId::new("operator-x")),
        },
        contact_evidence: vec![CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::ProfileBio,
            source_url: Some(format!("https://x.com/creator_{stable_id}")),
            text: "Contact real@example.com".into(),
        }],
        profile_observed_at: 1_800_000_000,
        notes: None,
        tags: None,
        quality: CreatorQualityInput::default(),
    }
}

fn select_all(prepared: PreparedCreatorSaveBatch) -> SelectedPreparedCreatorSaveBatch {
    let selections = prepared
        .rows()
        .into_iter()
        .map(|row| CreatorSaveRouteSelection {
            row_id: row.row_id,
            route_ids: row
                .route_options
                .into_iter()
                .map(|route| route.route_id)
                .collect(),
        })
        .collect();
    prepared.select_routes(selections).unwrap()
}

fn saved(outcome: &CreatorSaveRowOutcome) -> &CreatorRecord {
    match outcome {
        CreatorSaveRowOutcome::Saved { creator, .. } => creator,
        other => panic!("expected saved: {other:?}"),
    }
}

fn count(repository: &SqliteRepository, table: &str) -> u64 {
    repository
        .lock()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn durable_counts(repository: &SqliteRepository) -> Vec<u64> {
    [
        "creators",
        "platform_accounts",
        "creator_account_links",
        "platform_account_observations",
        "observation_sources",
        "creator_origins",
        "contact_locators",
        "creator_contact_routes",
        "locator_observations",
        "creator_contact_route_dependencies",
        "idempotency_records",
    ]
    .iter()
    .map(|table| count(repository, table))
    .collect()
}

#[test]
fn prepare_is_read_only_and_direct_sources_round_trip_without_manual_origins() {
    let (_directory, repository) = repository();
    let canonical = input("one", "1");
    let before = durable_counts(&repository);
    let prepared = prepare_entries(
        &repository,
        vec![canonical.clone()],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    assert_eq!(durable_counts(&repository), before);
    let selected = select_all(prepared);
    let (result, changed) =
        execute_prepared(&repository, selected.clone(), "execution-one".into()).unwrap();
    assert!(changed);
    let creator = saved(&result.outcomes[0]);
    assert!(
        creator.origins.is_empty(),
        "direct selection is not manual entry or discovery"
    );
    assert_eq!(creator.creator.follower_counts[&Platform::X], 42);
    assert_eq!(creator.platform_accounts[0].external_id, "1");
    for source in &creator.observation_sources {
        assert_eq!(source.account_ref_id, Some(AccountRefId::new("operator-x")));
        assert_eq!(
            source.metadata["creator_save_v1"],
            serde_json::to_value(&canonical.provenance).unwrap()
        );
        assert!(!matches!(source.kind, SourceKind::ManualEntry));
    }
    let saved_counts = durable_counts(&repository);
    let (replay, changed) =
        execute_prepared(&repository, selected, "execution-one".into()).unwrap();
    assert!(!changed);
    assert_eq!(saved(&replay.outcomes[0]).creator.id, creator.creator.id);
    assert_eq!(durable_counts(&repository), saved_counts);
    let prepared_again = prepare_entries(
        &repository,
        vec![canonical],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    assert_eq!(durable_counts(&repository), saved_counts);
    let (again, _) = execute_prepared(
        &repository,
        select_all(prepared_again),
        "execution-two".into(),
    )
    .unwrap();
    assert_eq!(saved(&again.outcomes[0]).creator.id, creator.creator.id);
    let mut expected = saved_counts;
    expected[10] += 1;
    assert_eq!(
        durable_counts(&repository),
        expected,
        "a new preparation does not duplicate source or locator facts"
    );
}

#[test]
fn held_payload_survives_provider_changes_and_execution_key_rejects_different_selection() {
    let (_directory, repository) = repository();
    let canonical = input("one", "1");
    let prepared = prepare_entries(
        &repository,
        vec![canonical.clone()],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    let selected = select_all(prepared);
    let mut changed = canonical;
    changed.account.display_name = "Unreviewed provider change".into();
    changed.profile.follower_count = Some(900);
    let (result, _) = execute_prepared(&repository, selected, "held-key".into()).unwrap();
    assert_eq!(saved(&result.outcomes[0]).creator.display_name, "Creator");
    assert_eq!(
        saved(&result.outcomes[0]).creator.follower_counts[&Platform::X],
        42
    );
    let changed = select_all(
        prepare_entries(&repository, vec![changed], CreatorSaveIntent::SaveOrUpdate).unwrap(),
    );
    assert!(
        execute_prepared(&repository, changed, "held-key".into())
            .unwrap_err()
            .contains("conflicts")
    );
}

#[test]
fn same_creator_rows_union_replacement_tags_and_read_back_final_facts() {
    let (_directory, repository) = repository();
    let seed = select_all(
        prepare_entries(
            &repository,
            vec![input("seed", "1")],
            CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap(),
    );
    execute_prepared(&repository, seed, "seed-before-tag-updates".into()).unwrap();
    let mut first = input("first", "1");
    first.tags = Some(vec!["first".into()]);
    let mut second = input("second", "1");
    second.tags = Some(vec!["second".into()]);
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![first, second],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "union".into(),
    )
    .unwrap()
    .0;
    assert_eq!(
        saved(&result.outcomes[0]).creator.id,
        saved(&result.outcomes[1]).creator.id
    );
    for outcome in &result.outcomes {
        assert_eq!(saved(outcome).creator.tags, vec!["first", "second"]);
    }
    let mut replacement = input("replacement", "1");
    replacement.tags = Some(vec!["replacement".into()]);
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![replacement],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "replacement".into(),
    )
    .unwrap()
    .0;
    assert_eq!(saved(&result.outcomes[0]).creator.tags, vec!["replacement"]);
}

#[test]
fn late_row_failure_rolls_back_its_creator_sources_and_contact_facts() {
    let (_directory, repository) = repository();
    let mut failing = input("failed", "2");
    failing.contact_evidence = vec![CreatorSaveContactEvidence::ProfileField {
        source: SourceKind::ProfileBio,
        source_url: failing.account.profile_url.clone(),
        text: "derived@example.com".into(),
    }];
    repository.lock().unwrap().execute_batch("CREATE TEMP TRIGGER fail_second_row BEFORE INSERT ON locator_observations WHEN NEW.observed_value = 'derived@example.com' BEGIN SELECT RAISE(ABORT, 'injected late route failure'); END;").unwrap();
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![input("good", "1"), failing],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "partial".into(),
    )
    .unwrap()
    .0;
    assert!(matches!(
        &result.outcomes[..],
        [
            CreatorSaveRowOutcome::Saved { .. },
            CreatorSaveRowOutcome::Failed { .. }
        ]
    ));
    assert_eq!(count(&repository, "creators"), 1);
    assert_eq!(count(&repository, "platform_accounts"), 1);
    assert_eq!(count(&repository, "idempotency_records"), 1);
    assert_eq!(repository.lock().unwrap().query_row("SELECT count(*) FROM observation_sources WHERE metadata_json LIKE '%source-failed%'", [], |row| row.get::<_, u64>(0)).unwrap(), 0);
}

#[test]
fn receipt_failure_rolls_back_all_row_effects_and_same_key_can_reconcile_then_retry() {
    let (_directory, repository) = repository();
    let prepared = select_all(
        prepare_entries(
            &repository,
            vec![input("one", "1")],
            CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap(),
    );
    repository.lock().unwrap().execute_batch("CREATE TEMP TRIGGER fail_save_receipt BEFORE INSERT ON idempotency_records WHEN NEW.action = 'creator_save_batch_v1' BEGIN SELECT RAISE(ABORT, 'injected receipt failure'); END;").unwrap();
    assert!(execute_prepared(&repository, prepared.clone(), "atomic-key".into()).is_err());
    assert_eq!(count(&repository, "creators"), 0);
    assert_eq!(count(&repository, "observation_sources"), 0);
    repository
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_save_receipt")
        .unwrap();
    let (result, _) = execute_prepared(&repository, prepared, "atomic-key".into()).unwrap();
    assert_eq!(
        saved(&result.outcomes[0]).platform_accounts[0].external_id,
        "1"
    );
}

#[test]
fn telegram_preparation_writes_nothing_and_one_admin_keeps_both_community_sources() {
    let (_directory, repository) = repository();
    let administrator = |row_id: &str, community_id: &str| {
        let mut input = input(row_id, "9");
        input.account.platform = Platform::Telegram;
        input.account.profile_url = Some("https://t.me/creator_9".into());
        input.contact_evidence.clear();
        input.provenance = CreatorSaveProvenance::AdministratorSelection {
            source_ref: format!("community-{community_id}"),
            role: CommunityAdministratorRole::Administrator,
            community: CommunityReference {
                external_id: community_id.into(),
                title: format!("Community {community_id}"),
                handle: None,
                target_kind: CommunityTargetKind::Supergroup,
            },
            source: CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: AccountRefId::new("operator-tg"),
            },
        };
        input
    };
    let seed = select_all(
        prepare_entries(
            &repository,
            vec![administrator("one", "100")],
            CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap(),
    );
    execute_prepared(&repository, seed, "seed-before-community-updates".into()).unwrap();
    let before = durable_counts(&repository);
    let prepared = prepare_entries(
        &repository,
        vec![administrator("one", "100"), administrator("two", "200")],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    assert_eq!(durable_counts(&repository), before);
    let result = execute_prepared(&repository, select_all(prepared), "admin-two-groups".into())
        .unwrap()
        .0;
    let creator = repository
        .get_creator(&saved(&result.outcomes[0]).creator.id)
        .unwrap();
    assert!(creator.origins.is_empty());
    let sources: Vec<_> = creator
        .observation_sources
        .iter()
        .filter(|source| source.kind == SourceKind::CommunityAdministrator)
        .collect();
    assert_eq!(sources.len(), 2);
    let mut ids = sources
        .iter()
        .map(|source| {
            source.metadata["creator_save_v1"]["community"]["external_id"]
                .as_str()
                .unwrap()
        })
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(ids, vec!["100", "200"]);
    assert!(
        sources
            .iter()
            .all(|source| source.account_ref_id == Some(AccountRefId::new("operator-tg")))
    );
}

#[test]
fn a_blocked_first_row_does_not_hide_the_first_eligible_identity_representative() {
    let (_directory, repository) = repository();
    let owner = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Route Owner".into(),
            notes: None,
        })
        .expect("route owner");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "blocked_handle".into(),
            display_value: "@blocked_handle".into(),
        })
        .expect("blocked route locator");
    repository
        .link_creator_contact_route(&owner.creator.id, &locator.id, Confidence::Exact)
        .expect("blocked route owner");

    let mut blocked = input("blocked", "shared-stable");
    blocked.account.handle = Some("blocked_handle".into());
    blocked.account.profile_url = Some("https://x.com/blocked_handle".into());
    let mut representative = input("representative", "shared-stable");
    representative.account.handle = Some("clear_handle".into());
    representative.account.profile_url = Some("https://x.com/clear_handle".into());
    let mut duplicate = input("duplicate", "shared-stable");
    duplicate.account.handle = Some("another_clear_handle".into());
    duplicate.account.profile_url = Some("https://x.com/another_clear_handle".into());

    let prepared = prepare_entries(
        &repository,
        vec![blocked, representative, duplicate],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .expect("mixed blocked and eligible rows");
    let rows = prepared.rows();
    assert_eq!(rows[0].status, CreatorSavePreparedStatus::ContactConflict);
    assert_eq!(rows[1].status, CreatorSavePreparedStatus::EligibleNew);
    assert_eq!(rows[2].status, CreatorSavePreparedStatus::DuplicateSkipped);
}

#[test]
fn every_row_in_a_contradictory_existing_update_group_is_blocked() {
    let (_directory, repository) = repository();
    let seed = input("seed", "777");
    let selected = select_all(
        prepare_entries(
            &repository,
            vec![seed.clone()],
            CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("seed preparation"),
    );
    execute_prepared(&repository, selected, "seed-existing-group".into()).expect("seed Creator");

    let mut first = input("first-update", "777");
    first.notes = Some("A".into());
    let mut second = input("second-update", "777");
    second.notes = Some("B".into());
    let mut third = input("third-update", "777");
    third.notes = Some("A".into());
    let prepared = prepare_entries(
        &repository,
        vec![first, second, third],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .expect("existing update group preparation");
    assert!(prepared.rows().iter().all(|row| {
        row.status == CreatorSavePreparedStatus::IdentityPending
            && row.reason == Some(CreatorSavePreparedReason::ConflictingUpdate)
    }));
}

#[test]
fn commit_failure_reconciles_receipt_before_an_explicit_same_payload_retry() {
    let (_directory, repository) = repository();
    let prepared = select_all(
        prepare_entries(
            &repository,
            vec![input("one", "1")],
            CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap(),
    );
    repository
        .lock()
        .unwrap()
        .commit_hook(Some(|| true))
        .expect("install injected commit rejection hook");
    let error =
        execute_prepared(&repository, prepared.clone(), "uncertain-commit".into()).unwrap_err();
    assert!(error.contains("outcome is unknown"), "{error}");
    assert_eq!(count(&repository, "idempotency_records"), 0);
    assert_eq!(count(&repository, "creators"), 0);
    repository
        .lock()
        .unwrap()
        .commit_hook(None::<fn() -> bool>)
        .expect("remove injected commit rejection hook");
    let (_, changed) =
        execute_prepared(&repository, prepared.clone(), "uncertain-commit".into()).unwrap();
    assert!(changed);
    let (_, changed) = execute_prepared(&repository, prepared, "uncertain-commit".into()).unwrap();
    assert!(!changed);
    assert_eq!(count(&repository, "creators"), 1);
    assert_eq!(count(&repository, "idempotency_records"), 1);
}

#[test]
fn one_unusable_row_is_reported_beside_the_rows_that_prepared() {
    let (_directory, repository) = repository();
    let mut unusable = input("two", "2");
    unusable.account.display_name = "   ".into();
    // The fixture shares one bio email across rows; independent Creators need
    // their own, or the second row is a genuinely ambiguous identity.
    let mut first = input("one", "1");
    first.contact_evidence.clear();
    let mut third = input("three", "3");
    third.contact_evidence.clear();
    let prepared = prepare_entries(
        &repository,
        vec![first, unusable, third],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    assert_eq!(
        prepared
            .rows()
            .iter()
            .map(|row| row.row_id.clone())
            .collect::<Vec<_>>(),
        ["one", "three"],
        "a row this batch cannot prepare must not discard the rows it can"
    );
    assert_eq!(
        prepared.failures(),
        [CreatorSavePreparationRowFailure {
            row_id: "two".into(),
            kind: CreatorSavePreparationErrorKind::InvalidFields,
            platform: Platform::X,
            display_name: "   ".into(),
            handle: Some("creator_2".into()),
            profile_url: Some("https://x.com/creator_2".into()),
        }]
    );
    // The rows that did prepare still hold real confirmation authority.
    let result = execute_prepared(&repository, select_all(prepared), "mixed".into())
        .unwrap()
        .0;
    assert_eq!(result.outcomes.len(), 2);
    assert!(
        result
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, CreatorSaveRowOutcome::Saved { .. }))
    );
    assert_eq!(count(&repository, "creators"), 2);
}

#[test]
fn a_batch_with_no_preparable_row_still_fails_with_that_rows_reason() {
    let (_directory, repository) = repository();
    let mut unusable = input("only", "1");
    unusable.account.display_name = " ".into();
    let error =
        prepare_entries(&repository, vec![unusable], CreatorSaveIntent::SaveOrUpdate).unwrap_err();
    assert_eq!(error.row_id.as_deref(), Some("only"));
    assert_eq!(error.kind, CreatorSavePreparationErrorKind::InvalidFields);
    assert_eq!(count(&repository, "creators"), 0);
}

#[test]
fn a_repeated_row_identifier_still_ends_the_whole_batch() {
    let (_directory, repository) = repository();
    // Per-row reporting has to name the row it belongs to, and a repeated
    // identifier makes every later route selection ambiguous. This one is not
    // degraded to a per-row failure.
    let error = prepare_entries(
        &repository,
        vec![input("same", "1"), input("same", "2")],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap_err();
    assert_eq!(error.row_id.as_deref(), Some("same"));
    assert_eq!(error.kind, CreatorSavePreparationErrorKind::InvalidFields);
}

#[test]
fn hub_parent_is_verified_read_only_and_must_be_selected_with_its_child() {
    let (_directory, repository) = repository();
    let mut canonical = input("one", "1");
    canonical.contact_evidence = vec![CreatorSaveContactEvidence::HeldHub {
        source_url: "https://linktr.ee/creator".into(),
        text: "hub@example.com".into(),
        links: vec![],
    }];
    let before = durable_counts(&repository);
    let error = prepare_entries(
        &repository,
        vec![canonical.clone()],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap_err();
    assert_eq!(error.row_id.as_deref(), Some(canonical.row_id.as_str()));
    assert_eq!(
        error.kind,
        crate::CreatorSavePreparationErrorKind::ContactNormalization
    );
    assert_eq!(durable_counts(&repository), before);
    canonical
        .contact_evidence
        .push(CreatorSaveContactEvidence::ProfileLink {
            source: SourceKind::ProfileWebsite,
            source_url: canonical.account.profile_url.clone(),
            display_text: "https://linktr.ee/creator".into(),
            target_url: "https://linktr.ee/creator".into(),
        });
    let prepared = prepare_entries(
        &repository,
        vec![canonical],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .unwrap();
    let preview = prepared.rows().remove(0);
    let without_parent = preview
        .route_options
        .iter()
        .filter(|route| route.channel != ContactChannel::LinkHub)
        .map(|route| route.route_id.clone())
        .collect();
    assert_eq!(
        prepared
            .clone()
            .select_routes(vec![CreatorSaveRouteSelection {
                row_id: preview.row_id,
                route_ids: without_parent
            }])
            .unwrap_err(),
        CreatorSaveSelectionError::MissingParentRoute
    );
    let result = execute_prepared(&repository, select_all(prepared), "hub".into())
        .unwrap()
        .0;
    assert_eq!(saved(&result.outcomes[0]).contact_routes.len(), 3);
    assert_eq!(count(&repository, "creator_contact_route_dependencies"), 1);
}

#[test]
fn real_discovery_keeps_historical_origin_and_source_bytes() {
    let (_directory, repository) = repository();
    let mut canonical = input("one", "1");
    let origin = CreatorCandidateOrigin::PlatformSearch {
        discovery_id: DiscoveryId::new("real-discovery"),
        account_ref_id: AccountRefId::new("operator-x"),
        query: "real query".into(),
        search_kind: "users".into(),
    };
    canonical.provenance = CreatorSaveProvenance::Discovery {
        candidate_id: "real-candidate".into(),
        origin: origin.clone(),
        selection_context: None,
    };
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "discovery".into(),
    )
    .unwrap()
    .0;
    let creator = saved(&result.outcomes[0]);
    assert!(
        matches!(&creator.origins[..], [CreatorOrigin::PlatformSearch { discovery_id, .. }] if discovery_id.as_str() == "real-discovery")
    );
    let source = creator
        .observation_sources
        .iter()
        .find(|source| {
            source.external_ref.as_deref() == Some("candidate:real-discovery:real-candidate")
        })
        .unwrap();
    assert_eq!(
        source.metadata,
        provenance::origin_metadata(&origin, "real-candidate")
    );
    assert!(
        creator
            .observation_sources
            .iter()
            .any(|source| source.external_ref.as_deref()
                == Some(
                    "contact:real-discovery:profile_bio:https://x.com/creator_1:real-candidate"
                ))
    );
}

#[test]
fn full_prepared_encoding_binds_evidence_that_has_the_same_visible_preview() {
    let (_directory, repository) = repository();
    let mut first = input("one", "1");
    first.contact_evidence = vec![CreatorSaveContactEvidence::ProfileField {
        source: SourceKind::ProfileBio,
        source_url: first.account.profile_url.clone(),
        text: "First public evidence with no contact addresses".into(),
    }];
    let mut second = first.clone();
    second.contact_evidence = vec![CreatorSaveContactEvidence::ProfileField {
        source: SourceKind::ProfileBio,
        source_url: second.account.profile_url.clone(),
        text: "Different public evidence with no contact addresses".into(),
    }];
    let first = prepare_entries(&repository, vec![first], CreatorSaveIntent::SaveOrUpdate).unwrap();
    let second =
        prepare_entries(&repository, vec![second], CreatorSaveIntent::SaveOrUpdate).unwrap();
    assert_eq!(
        serde_json::to_value(first.rows()).unwrap(),
        serde_json::to_value(second.rows()).unwrap()
    );
    assert_ne!(
        first.encoded_payload().unwrap(),
        second.encoded_payload().unwrap()
    );
}

#[test]
fn source_fact_changes_are_history_and_identical_new_reads_do_not_duplicate_facts() {
    let (_directory, repository) = repository();
    let canonical = input("one", "1");
    let first = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "initial-facts".into(),
    )
    .unwrap()
    .0;
    let original = saved(&first.outcomes[0]);
    assert_eq!(
        original.account_observations[0].observed_at,
        canonical.profile_observed_at
    );
    let before = durable_counts(&repository);
    let mut observed_again = canonical.clone();
    observed_again.profile_observed_at += 10;
    execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![observed_again],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "same-facts-new-read".into(),
    )
    .unwrap();
    let mut expected = before.clone();
    expected[10] += 1;
    assert_eq!(durable_counts(&repository), expected);
    let mut new_source = canonical;
    if let CreatorSaveProvenance::DirectSelection { description, .. } = &mut new_source.provenance {
        *description = Some("A later, different public source description".into());
    }
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![new_source.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "changed-source".into(),
    )
    .unwrap()
    .0;
    let creator = saved(&result.outcomes[0]);
    assert_eq!(creator.creator.id, original.creator.id);
    assert!(
        creator
            .observation_sources
            .iter()
            .any(|source| source.metadata["creator_save_v1"]["description"]
                == "User selected this public profile")
    );
    assert!(
        creator
            .observation_sources
            .iter()
            .any(|source| source.metadata["creator_save_v1"]
                == serde_json::to_value(new_source.provenance.clone()).unwrap())
    );
    assert_eq!(creator.account_observations.len(), 2);
}

#[test]
fn same_second_profile_changes_use_real_insertion_order_and_older_reads_do_not_replace_counts() {
    let (_directory, repository) = repository();
    let canonical = input("one", "1");
    execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "first-profile".into(),
    )
    .unwrap();
    let mut changed = canonical.clone();
    changed.account.display_name = "Latest confirmed profile".into();
    changed.profile.follower_count = Some(99);
    let second = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![changed.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "second-profile".into(),
    )
    .unwrap()
    .0;
    let creator = saved(&second.outcomes[0]);
    assert_eq!(creator.account_observations.len(), 2);
    assert_eq!(
        creator.account_observations[0].display_name.as_deref(),
        Some("Latest confirmed profile")
    );
    assert_eq!(
        creator.account_observations[0].observed_at,
        canonical.profile_observed_at
    );
    assert_eq!(
        creator.account_observations[1].observed_at,
        canonical.profile_observed_at
    );
    assert_eq!(creator.creator.follower_counts[&Platform::X], 99);
    // A -> B -> A is another real observation, even though A existed earlier.
    let third = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "third-profile".into(),
    )
    .unwrap()
    .0;
    assert_eq!(saved(&third.outcomes[0]).account_observations.len(), 3);
    assert_eq!(
        saved(&third.outcomes[0]).account_observations[0]
            .display_name
            .as_deref(),
        Some("Creator")
    );
    changed.profile_observed_at += 20;
    execute_prepared(
        &repository,
        select_all(
            prepare_entries(&repository, vec![changed], CreatorSaveIntent::SaveOrUpdate).unwrap(),
        ),
        "newer-read".into(),
    )
    .unwrap();
    let mut delayed = canonical;
    delayed.profile_observed_at += 5;
    delayed.profile.follower_count = Some(50);
    let result = execute_prepared(
        &repository,
        select_all(
            prepare_entries(&repository, vec![delayed], CreatorSaveIntent::SaveOrUpdate).unwrap(),
        ),
        "delayed-read".into(),
    )
    .unwrap()
    .0;
    assert_eq!(
        saved(&result.outcomes[0]).creator.follower_counts[&Platform::X],
        99
    );
    assert_eq!(
        saved(&result.outcomes[0]).account_observations[0]
            .display_name
            .as_deref(),
        Some("Latest confirmed profile")
    );
}

#[test]
fn list_get_and_inbox_read_canonical_profiles_and_retained_native_observations() {
    let (_directory, repository) = repository();
    let canonical = input("one", "1");
    let first = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "legacy-seed".into(),
    )
    .unwrap()
    .0;
    let creator = saved(&first.outcomes[0]);
    let creator_id = creator.creator.id.clone();
    let account_id = &creator.platform_accounts[0].id;
    let route_id = &creator
        .contact_routes
        .iter()
        .find(|route| route.locator.channel == ContactChannel::X)
        .unwrap()
        .route
        .id;
    {
        let connection = repository.lock().unwrap();
        // A released v36 observation: retain its native field names verbatim.
        connection.execute("UPDATE platform_account_observations SET public_json = ?1 WHERE platform_account_id = ?2", params![r#"{"profile_image_url":"https://img.example/legacy.jpg","followers":42}"#, account_id.as_str()]).unwrap();
        connection.execute("INSERT INTO resolved_targets(id, account_ref_id, platform, target_kind, external_target_id, platform_account_id, metadata_json, created_at, updated_at) VALUES('avatar-target', 'operator-x', 'x', 'user', '1', ?1, '{}', 1, 1)", [account_id.as_str()]).unwrap();
        connection.execute_batch("INSERT INTO conversations(id, account_ref_id, resolved_target_id, channel, provider_thread_id, created_at, updated_at) VALUES('avatar-conversation', 'operator-x', 'avatar-target', 'x', 'thread-1', 1, 1)").unwrap();
        connection.execute("INSERT INTO conversation_contact_routes(conversation_id, contact_route_id, created_at) VALUES('avatar-conversation', ?1, 1)", [route_id.as_str()]).unwrap();
    }
    let old_record = repository.get_creator(&creator_id).unwrap();
    assert_eq!(
        old_record.account_observations[0].public_data["followers"].as_u64(),
        Some(42)
    );
    assert_eq!(
        repository.list_creators(Default::default()).unwrap().items[0]
            .avatar_url
            .as_deref(),
        Some("https://img.example/legacy.jpg")
    );
    assert_eq!(
        repository
            .list_conversations(Default::default())
            .unwrap()
            .items[0]
            .creator_avatar_url
            .as_deref(),
        Some("https://img.example/legacy.jpg")
    );
    let mut updated = canonical;
    updated.account.display_name = "Canonical profile".into();
    updated.profile.avatar_url = Some("https://img.example/canonical.jpg".into());
    updated.profile.follower_count = Some(77);
    execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![updated.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "canonical-update".into(),
    )
    .unwrap();
    let record = repository.get_creator(&creator_id).unwrap();
    assert_eq!(record.account_observations.len(), 2);
    assert_eq!(
        record.account_observations[0].observed_at,
        record.account_observations[1].observed_at
    );
    assert_eq!(
        record.account_observations[0].public_data["avatar_url"],
        "https://img.example/canonical.jpg"
    );
    assert_eq!(
        record.account_observations[0].display_name.as_deref(),
        Some("Canonical profile")
    );
    assert_eq!(
        record.account_observations[0].public_data["follower_count"].as_u64(),
        Some(77)
    );
    let list = repository.list_creators(Default::default()).unwrap();
    assert_eq!(
        list.items[0].avatar_url.as_deref(),
        Some("https://img.example/canonical.jpg")
    );
    assert_eq!(list.items[0].follower_counts[&Platform::X], 77);
    let inbox = repository.list_conversations(Default::default()).unwrap();
    assert_eq!(
        inbox.items[0].creator_avatar_url.as_deref(),
        Some("https://img.example/canonical.jpg")
    );
    assert_eq!(
        super::super::creators::avatar_url_from_public_data(
            Platform::X,
            &serde_json::json!({"avatar_url":"https://img.example/canonical.jpg","profile_image_url":"https://img.example/legacy.jpg"})
        ),
        Some("https://img.example/canonical.jpg".into())
    );
}

#[test]
fn materialized_profiles_at_the_business_limit_prepare_select_execute_and_replay() {
    let (_directory, repository) = repository();
    let rows = MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let inputs: Vec<_> = (0..rows).map(|index| {
        let mut row = input(&format!("batch-row-{index}"), &(10_000 + index).to_string());
        row.account.display_name = format!("Research Creator {index}");
        row.profile.avatar_url = Some(format!("https://images.example.com/creators/{index}/avatar.jpg"));
        row.profile.description = Some("Independent researcher publishing original weekly technology analysis, source-linked interviews, and practical product demonstrations for an active specialist audience. ".repeat(3));
        row.profile.website_urls = vec![format!("https://creator-{index}.example.com/contact")];
        row.profile.follower_count = Some(10_000 + index as u64);
        row.notes = Some("Selected for the current campaign after reviewing recent original posts, audience fit, and a usable public contact route. ".repeat(2));
        row.contact_evidence = vec![CreatorSaveContactEvidence::Observation { channel: ContactChannel::Email, kind: LocatorKind::EmailAddress, value: format!("creator-{index}@example.com"), source: SourceKind::ProfileBio, source_url: row.account.profile_url.clone() }];
        row.contact_evidence.push(CreatorSaveContactEvidence::ProfileLink {
            source: SourceKind::ProfileWebsite,
            source_url: row.account.profile_url.clone(),
            display_text: "Website".into(),
            target_url: format!("https://creator-{index}.example.com/contact"),
        });
        row
    }).collect();
    let canonical_bytes = serde_json::to_vec(&inputs).unwrap().len();
    assert!(canonical_bytes <= MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES);
    let prepared = prepare_entries(&repository, inputs, CreatorSaveIntent::SaveOrUpdate).unwrap();
    assert_eq!(prepared.rows().len(), rows);
    let prepared_bytes = prepared.encoded_payload().unwrap().len();
    eprintln!(
        "{rows} canonical profiles: input_bytes={canonical_bytes}, prepared_bytes={prepared_bytes}, materialized_cap={MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES}"
    );
    assert_eq!(count(&repository, "creators"), 0);
    assert_eq!(count(&repository, "idempotency_records"), 0);
    let selected = select_all(prepared);
    let (result, changed) = execute_prepared(
        &repository,
        selected.clone(),
        "business-limit-profile-batch".into(),
    )
    .unwrap();
    assert!(changed);
    assert_eq!(result.outcomes.len(), rows);
    assert!(
        result
            .outcomes
            .iter()
            .all(|outcome| matches!(outcome, CreatorSaveRowOutcome::Saved { .. }))
    );
    assert_eq!(count(&repository, "creators"), rows as u64);
    assert_eq!(count(&repository, "platform_accounts"), rows as u64);
    assert_eq!(count(&repository, "creator_origins"), 0);
    assert_eq!(count(&repository, "idempotency_records"), 1);
    assert_eq!(
        saved(&result.outcomes[rows - 1]).creator.follower_counts[&Platform::X],
        (10_000 + rows - 1) as u64
    );
    let before_replay = durable_counts(&repository);
    let (replayed, changed) =
        execute_prepared(&repository, selected, "business-limit-profile-batch".into()).unwrap();
    assert!(!changed);
    assert_eq!(replayed.outcomes.len(), rows);
    assert_eq!(durable_counts(&repository), before_replay);
}

#[test]
fn provenance_regression_optional_descriptions_prepare_and_read_back_without_placeholders() {
    let (_directory, repository) = repository();
    let mut direct = input("direct-without-description", "11");
    direct.contact_evidence.clear();
    if let CreatorSaveProvenance::DirectSelection { description, .. } = &mut direct.provenance {
        *description = None;
    }
    let mut provider = input("provider-without-description", "22");
    provider.contact_evidence.clear();
    provider.provenance = CreatorSaveProvenance::ProviderRead {
        source_ref: "provider-page-22".into(),
        operation_id: "x.accounts.get".into(),
        source_url: Some("https://x.com/creator_22".into()),
        description: None,
        operator_account_ref_id: Some(AccountRefId::new("operator-x")),
    };
    let prepared = prepare_entries(
        &repository,
        vec![direct, provider],
        CreatorSaveIntent::SaveOrUpdate,
    )
    .expect("omitted descriptions are valid canonical input");
    assert_eq!(count(&repository, "creators"), 0);
    assert!(prepared.rows().iter().all(|row| matches!(
        &row.provenance,
        CreatorSaveProvenance::DirectSelection {
            description: None,
            ..
        } | CreatorSaveProvenance::ProviderRead {
            description: None,
            ..
        }
    )));
    let result = execute_prepared(
        &repository,
        select_all(prepared),
        "optional-descriptions".into(),
    )
    .unwrap()
    .0;
    for outcome in &result.outcomes {
        let record = repository.get_creator(&saved(outcome).creator.id).unwrap();
        assert!(record.origins.is_empty());
        assert!(
            record
                .observation_sources
                .iter()
                .all(|source| source.metadata["creator_save_v1"]["description"].is_null())
        );
        assert!(
            record
                .observation_sources
                .iter()
                .all(|source| source.metadata["creator_save_v1"]["description"] != "")
        );
    }
}

#[test]
fn provenance_regression_discovery_context_is_immutable_distinct_and_preserves_legacy_bytes() {
    let (_directory, repository) = repository();
    let mut canonical = input("context-row", "33");
    canonical.provenance = CreatorSaveProvenance::Discovery {
        candidate_id: "real-context-candidate".into(),
        origin: CreatorCandidateOrigin::PlatformSearch {
            discovery_id: DiscoveryId::new("real-context-discovery"),
            account_ref_id: AccountRefId::new("operator-x"),
            query: "research creators".into(),
            search_kind: "users".into(),
        },
        selection_context: None,
    };
    let baseline = execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical.clone()],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "legacy-context-baseline".into(),
    )
    .unwrap()
    .0;
    let original = saved(&baseline.outcomes[0]);
    let original_creator_id = original.creator.id.clone();
    let original_origins = serde_json::to_value(&original.origins).unwrap();
    let legacy_rows = |repository: &SqliteRepository| {
        let connection = repository.lock().unwrap();
        let mut statement = connection.prepare("SELECT json_array(id, kind, platform, source_url, external_ref, account_ref_id, metadata_json, observed_at) FROM observation_sources WHERE external_ref NOT LIKE 'creator-discovery-selection:%' ORDER BY id").unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let original_source_bytes = legacy_rows(&repository);
    let context = CreatorDiscoverySelectionContext {
        operation_id: "x.accounts.search".into(),
        source_ref: "actual-provider-page-1".into(),
        source_url: Some("https://example.com/research/source-1".into()),
        description: None,
    };
    let mut contexts = vec![context.clone(); 5];
    contexts[1].operation_id = "x.accounts.search.advanced".into();
    contexts[2].source_ref = "actual-provider-page-2".into();
    contexts[3].source_url = Some("https://example.com/research/source-2".into());
    contexts[4].description = Some("Selected from the original cited research list".into());
    let mut encodings = std::collections::BTreeSet::new();
    for (index, context) in contexts.iter().enumerate() {
        let mut row = canonical.clone();
        if let CreatorSaveProvenance::Discovery {
            selection_context, ..
        } = &mut row.provenance
        {
            *selection_context = Some(context.clone());
        }
        let before_prepare = durable_counts(&repository);
        let prepared =
            prepare_entries(&repository, vec![row], CreatorSaveIntent::SaveOrUpdate).unwrap();
        assert_eq!(durable_counts(&repository), before_prepare);
        assert!(
            encodings.insert(prepared.encoded_payload().unwrap()),
            "changing only context field {index} must change the immutable payload"
        );
        let result = execute_prepared(
            &repository,
            select_all(prepared),
            format!("discovery-context-{index}"),
        )
        .unwrap()
        .0;
        assert_eq!(saved(&result.outcomes[0]).creator.id, original_creator_id);
        assert_eq!(
            legacy_rows(&repository),
            original_source_bytes,
            "selection context must not rewrite legacy metadata, external refs, IDs or timestamps"
        );
    }
    let record = repository.get_creator(&original_creator_id).unwrap();
    assert_eq!(
        serde_json::to_value(&record.origins).unwrap(),
        original_origins
    );
    assert_eq!(
        record.account_observations.len(),
        original.account_observations.len(),
        "selection provenance does not duplicate the profile model"
    );
    let sources: Vec<_> = record
        .observation_sources
        .iter()
        .filter(|source| {
            source
                .external_ref
                .as_deref()
                .is_some_and(|value| value.starts_with("creator-discovery-selection:v1:"))
        })
        .collect();
    assert_eq!(sources.len(), contexts.len());
    let source_refs: std::collections::BTreeSet<_> = sources
        .iter()
        .map(|source| source.external_ref.as_deref().unwrap())
        .collect();
    assert_eq!(source_refs.len(), contexts.len());
    for context in &contexts {
        let source = sources
            .iter()
            .find(|source| {
                source.metadata["creator_discovery_selection_v1"]["selection_context"]
                    == serde_json::to_value(context).unwrap()
            })
            .expect("all supplied context facts are readable through get_creator");
        assert_eq!(source.source_url, context.source_url);
        assert_eq!(source.account_ref_id, Some(AccountRefId::new("operator-x")));
        assert!(
            record
                .contact_routes
                .iter()
                .any(|route| route
                    .observations
                    .iter()
                    .any(|observation| observation.source.id == source.id
                        && observation.observation.platform_account_id.as_ref()
                            == Some(&record.platform_accounts[0].id))),
            "context is attached to the confirmed initial account route"
        );
    }
    if let CreatorSaveProvenance::Discovery {
        selection_context, ..
    } = &mut canonical.provenance
    {
        *selection_context = Some(context);
    }
    let before_repeat = durable_counts(&repository);
    execute_prepared(
        &repository,
        select_all(
            prepare_entries(
                &repository,
                vec![canonical],
                CreatorSaveIntent::SaveOrUpdate,
            )
            .unwrap(),
        ),
        "discovery-context-repeat".into(),
    )
    .unwrap();
    let mut expected = before_repeat;
    expected[10] += 1;
    assert_eq!(
        durable_counts(&repository),
        expected,
        "same complete context has stable source and locator-observation identity"
    );
}

#[test]
fn export_creator_candidate_node_compatibility_fixture() {
    let Some(directory) = std::env::var_os("ONECREW_CREATOR_CANDIDATE_ORACLE_DIRECTORY") else { return; };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let repository = SqliteRepository::open(directory.join("creator.sqlite")).unwrap();
    repository.upsert_account_ref(AccountRefId::new("operator-x"), Platform::X, "profile", "@operator").unwrap();
    let mut first = input("row-original-x", "123456");
    first.profile.follower_count = Some(u64::MAX);
    first.tags = Some(vec!["公开".into()]);
    let mut second = input("row-original-youtube", "UCabcdefghijklmnopqrstuv");
    second.account.platform = Platform::YouTube;
    second.account.target_kind = Some(TargetKind::Channel);
    second.account.handle = None;
    second.account.profile_url = Some("https://youtube.com/channel/UCabcdefghijklmnopqrstuv".into());
    second.provenance = CreatorSaveProvenance::DirectSelection { source_ref: "original-youtube".into(), source_url: None, description: None, operator_account_ref_id: None };
    second.contact_evidence.clear();
    let inputs = vec![first, second];
    std::fs::write(directory.join("inputs.json"), serde_json::to_vec(&inputs).unwrap()).unwrap();
    let prepared = prepare_entries(&repository, inputs, CreatorSaveIntent::SaveOrUpdate).unwrap();
    std::fs::write(directory.join("prepared.json"), prepared.encoded_payload().unwrap()).unwrap();
    let selections = prepared.rows().iter().map(|row| CreatorSaveRouteSelection { row_id: row.row_id.clone(), route_ids: row.route_options.iter().map(|route| route.route_id.clone()).collect() }).collect::<Vec<_>>();
    std::fs::write(directory.join("selections.json"), serde_json::to_vec(&selections).unwrap()).unwrap();
    let selected = prepared.select_routes(selections).unwrap();
    std::fs::write(directory.join("selected.json"), serde_json::to_vec(&selected).unwrap()).unwrap();
    let (result, changed) = execute_prepared(&repository, selected, "creator-candidate-rust-oracle".into()).unwrap();
    assert!(changed);
    assert!(result.outcomes.iter().all(|outcome| matches!(outcome, CreatorSaveRowOutcome::Saved { .. })));
    std::fs::write(directory.join("result.json"), serde_json::to_vec(&result).unwrap()).unwrap();
}
