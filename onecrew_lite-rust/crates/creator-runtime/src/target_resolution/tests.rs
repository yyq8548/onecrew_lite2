use crate::test_support::assert_runtime_health;
use crate::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn target_resolution_and_creator_update_bound_preflight_and_commit_segments() {
    use tokio::sync::broadcast::error::TryRecvError;

    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let creator = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator 42".into(),
            notes: None,
        })
        .unwrap();
    let operator = runtime
        .sync_account_ref(
            AccountRefId::new("phase-2-operator"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let account = runtime
        .repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-creator-42".into(),
        })
        .unwrap();
    runtime
        .repository
        .link_creator_account(
            &creator.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let route = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: creator.creator.id.clone(),
            channel: ContactChannel::X,
            value: "creator_42".into(),
            source_url: None,
        })
        .unwrap();

    let (target_read_started, release_target_read) = runtime.install_next_repository_read_barrier();
    let (target_write_started, release_target_write) =
        runtime.install_next_repository_write_barrier();
    let target_runtime = runtime.clone();
    let creator_id = creator.creator.id.clone();
    let operator_id = operator.id.clone();
    let resolve = tokio::spawn(async move {
        target_runtime
            .prepare_contact_target_resolution(ResolveContactTargetInput {
                creator_id,
                contact_route_id: route.route_id,
                operator_account_ref_id: operator_id,
                refresh: true,
            })
            .await
    });
    target_read_started.await.unwrap();
    assert_runtime_health().await;
    assert_eq!(count(&directory, "resolved_targets"), 0);
    release_target_read.send(()).unwrap();
    let CreatorContactTargetPreparation::Read(context) = resolve.await.unwrap().unwrap() else {
        panic!("provider read required")
    };
    assert_eq!(count(&directory, "resolved_targets"), 0);
    let finish_runtime = runtime.clone();
    let finish = tokio::spawn(async move {
        finish_runtime
            .finish_contact_target_resolution(context, evidence("x-creator-42"))
            .await
    });
    target_write_started.await.unwrap();
    assert_runtime_health().await;
    assert_eq!(count(&directory, "resolved_targets"), 0);
    release_target_write.send(()).unwrap();
    assert_eq!(
        finish.await.unwrap().unwrap().status,
        TargetResolutionStatus::Resolved
    );

    // The explicit-field update path: a bounded Creator read, then a bounded
    // Creator update of the chosen follower count.
    let mut changes = runtime.subscribe_changes();
    let (update_read_started, release_update_read) = runtime.install_next_repository_read_barrier();
    let (update_write_started, release_update_write) =
        runtime.install_next_repository_write_barrier();
    let update_runtime = runtime.clone();
    let update_creator_id = creator.creator.id;
    let update = tokio::spawn(async move {
        update_runtime
            .get_creator_async(update_creator_id.clone())
            .await
            .expect("bounded Creator read");
        update_runtime
            .update_creator_async(UpdateCreatorInput {
                tags: None,
                creator_id: update_creator_id,
                display_name: None,
                notes: None,
                score: None,
                assessment: None,
                follower_counts: Some([(Platform::X, 61_000)].into_iter().collect()),
            })
            .await
    });
    update_read_started.await.unwrap();
    assert_runtime_health().await;
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));
    release_update_read.send(()).unwrap();
    update_write_started.await.unwrap();
    assert_runtime_health().await;
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));
    release_update_write.send(()).unwrap();
    let updated = update.await.unwrap().expect("bounded Creator update");
    assert_eq!(
        updated.creator.follower_counts.get(&Platform::X),
        Some(&61_000)
    );
    assert_eq!(
        changes.recv().await.unwrap().scopes,
        vec![CreatorChangeScope::Creators]
    );
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));
}

