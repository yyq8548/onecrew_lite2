use std::sync::{Arc, Barrier};

use super::*;
use crate::{
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    CreatorContactRouteDeleteBlockReason, CreatorContactRouteDeleteBlockingArtifactKind,
    CreatorContactRouteDeleteStaleReason, CreatorContactRouteDependencyKind, DeleteCreatorInput,
    ManualCreatorContactRouteAddStatus, PrepareCreatorContactRouteDeleteInput,
    PrepareCreatorContactRouteDeleteResult,
};

fn create_creator(repository: &SqliteRepository, name: &str) -> CreatorId {
    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: name.into(),
            notes: None,
        })
        .expect("Creator")
        .creator
        .id
}

fn canonical(channel: ContactChannel, value: &str) -> ContactLocatorInput {
    crate::contact_routes::normalize_discovered_contact_route(&DiscoveredContactRoute {
        channel,
        kind: LocatorKind::Handle,
        value: value.into(),
        source: SourceKind::ManualEntry,
        source_url: None,
    })
    .expect("canonical route")
}

fn add_manual(
    repository: &SqliteRepository,
    creator_id: &CreatorId,
    channel: ContactChannel,
    value: &str,
) -> crate::ManualCreatorContactRouteAddResult {
    repository
        .add_manual_creator_contact_route(
            ManualCreatorContactRouteAddInput {
                creator_id: creator_id.clone(),
                channel,
                value: value.into(),
                source_url: None,
            },
            canonical(channel, value),
        )
        .expect("manual route")
}

fn update_manual(
    repository: &SqliteRepository,
    creator_id: &CreatorId,
    route_id: &crate::CreatorContactRouteId,
    channel: ContactChannel,
    value: &str,
    source_url: Option<&str>,
) -> Result<crate::ManualCreatorContactRouteUpdateResult, String> {
    repository.update_manual_creator_contact_route(
        ManualCreatorContactRouteUpdateInput {
            creator_id: creator_id.clone(),
            contact_route_id: route_id.clone(),
            channel,
            value: value.into(),
            source_url: source_url.map(str::to_owned),
        },
        canonical(channel, value),
    )
}

fn add_discovered_route(
    repository: &SqliteRepository,
    creator_id: &CreatorId,
    channel: ContactChannel,
    value: &str,
    source_kind: SourceKind,
) -> crate::CreatorContactRoute {
    let locator = repository
        .upsert_contact_locator(canonical(channel, value))
        .expect("locator");
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: source_kind,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/example".into()),
            external_ref: None,
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .expect("source");
    repository
        .append_locator_observation(&locator.id, &source.id, Some(creator_id), None, value)
        .expect("observation");
    repository
        .link_creator_contact_route(creator_id, &locator.id, Confidence::Possible)
        .expect("route")
}

fn prepare_deletion(
    repository: &SqliteRepository,
    creator_id: &CreatorId,
    route_id: &crate::CreatorContactRouteId,
) -> PrepareCreatorContactRouteDeleteResult {
    repository
        .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
            creator_id: creator_id.clone(),
            contact_route_id: route_id.clone(),
        })
        .expect("delete preparation")
}

fn prepared_operation_id(
    prepared: PrepareCreatorContactRouteDeleteResult,
) -> crate::CreatorContactRouteDeleteOperationId {
    match prepared {
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
            operation_id,
            remote_not_deleted,
            next_tool,
            ..
        } => {
            assert!(remote_not_deleted);
            assert_eq!(next_tool, "onecrew.creator.contact_route.delete.confirm");
            operation_id
        }
        other => panic!("expected confirmation requirement, got {other:?}"),
    }
}

struct CommunityAdministratorFixture {
    source_creator_id: CreatorId,
    source_route_id: crate::CreatorContactRouteId,
    administrator_creator_id: CreatorId,
    administrator_route_id: crate::CreatorContactRouteId,
}

