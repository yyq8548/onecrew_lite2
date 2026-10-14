use super::*;
use crate::MailboxIssue;

#[test]
fn mailbox_identity_mapping_supports_many_projections_and_connection_scoped_deletion() {
    let (_directory, repository) = repository();
    let first = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Primary".into(),
            email_address: "primary@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .unwrap();
    let second = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
            provider: "resend".into(),
            label: "Alias".into(),
            email_address: "alias@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .unwrap();
    let auth_connection_id = EmailAuthConnectionId::new("auth-connection-1");
    let secret_record_id = EmailSecretRecordId::new("secret-record-1");
    let first_mapping = repository
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: first.mailbox_ref_id.clone(),
            auth_connection_id: auth_connection_id.clone(),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: secret_record_id.clone(),
            connection_label: "Primary connection".into(),
            projection_kind: MailboxProjectionKind::Mailbox,
            capability_projection_revision: 2,
            checkpoint_namespace: "opaque-checkpoint-primary".into(),
        })
        .unwrap();
    assert_eq!(first_mapping.mailbox_ref_id, first.mailbox_ref_id);
    let second_mapping = repository
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: second.mailbox_ref_id.clone(),
            auth_connection_id: auth_connection_id.clone(),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id,
            connection_label: "Primary connection".into(),
            projection_kind: MailboxProjectionKind::SendAs,
            capability_projection_revision: 2,
            checkpoint_namespace: "opaque-checkpoint-alias".into(),
        })
        .unwrap();
    assert_eq!(
        second_mapping.projection_kind,
        MailboxProjectionKind::SendAs
    );
    assert!(
        repository
            .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
                mailbox_ref_id: first.mailbox_ref_id.clone(),
                auth_connection_id: auth_connection_id.clone(),
                provider_kind: EmailProviderKind::Resend,
                secret_record_id: EmailSecretRecordId::new("different-secret"),
                connection_label: "wrong binding".into(),
                projection_kind: MailboxProjectionKind::Mailbox,
                capability_projection_revision: 2,
                checkpoint_namespace: "opaque-checkpoint-wrong".into(),
            })
            .is_err()
    );
    assert!(
        repository
            .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
                mailbox_ref_id: first.mailbox_ref_id.clone(),
                auth_connection_id: EmailAuthConnectionId::new("smtp-auth"),
                provider_kind: EmailProviderKind::Smtp,
                secret_record_id: EmailSecretRecordId::new("smtp-secret"),
                connection_label: "wrong provider".into(),
                projection_kind: MailboxProjectionKind::Mailbox,
                capability_projection_revision: 1,
                checkpoint_namespace: "opaque-checkpoint-wrong-provider".into(),
            })
            .is_err()
    );

    let primary_binding = repository
        .resolve_mailbox_auth_binding(&first.mailbox_ref_id)
        .unwrap();
    assert_eq!(
        primary_binding.auth_connection.secret_record_id.as_str(),
        "secret-record-1"
    );
    assert_eq!(
        primary_binding.auth_connection.auth_connection_id,
        auth_connection_id
    );
    let alias_binding = repository
        .resolve_mailbox_auth_binding(&second.mailbox_ref_id)
        .unwrap();
    assert_eq!(primary_binding.email_address, "primary@example.com");
    assert_eq!(alias_binding.email_address, "alias@example.com");
    assert_eq!(
        primary_binding.auth_connection,
        alias_binding.auth_connection
    );
    let claim = repository
        .begin_email_auth_connection_deletion(&auth_connection_id)
        .unwrap();
    assert_eq!(
        claim,
        EmailAuthConnectionDeletionClaim::Acquired {
            mailbox_ref_ids: vec![first.mailbox_ref_id.clone(), second.mailbox_ref_id.clone()]
        }
    );
    let guard_count = repository
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM mailbox_credential_deletion_guards",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(guard_count, 2);
    repository
        .finish_email_auth_connection_deletion(&auth_connection_id)
        .unwrap();
    assert_eq!(
        repository
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM mailbox_credential_deletion_guards",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn primary_mailbox_reconciliation_refreshes_existing_auth_availability() {
    let (_directory, repository) = repository();
    let mailbox_id = MailboxRefId::new("11111111-1111-4111-8111-111111111111");
    let auth_id =
        crate::resend_auth_connection_id(uuid::Uuid::parse_str(mailbox_id.as_str()).unwrap());
    let mapping = || MailboxIdentityMappingInput {
        mailbox_ref_id: mailbox_id.clone(),
        auth_connection_id: auth_id.clone(),
        provider_kind: EmailProviderKind::Resend,
        secret_record_id: EmailSecretRecordId::new(mailbox_id.as_str()),
        connection_label: "Recovered mailbox".into(),
        projection_kind: MailboxProjectionKind::Mailbox,
        capability_projection_revision: 1,
        checkpoint_namespace: format!("email-checkpoint-{}", mailbox_id.as_str()),
    };
    repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: mailbox_id.clone(),
            provider: "resend".into(),
            label: "Unavailable".into(),
            email_address: "recovered@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: false,
            last_error: Some("credential missing".into()),
        })
        .unwrap();
    repository
        .upsert_mailbox_identity_mapping(mapping())
        .unwrap();
    assert!(
        !repository
            .resolve_mailbox_auth_binding(&mailbox_id)
            .unwrap()
            .auth_connection
            .available
    );

    repository
        .reconcile_mailbox_refs(vec![MailboxRefInput {
            id: mailbox_id.clone(),
            provider: "resend".into(),
            label: "Recovered mailbox".into(),
            email_address: "recovered@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        }])
        .unwrap();
    repository
        .upsert_mailbox_identity_mapping(mapping())
        .unwrap();
    let recovered = repository
        .resolve_mailbox_auth_binding(&mailbox_id)
        .unwrap()
        .auth_connection;
    assert!(recovered.available);
    assert_eq!(recovered.label, "Recovered mailbox");
}

#[test]
fn v31_identity_split_migration_preserves_mailbox_and_email_facts() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v31(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('11111111-1111-4111-8111-111111111111', 'resend', 'Migrated',
                     'migrated@example.com', '[\"send\"]', 1, 11, 12);
             INSERT INTO email_threads
               (id, mailbox_ref_id, provider_thread_key, subject, active_folder,
                created_at, updated_at)
             VALUES ('thread-v31', '11111111-1111-4111-8111-111111111111',
                     'provider-thread-v31', 'Preserved', 'sent', 11, 12);
             INSERT INTO emails
               (id, thread_id, provider_message_id, direction, subject, body_text,
                references_json, delivery_status, headers_json, occurred_at, origin)
             VALUES ('email-v31', 'thread-v31', 'provider-message-v31', 'outbound',
                     'Preserved', 'Body', '[]', 'sent', '{}', 13, 'confirmed_send');",
        )
        .unwrap();
    drop(connection);

    let result = crate::CreatorRuntime::upgrade_database(&path).expect("v31 upgrade succeeds");
    assert_eq!(
        (result.from_version, result.to_version),
        (31, crate::CREATOR_SCHEMA_VERSION)
    );
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT mailbox_ref_id, provider_thread_key FROM email_threads
                 WHERE id = 'thread-v31'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap(),
        (
            "11111111-1111-4111-8111-111111111111".into(),
            "provider-thread-v31".into()
        )
    );
    let identity = connection
        .query_row(
            "SELECT mapping.mailbox_ref_id, mapping.auth_connection_id,
                    mapping.provider_kind, mapping.projection_kind,
                    mapping.capability_projection_revision,
                    mapping.checkpoint_namespace, auth.secret_record_id
             FROM mailbox_identity_mappings mapping
             JOIN email_auth_connections auth
               ON auth.id = mapping.auth_connection_id
             WHERE mapping.mailbox_ref_id = '11111111-1111-4111-8111-111111111111'",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .unwrap();
    let expected_auth = crate::resend_auth_connection_id(
        uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
    );
    assert_eq!(identity.0, "11111111-1111-4111-8111-111111111111");
    assert_eq!(identity.1, expected_auth.as_str());
    assert_ne!(identity.1, identity.0);
    assert_eq!(identity.2, "resend");
    assert_eq!(identity.3, "mailbox");
    assert_eq!(identity.4, 1);
    assert_eq!(
        identity.5,
        "email-checkpoint-11111111-1111-4111-8111-111111111111"
    );
    assert_eq!(identity.6, identity.0);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM emails WHERE id = 'email-v31'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        CreatorDatabaseStatus::Ready
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, i64>(0)
            },)
            .unwrap(),
        0,
        "v31 -> v32 migration must leave no foreign-key violations"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('email_threads')
                 WHERE \"table\" = 'mailbox_refs'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1,
        "email_threads must reference final mailbox_refs, not a rebuild temporary"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE name LIKE 'mailbox_refs_rebuild%'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0,
        "mailbox rebuild temporary must not remain in sqlite_master"
    );
}

