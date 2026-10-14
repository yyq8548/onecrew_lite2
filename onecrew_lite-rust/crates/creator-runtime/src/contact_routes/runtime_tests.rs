use rusqlite::Connection;

use crate::test_support::x_save_entry;
use crate::{
    AccountRefId, Confidence, ConfirmCreatorContactRouteDeleteInput,
    ConfirmCreatorContactRouteDeleteResult, ContactChannel, CreatorChangeScope,
    CreatorContactRouteAction, CreatorGateways, CreatorOrigin, CreatorRuntime,
    CreatorSaveRouteSelection, CreatorSaveRowOutcome, LocatorKind,
    ManualCreatorContactRouteAddInput, ManualCreatorContactRouteAddStatus,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateStatus,
    ManualCreatorCreateInput, Platform, PrepareCreatorContactRouteDeleteInput,
    PrepareCreatorContactRouteDeleteResult, SourceKind,
};

#[test]
fn manual_creator_and_contact_route_are_atomic_canonical_and_publish_after_commit() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let mut changes = runtime.subscribe_changes();

    let created = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "  Manual Creator  ".into(),
            notes: Some("  met at conference  ".into()),
            contact_routes: Vec::new(),
        })
        .expect("manual Creator");
    assert_eq!(created.display_name, "Manual Creator");
    let creation_change = changes.try_recv().expect("Creator creation change");
    assert_eq!(creation_change.scopes, vec![CreatorChangeScope::Creators]);
    let creator = runtime
        .get_creator(&created.creator_id)
        .expect("committed Creator visible when event is read");
    assert!(creator.platform_accounts.is_empty());
    assert_eq!(creator.relationship.stage, crate::CreatorStage::Prospect);
    assert_eq!(creator.creator.notes.as_deref(), Some("met at conference"));
    assert!(matches!(
        creator.origins.as_slice(),
        [CreatorOrigin::ManualEntry { creator_id, .. }] if creator_id == &created.creator_id
    ));
    assert_eq!(creator.observation_sources.len(), 1);
    assert_eq!(creator.observation_sources[0].kind, SourceKind::ManualEntry);

    let added = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: created.creator_id.clone(),
            channel: ContactChannel::Email,
            value: "  PERSON@Example.COM  ".into(),
            source_url: Some("  https://example.com/about  ".into()),
        })
        .expect("manual route");
    assert_eq!(added.status, ManualCreatorContactRouteAddStatus::Added);
    assert_eq!(added.locator_kind, LocatorKind::EmailAddress);
    assert_eq!(added.normalized_value, "person@example.com");
    assert_eq!(added.confidence, Confidence::Asserted);
    let route_change = changes.try_recv().expect("route change");
    assert_eq!(route_change.scopes, vec![CreatorChangeScope::Creators]);
    let with_route = runtime
        .get_creator(&created.creator_id)
        .expect("committed route visible when event is read");
    assert_eq!(with_route.contact_routes.len(), 1);
    assert_eq!(
        with_route.contact_routes[0].observations[0].source.kind,
        SourceKind::ManualEntry
    );
    assert_eq!(
        with_route.contact_routes[0].observations[0]
            .source
            .source_url
            .as_deref(),
        Some("https://example.com/about")
    );
    assert_eq!(
        with_route.contact_routes[0].allowed_actions,
        vec![
            CreatorContactRouteAction::Update,
            CreatorContactRouteAction::Delete,
        ]
    );

    let connection = Connection::open(directory.path().join("crm.db")).expect("open Creator db");
    let sources_before_duplicate = connection
        .query_row("SELECT COUNT(*) FROM observation_sources", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    drop(connection);
    let duplicate = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: created.creator_id.clone(),
            channel: ContactChannel::Email,
            value: "person@example.com".into(),
            source_url: None,
        })
        .expect("same owner duplicate");
    assert_eq!(
        duplicate.status,
        ManualCreatorContactRouteAddStatus::AlreadyExists
    );
    assert!(changes.try_recv().is_err(), "duplicate must not publish");
    let connection = Connection::open(directory.path().join("crm.db")).expect("open Creator db");
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM observation_sources", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        sources_before_duplicate
    );
    drop(connection);

    let updated = runtime
        .update_manual_creator_contact_route(ManualCreatorContactRouteUpdateInput {
            creator_id: created.creator_id.clone(),
            contact_route_id: added.route_id.clone(),
            channel: ContactChannel::Email,
            value: "PERSON@example.com".into(),
            source_url: Some("https://example.com/contact".into()),
        })
        .expect("same canonical route update");
    assert_eq!(
        updated.status,
        ManualCreatorContactRouteUpdateStatus::Updated
    );
    assert_eq!(updated.route_id, added.route_id);
    let mut invalid_update_result = serde_json::to_value(&updated).unwrap();
    invalid_update_result
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), serde_json::json!(true));
    assert!(
        serde_json::from_value::<crate::ManualCreatorContactRouteUpdateResult>(
            invalid_update_result
        )
        .is_err()
    );
    assert_eq!(
        changes.try_recv().expect("route update change").scopes,
        vec![CreatorChangeScope::Creators]
    );
    let unchanged = runtime
        .update_manual_creator_contact_route(ManualCreatorContactRouteUpdateInput {
            creator_id: created.creator_id.clone(),
            contact_route_id: updated.route_id.clone(),
            channel: ContactChannel::Email,
            value: "PERSON@example.com".into(),
            source_url: Some("https://example.com/contact".into()),
        })
        .expect("exact route replay");
    assert_eq!(
        unchanged.status,
        ManualCreatorContactRouteUpdateStatus::Unchanged
    );
    assert!(
        changes.try_recv().is_err(),
        "unchanged update must not publish"
    );

    let replacement = runtime
        .update_manual_creator_contact_route(ManualCreatorContactRouteUpdateInput {
            creator_id: created.creator_id.clone(),
            contact_route_id: updated.route_id.clone(),
            channel: ContactChannel::Email,
            value: "new@example.com".into(),
            source_url: None,
        })
        .expect("canonical replacement");
    assert_eq!(
        replacement.status,
        ManualCreatorContactRouteUpdateStatus::ReplacementRequired
    );
    assert_eq!(replacement.route_id, updated.route_id);
    assert!(
        changes.try_recv().is_err(),
        "replacement-required update must not publish"
    );
    let after_replacement = runtime.get_creator(&created.creator_id).unwrap();
    assert_eq!(after_replacement.contact_routes.len(), 1);

    let prepared = runtime
        .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
            creator_id: created.creator_id.clone(),
            contact_route_id: updated.route_id.clone(),
        })
        .expect("route deletion preparation");
    let operation_id = match prepared {
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
            operation_id,
            remote_not_deleted,
            ..
        } => {
            assert!(remote_not_deleted);
            operation_id
        }
        other => panic!("unexpected delete preparation: {other:?}"),
    };
    assert!(changes.try_recv().is_err(), "prepare must not publish");
    let deleted = runtime
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: operation_id.clone(),
        })
        .expect("route deletion confirmation");
    assert!(matches!(
        deleted,
        ConfirmCreatorContactRouteDeleteResult::Deleted { .. }
    ));
    let mut invalid_delete_result = serde_json::to_value(&deleted).unwrap();
    invalid_delete_result
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), serde_json::json!(true));
    assert!(
        serde_json::from_value::<ConfirmCreatorContactRouteDeleteResult>(invalid_delete_result)
            .is_err()
    );
    assert_eq!(
        changes.try_recv().expect("route deletion change").scopes,
        vec![CreatorChangeScope::Creators]
    );
    let replay = runtime
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id,
        })
        .expect("route deletion replay");
    assert_eq!(replay, deleted);
    assert!(
        changes.try_recv().is_err(),
        "delete replay must not publish"
    );

    runtime
        .update_creator_stage(crate::UpdateCreatorStageInput {
            creator_id: created.creator_id.clone(),
            stage: crate::CreatorStage::Engaged,
        })
        .expect("stage update");
    assert_eq!(
        changes.try_recv().expect("stage change").scopes,
        vec![CreatorChangeScope::Creators]
    );
    runtime
        .update_creator_stage(crate::UpdateCreatorStageInput {
            creator_id: created.creator_id.clone(),
            stage: crate::CreatorStage::Engaged,
        })
        .expect("stage no-op");
    assert!(changes.try_recv().is_err(), "same stage is a no-op");

    let other = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Other Creator".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .expect("other Creator");
    changes.try_recv().expect("other creation change");
    runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: created.creator_id.clone(),
            channel: ContactChannel::Email,
            value: "owned@example.com".into(),
            source_url: None,
        })
        .expect("owned collision route");
    changes.try_recv().expect("owned route change");
    let connection = Connection::open(directory.path().join("crm.db")).expect("open Creator db");
    let facts_before_collision = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM observation_sources),
               (SELECT COUNT(*) FROM locator_observations)",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .unwrap();
    drop(connection);
    let error = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: other.creator_id.clone(),
            channel: ContactChannel::Email,
            value: "OWNED@example.com".into(),
            source_url: None,
        })
        .expect_err("cross-Creator sharing must fail");
    assert_eq!(error, "contact route already belongs to another Creator");
    assert!(
        changes.try_recv().is_err(),
        "failed transaction must not publish"
    );
    let connection = Connection::open(directory.path().join("crm.db")).expect("open Creator db");
    assert_eq!(
        connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM observation_sources),
                   (SELECT COUNT(*) FROM locator_observations)",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .unwrap(),
        facts_before_collision,
        "cross-owner rejection must roll back every provenance write"
    );
    drop(connection);

    let missing_error = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: crate::CreatorId::new("missing-creator"),
            channel: ContactChannel::Website,
            value: "https://example.com".into(),
            source_url: None,
        })
        .expect_err("missing Creator must reject routes");
    assert_eq!(missing_error, "creator not found");
    assert!(changes.try_recv().is_err());

    runtime.archive_creator(&other.creator_id).expect("archive");
    changes.try_recv().expect("archive change");
    runtime
        .archive_creator(&other.creator_id)
        .expect("archive no-op");
    assert!(changes.try_recv().is_err(), "archive replay is a no-op");
    let error = runtime
        .add_manual_creator_contact_route(ManualCreatorContactRouteAddInput {
            creator_id: other.creator_id,
            channel: ContactChannel::X,
            value: "archived_creator".into(),
            source_url: None,
        })
        .expect_err("archived Creator must reject routes");
    assert!(error.contains("archived Creator cannot be used for outreach"));
    assert!(changes.try_recv().is_err());
}