fn save_community_administrator_fixture(
    repository: &SqliteRepository,
) -> CommunityAdministratorFixture {
    let source_creator_id = create_creator(repository, "Source community");
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("telegram-delete-operator"),
            Platform::Telegram,
            "profile",
            "@operator",
        )
        .expect("Telegram operator");
    let source_locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Telegram,
            kind: LocatorKind::Handle,
            normalized_value: "source_delete_community".into(),
            display_value: "@source_delete_community".into(),
        })
        .expect("source locator");
    let source_route = repository
        .link_creator_contact_route(&source_creator_id, &source_locator.id, Confidence::Exact)
        .expect("source route");
    let resolved = repository
        .commit_contact_target_resolution(CommitContactTargetResolutionInput {
            creator_id: source_creator_id.clone(),
            contact_route_id: source_route.id.clone(),
            locator_id: source_locator.id,
            account_ref_id: operator.id.clone(),
            platform: Platform::Telegram,
            target_kind: TargetKind::Supergroup,
            external_target_id: "delete-supergroup".into(),
            attach_to_creator: false,
            metadata: serde_json::json!({ "title": "Delete source community" }),
        })
        .expect("source resolution");
    let CommitContactTargetResolutionResult::Active { value: resolved } = resolved else {
        panic!("source resolution must be active")
    };
    let discovery_id = DiscoveryId::new("delete-community-administrators");
    let candidate_id = "telegram:user:delete-admin".to_string();
    let candidate = crate::CanonicalCreatorSaveInput {
        row_id: candidate_id.clone(),
        provenance: crate::CreatorSaveProvenance::Discovery {
            candidate_id: candidate_id.clone(),
            origin: CreatorCandidateOrigin::CommunityAdministrator {
                discovery_id: discovery_id.clone(),
                role: CommunityAdministratorRole::Owner,
                community: CommunityReference {
                    external_id: "delete-supergroup".into(),
                    title: "Delete source community".into(),
                    handle: Some("source_delete_community".into()),
                    target_kind: CommunityTargetKind::Supergroup,
                },
                source: CommunityAdministratorSource::CreatorContactRoute {
                    source_creator_id: source_creator_id.clone(),
                    source_contact_route_id: Some(source_route.id.clone()),
                    operator_account_ref_id: operator.id,
                    source_resolved_target_id: resolved.target.id,
                },
            },
            selection_context: None,
        },
        account: PlatformAccountCandidate {
            platform: Platform::Telegram,
            external_id: Some("delete-admin".into()),
            target_kind: Some(TargetKind::User),
            handle: Some("delete_admin".into()),
            display_name: "Delete Admin".into(),
            profile_url: Some("https://t.me/delete_admin".into()),
        },
        profile: Default::default(),
        profile_observed_at: 1_700_000_000,
        contact_evidence: vec![],
        notes: None,
        tags: None,
        quality: Default::default(),
    };
    let saved =
        persist_entry(repository, save_input(candidate, None)).expect("administrator Creator");
    let administrator_route_id = saved
        .creator
        .contact_routes
        .iter()
        .find(|route| route.locator.channel == ContactChannel::Telegram)
        .expect("administrator Telegram route")
        .route
        .id
        .clone();
    CommunityAdministratorFixture {
        source_creator_id,
        source_route_id: source_route.id,
        administrator_creator_id: saved.creator.creator.id,
        administrator_route_id,
    }
}

#[test]
fn route_capabilities_and_root_initial_guards_are_runtime_derived() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Capabilities");
    let manual = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "manual@example.com",
    );
    let discovered = add_discovered_route(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "bio@example.com",
        SourceKind::ProfileBio,
    );
    let initial = add_discovered_route(
        &repository,
        &creator_id,
        ContactChannel::X,
        "initial",
        SourceKind::InitialPlatformAccount,
    );
    let mixed = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "mixed@example.com",
    );
    let mixed_locator = repository
        .creator_contact_routes(&creator_id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == mixed.route_id)
        .unwrap()
        .locator;
    let initial_source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/mixed".into()),
            external_ref: None,
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .unwrap();
    repository
        .append_locator_observation(
            &mixed_locator.id,
            &initial_source.id,
            Some(&creator_id),
            None,
            "mixed@example.com",
        )
        .unwrap();
    let lineage_parent = add_discovered_route(
        &repository,
        &creator_id,
        ContactChannel::LinkHub,
        "https://linktr.ee/mixed-parent",
        SourceKind::ProfileWebsite,
    );
    repository
        .link_creator_contact_route_dependency(
            &lineage_parent.id,
            &mixed.route_id,
            CreatorContactRouteDependencyKind::LinkHub,
        )
        .unwrap();

    let records = repository.creator_contact_routes(&creator_id).unwrap();
    let actions = |route_id: &crate::CreatorContactRouteId| {
        records
            .iter()
            .find(|record| record.route.id == *route_id)
            .unwrap()
            .allowed_actions
            .clone()
    };
    assert_eq!(
        actions(&manual.route_id),
        vec![
            CreatorContactRouteAction::Update,
            CreatorContactRouteAction::Delete,
        ]
    );
    assert_eq!(
        actions(&discovered.id),
        vec![CreatorContactRouteAction::Delete]
    );
    assert!(actions(&initial.id).is_empty());
    assert!(actions(&mixed.route_id).is_empty());

    let blocked = prepare_deletion(&repository, &creator_id, &mixed.route_id);
    assert!(matches!(
        blocked,
        PrepareCreatorContactRouteDeleteResult::Blocked {
            reason: CreatorContactRouteDeleteBlockReason::RootInitialPlatformAccount,
            ..
        }
    ));

    let operation_id =
        prepared_operation_id(prepare_deletion(&repository, &creator_id, &discovered.id));
    let (deleted, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id,
        })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        deleted,
        ConfirmCreatorContactRouteDeleteResult::Deleted { .. }
    ));
    assert!(
        repository
            .creator_contact_routes(&creator_id)
            .unwrap()
            .iter()
            .all(|record| record.route.id != discovered.id)
    );

    repository.archive_creator(&creator_id).unwrap();
    assert!(
        repository
            .creator_contact_routes(&creator_id)
            .unwrap()
            .iter()
            .all(|record| record.allowed_actions.is_empty())
    );
    assert!(matches!(
        prepare_deletion(&repository, &creator_id, &manual.route_id),
        PrepareCreatorContactRouteDeleteResult::Blocked {
            reason: CreatorContactRouteDeleteBlockReason::ArchivedCreator,
            ..
        }
    ));
}

