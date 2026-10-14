use rusqlite::Connection;

use crate::{
    ContactChannel, CreatorChangeScope, CreatorGateways, CreatorRuntime,
    MAX_MANUAL_CREATOR_BATCH_ITEMS, ManualCreatorBatchCreateInput, ManualCreatorBatchItemOutcome,
    ManualCreatorBatchValidation, ManualCreatorCreateInput, ManualCreatorInitialContactRouteInput,
};

#[test]
fn creator_tags_validate_normalized_boundaries_without_losing_content() {
    use crate::{CreatorTagsError as Error, validate_creator_tags};
    assert_eq!(
        validate_creator_tags(&["rust".into(), " rust ".into()]),
        Err(Error::Duplicate)
    );
    assert_eq!(
        validate_creator_tags(&["rust\n".into()]),
        Err(Error::ControlCharacters)
    );
    assert_eq!(
        validate_creator_tags(&["a".into(), "b".into(), "c".into(), "d".into()]),
        Err(Error::TooMany)
    );
    assert_eq!(
        validate_creator_tags(&["界".repeat(13)]),
        Err(Error::InvalidLength)
    );
    assert_eq!(
        validate_creator_tags(&["   ".into()]),
        Err(Error::InvalidLength)
    );
    let tags = vec![
        format!("  {}  ", "界".repeat(12)),
        "Rust".into(),
        "rust".into(),
    ];
    assert_eq!(validate_creator_tags(&tags), Ok(()));
    assert_eq!(
        super::normalize_creator_tags(tags).unwrap(),
        vec!["界".repeat(12), "Rust".into(), "rust".into()]
    );
}

#[test]
fn creator_tags_schema_preserves_nullable_optional_bounded_lists() {
    let schema =
        serde_json::to_value(schemars::schema_for!(crate::ManualCreatorCreateInput)).unwrap();
    let tags = &schema["properties"]["tags"];
    assert_eq!(tags["type"], serde_json::json!(["array", "null"]));
    assert_eq!(tags["maxItems"], 3);
    assert_eq!(tags["uniqueItems"], true);
    assert_eq!(tags["items"]["minLength"], 1);
    assert_eq!(tags["items"]["maxLength"], 12);
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "tags")
    );
    for value in [
        serde_json::json!({"display_name":"Creator"}),
        serde_json::json!({"display_name":"Creator", "tags":null}),
    ] {
        assert!(
            serde_json::from_value::<crate::ManualCreatorCreateInput>(value)
                .unwrap()
                .tags
                .is_none()
        );
    }
}

#[test]
fn manual_creator_create_includes_initial_routes_in_one_authoritative_readback() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let created = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "One-step Creator".into(),
            notes: None,
            contact_routes: vec![ManualCreatorInitialContactRouteInput {
                channel: ContactChannel::Email,
                value: "  PERSON@Example.COM  ".into(),
                source_url: Some(" https://example.com/about ".into()),
            }],
        })
        .expect("atomic Creator and route create");
    assert_eq!(created.stage, crate::CreatorStage::Prospect);
    assert_eq!(created.contact_routes.len(), 1);
    assert_eq!(
        created.contact_routes[0].locator.normalized_value,
        "person@example.com"
    );
    let record = runtime.get_creator(&created.creator_id).unwrap();
    assert_eq!(record.contact_routes.len(), 1);
    assert_eq!(record.observation_sources.len(), 2);
    assert_eq!(record.origins.len(), 1);
    let audit_route: String = Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT contact_route_id FROM audit_events
             WHERE kind = 'creator_contact_route_added' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_route, created.contact_routes[0].route.id.as_str());
}