fn count(directory: &tempfile::TempDir, table: &str) -> u64 {
    rusqlite::Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn evidence(external_id: &str) -> PlatformTargetResolution {
    PlatformTargetResolution::Resolved {
        target: PlatformResolvedTarget {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_target_id: external_id.into(),
            handle: Some("creator_42".into()),
            display_name: Some("Identity name".into()),
            canonical_url: Some("https://x.com/creator_42".into()),
        },
    }
}
fn fixture() -> (tempfile::TempDir, CreatorRuntime, ResolveContactTargetInput) {
    let directory = tempfile::tempdir().unwrap();
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let creator = runtime
        .repository
        .create_creator(CreateCreatorInput {
            display_name: "Creator".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    let operator = runtime
        .sync_account_ref(
            AccountRefId::new("operator"),
            Platform::X,
            "profile",
            "operator",
        )
        .unwrap();
    let route = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: creator.creator.id.clone(),
            channel: ContactChannel::X,
            value: "creator_42".into(),
            source_url: None,
        })
        .unwrap();
    let input = ResolveContactTargetInput {
        creator_id: creator.creator.id,
        contact_route_id: route.route_id,
        operator_account_ref_id: operator.id,
        refresh: false,
    };
    (directory, runtime, input)
}

#[tokio::test]
async fn cached_owned_resolution_is_read_only_and_keeps_profile_observations_untouched() {
    let (directory, runtime, input) = fixture();
    let CreatorContactTargetPreparation::Read(context) = runtime
        .prepare_contact_target_resolution(input.clone())
        .await
        .unwrap()
    else {
        panic!("read")
    };
    assert_eq!(context.route().route.id, input.contact_route_id);
    assert_eq!(context.operator_account().id, input.operator_account_ref_id);
    let result = runtime
        .finish_contact_target_resolution(context, evidence("stable-1"))
        .await
        .unwrap();
    let first = result.resolved.unwrap();
    assert_eq!(count(&directory, "platform_account_observations"), 0);
    let before = rusqlite::Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT metadata_json || ':' || updated_at FROM resolved_targets",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    let CreatorContactTargetPreparation::Ready(result) = runtime
        .prepare_contact_target_resolution(input)
        .await
        .unwrap()
    else {
        panic!("cached owned ready")
    };
    let result = *result;
    assert_eq!(result.resolved.unwrap().resolution.id, first.resolution.id);
    let after = rusqlite::Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT metadata_json || ':' || updated_at FROM resolved_targets",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(count(&directory, "platform_account_observations"), 0);
    assert_eq!(count(&directory, "identity_conflicts"), 0);
}

#[tokio::test]
async fn cached_unattached_identity_requires_explicit_commit_without_a_provider_read() {
    let (directory, runtime, input) = fixture();
    let route = runtime
        .repository
        .creator_contact_routes(&input.creator_id)
        .unwrap()
        .remove(0);
    let target = runtime
        .repository
        .upsert_resolved_target(
            &input.operator_account_ref_id,
            Platform::X,
            TargetKind::User,
            "stable-1",
            None,
            serde_json::json!({"handle":"creator_42","display_name":"Identity name"}),
        )
        .unwrap();
    runtime
        .repository
        .record_locator_resolution(
            &route.locator.id,
            &input.operator_account_ref_id,
            &target.id,
        )
        .unwrap();
    let CreatorContactTargetPreparation::Commit { context, evidence } = runtime
        .prepare_contact_target_resolution(input.clone())
        .await
        .unwrap()
    else {
        panic!("cached identity needs attachment")
    };
    assert_eq!(count(&directory, "creator_account_links"), 0);
    assert_eq!(count(&directory, "platform_accounts"), 0);
    let result = runtime
        .finish_contact_target_resolution(context, evidence)
        .await
        .unwrap();
    assert_eq!(result.status, TargetResolutionStatus::Resolved);
    assert_eq!(count(&directory, "creator_account_links"), 1);
    assert!(matches!(
        runtime
            .prepare_contact_target_resolution(input)
            .await
            .unwrap(),
        CreatorContactTargetPreparation::Ready(_)
    ));
}

#[tokio::test]
async fn cached_identity_owned_by_another_creator_needs_a_conflict_commit() {
    let (directory, runtime, first_input) = fixture();
    let CreatorContactTargetPreparation::Read(context) = runtime
        .prepare_contact_target_resolution(first_input.clone())
        .await
        .unwrap()
    else {
        panic!("read")
    };
    let target = runtime
        .finish_contact_target_resolution(context, evidence("stable-1"))
        .await
        .unwrap()
        .resolved
        .unwrap()
        .target;
    let second = runtime
        .repository
        .create_creator(CreateCreatorInput {
            display_name: "Second".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    let route = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: second.creator.id.clone(),
            channel: ContactChannel::X,
            value: "different_alias".into(),
            source_url: None,
        })
        .unwrap();
    let locator = runtime
        .repository
        .creator_contact_routes(&second.creator.id)
        .unwrap()
        .remove(0)
        .locator;
    runtime
        .repository
        .record_locator_resolution(
            &locator.id,
            &first_input.operator_account_ref_id,
            &target.id,
        )
        .unwrap();
    let input = ResolveContactTargetInput {
        creator_id: second.creator.id.clone(),
        contact_route_id: route.route_id,
        operator_account_ref_id: first_input.operator_account_ref_id,
        refresh: false,
    };
    let CreatorContactTargetPreparation::Commit { context, evidence } = runtime
        .prepare_contact_target_resolution(input)
        .await
        .unwrap()
    else {
        panic!("owner conflict needs durable commit")
    };
    assert_eq!(count(&directory, "identity_conflicts"), 0);
    let result = runtime
        .finish_contact_target_resolution(context, evidence)
        .await
        .unwrap();
    assert_eq!(result.status, TargetResolutionStatus::Conflict);
    assert_eq!(
        result.conflict.unwrap().kind,
        "platform_account_already_confirmed"
    );
    assert!(
        runtime
            .get_creator(&second.creator.id)
            .unwrap()
            .account_links
            .is_empty()
    );
    assert_eq!(count(&directory, "identity_conflicts"), 1);
}

#[tokio::test]
async fn finish_rechecks_route_locator_creator_and_operator_before_any_identity_write() {
    for mutation in [
        "delete_route",
        "UPDATE creators SET archived_at=123",
        "UPDATE creator_relationships SET stage='archived'",
        "UPDATE account_refs SET active=0",
    ] {
        let (directory, runtime, input) = fixture();
        let CreatorContactTargetPreparation::Read(context) = runtime
            .prepare_contact_target_resolution(input.clone())
            .await
            .unwrap()
        else {
            panic!("read")
        };
        if mutation == "delete_route" {
            let PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
                operation_id, ..
            } = runtime
                .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
                    creator_id: input.creator_id,
                    contact_route_id: input.contact_route_id,
                })
                .expect("prepare real route deletion")
            else {
                panic!("route deletion must be available before target commit")
            };
            let deleted = runtime
                .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
                    operation_id,
                })
                .expect("delete route and its dependent local facts");
            assert!(matches!(
                deleted,
                ConfirmCreatorContactRouteDeleteResult::Deleted { .. }
            ));
        } else {
            rusqlite::Connection::open(directory.path().join("crm.db"))
                .unwrap()
                .execute(mutation, [])
                .unwrap();
        }
        assert_eq!(
            runtime
                .finish_contact_target_resolution(context, evidence("stable-1"))
                .await
                .unwrap_err(),
            CreatorTargetResolutionError::ContextUnavailable
        );
        for table in [
            "resolved_targets",
            "platform_accounts",
            "creator_account_links",
            "locator_resolutions",
        ] {
            assert_eq!(count(&directory, table), 0, "{mutation} wrote {table}");
        }
    }
    let (directory, runtime, input) = fixture();
    let CreatorContactTargetPreparation::Read(context) = runtime
        .prepare_contact_target_resolution(input.clone())
        .await
        .unwrap()
    else {
        panic!("read")
    };
    let new_locator = runtime
        .repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "changed_alias".into(),
            display_value: "changed_alias".into(),
        })
        .unwrap();
    rusqlite::Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE creator_contact_routes SET locator_id=?1 WHERE id=?2",
            rusqlite::params![new_locator.id.as_str(), input.contact_route_id.as_str()],
        )
        .unwrap();
    assert_eq!(
        runtime
            .finish_contact_target_resolution(context, evidence("stable-1"))
            .await
            .unwrap_err(),
        CreatorTargetResolutionError::ContextUnavailable
    );
    assert_eq!(count(&directory, "resolved_targets"), 0);
}