#[test]
fn manual_update_only_changes_same_canonical_metadata() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Editable");
    let original = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Telegram,
        "old_handle",
    );
    let original_record = repository
        .creator_contact_routes(&creator_id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == original.route_id)
        .unwrap();
    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "UPDATE creator_contact_routes SET updated_at = 102 WHERE id = ?1",
                [original_record.route.id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE contact_locators SET updated_at = 101 WHERE id = ?1",
                [original_record.locator.id.as_str()],
            )
            .unwrap();
    }
    let audit_count = repository
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| {
            row.get::<_, u64>(0)
        })
        .unwrap();
    let unchanged = update_manual(
        &repository,
        &creator_id,
        &original.route_id,
        ContactChannel::Telegram,
        "old_handle",
        None,
    )
    .unwrap();
    assert_eq!(
        unchanged.status,
        ManualCreatorContactRouteUpdateStatus::Unchanged
    );
    assert_eq!(unchanged.route.route.updated_at, 102);
    assert_eq!(unchanged.route.locator.updated_at, 101);
    assert_eq!(
        repository
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        audit_count
    );

    let updated = update_manual(
        &repository,
        &creator_id,
        &original.route_id,
        ContactChannel::Telegram,
        "@old_handle",
        Some("https://t.me/old_handle"),
    )
    .unwrap();
    assert_eq!(
        updated.status,
        ManualCreatorContactRouteUpdateStatus::Updated
    );
    assert_eq!(updated.route_id, original.route_id);
    assert_eq!(
        updated.route.observations[0].source.source_url.as_deref(),
        Some("https://t.me/old_handle")
    );

    let replacement = update_manual(
        &repository,
        &creator_id,
        &original.route_id,
        ContactChannel::Telegram,
        "new_handle",
        None,
    )
    .unwrap();
    assert_eq!(
        replacement.status,
        ManualCreatorContactRouteUpdateStatus::ReplacementRequired
    );
    assert_eq!(replacement.route_id, original.route_id);
    let records = repository.creator_contact_routes(&creator_id).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].locator.normalized_value, "old_handle");
}

#[test]
fn manual_add_rejects_same_and_cross_creator_canonical_collisions() {
    let (_directory, repository) = repository();
    let first = create_creator(&repository, "First");
    let second = create_creator(&repository, "Second");
    let first_route = add_manual(
        &repository,
        &first,
        ContactChannel::Email,
        "owner@example.com",
    );
    let duplicate = repository
        .add_manual_creator_contact_route(
            ManualCreatorContactRouteAddInput {
                creator_id: first.clone(),
                channel: ContactChannel::Email,
                value: "OWNER@example.com".into(),
                source_url: None,
            },
            canonical(ContactChannel::Email, "OWNER@example.com"),
        )
        .unwrap();
    assert_eq!(
        duplicate.status,
        ManualCreatorContactRouteAddStatus::AlreadyExists
    );
    assert_eq!(duplicate.route_id, first_route.route_id);

    let error = repository
        .add_manual_creator_contact_route(
            ManualCreatorContactRouteAddInput {
                creator_id: second,
                channel: ContactChannel::Email,
                value: "owner@example.com".into(),
                source_url: None,
            },
            canonical(ContactChannel::Email, "owner@example.com"),
        )
        .expect_err("cross-Creator collision");
    assert!(error.contains("already belongs"), "{error}");
}