/// The confirmation-claim half of `contact_confirmations.rs` is gone; the
/// save path still writes every confirmed route, its provenance and the
/// link-hub dependency edge through the retained persistence half, row for
/// row, and mints no confirmation claim of its own.
#[tokio::test]
async fn canonical_save_persists_confirmed_routes_and_the_link_hub_dependency_rows() {
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
    let candidate = x_save_entry(account_ref.id.clone(), true);

    let prepared = runtime
        .prepare_creator_save_batch(
            vec![candidate.clone()],
            crate::CreatorSaveIntent::SaveOrUpdate,
        )
        .expect("prepare save");
    let rows = prepared.rows();
    let route_options = &rows[0].route_options;
    let route_id = |channel: ContactChannel| {
        route_options
            .iter()
            .find(|route| route.channel == channel)
            .map(|route| route.route_id.clone())
            .unwrap_or_else(|| panic!("{channel:?} route option"))
    };
    let selected = prepared
        .select_routes(vec![CreatorSaveRouteSelection {
            row_id: rows[0].row_id.clone(),
            route_ids: vec![
                route_id(ContactChannel::X),
                route_id(ContactChannel::LinkHub),
                route_id(ContactChannel::Telegram),
            ],
        }])
        .expect("select reviewed routes");
    let CreatorSaveRowOutcome::Saved { creator, .. } = runtime
        .execute_creator_save_batch(selected, "link-hub-routes".into())
        .expect("confirm save")
        .outcomes
        .remove(0)
    else {
        panic!("candidate must save")
    };
    let creator_id = creator.creator.id.clone();

    let normalized = |value: &str| {
        crate::contact_routes::normalize_url(value, false)
            .expect("fixture URL normalizes")
            .normalized_value
    };
    let connection = Connection::open(directory.path().join("crm.db")).expect("open Creator db");
    let routes = connection
        .prepare(
            "SELECT locator.channel, locator.kind, locator.normalized_value, route.confidence
             FROM creator_contact_routes route
             JOIN contact_locators locator ON locator.id = route.locator_id
             WHERE route.creator_id = ?1
             ORDER BY locator.channel",
        )
        .unwrap()
        .query_map([creator_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        routes,
        vec![
            (
                "link_hub".to_owned(),
                "url".to_owned(),
                normalized("https://linktr.ee/creator_42"),
                "asserted".to_owned(),
            ),
            (
                "telegram".to_owned(),
                "handle".to_owned(),
                "kriptomessitg".to_owned(),
                "asserted".to_owned(),
            ),
            // The initial platform route is the account itself: exact.
            (
                "x".to_owned(),
                "handle".to_owned(),
                "creator_42".to_owned(),
                "exact".to_owned(),
            ),
        ]
    );

    let observations = connection
        .prepare(
            "SELECT locator.channel, source.kind, source.source_url, observation.observed_value
             FROM locator_observations observation
             JOIN contact_locators locator ON locator.id = observation.locator_id
             JOIN observation_sources source ON source.id = observation.source_id
             WHERE observation.creator_id = ?1
             ORDER BY locator.channel, source.kind",
        )
        .unwrap()
        .query_map([creator_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        observations,
        vec![
            (
                "link_hub".to_owned(),
                "profile_website".to_owned(),
                Some("https://x.com/creator_42".to_owned()),
                "https://linktr.ee/creator_42".to_owned(),
            ),
            (
                "telegram".to_owned(),
                "link_hub".to_owned(),
                Some("https://linktr.ee/creator_42".to_owned()),
                "https://t.me/kriptomessitg".to_owned(),
            ),
            (
                "x".to_owned(),
                "initial_platform_account".to_owned(),
                Some("https://x.com/creator_42".to_owned()),
                "creator_42".to_owned(),
            ),
        ]
    );

    // The Telegram route was found on the link hub: exactly one dependency
    // edge, from the confirmed hub route to it.
    let dependencies = connection
        .prepare(
            "SELECT parent.channel, child.channel, dependency.kind
             FROM creator_contact_route_dependencies dependency
             JOIN creator_contact_routes parent_route
               ON parent_route.id = dependency.parent_contact_route_id
             JOIN contact_locators parent ON parent.id = parent_route.locator_id
             JOIN creator_contact_routes child_route
               ON child_route.id = dependency.child_contact_route_id
             JOIN contact_locators child ON child.id = child_route.locator_id
             WHERE parent_route.creator_id = ?1",
        )
        .unwrap()
        .query_map([creator_id.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        dependencies,
        vec![(
            "link_hub".to_owned(),
            "telegram".to_owned(),
            "link_hub".to_owned()
        )]
    );

    let confirmation_claims = connection
        .query_row(
            "SELECT COUNT(*) FROM idempotency_records
             WHERE action = 'creator_contact_confirmation'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(
        confirmation_claims, 0,
        "a save writes routes directly; no contact confirmation claim exists any more"
    );
}

#[test]
fn legacy_shared_locator_confirmation_field_is_rejected() {
    use crate::{
        ContactLocatorInput, ContactRouteCollision, ContactRouteConfirmation, ContactRoutePreview,
    };
    let confirmation = ContactRouteConfirmation {
        preview: ContactRoutePreview {
            locator: ContactLocatorInput {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                normalized_value: "person@example.com".into(),
                display_value: "person@example.com".into(),
            },
            observations: Vec::new(),
            collision: ContactRouteCollision::None,
        },
        confidence: Confidence::Asserted,
    };
    let mut json = serde_json::to_value(confirmation).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("allow_shared_locator".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<ContactRouteConfirmation>(json).is_err());

    let mut state_json = serde_json::to_value(ContactRouteConfirmation {
        preview: ContactRoutePreview {
            locator: ContactLocatorInput {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                normalized_value: "person@example.com".into(),
                display_value: "person@example.com".into(),
            },
            observations: Vec::new(),
            collision: ContactRouteCollision::None,
        },
        confidence: Confidence::Asserted,
    })
    .unwrap();
    state_json
        .as_object_mut()
        .unwrap()
        .insert("state".into(), serde_json::json!("confirmed"));
    assert!(serde_json::from_value::<ContactRouteConfirmation>(state_json).is_err());
}