#[test]
fn manual_creator_batch_is_ordered_idempotent_and_per_item() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let items = vec![
        ManualCreatorCreateInput {
            tags: None,
            display_name: "Batch one".into(),
            notes: None,
            contact_routes: Vec::new(),
        },
        ManualCreatorCreateInput {
            tags: None,
            display_name: "  ".into(),
            notes: None,
            contact_routes: Vec::new(),
        },
    ];
    let result = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-1".into(),
            validation: ManualCreatorBatchValidation::Permissive,
            items: items.clone(),
        })
        .unwrap();
    assert!(!result.replayed);
    assert!(matches!(
        result.outcomes[0],
        ManualCreatorBatchItemOutcome::Accepted { .. }
    ));
    assert!(matches!(
        result.outcomes[1],
        ManualCreatorBatchItemOutcome::Rejected { .. }
    ));
    let replay = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-1".into(),
            validation: ManualCreatorBatchValidation::Permissive,
            items,
        })
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.outcomes.len(), 2);
    let conflict = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-1".into(),
            validation: ManualCreatorBatchValidation::Permissive,
            items: vec![ManualCreatorCreateInput {
                tags: None,
                display_name: "Different request".into(),
                notes: None,
                contact_routes: Vec::new(),
            }],
        })
        .unwrap_err();
    assert!(conflict.contains("different request"), "{conflict}");
}

#[test]
fn manual_creator_batch_strict_error_identifies_the_input_item() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Existing strict conflict owner".into(),
            notes: None,
            contact_routes: vec![ManualCreatorInitialContactRouteInput {
                channel: ContactChannel::X,
                value: "strict_conflict_route".into(),
                source_url: None,
            }],
        })
        .expect("existing Creator");
    let error = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-strict-item-error".into(),
            validation: ManualCreatorBatchValidation::Strict,
            items: vec![
                ManualCreatorCreateInput {
                    tags: None,
                    display_name: "valid item".into(),
                    notes: None,
                    contact_routes: Vec::new(),
                },
                ManualCreatorCreateInput {
                    tags: None,
                    display_name: "strict conflict item".into(),
                    notes: None,
                    contact_routes: vec![ManualCreatorInitialContactRouteInput {
                        channel: ContactChannel::X,
                        value: "strict_conflict_route".into(),
                        source_url: None,
                    }],
                },
            ],
        })
        .expect_err("strict preflight must identify the invalid item");
    assert_eq!(
        error,
        "Creator batch item 1 rejected: contact route already belongs to another Creator"
    );

    let connection = Connection::open(directory.path().join("crm.db")).expect("database");
    let creators: u64 = connection
        .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
        .expect("creator count");
    let claims: u64 = connection
        .query_row("SELECT COUNT(*) FROM idempotency_records", [], |row| {
            row.get(0)
        })
        .expect("claim count");
    assert_eq!(creators, 1);
    assert_eq!(claims, 0);
}

#[test]
fn manual_creator_batch_accepts_every_unique_creator_and_route_at_the_business_limit() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let items = (0..MAX_MANUAL_CREATOR_BATCH_ITEMS)
        .map(|index| ManualCreatorCreateInput {
            tags: None,
            display_name: format!("Unique batch Creator {index}"),
            notes: None,
            contact_routes: vec![ManualCreatorInitialContactRouteInput {
                channel: ContactChannel::X,
                value: format!("unique_batch_route_{index}"),
                source_url: None,
            }],
        })
        .collect::<Vec<_>>();
    let result = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-limit-unique".into(),
            validation: ManualCreatorBatchValidation::Strict,
            items,
        })
        .expect("a full business batch of unique Creators should commit");
    assert_eq!(result.outcomes.len(), MAX_MANUAL_CREATOR_BATCH_ITEMS);
    assert!(result.outcomes.iter().enumerate().all(|(index, outcome)| {
        matches!(
            outcome,
            ManualCreatorBatchItemOutcome::Accepted { input_index, creator }
                if *input_index == index && creator.contact_routes.len() == 1
        )
    }));

    let connection = Connection::open(directory.path().join("crm.db")).expect("database");
    let creators: u64 = connection
        .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
        .expect("creator count");
    let routes: u64 = connection
        .query_row("SELECT COUNT(*) FROM creator_contact_routes", [], |row| {
            row.get(0)
        })
        .expect("route count");
    assert_eq!(creators, MAX_MANUAL_CREATOR_BATCH_ITEMS as u64);
    assert_eq!(routes, MAX_MANUAL_CREATOR_BATCH_ITEMS as u64);
}