#[test]
fn deletion_is_replay_safe_purges_resolution_and_requires_children_first() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Delete replay");
    let route = add_manual(&repository, &creator_id, ContactChannel::X, "delete_me");
    let record = repository
        .creator_contact_routes(&creator_id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == route.route_id)
        .unwrap();
    let operator = repository
        .upsert_account_ref(
            AccountRefId::new("x-operator"),
            Platform::X,
            "profile",
            "@operator",
        )
        .unwrap();
    let target = repository
        .upsert_resolved_target(
            &operator.id,
            Platform::X,
            TargetKind::User,
            "delete-me-target",
            None,
            serde_json::json!({}),
        )
        .unwrap();
    repository
        .record_locator_resolution(&record.locator.id, &operator.id, &target.id)
        .unwrap();
    let operation_id =
        prepared_operation_id(prepare_deletion(&repository, &creator_id, &route.route_id));
    let input = ConfirmCreatorContactRouteDeleteInput {
        operation_id: operation_id.clone(),
    };
    let (first, changed) = repository
        .confirm_creator_contact_route_delete(input.clone())
        .unwrap();
    assert!(changed);
    let ConfirmCreatorContactRouteDeleteResult::Deleted { deleted, .. } = &first else {
        panic!("route deletion must complete")
    };
    assert_eq!(deleted.contact_routes, 1);
    assert_eq!(deleted.target_resolutions, 1);
    assert_eq!(deleted.resolved_targets, 1);
    assert_eq!(deleted.contact_locators, 1);
    let (replay, changed) = repository
        .confirm_creator_contact_route_delete(input)
        .unwrap();
    assert!(!changed);
    assert_eq!(replay, first);
    let connection = repository.lock().unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM locator_resolutions WHERE locator_id = ?1",
                [record.locator.id.as_str()],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM resolved_targets WHERE id = ?1",
                [target.id.as_str()],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM contact_locators WHERE id = ?1",
                [record.locator.id.as_str()],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
    drop(connection);
    assert!(
        repository
            .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
                creator_id: creator_id.clone(),
                contact_route_id: route.route_id,
            })
            .is_err(),
        "a new prepare must not resurrect a deleted route"
    );

    let parent = add_discovered_route(
        &repository,
        &creator_id,
        ContactChannel::LinkHub,
        "https://linktr.ee/parent",
        SourceKind::ProfileWebsite,
    );
    let child = add_discovered_route(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "child@example.com",
        SourceKind::LinkHub,
    );
    repository
        .link_creator_contact_route_dependency(
            &parent.id,
            &child.id,
            CreatorContactRouteDependencyKind::LinkHub,
        )
        .unwrap();
    let blocked = prepare_deletion(&repository, &creator_id, &parent.id);
    match blocked {
        PrepareCreatorContactRouteDeleteResult::Blocked {
            reason, children, ..
        } => {
            assert_eq!(
                reason,
                CreatorContactRouteDeleteBlockReason::HasDerivedContactRoutes
            );
            assert_eq!(children.len(), 1);
            assert_eq!(children[0].contact_route_id, child.id);
        }
        other => panic!("parent should be blocked: {other:?}"),
    }
    let child_operation =
        prepared_operation_id(prepare_deletion(&repository, &creator_id, &child.id));
    repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: child_operation,
        })
        .unwrap();
    let parent_operation =
        prepared_operation_id(prepare_deletion(&repository, &creator_id, &parent.id));
    repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: parent_operation,
        })
        .unwrap();
}