#[test]
fn mailbox_projection_rejects_unsupported_smtp_and_contains_no_secret() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Partnerships".into(),
            email_address: "PARTNERSHIPS@example.com".into(),
            display_name: Some("Reach".into()),
            capabilities: vec![EmailCapability::Send, EmailCapability::SentReadback],
            available: true,
            last_error: None,
        })
        .expect("Resend mailbox projection");
    assert_eq!(mailbox.email_address, "partnerships@example.com");
    assert_eq!(repository.list_mailbox_refs().expect("mailboxes").len(), 1);

    let error = match repository.upsert_mailbox_ref(MailboxRefInput {
        id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
        provider: "smtp".into(),
        label: "Legacy".into(),
        email_address: "legacy@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Send],
        available: true,
        last_error: None,
    }) {
        Ok(_) => panic!("SMTP must not be accepted"),
        Err(error) => error,
    };
    assert!(error.contains("unsupported mailbox provider"));
}

#[test]
fn mailbox_credential_deletion_blockers_are_typed_and_only_include_active_work() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Partnerships".into(),
            email_address: "partnerships@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .unwrap();
    let auth_connection_id = EmailAuthConnectionId::new("auth-connection-blockers");
    repository
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            auth_connection_id: auth_connection_id.clone(),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new("secret-record-blockers"),
            connection_label: "Partnerships".into(),
            projection_kind: MailboxProjectionKind::Mailbox,
            capability_projection_revision: 1,
            checkpoint_namespace: "opaque-checkpoint-blockers".into(),
        })
        .unwrap();
    let draft = repository
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Draft".into(),
            text: "Body".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    {
        let connection = repository.lock().unwrap();
        for (index, state) in [
            "in_progress",
            "remote_started",
            "remote_accepted",
            "remote_verified",
            "completed",
            "failed",
        ]
        .into_iter()
        .enumerate()
        {
            connection
                .execute(
                    "INSERT INTO idempotency_records
                       (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                        created_at, updated_at)
                     VALUES (?1, 'email_send', ?2, 'fingerprint', ?3, '{}', ?4, ?4)",
                    rusqlite::params![
                        format!("recovery-{state}"),
                        mailbox.mailbox_ref_id.as_str(),
                        state,
                        index as u64
                    ],
                )
                .unwrap();
        }
        connection
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = 'draft-recovery', send_reserved_at = 1
                 WHERE id = ?1",
                [draft.id.as_str()],
            )
            .unwrap();
    }

    let EmailAuthConnectionDeletionClaim::Blocked {
        auth_connection_id: blocked_connection,
        blockers,
    } = repository
        .begin_email_auth_connection_deletion(&auth_connection_id)
        .unwrap()
    else {
        panic!("active work must block the deletion claim")
    };
    assert_eq!(blocked_connection, auth_connection_id);
    let [blockers] = blockers.as_slice() else {
        panic!("one mapped mailbox reports its blockers")
    };
    assert_eq!(blockers.mailbox_ref_id, mailbox.mailbox_ref_id);
    assert_eq!(blockers.recoveries.len(), 4);
    assert_eq!(
        blockers
            .recoveries
            .iter()
            .map(|blocker| blocker.stage.clone())
            .collect::<Vec<_>>(),
        vec![
            EmailRecoveryStage::InProgress,
            EmailRecoveryStage::RemoteStarted,
            EmailRecoveryStage::RemoteAccepted,
            EmailRecoveryStage::RemoteVerified,
        ]
    );
    assert_eq!(blockers.sending_drafts.len(), 1);
    assert_eq!(blockers.sending_drafts[0].draft_id, draft.id);

    {
        let connection = repository.lock().unwrap();
        connection
            .execute(
                "DELETE FROM idempotency_records
                 WHERE state IN ('in_progress','remote_started','remote_accepted','remote_verified')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = NULL, send_reserved_at = NULL
                 WHERE id = ?1",
                [draft.id.as_str()],
            )
            .unwrap();
    }
    assert_eq!(
        repository
            .begin_email_auth_connection_deletion(&auth_connection_id)
            .unwrap(),
        EmailAuthConnectionDeletionClaim::Acquired {
            mailbox_ref_ids: vec![mailbox.mailbox_ref_id.clone()],
        }
    );
    {
        let connection = repository.lock().unwrap();
        for action in ["email_send", "manual_email_send", "email_draft_send"] {
            let error = connection
                .execute(
                    "INSERT INTO idempotency_records
                       (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'fingerprint', 'remote_started', '{}', 10, 10)",
                    rusqlite::params![
                        format!("guarded-{action}"),
                        action,
                        mailbox.mailbox_ref_id.as_str()
                    ],
                )
                .expect_err("deletion guard must reject new email claims");
            assert!(
                error
                    .to_string()
                    .contains("mailbox credential deletion is in progress")
            );
        }
    }
    repository
        .finish_email_auth_connection_deletion(&auth_connection_id)
        .unwrap();
}