#[test]
fn creator_candidate_batch_runtime_rejects_one_over_the_limit_before_identity_lookup() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let candidate = crate::CanonicalCreatorSaveInput {
        profile_observed_at: 1_700_000_000,
        row_id: "missing-candidate".into(),
        account: crate::PlatformAccountCandidate {
            platform: crate::Platform::X,
            external_id: Some("missing-identity".into()),
            target_kind: Some(crate::TargetKind::User),
            handle: Some("missing".into()),
            display_name: "Missing".into(),
            profile_url: None,
        },
        profile: Default::default(),
        provenance: crate::CreatorSaveProvenance::DirectSelection {
            source_ref: "missing-candidate".into(),
            source_url: None,
            description: Some("fixture".into()),
            operator_account_ref_id: None,
        },
        contact_evidence: Vec::new(),
        tags: None,
        notes: None,
        quality: Default::default(),
    };
    let error = runtime
        .prepare_creator_save_batch(
            vec![candidate; crate::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS + 1],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect_err("batches over the business limit must be rejected before identity lookup");
    assert_eq!(error.row_id, None);
    assert_eq!(
        error.kind,
        crate::CreatorSavePreparationErrorKind::InvalidFields
    );
}

#[test]
fn manual_creator_batch_permissive_reports_conflicting_item_and_commits_valid_items() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Existing owner".into(),
            notes: None,
            contact_routes: vec![ManualCreatorInitialContactRouteInput {
                channel: ContactChannel::X,
                value: "already_owned".into(),
                source_url: None,
            }],
        })
        .expect("existing Creator");
    let result = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-permissive-conflict".into(),
            validation: ManualCreatorBatchValidation::Permissive,
            items: vec![
                ManualCreatorCreateInput {
                    tags: None,
                    display_name: "Rejected conflict".into(),
                    notes: None,
                    contact_routes: vec![ManualCreatorInitialContactRouteInput {
                        channel: ContactChannel::X,
                        value: "already_owned".into(),
                        source_url: None,
                    }],
                },
                ManualCreatorCreateInput {
                    tags: None,
                    display_name: "Accepted after conflict".into(),
                    notes: None,
                    contact_routes: Vec::new(),
                },
            ],
        })
        .expect("permissive batch");
    assert!(matches!(
        &result.outcomes[0],
        ManualCreatorBatchItemOutcome::Rejected { input_index: 0, message }
            if message.contains("already belongs")
    ));
    assert!(matches!(
        &result.outcomes[1],
        ManualCreatorBatchItemOutcome::Accepted { input_index: 1, .. }
    ));

    let connection = Connection::open(directory.path().join("crm.db")).expect("database");
    let creators: u64 = connection
        .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
        .expect("creator count");
    assert_eq!(creators, 2);
}

#[test]
fn manual_creator_batch_rejects_over_capacity_before_claim() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let item = ManualCreatorCreateInput {
        tags: None,
        display_name: "Too many".into(),
        notes: None,
        contact_routes: Vec::new(),
    };
    let error = runtime
        .create_manual_creator_batch(ManualCreatorBatchCreateInput {
            idempotency_key: "creator-batch-over-capacity".into(),
            validation: ManualCreatorBatchValidation::Strict,
            items: vec![item; MAX_MANUAL_CREATOR_BATCH_ITEMS + 1],
        })
        .unwrap_err();
    assert!(error.contains("at most 50"), "{error}");
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let creators: u64 = connection
        .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
        .unwrap();
    let claims: u64 = connection
        .query_row("SELECT COUNT(*) FROM idempotency_records", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(creators, 0);
    assert_eq!(claims, 0);
}