#[test]
fn derived_initial_child_deletes_first_and_parent_detaches_readable_history() {
    let (_directory, repository) = repository();
    let fixture = save_community_administrator_fixture(&repository);

    let child = repository
        .creator_contact_routes(&fixture.administrator_creator_id)
        .unwrap()
        .into_iter()
        .find(|route| route.route.id == fixture.administrator_route_id)
        .expect("derived administrator route");
    assert_eq!(
        child.allowed_actions,
        vec![CreatorContactRouteAction::Delete],
        "a pure derived initial route is deletable"
    );
    let child_operation = prepared_operation_id(prepare_deletion(
        &repository,
        &fixture.administrator_creator_id,
        &fixture.administrator_route_id,
    ));
    repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: child_operation,
        })
        .unwrap();

    let prepared = prepare_deletion(
        &repository,
        &fixture.source_creator_id,
        &fixture.source_route_id,
    );
    let (operation_id, prepared_impact) = match prepared {
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
            operation_id,
            impact,
            ..
        } => (operation_id, impact),
        other => panic!("source route should be deletable after its child: {other:?}"),
    };
    assert_eq!(prepared_impact.creator_origins_detached, 1);
    assert_eq!(prepared_impact.contact_routes, 1);
    assert_eq!(
        prepared_impact.local_records_total,
        prepared_impact.contact_routes
            + prepared_impact.dependencies
            + prepared_impact.email_threads
            + prepared_impact.email_messages
            + prepared_impact.email_recipients
            + prepared_impact.email_drafts
            + prepared_impact.direct_message_conversations
            + prepared_impact.direct_messages
            + prepared_impact.outreach_touchpoints
            + prepared_impact.audit_events
            + prepared_impact.idempotency_records
            + prepared_impact.target_resolutions
            + prepared_impact.identity_conflicts
            + prepared_impact.observations
            + prepared_impact.contact_locators
            + prepared_impact.observation_sources
            + prepared_impact.resolved_targets
            + prepared_impact.creator_origins_detached
    );
    let (deleted, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id,
        })
        .unwrap();
    assert!(changed);
    let ConfirmCreatorContactRouteDeleteResult::Deleted {
        deleted: deleted_impact,
        ..
    } = deleted
    else {
        panic!("source route deletion must complete")
    };
    assert_eq!(deleted_impact, prepared_impact);

    let administrator = repository
        .get_creator(&fixture.administrator_creator_id)
        .expect("administrator origin remains readable");
    let historical_origin = administrator
        .origins
        .iter()
        .find_map(|origin| match origin {
            CreatorOrigin::CommunityAdministrator {
                source:
                    CommunityAdministratorSource::CreatorContactRoute {
                        source_contact_route_id,
                        ..
                    },
                ..
            } => Some(source_contact_route_id),
            _ => None,
        })
        .expect("administrator origin");
    assert_eq!(historical_origin, &None);

    let retired_table_count: u64 = repository
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE name='creator_candidate_snapshots'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        retired_table_count, 0,
        "route deletion never relies on retired snapshot storage"
    );
}

