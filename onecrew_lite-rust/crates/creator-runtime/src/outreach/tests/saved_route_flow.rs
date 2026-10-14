use crate::test_support::x_save_entry;
use crate::*;
use rusqlite::Connection;

#[tokio::test]
async fn saved_route_preserves_identity_and_supplies_the_new_durable_outreach_context() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let account_ref = runtime
        .sync_account_ref(
            AccountRefId::new("phase-2-operator"),
            Platform::X,
            "profile",
            "@operator",
        )
        .expect("account reference");
    let candidate = x_save_entry(account_ref.id.clone(), false);
    let mut save_input = candidate.clone();
    save_input.notes = Some("search candidate".into());
    let mut creator_changes = runtime.subscribe_changes();
    let prepared = runtime
        .prepare_creator_save_batch(
            vec![save_input.clone()],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("prepare Creator save");
    let prepared_rows = prepared.rows();
    let prepared_row = &prepared_rows[0];
    let route_options = &prepared_row.route_options;
    assert!(
        creator_changes.try_recv().is_err(),
        "confirmation_required must not publish"
    );
    assert_eq!(prepared_row.row_id, candidate.row_id);
    let required_route_id = route_options
        .iter()
        .find(|route| route.required && route.channel == ContactChannel::X)
        .map(|route| route.route_id.clone())
        .expect("required initial X route");
    let optional_route_id = route_options
        .iter()
        .find(|route| !route.required)
        .map(|route| route.route_id.clone())
        .expect("optional discovered route");

    let invalid_route_error = prepared
        .clone()
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: prepared_row.row_id.clone(),
            route_ids: vec![
                required_route_id.clone(),
                "route-from-another-operation".into(),
            ],
        }])
        .expect_err("foreign route ID must be rejected");
    assert_eq!(
        invalid_route_error,
        crate::CreatorSaveSelectionError::UnknownRoute
    );
    assert!(creator_changes.try_recv().is_err());
    assert!(
        runtime
            .list_creators(Default::default())
            .expect("creators")
            .items
            .is_empty()
    );

    let missing_required_error = prepared
        .clone()
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: prepared_row.row_id.clone(),
            route_ids: vec![optional_route_id],
        }])
        .expect_err("required initial route must be selected");
    assert_eq!(
        missing_required_error,
        crate::CreatorSaveSelectionError::MissingRequiredRoute
    );
    assert!(creator_changes.try_recv().is_err());
    assert!(
        runtime
            .list_creators(Default::default())
            .expect("creators")
            .items
            .is_empty()
    );

    let confirmation = prepared
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: prepared_row.row_id.clone(),
            route_ids: vec![required_route_id],
        }])
        .expect("select initial route");
    let saved = runtime
        .execute_creator_save_batch(confirmation.clone(), "outreach-creator-save".into())
        .expect("confirm Creator save")
        .outcomes
        .remove(0);
    let saved_change = creator_changes.try_recv().expect("search save change");
    assert_eq!(saved_change.scopes, vec![CreatorChangeScope::Creators]);
    let committed_creator_id = match &saved {
        CreatorSaveRowOutcome::Saved { creator, .. } => creator.creator.id.clone(),
        CreatorSaveRowOutcome::Failed { .. } => {
            panic!("confirmed search candidate must be saved")
        }
        CreatorSaveRowOutcome::Skipped { .. } => {
            panic!("confirmed search candidate was unexpectedly skipped")
        }
    };
    runtime
        .get_creator(&committed_creator_id)
        .expect("committed Creator is readable when change is received");
    let replay = runtime
        .execute_creator_save_batch(confirmation, "outreach-creator-save".into())
        .expect("replay Creator save confirmation")
        .outcomes
        .remove(0);
    assert!(
        creator_changes.try_recv().is_err(),
        "confirmation replay is a no-op"
    );
    let CreatorSaveRowOutcome::Saved {
        creator: saved_creator,
        created,
        outreach_context,
        ..
    } = saved
    else {
        panic!("confirmed search candidate must be saved")
    };
    let CreatorSaveRowOutcome::Saved {
        creator: replay_creator,
        created: replay_created,
        outreach_context: replay_outreach_context,
        ..
    } = replay
    else {
        panic!("confirmation replay must return the saved result")
    };
    assert!(created);
    assert!(replay_created);
    assert!(outreach_context.is_none());
    assert!(replay_outreach_context.is_none());
    assert_eq!(replay_creator.creator.id, saved_creator.creator.id);
    assert_eq!(
        saved_creator.creator.notes.as_deref(),
        Some("search candidate")
    );
    assert_eq!(
        runtime
            .prepare_creator_save_batch(vec![save_input], crate::CreatorSaveIntent::SaveOrUpdate)
            .unwrap()
            .rows()[0]
            .identity_resolution,
        IdentityResolution::ExactExistingCreator {
            creator_id: saved_creator.creator.id.clone(),
        }
    );
    assert_eq!(
        runtime
            .get_creator(&saved_creator.creator.id)
            .unwrap()
            .platform_accounts
            .len(),
        1
    );
    let route = saved_creator
        .contact_routes
        .iter()
        .find(|record| record.locator.channel == ContactChannel::X)
        .expect("persisted X route");
    let target_platform_account = saved_creator
        .platform_accounts
        .iter()
        .find(|account| account.platform == Platform::X)
        .expect("saved X platform account");
    let target = runtime
        .repository
        .upsert_resolved_target(
            &account_ref.id,
            Platform::X,
            TargetKind::User,
            "x-creator-42",
            Some(&target_platform_account.id),
            serde_json::json!({"handle":"creator_42"}),
        )
        .expect("provider verified stable target fact");
    let resolution = runtime
        .repository
        .record_locator_resolution(&route.locator.id, &account_ref.id, &target.id)
        .unwrap();
    let context = runtime
        .creator_relationship_context(PrepareRelationshipActionInput {
            creator_id: saved_creator.creator.id.clone(),
            contact_route_id: route.route.id.clone(),
            resolved_target_id: target.id.clone(),
            operator_account_ref_id: account_ref.id.clone(),
            action: RelationshipMutation::Follow,
        })
        .await
        .expect("saved route context");
    assert_eq!(context.target.external_target_id, "x-creator-42");
    assert_eq!(
        context.target.platform_account_id,
        target_platform_account.id
    );
    assert_eq!(context.contact_route_id, route.route.id);
    let LocatorResolutionWriteResult::Active { value: resolution } = resolution else {
        panic!("active resolution")
    };
    assert_eq!(context.resolved_target_id, resolution.target.id);
    let intent =
        PreparedCreatorOutreachIntent::relationship(context, RelationshipMutation::Follow).unwrap();
    assert!(
        runtime
            .creator_outreach_read("follow-saved", &intent)
            .await
            .unwrap()
            .is_none()
    );
    let lease = runtime.acquire_creator_outreach("follow-saved").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    runtime
        .creator_outreach_stage_verified(
            &lease,
            &intent,
            CreatorOutreachVerification::Relationship {
                following: RelationshipValue::Yes,
                can_message: RelationshipValue::Yes,
                checked_at: 123,
            },
        )
        .await
        .unwrap();
    let recorded = runtime
        .creator_outreach_record_relationship(&lease)
        .await
        .unwrap();
    assert_eq!(recorded.relationship.value, RelationshipValue::Yes);
    let CreatorOutreachRecord::Relationship(replay) =
        runtime.creator_outreach_recorded(&lease).await.unwrap()
    else {
        panic!("follow record")
    };
    assert_eq!(recorded.audit_event.id, replay.audit_event.id);
    assert_eq!(recorded.touchpoint.id, replay.touchpoint.id);
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    for (table, count) in [
        ("resolved_targets", 1),
        ("locator_resolutions", 1),
        ("touchpoints", 1),
    ] {
        let query = format!("SELECT COUNT(*) FROM {table}");
        assert_eq!(
            connection
                .query_row(&query, [], |row| row.get::<_, u64>(0))
                .unwrap(),
            count
        );
    }
}