#[test]
fn creator_update_patches_quality_facts_and_rejects_archived() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let mut changes = runtime.subscribe_changes();
    let created = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Manual Creator".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .expect("manual Creator");
    changes.try_recv().expect("creation change");

    let empty_update = |creator_id: crate::CreatorId| crate::UpdateCreatorInput {
        tags: None,
        creator_id,
        display_name: None,
        notes: None,
        score: None,
        assessment: None,
        follower_counts: None,
    };

    let record = runtime.get_creator(&created.creator_id).expect("creator");
    assert_eq!(record.creator.score, None, "manual creators carry no score");
    assert_eq!(record.creator.assessment, None);
    assert!(record.creator.follower_counts.is_empty());

    let error = runtime
        .update_creator(empty_update(created.creator_id.clone()))
        .expect_err("empty patch must fail");
    assert_eq!(
        error,
        "creator update requires at least one field to change"
    );
    let error = runtime
        .update_creator(crate::UpdateCreatorInput {
            score: Some(101),
            ..empty_update(created.creator_id.clone())
        })
        .expect_err("score above 100 must fail");
    assert_eq!(error, "creator score must be between 0 and 100");
    assert!(
        changes.try_recv().is_err(),
        "failed updates must not publish"
    );

    let updated = runtime
        .update_creator(crate::UpdateCreatorInput {
            score: Some(82),
            assessment: Some("  strong crypto reach  ".into()),
            follower_counts: Some([(crate::Platform::X, 12_000u64)].into_iter().collect()),
            ..empty_update(created.creator_id.clone())
        })
        .expect("quality patch");
    assert_eq!(updated.creator.score, Some(82));
    assert_eq!(
        updated.creator.assessment.as_deref(),
        Some("strong crypto reach")
    );
    assert_eq!(
        updated.creator.follower_counts.get(&crate::Platform::X),
        Some(&12_000)
    );
    assert_eq!(
        changes.try_recv().expect("update change").scopes,
        vec![CreatorChangeScope::Creators]
    );

    let merged = runtime
        .update_creator(crate::UpdateCreatorInput {
            display_name: Some("  Renamed Creator  ".into()),
            follower_counts: Some([(crate::Platform::YouTube, 500u64)].into_iter().collect()),
            ..empty_update(created.creator_id.clone())
        })
        .expect("merge patch");
    assert_eq!(merged.creator.display_name, "Renamed Creator");
    assert_eq!(
        merged.creator.follower_counts.get(&crate::Platform::X),
        Some(&12_000),
        "platforms absent from the patch keep their counts"
    );
    assert_eq!(
        merged
            .creator
            .follower_counts
            .get(&crate::Platform::YouTube),
        Some(&500)
    );
    assert_eq!(
        merged.creator.score,
        Some(82),
        "absent score stays unchanged"
    );
    changes.try_recv().expect("merge change");

    let summary_page = runtime
        .list_creators(crate::ListCreatorsInput::default())
        .expect("creator list");
    let summary = summary_page
        .items
        .iter()
        .find(|summary| summary.id == created.creator_id)
        .expect("summary row");
    assert_eq!(summary.score, Some(82));
    assert_eq!(summary.assessment.as_deref(), Some("strong crypto reach"));
    assert_eq!(
        summary.follower_counts.get(&crate::Platform::X),
        Some(&12_000)
    );

    runtime
        .archive_creator(&created.creator_id)
        .expect("archive");
    changes.try_recv().expect("archive change");
    let error = runtime
        .update_creator(crate::UpdateCreatorInput {
            score: Some(10),
            ..empty_update(created.creator_id.clone())
        })
        .expect_err("archived Creator must reject updates");
    assert!(error.contains("archived Creator"));
    assert!(changes.try_recv().is_err());
}