#[test]
fn unowned_same_creator_email_address_is_safely_attributed_and_deleted() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Email history owner");
    let route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "history@example.com",
    );
    let route_record = repository
        .creator_contact_routes(&creator_id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == route.route_id)
        .unwrap();
    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "INSERT INTO mailbox_refs
                   (id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at)
                 VALUES ('mailbox-delete', 'resend', 'Delete mailbox', 'sender@example.com',
                         NULL, '{}', 1, NULL, 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_threads
                   (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    created_at, updated_at)
                 VALUES ('thread-delete', 'mailbox-delete', ?1, NULL, 'provider-thread-delete',
                         'History', 0, 'sent', NULL, NULL, 1, 1)",
                [creator_id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO emails
                   (id, thread_id, provider_message_id, internet_message_id, direction,
                    subject, body_text, body_html, in_reply_to, references_json,
                    delivery_status, headers_json, occurred_at, origin)
                 VALUES ('email-delete', 'thread-delete', 'provider-email-delete', NULL,
                         'outbound', 'History', 'body', NULL, NULL, '[]', 'sent', '{}', 1, 'confirmed_send')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_recipients
                   (email_id, recipient_kind, address, locator_id)
                 VALUES ('email-delete', 'from', 'sender@example.com', NULL),
                        ('email-delete', 'to', 'HISTORY@example.com', NULL)",
                [],
            )
            .unwrap();
    }

    let prepared = prepare_deletion(&repository, &creator_id, &route.route_id);
    let (operation_id, impact) = match prepared {
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
            operation_id,
            impact,
            ..
        } => (operation_id, impact),
        other => panic!("same-Creator raw email history is safely attributable: {other:?}"),
    };
    assert_eq!(impact.email_threads, 1);
    assert_eq!(impact.email_messages, 1);
    assert_eq!(impact.email_recipients, 2);
    assert_eq!(impact.contact_locators, 1);
    assert_eq!(impact.observation_sources, 1);
    let (deleted, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id,
        })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        deleted,
        ConfirmCreatorContactRouteDeleteResult::Deleted { deleted, .. }
            if deleted == impact
    ));
    let connection = repository.lock().unwrap();
    for table in ["email_threads", "emails", "email_recipients"] {
        let count = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} must be deleted locally");
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM contact_locators WHERE id = ?1",
                [route_record.locator.id.as_str()],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn email_thread_without_creator_or_route_is_shared_history() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Unowned email history");
    let route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "unowned-history@example.com",
    );
    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "INSERT INTO mailbox_refs
                   (id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at)
                 VALUES ('mailbox-unowned-delete', 'resend', 'Unowned mailbox',
                         'sender@example.com', NULL, '{}', 1, NULL, 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_threads
                   (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    created_at, updated_at)
                 VALUES ('thread-unowned-delete', 'mailbox-unowned-delete', NULL, NULL,
                         'provider-thread-unowned-delete', 'Unowned', 0, 'inbox',
                         NULL, NULL, 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO emails
                   (id, thread_id, provider_message_id, internet_message_id, direction,
                    subject, body_text, body_html, in_reply_to, references_json,
                    delivery_status, headers_json, occurred_at, origin)
                 VALUES ('email-unowned-delete', 'thread-unowned-delete',
                         'provider-email-unowned-delete', NULL, 'inbound', 'Unowned', 'body',
                         NULL, NULL, '[]', 'received', '{}', 1, 'provider_import')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_recipients
                   (email_id, recipient_kind, address, locator_id)
                 VALUES ('email-unowned-delete', 'from',
                         'Unowned Creator <UNOWNED-HISTORY@example.com>', NULL)",
                [],
            )
            .unwrap();
    }

    let blocked = prepare_deletion(&repository, &creator_id, &route.route_id);
    let PrepareCreatorContactRouteDeleteResult::Blocked {
        blocking_artifacts, ..
    } = blocked
    else {
        panic!("unowned email history must block deletion")
    };
    assert!(blocking_artifacts.iter().any(|artifact| {
        artifact.kind == CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory
    }));
    let connection = repository.lock().unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM email_threads WHERE id = 'thread-unowned-delete'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn multi_participant_email_thread_blocks_whole_thread_deletion() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Primary email participant");
    let other_creator_id = create_creator(&repository, "CC email participant");
    let route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "primary-participant@example.com",
    );
    add_manual(
        &repository,
        &other_creator_id,
        ContactChannel::Email,
        "other-participant@example.com",
    );
    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "INSERT INTO mailbox_refs
                   (id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at)
                 VALUES ('mailbox-multipart-delete', 'resend', 'Multipart mailbox',
                         'sender@example.com', NULL, '{}', 1, NULL, 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_threads
                   (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    created_at, updated_at)
                 VALUES ('thread-multipart-delete', 'mailbox-multipart-delete', ?1, ?2,
                         'provider-thread-multipart-delete', 'Multipart', 0, 'sent',
                         NULL, NULL, 1, 1)",
                rusqlite::params![creator_id.as_str(), route.route_id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO emails
                   (id, thread_id, provider_message_id, internet_message_id, direction,
                    subject, body_text, body_html, in_reply_to, references_json,
                    delivery_status, headers_json, occurred_at, origin)
                 VALUES ('email-multipart-delete', 'thread-multipart-delete',
                         'provider-email-multipart-delete', NULL, 'outbound', 'Multipart',
                         'body', NULL, NULL, '[]', 'sent', '{}', 1, 'confirmed_send')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_recipients
                   (email_id, recipient_kind, address, locator_id)
                 VALUES ('email-multipart-delete', 'to',
                         'primary-participant@example.com', NULL),
                        ('email-multipart-delete', 'cc',
                         'Other Creator <other-participant@example.com>', NULL)",
                [],
            )
            .unwrap();
    }

    let blocked = prepare_deletion(&repository, &creator_id, &route.route_id);
    let PrepareCreatorContactRouteDeleteResult::Blocked {
        blocking_artifacts, ..
    } = blocked
    else {
        panic!("a multi-participant thread must not be deleted as one route's history")
    };
    assert!(blocking_artifacts.iter().any(|artifact| {
        artifact.kind == CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory
    }));
    assert!(
        repository
            .creator_contact_routes(&creator_id)
            .unwrap()
            .iter()
            .any(|record| record.route.id == route.route_id)
    );
}