#[test]
fn mailbox_snapshot_reconcile_marks_missing_credential_unavailable_and_preserves_email_graph() {
    let (_directory, repository) = repository();
    let stale = MailboxRefInput {
        id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "resend".into(),
        label: "Stale".into(),
        email_address: "stale@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    let current = MailboxRefInput {
        id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
        provider: "resend".into(),
        label: "Current".into(),
        email_address: "current@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    repository
        .reconcile_mailbox_refs(vec![stale.clone(), current.clone()])
        .expect("initial snapshot");
    let creator = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Email Creator".into(),
            notes: None,
        })
        .expect("creator");
    let (thread, emails) = repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: stale.id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: "resend:stale-thread".into(),
            subject: "Stale mailbox".into(),
            messages: vec![EmailMessageInput {
                provider_message_id: "stale-message".into(),
                internet_message_id: Some("<stale@example.com>".into()),
                direction: EmailDirection::Inbound,
                from: "creator@example.com".into(),
                to: vec![stale.email_address.clone()],
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: None,
                references: Vec::new(),
                subject: "Stale mailbox".into(),
                text: Some("Body".into()),
                html: None,
                delivery_status: "received".into(),
                headers: serde_json::json!({}),
                occurred_at: 1,
                origin: crate::models::EmailOrigin::ProviderImport,
                attachments: Vec::new(),
            }],
        })
        .expect("email snapshot");
    let email = &emails[0];
    {
        let connection = repository.lock().expect("repository lock");
        connection
            .execute(
                "INSERT INTO touchpoints
                   (id, creator_id, kind, message_id, email_id, account_relationship_id,
                    occurred_at, metadata_json)
                 VALUES ('stale-touchpoint', ?1, 'email', NULL, ?2, NULL, 2, '{}')",
                rusqlite::params![creator.creator.id.as_str(), email.id.as_str()],
            )
            .expect("email touchpoint");
        connection
            .execute(
                "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                    contact_locator_id, email_id, payload_json, created_at)
                 VALUES ('stale-audit', 'email', ?1, NULL, NULL, ?2, NULL, ?3, '{}', 2)",
                rusqlite::params![
                    creator.creator.id.as_str(),
                    stale.id.as_str(),
                    email.id.as_str()
                ],
            )
            .expect("email audit event");
    }

    let mut unavailable = current;
    unavailable.available = false;
    unavailable.last_error = Some("temporary provider failure".into());
    let mailboxes = repository
        .reconcile_mailbox_refs(vec![unavailable])
        .expect("replacement snapshot");
    assert_eq!(mailboxes.len(), 2);
    let stale_projection = mailboxes
        .iter()
        .find(|mailbox| mailbox.mailbox_ref_id == stale.id)
        .expect("retained stale mailbox");
    assert!(!stale_projection.available);
    assert_eq!(
        stale_projection.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );

    let connection = repository.lock().expect("repository lock");
    let count = |sql: &str, id: &str| {
        connection
            .query_row(sql, [id], |row| row.get::<_, u64>(0))
            .expect("row count")
    };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM mailbox_refs WHERE id = ?1",
            stale.id.as_str()
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM email_threads WHERE id = ?1",
            thread.id.as_str()
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM emails WHERE id = ?1",
            email.id.as_str()
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM touchpoints WHERE id = ?1",
            "stale-touchpoint"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM audit_events WHERE id = ?1",
            "stale-audit"
        ),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM creators WHERE id = ?1",
            creator.creator.id.as_str()
        ),
        1
    );
}