#[tokio::test]
async fn finish_refuses_a_route_transferred_to_another_creator_after_prepare() {
    let (directory, runtime, input) = fixture();
    let CreatorContactTargetPreparation::Read(context) = runtime
        .prepare_contact_target_resolution(input.clone())
        .await
        .unwrap()
    else {
        panic!("read")
    };
    let other = runtime
        .repository
        .create_creator(CreateCreatorInput {
            display_name: "Other owner".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    rusqlite::Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE creator_contact_routes SET creator_id=?1 WHERE id=?2",
            rusqlite::params![other.creator.id.as_str(), input.contact_route_id.as_str()],
        )
        .unwrap();
    assert_eq!(
        runtime
            .finish_contact_target_resolution(context, evidence("stable-1"))
            .await
            .unwrap_err(),
        CreatorTargetResolutionError::ContextUnavailable
    );
    assert_eq!(count(&directory, "resolved_targets"), 0);
    assert_eq!(count(&directory, "creator_account_links"), 0);
}

#[tokio::test]
async fn invalid_provider_identity_and_expected_missing_results_create_no_facts() {
    let (directory, runtime, input) = fixture();
    for invalid in ["", " stable-1 "] {
        let CreatorContactTargetPreparation::Read(context) = runtime
            .prepare_contact_target_resolution(input.clone())
            .await
            .unwrap()
        else {
            panic!("read")
        };
        assert_eq!(
            runtime
                .finish_contact_target_resolution(context, evidence(invalid))
                .await
                .unwrap_err(),
            CreatorTargetResolutionError::InvalidEvidence
        );
    }
    let CreatorContactTargetPreparation::Read(context) = runtime
        .prepare_contact_target_resolution(input)
        .await
        .unwrap()
    else {
        panic!("read")
    };
    let result = runtime
        .finish_contact_target_resolution(
            context,
            PlatformTargetResolution::NotFound {
                reason: "not found".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(result.status, TargetResolutionStatus::NotFound);
    assert_eq!(count(&directory, "resolved_targets"), 0);
}