#[test]
fn cross_creator_owned_artifacts_block_route_deletion_without_mutation() {
    let (_directory, repository) = repository();
    let owner = create_creator(&repository, "Route owner");
    let other = create_creator(&repository, "Other owner");
    let route = add_manual(
        &repository,
        &owner,
        ContactChannel::Email,
        "shared-history@example.com",
    );
    let route_record = repository
        .creator_contact_routes(&owner)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == route.route_id)
        .unwrap();
    let other_source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::ProfileBio,
            platform: Some(Platform::X),
            source_url: Some("https://x.com/other".into()),
            external_ref: None,
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .unwrap();
    repository
        .append_locator_observation(
            &route_record.locator.id,
            &other_source.id,
            Some(&other),
            None,
            "shared-history@example.com",
        )
        .unwrap();
    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "INSERT INTO identity_conflicts
                   (id, conflict_key, kind, creator_id, platform_account_id, locator_id,
                    resolved_target_id, evidence_json, state, created_at, resolved_at)
                 VALUES ('other-conflict', 'other-conflict-key', 'test', ?1, NULL, ?2,
                         NULL, '{}', 'open', 1, NULL)",
                rusqlite::params![other.as_str(), route_record.locator.id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO mailbox_refs
                   (id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at)
                 VALUES ('mailbox-shared-delete', 'resend', 'Shared mailbox',
                         'sender@example.com', NULL, '{}', 1, NULL, 1, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_threads
                   (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    created_at, updated_at)
                 VALUES ('thread-shared-delete', 'mailbox-shared-delete', ?1, ?2,
                         'provider-thread-shared-delete', 'Shared', 0, 'sent',
                         NULL, NULL, 1, 1)",
                rusqlite::params![other.as_str(), route.route_id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO email_drafts
                   (id, mailbox_ref_id, to_address, creator_id, contact_route_id,
                    reply_to_email_id, subject, body_text, body_html, template_id,
                    template_variables_json, state, sending_idempotency_key, send_reserved_at,
                    revision, created_at, updated_at)
                 VALUES ('draft-shared-delete', 'mailbox-shared-delete',
                         'shared-history@example.com', ?1, ?2, NULL, 'Shared', 'body', NULL,
                         NULL, '{}', 'draft', NULL, NULL, 1, 1, 1)",
                rusqlite::params![other.as_str(), route.route_id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO touchpoints
                   (id, creator_id, contact_route_id, kind, message_id, email_id,
                    account_relationship_id, occurred_at, metadata_json)
                 VALUES ('touchpoint-shared-delete', ?1, ?2, 'test', NULL, NULL, NULL, 1, '{}')",
                rusqlite::params![other.as_str(), route.route_id.as_str()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                    contact_locator_id, contact_route_id, email_id, payload_json,
                    created_at)
                 VALUES ('audit-shared-delete', 'test', ?1, NULL, NULL, NULL, ?2, ?3,
                         NULL, '{}', 1)",
                rusqlite::params![
                    other.as_str(),
                    route_record.locator.id.as_str(),
                    route.route_id.as_str(),
                ],
            )
            .unwrap();
    }

    let blocked = prepare_deletion(&repository, &owner, &route.route_id);
    let PrepareCreatorContactRouteDeleteResult::Blocked {
        blocking_artifacts, ..
    } = blocked
    else {
        panic!("cross-Creator artifacts must block deletion")
    };
    for expected in [
        CreatorContactRouteDeleteBlockingArtifactKind::SharedContactLocator,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory,
        CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory,
    ] {
        assert!(
            blocking_artifacts
                .iter()
                .any(|artifact| artifact.kind == expected),
            "missing blocker {expected:?}: {blocking_artifacts:?}"
        );
    }
    assert!(
        repository
            .creator_contact_routes(&owner)
            .unwrap()
            .iter()
            .any(|record| record.route.id == route.route_id)
    );
}

#[test]
fn concurrent_confirmation_has_exactly_one_changed_commit() {
    let (directory, repository) = repository();
    let creator_id = create_creator(&repository, "Concurrent delete");
    let route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "delete-concurrently@example.com",
    );
    let operation_id =
        prepared_operation_id(prepare_deletion(&repository, &creator_id, &route.route_id));
    let path = directory.path().join("crm.db");
    let first_repository = SqliteRepository::open(&path).unwrap();
    let second_repository = SqliteRepository::open(&path).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let results = std::thread::scope(|scope| {
        let first_barrier = barrier.clone();
        let first_operation = operation_id.clone();
        let first = scope.spawn(move || {
            first_barrier.wait();
            first_repository.confirm_creator_contact_route_delete(
                ConfirmCreatorContactRouteDeleteInput {
                    operation_id: first_operation,
                },
            )
        });
        let second_barrier = barrier.clone();
        let second = scope.spawn(move || {
            second_barrier.wait();
            second_repository.confirm_creator_contact_route_delete(
                ConfirmCreatorContactRouteDeleteInput { operation_id },
            )
        });
        (
            first.join().unwrap().unwrap(),
            second.join().unwrap().unwrap(),
        )
    });
    assert_eq!(u8::from(results.0.1) + u8::from(results.1.1), 1);
    assert_eq!(results.0.0, results.1.0);
}