#[test]
fn invalid_mailbox_snapshot_does_not_partially_update_projection() {
    let (_directory, repository) = repository();
    let original = MailboxRefInput {
        id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "resend".into(),
        label: "Original".into(),
        email_address: "original@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    repository
        .reconcile_mailbox_refs(vec![original.clone()])
        .expect("initial snapshot");

    let mut changed = original;
    changed.label = "Changed".into();
    let invalid = MailboxRefInput {
        id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
        provider: "smtp".into(),
        label: "Invalid".into(),
        email_address: "invalid@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    let error = match repository.reconcile_mailbox_refs(vec![changed, invalid]) {
        Ok(_) => panic!("invalid full snapshot must fail"),
        Err(error) => error,
    };
    assert!(error.contains("unsupported mailbox provider"));

    let mailboxes = repository.list_mailbox_refs().expect("mailboxes");
    assert_eq!(mailboxes.len(), 1);
    assert_eq!(mailboxes[0].label, "Original");
}

#[test]
fn mailbox_reconcile_preserves_scoped_idempotency_payloads_for_missing_credential() {
    let (_directory, repository) = repository();
    let stale = MailboxRefInput {
        id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "resend".into(),
        label: "Stale".into(),
        email_address: "stale@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Send],
        available: true,
        last_error: None,
    };
    let current = MailboxRefInput {
        id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
        provider: "resend".into(),
        label: "Current".into(),
        email_address: "current@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Send],
        available: true,
        last_error: None,
    };
    repository
        .reconcile_mailbox_refs(vec![stale.clone(), current.clone()])
        .expect("initial mailbox snapshot");
    {
        let connection = repository.lock().expect("repository lock");
        let insert =
            |key: &str, action: &str, mailbox_ref_id: Option<&MailboxRefId>, result: &str| {
                connection
                    .execute(
                        "INSERT INTO idempotency_records
                       (key, action, mailbox_ref_id, state, result_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'completed', ?4, 1, 1)",
                        rusqlite::params![
                            key,
                            action,
                            mailbox_ref_id.map(MailboxRefId::as_str),
                            result
                        ],
                    )
                    .expect("idempotency record");
            };
        let stale_result = serde_json::json!({
            "thread": { "mailbox_ref_id": stale.id.as_str() },
            "email": { "text": "sensitive stale email body" }
        })
        .to_string();
        let current_result = serde_json::json!({
            "thread": { "mailbox_ref_id": current.id.as_str() },
            "email": { "text": "current email body" }
        })
        .to_string();
        let unrelated_result = serde_json::json!({
            "thread": { "mailbox_ref_id": stale.id.as_str() },
            "relationship": { "value": "yes" }
        })
        .to_string();
        insert("stale-email", "email_send", Some(&stale.id), &stale_result);
        insert(
            "stale-manual-email",
            "manual_email_send",
            Some(&stale.id),
            &stale_result,
        );
        insert(
            "current-email",
            "email_send",
            Some(&current.id),
            &current_result,
        );
        insert(
            "unrelated-action",
            "relationship_update",
            None,
            &unrelated_result,
        );
        insert(
            "malformed-email",
            "email_send",
            Some(&current.id),
            "not-json",
        );
    }

    repository
        .reconcile_mailbox_refs(vec![current])
        .expect("replacement mailbox snapshot");

    let connection = repository.lock().expect("repository lock");
    let mut statement = connection
        .prepare("SELECT key FROM idempotency_records ORDER BY key")
        .expect("idempotency keys");
    let keys = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("idempotency rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("idempotency key values");
    assert_eq!(
        keys,
        vec![
            "current-email",
            "malformed-email",
            "stale-email",
            "stale-manual-email",
            "unrelated-action"
        ]
    );
    let stale_plaintext = connection
        .query_row(
            "SELECT COUNT(*) FROM idempotency_records
             WHERE result_json LIKE '%sensitive stale email body%'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .expect("stale plaintext count");
    assert_eq!(stale_plaintext, 2);
}

#[test]
fn email_thread_ownership_cannot_be_inherited_or_rebound() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Partnerships".into(),
            email_address: "partnerships@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox],
            available: true,
            last_error: None,
        })
        .expect("mailbox");
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            normalized_value: "shared@example.com".into(),
            display_value: "shared@example.com".into(),
        })
        .expect("email locator");
    let first = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "First".into(),
            notes: None,
        })
        .expect("first Creator");
    let first_route = repository
        .link_creator_contact_route(&first.creator.id, &locator.id, Confidence::Exact)
        .expect("first route");
    let second = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Second".into(),
            notes: None,
        })
        .expect("second Creator");
    let second_route = repository
        .insert_historical_shared_contact_route_for_test(
            &second.creator.id,
            &locator.id,
            Confidence::Exact,
        )
        .expect("second route");
    let message = |provider_message_id: &str| EmailMessageInput {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(format!("<{provider_message_id}@example.com>")),
        direction: EmailDirection::Inbound,
        from: "shared@example.com".into(),
        to: vec![mailbox.email_address.clone()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        subject: "Hello".into(),
        text: Some("Body".into()),
        html: None,
        delivery_status: "unread".into(),
        headers: serde_json::json!({}),
        occurred_at: 1,
        origin: crate::models::EmailOrigin::ProviderImport,
        attachments: Vec::new(),
    };
    let owned = EmailSnapshotInput {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        creator_id: Some(first.creator.id.clone()),
        contact_route_id: Some(first_route.id.clone()),
        provider_thread_key: "resend:owned-thread".into(),
        subject: "Hello".into(),
        messages: vec![message("owned-1")],
    };
    let (thread, messages) = repository
        .persist_email_snapshot(owned.clone())
        .expect("owned thread");
    assert_eq!(messages.len(), 1);

    let mut unowned = owned.clone();
    unowned.creator_id = None;
    unowned.contact_route_id = None;
    unowned.messages = vec![message("unowned-1")];
    let error = repository
        .persist_email_snapshot(unowned)
        .expect_err("unowned input must not inherit an owner");
    assert!(error.contains("unowned email snapshot"));

    let mut rebound = owned.clone();
    rebound.creator_id = Some(second.creator.id);
    rebound.contact_route_id = Some(second_route.id);
    rebound.messages = vec![message("rebound-1")];
    let error = repository
        .persist_email_snapshot(rebound)
        .expect_err("thread must not be rebound");
    assert!(error.contains("cannot be rebound"));

    repository
        .persist_email_snapshot(owned)
        .expect("same-owner replay remains idempotent");
    let stored_thread = repository
        .get_email_thread_record(&thread.id)
        .expect("stored thread");
    let stored_messages = repository
        .get_email_thread_messages(&thread.id, 10, None)
        .expect("stored messages")
        .items;
    assert_eq!(stored_thread.creator_id, Some(first.creator.id));
    assert_eq!(stored_thread.contact_route_id, Some(first_route.id));
    assert_eq!(stored_messages.len(), 1);
    assert_eq!(stored_messages[0].provider_message_id, "owned-1");
}