#[test]
fn deletion_fails_closed_for_stale_expired_and_ambiguous_plans() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Delete guards");

    let stale_route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "stale@example.com",
    );
    let stale_operation = prepared_operation_id(prepare_deletion(
        &repository,
        &creator_id,
        &stale_route.route_id,
    ));
    update_manual(
        &repository,
        &creator_id,
        &stale_route.route_id,
        ContactChannel::Email,
        "stale@example.com",
        Some("https://example.com/contact"),
    )
    .unwrap();
    let (stale, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: stale_operation.clone(),
        })
        .unwrap();
    assert!(!changed);
    assert!(matches!(
        stale,
        ConfirmCreatorContactRouteDeleteResult::Stale {
            reason: CreatorContactRouteDeleteStaleReason::PlanChanged,
            ..
        }
    ));
    let (stale_replay, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: stale_operation.clone(),
        })
        .unwrap();
    assert!(!changed);
    assert_eq!(stale_replay, stale);
    assert_eq!(
        repository
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM creator_contact_route_delete_operations WHERE id = ?1",
                [stale_operation.as_str()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "completed"
    );

    let expired_route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "expired@example.com",
    );
    let expired_operation = prepared_operation_id(prepare_deletion(
        &repository,
        &creator_id,
        &expired_route.route_id,
    ));
    repository
        .lock()
        .unwrap()
        .execute(
            "UPDATE creator_contact_route_delete_operations SET expires_at = 0 WHERE id = ?1",
            [expired_operation.as_str()],
        )
        .unwrap();
    let (expired, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: expired_operation.clone(),
        })
        .unwrap();
    assert!(!changed);
    assert!(matches!(
        expired,
        ConfirmCreatorContactRouteDeleteResult::Expired { .. }
    ));
    let (expired_replay, changed) = repository
        .confirm_creator_contact_route_delete(ConfirmCreatorContactRouteDeleteInput {
            operation_id: expired_operation,
        })
        .unwrap();
    assert!(!changed);
    assert_eq!(expired_replay, expired);

    let ambiguous_route = add_manual(
        &repository,
        &creator_id,
        ContactChannel::Email,
        "ambiguous@example.com",
    );
    let ambiguous_record = repository
        .creator_contact_routes(&creator_id)
        .unwrap()
        .into_iter()
        .find(|record| record.route.id == ambiguous_route.route_id)
        .unwrap();
    repository
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO audit_events
               (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                contact_locator_id, contact_route_id, email_id, payload_json,
                created_at)
             VALUES ('legacy-ambiguous-audit', 'legacy', ?1, NULL, NULL, NULL,
                     ?2, NULL, NULL, '{}', 0)",
            rusqlite::params![creator_id.as_str(), ambiguous_record.locator.id.as_str()],
        )
        .unwrap();
    match prepare_deletion(&repository, &creator_id, &ambiguous_route.route_id) {
        PrepareCreatorContactRouteDeleteResult::Blocked {
            reason,
            blocking_artifacts,
            ..
        } => {
            assert_eq!(
                reason,
                CreatorContactRouteDeleteBlockReason::AmbiguousLegacyHistory
            );
            assert!(blocking_artifacts.iter().any(|artifact| {
                artifact.kind
                    == CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory
            }));
        }
        other => panic!("ambiguous history must block deletion: {other:?}"),
    }
}

#[test]
fn delete_creator_removes_owned_rows_and_clears_references() {
    let (_directory, repository) = repository();
    let creator_id = create_creator(&repository, "Deletable");
    add_manual(
        &repository,
        &creator_id,
        ContactChannel::Telegram,
        "@deleteme",
    );

    let result = repository
        .delete_creator(DeleteCreatorInput {
            creator_id: creator_id.clone(),
        })
        .expect("delete creator");
    assert_eq!(result.removed_contact_routes, 1);
    assert!(
        repository.get_creator(&creator_id).is_err(),
        "creator is gone"
    );
    assert!(
        repository
            .delete_creator(DeleteCreatorInput {
                creator_id: creator_id.clone(),
            })
            .is_err(),
        "double delete is a clean error"
    );
}
