use std::path::Path;

use super::*;

#[test]
fn v34_tags_upgrade_preserves_ids_pending_fingerprints_and_replay_snapshots() {
    let (directory, repository) = repository();
    let record = repository
        .create_creator(CreateCreatorInput {
            display_name: "Released Creator".into(),
            notes: Some("keep notes".into()),
            tags: None,
        })
        .unwrap();
    let creator_id = record.creator.id.clone();
    let mut snapshot = serde_json::to_value(record.creator).unwrap();
    snapshot.as_object_mut().unwrap().remove("tags");
    let payload = serde_json::json!({"confirmation_fingerprint":"keep-confirmation",
        "result":{"creator":{"creator":snapshot}}})
    .to_string();
    let path = directory.path().join("crm.db");
    drop(repository);
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v34(&connection).unwrap();
    let batch = serde_json::json!({"outcomes":[{"kind":"accepted", "input_index":0,
        "creator":{"creator_id":creator_id, "display_name":"Released Creator", "stage":"prospect", "created_at":1, "contact_routes":[]}}], "replayed":false}).to_string();
    connection.execute("INSERT INTO idempotency_records
        (key, action, request_fingerprint, state, result_json, created_at, updated_at)
        VALUES ('batch-replayed', 'creator_batch_create', 'keep-batch-request', 'completed', ?1, 1, 2)", [&batch]).unwrap();
    connection.execute("INSERT INTO idempotency_records
        (key, action, request_fingerprint, state, result_json, created_at, updated_at)
        VALUES ('replayed', 'creator_candidate_save_confirmation', 'keep-request', 'completed', ?1, 1, 2)", [&payload]).unwrap();
    let pending = "{\"quality\":{},\"notes\":\"preserve pending bytes\"}";
    connection.execute("INSERT INTO idempotency_records
        (key, action, request_fingerprint, state, result_json, created_at, updated_at)
        VALUES ('pending', 'creator_candidate_save_confirmation', 'pending-fingerprint', 'in_progress', ?1, 3, 4)", [pending]).unwrap();
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 34,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
    assert_eq!(backup_count(directory.path()), 0);
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(backup_count(directory.path()), 1);
    assert_eq!(upgraded.impact.removed_creator_save_confirmations, 2);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM idempotency_records WHERE key IN ('replayed','pending')",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0,
        "v37 removes the retired confirmations"
    );
    // The real recovery point retains the v34 bytes. Reconstruct only the
    // immediate v35 successor to keep proving the released tags edge's
    // fingerprint and replay conversion before the later retirement.
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key='replayed'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        payload
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key='pending'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        pending
    );
    let mut v35_reference = Connection::open_in_memory().unwrap();
    {
        let copy = rusqlite::backup::Backup::new(&backup, &mut v35_reference).unwrap();
        copy.run_to_completion(100, std::time::Duration::from_millis(1), None)
            .unwrap();
    }
    v35_reference
        .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&v35_reference, 34, 35).unwrap();
    v35_reference
        .execute_batch("COMMIT; PRAGMA foreign_keys=ON;")
        .unwrap();
    let replay: String = v35_reference
        .query_row(
            "SELECT result_json FROM idempotency_records WHERE key='replayed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let replay: serde_json::Value = serde_json::from_str(&replay).unwrap();
    let replayed: crate::Creator =
        serde_json::from_value(replay["result"]["creator"]["creator"].clone()).unwrap();
    assert_eq!(replayed.id, creator_id);
    assert!(replayed.tags.is_empty());
    assert_eq!(replay["confirmation_fingerprint"], "keep-confirmation");
    assert_eq!(
        v35_reference
            .query_row(
                "SELECT request_fingerprint FROM idempotency_records WHERE key='replayed'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "keep-request"
    );
    assert_eq!(
        v35_reference
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key='pending'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        pending
    );
    let reopened = SqliteRepository::open(&path).unwrap();
    let batch: String = connection
        .query_row(
            "SELECT result_json FROM idempotency_records WHERE key='batch-replayed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let batch: crate::ManualCreatorBatchCreateResult = serde_json::from_str(&batch).unwrap();
    assert!(
        matches!(&batch.outcomes[0], crate::ManualCreatorBatchItemOutcome::Accepted { creator, .. } if creator.creator_id == creator_id && creator.tags.is_empty())
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT request_fingerprint FROM idempotency_records WHERE key='batch-replayed'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "keep-batch-request"
    );
    assert_eq!(
        reopened
            .get_creator(&creator_id)
            .unwrap()
            .creator
            .notes
            .as_deref(),
        Some("keep notes")
    );
    assert!(
        reopened
            .get_creator(&creator_id)
            .unwrap()
            .creator
            .tags
            .is_empty()
    );
}

#[test]
fn malformed_v34_replay_blocks_before_backup_or_tags_writes() {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v34(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO idempotency_records
        (key, action, state, result_json, created_at, updated_at)
        VALUES ('broken', 'creator_candidate_save_confirmation', 'completed', '{}', 1, 1);",
        )
        .unwrap();
    assert!(crate::CreatorRuntime::upgrade_database(&path).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        34
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('creators') WHERE name='tags_json'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(backup_count(directory.path()), 0);
}

fn seeded_v27_task_retirement_impact() -> crate::CreatorDatabaseUpgradeImpact {
    crate::CreatorDatabaseUpgradeImpact {
        deleted_tasks: 1,
        deleted_task_runs: 1,
        ..Default::default()
    }
}

fn assert_retired_task_schema_absent(connection: &Connection) {
    let retired_objects: u64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('tasks','task_runs','task_attention')
         OR tbl_name IN ('tasks','task_runs','task_attention')",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(retired_objects, 0);
    let task_columns: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('reply_triage') WHERE name='task_run_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(task_columns, 0);
}

fn seeded_v33_task_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v33(&connection).unwrap();
    connection.execute_batch(r#"
        INSERT INTO campaigns(id,name,status,goal,platforms_json,created_at,updated_at)
          VALUES ('campaign-v33','Preserved campaign','reviewing','Preserved goal','["x"]',1,2);
        INSERT INTO tasks(id,name,campaign_id,instruction,schedule_json,status,created_by,created_at,updated_at,kind)
          VALUES ('task-v33','Retired task','campaign-v33','Retire work order','{}','active','agent',1,2,'custom');
        INSERT INTO task_runs(id,task_id,scheduled_for,state,idempotency_key,claimed_at,lease_expires_at,report_summary,evidence_json,finished_at,origin)
          VALUES ('run-v33','task-v33',1,'reported_failed','claim-v33',1,2,'Retired failure','{}',2,'external');
        INSERT INTO task_attention(id,task_id,kind,task_run_id,occurrence_key,occurred_at)
          VALUES ('attention-v33','task-v33','run_failed','run-v33','run-v33',2);
        INSERT INTO idempotency_records(key,action,state,result_json,created_at,updated_at)
          VALUES ('task-receipt','task_create','completed','{"id":"task-v33"}',1,2),
                 ('manual-receipt','manual_email_send','completed','{"preserved":true}',1,2),
                 ('other-receipt','task_other','completed','{"preserved":true}',1,2);
        INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at)
          VALUES ('account-v33','x','profile','Preserved account',1,2);
        INSERT INTO resolved_targets(id,account_ref_id,platform,target_kind,external_target_id,created_at,updated_at)
          VALUES ('target-v33','account-v33','x','user','user-v33',1,2);
        INSERT INTO conversations(id,account_ref_id,resolved_target_id,channel,provider_thread_id,created_at,updated_at)
          VALUES ('conversation-v33','account-v33','target-v33','x','thread-v33',1,2);
        INSERT INTO messages(id,conversation_id,external_message_id,direction,body,status,sent_at)
          VALUES ('message-v33','conversation-v33','message-v33','inbound','Preserved body','received',2);
        INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at)
          VALUES ('mailbox-v33','resend','Preserved mailbox','fixture@example.invalid','{}',1,1,2);
        INSERT INTO email_threads(id,mailbox_ref_id,provider_thread_key,subject,active_folder,created_at,updated_at)
          VALUES ('thread-v33','mailbox-v33','thread-v33','Preserved subject','inbox',1,2);
        INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,body_text,references_json,delivery_status,headers_json,occurred_at,origin)
          VALUES ('email-v33','thread-v33','email-v33','inbound','Preserved subject','Preserved email','[]','received','{}',2,'provider_import');
        INSERT INTO reply_triage(id,channel,email_id,message_id,intent,summary,suggested_reply,task_run_id,created_at,acknowledged_at)
          VALUES ('triage-dm-v33','dm',NULL,'message-v33','question','Preserved judgment','Preserved suggested reply','run-v33',3,4),
                 ('triage-email-v33','email','email-v33',NULL,'interested','Preserved email judgment',NULL,NULL,3,NULL);
    "#).unwrap();
    drop(connection);
    (directory, path)
}

fn reply_business_snapshot(connection: &Connection) -> Vec<String> {
    let mut statement = connection.prepare(
        "SELECT json_array(id,channel,email_id,message_id,intent,summary,suggested_reply,created_at,acknowledged_at)
         FROM reply_triage ORDER BY id"
    ).unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<Vec<String>, _>>()
        .unwrap()
}

#[test]
fn v33_upgrade_removes_task_storage_preserves_replies_and_backs_up_wal_rows() {
    let (directory, path) = seeded_v33_task_database();
    let source = Connection::open(&path).unwrap();
    source
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    source.execute_batch(
        "INSERT INTO tasks(id,instruction,schedule_json,status,created_by,created_at,updated_at,kind)
         VALUES ('task-wal','Committed WAL task','{}','paused','ui',5,5,'custom');"
    ).unwrap();
    let before = reply_business_snapshot(&source);
    // The v33 fixture's one Campaign row is retired by the 35 -> 36 edge of
    // the same chain.
    let expected = crate::CreatorDatabaseUpgradeImpact {
        deleted_tasks: 2,
        deleted_task_runs: 1,
        deleted_task_attention: 1,
        deleted_task_idempotency_records: 1,
        cleared_reply_triage_task_refs: 1,
        removed_campaigns: 1,
        ..Default::default()
    };
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 33,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: expected.clone(),
        }
    );
    assert_eq!(
        backup_count(directory.path()),
        0,
        "preflight must not write"
    );
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(upgraded.impact, expected);
    assert_eq!(backup_count(directory.path()), 1);
    assert_retired_task_schema_absent(&source);
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM idempotency_records WHERE action='task_create'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(source.query_row("SELECT COUNT(*) FROM idempotency_records WHERE key IN ('manual-receipt','other-receipt') AND result_json='{\"preserved\":true}'", [], |row| row.get::<_, u64>(0)).unwrap(), 2);
    assert_eq!(reply_business_snapshot(&source), before);
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'campaigns'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0,
        "the retired Campaign row leaves with its table"
    );
    assert_eq!(
        source
            .query_row(
                "SELECT body FROM messages WHERE id='message-v33'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Preserved body"
    );
    assert_eq!(
        source
            .query_row(
                "SELECT body_text FROM emails WHERE id='email-v33'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Preserved email"
    );
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key='task-receipt'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "{\"id\":\"task-v33\"}"
    );
    assert_eq!(
        backup
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT task_run_id FROM reply_triage WHERE id='triage-dm-v33'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "run-v33"
    );
    assert_eq!(reply_business_snapshot(&backup), before);
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );
}

#[test]
fn v33_task_retirement_validation_failure_rolls_back_all_deletions() {
    let (_directory, path) = seeded_v33_task_database();
    let before = reply_business_snapshot(&Connection::open(&path).unwrap());
    let error = schema::upgrade_with_post_migration_mutation_for_test(&path, |transaction| {
        transaction
            .execute_batch("CREATE TABLE unexpected_retirement_drift(id TEXT)")
            .unwrap();
    })
    .unwrap_err();
    assert!(!error.storage_upgraded);
    assert_eq!(error.support_code, "LOCAL_DATA_VALIDATION_FAILED");
    let source = Connection::open(&path).unwrap();
    assert_eq!(
        source
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        33
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM task_attention", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM task_runs", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(reply_business_snapshot(&source), before);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 33,
            ..
        }
    ));
}

#[test]
fn v33_task_retirement_rejects_changed_destructive_impact_before_backup() {
    let (directory, path) = seeded_v33_task_database();
    let concurrent_path = path.clone();
    let error = schema::upgrade_with_before_lock_for_test(&path, move || {
        Connection::open(concurrent_path).unwrap().execute_batch(
            "INSERT INTO tasks(id,instruction,schedule_json,status,created_by,created_at,updated_at,kind)
             VALUES ('concurrent-task','Concurrent task','{}','paused','ui',5,5,'custom');"
        ).unwrap();
    }).unwrap_err();
    assert_eq!(error.support_code, "LOCAL_DATA_SOURCE_CHANGED");
    assert!(!error.storage_upgraded);
    assert_eq!(backup_count(directory.path()), 0);
    let source = Connection::open(&path).unwrap();
    assert_eq!(
        source
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        33
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, u64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn empty_database_is_initialized_to_the_only_supported_schema() {
    let (directory, _) = repository();
    let connection = Connection::open(directory.path().join("crm.db")).expect("open db");
    assert_retired_task_schema_absent(&connection);
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .expect("schema version");
    assert_eq!(version, schema::CURRENT_SCHEMA_VERSION);
    let route_columns = connection
        .prepare("PRAGMA table_info(creator_contact_routes)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!route_columns.iter().any(|column| column == "state"));
    for table in [
        "creator_contact_route_dependencies",
        "creator_contact_route_delete_operations",
        "conversation_contact_routes",
    ] {
        assert!(
            connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap(),
            "missing v13 table {table}"
        );
    }
    for table in ["touchpoints", "audit_events", "idempotency_records"] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(
            columns.iter().any(|column| column == "contact_route_id"),
            "{table} is missing typed route attribution"
        );
    }
    let on_delete = connection
        .query_row(
            "SELECT on_delete FROM pragma_foreign_key_list('creator_origins')
             WHERE \"from\" = 'source_contact_route_id'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert_eq!(on_delete, "SET NULL");
    for table in ["creator_candidate_snapshots", "tiktok_one_account_bindings"] {
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name=?1",
                    [table],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    }
    let lead_table_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='leads')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .expect("lead table check");
    assert!(!lead_table_exists);
}

#[test]
fn historical_v33_task_runs_reject_retired_origins_and_chat_sessions() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v33(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO tasks
               (id, instruction, schedule_json, status, created_by, created_at, updated_at, kind)
             VALUES ('task-v31-check', 'check', '{}', 'active', 'ui', 1, 1, 'custom');
             INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at, lease_expires_at,
                report_summary, evidence_json, finished_at, origin, chat_session_id)
             VALUES ('run-v31-good', 'task-v31-check', 1, 'reported_done', 'v31-good', 1, 2,
                     'done', '{}', 2, 'external', NULL);",
        )
        .unwrap();
    for (origin, session, key) in [
        ("envoy", "NULL", "v31-envoy"),
        ("dispatcher", "NULL", "v31-dispatcher"),
        ("external", "'session-retired'", "v31-session"),
    ] {
        let sql = format!(
            "INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at, lease_expires_at,
                report_summary, evidence_json, finished_at, origin, chat_session_id)
             VALUES ('run-{key}', 'task-v31-check', 1, 'reported_done', '{key}', 1, 2,
                     'done', '{{}}', 2, '{origin}', {session})"
        );
        connection
            .execute(&sql, [])
            .expect_err("retired origin/session must be rejected");
    }
}

#[test]
fn v30_to_v31_purges_retired_origins_and_preserves_external_data() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v30(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO tasks
               (id, instruction, schedule_json, status, created_by, created_at, updated_at, kind)
             VALUES ('task-v30-origins', 'migrate', '{}', 'active', 'ui', 1, 1, 'custom');
             INSERT INTO creators
               (id, display_name, created_at, updated_at, follower_counts_json)
             VALUES ('creator-v30', 'Preserved Creator', 1, 1, '{}');
             INSERT INTO creator_relationships
               (creator_id, stage, created_at, updated_at)
             VALUES ('creator-v30', 'engaged', 1, 1);
             INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available, created_at, updated_at)
             VALUES ('mailbox-v30', 'resend', 'Preserved mailbox', 'mailbox-v30@example.invalid', '{}', 1, 1, 1);
             INSERT INTO email_threads
               (id, mailbox_ref_id, creator_id, provider_thread_key, subject, active_folder, created_at, updated_at)
             VALUES ('thread-v30', 'mailbox-v30', 'creator-v30', 'provider-thread-v30', 'Preserved email', 'inbox', 1, 1);
             INSERT INTO emails
               (id, thread_id, provider_message_id, direction, subject, body_text, references_json,
                delivery_status, headers_json, occurred_at, origin)
             VALUES ('email-v30', 'thread-v30', 'provider-message-v30', 'inbound', 'Preserved email',
                     'Preserved body', '[]', 'received', '{}', 5, 'provider_import');
             INSERT INTO account_refs
               (id, platform, kind, label, created_at, updated_at)
             VALUES ('account-v30', 'x', 'profile', 'v30 account', 1, 1);
             INSERT INTO resolved_targets
               (id, account_ref_id, platform, target_kind, external_target_id, created_at, updated_at)
             VALUES ('target-v30', 'account-v30', 'x', 'user', 'target-v30', 1, 1);
             INSERT INTO conversations
               (id, account_ref_id, resolved_target_id, channel, provider_thread_id, created_at, updated_at)
             VALUES ('conversation-v30', 'account-v30', 'target-v30', 'x', 'thread-v30', 1, 1);
             INSERT INTO messages
               (id, conversation_id, external_message_id, direction, body, status, sent_at)
             VALUES ('message-v30', 'conversation-v30', 'message-v30', 'inbound', 'hello', 'received', 6);
             INSERT INTO messages
               (id, conversation_id, external_message_id, direction, body, status, sent_at)
             VALUES ('message-v30-external', 'conversation-v30', 'message-v30-external', 'inbound', 'hello', 'received', 7);
             INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at, lease_expires_at,
                report_summary, evidence_json, finished_at, origin, chat_session_id)
             VALUES ('run-v30-envoy-1', 'task-v30-origins', 1, 'reported_failed', 'key-envoy-1', 2, 3,
                     'envoy failure 1', '{\"source\":\"envoy\"}', 4, 'envoy', 'chat-1'),
                    ('run-v30-envoy-2', 'task-v30-origins', 2, 'reported_failed', 'key-envoy-2', 3, 4,
                     'envoy failure 2', '{\"source\":\"envoy\"}', 5, 'envoy', 'chat-2'),
                    ('run-v30-envoy-3', 'task-v30-origins', 3, 'reported_failed', 'key-envoy-3', 4, 5,
                     'envoy failure 3', '{\"source\":\"envoy\"}', 6, 'envoy', 'chat-3'),
                    ('run-v30-envoy-4', 'task-v30-origins', 4, 'reported_failed', 'key-envoy-4', 5, 6,
                     'envoy failure 4', '{\"source\":\"envoy\"}', 7, 'envoy', 'chat-4'),
                    ('run-v30-envoy-5', 'task-v30-origins', 5, 'reported_failed', 'key-envoy-5', 6, 7,
                     'envoy failure 5', '{\"source\":\"envoy\"}', 8, 'envoy', 'chat-5'),
                    ('run-v30-external', 'task-v30-origins', 6, 'expired', 'key-external', 7, 8,
                     NULL, NULL, 9, 'external', NULL);
             INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, occurred_at)
             VALUES ('attention-v30-1', 'task-v30-origins', 'run_failed', 'run-v30-envoy-1',
                     'run_failed:run-v30-envoy-1', 10),
                    ('attention-v30-2', 'task-v30-origins', 'run_failed', 'run-v30-envoy-2',
                     'run_failed:run-v30-envoy-2', 11),
                    ('attention-v30-3', 'task-v30-origins', 'run_failed', 'run-v30-envoy-3',
                     'run_failed:run-v30-envoy-3', 12),
                    ('attention-v30-4', 'task-v30-origins', 'run_failed', 'run-v30-envoy-4',
                     'run_failed:run-v30-envoy-4', 13),
                    ('attention-v30-5', 'task-v30-origins', 'run_failed', 'run-v30-envoy-5',
                     'run_failed:run-v30-envoy-5', 14);",
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO reply_triage
               (id, channel, message_id, intent, summary, suggested_reply, task_run_id,
                created_at, acknowledged_at)
             VALUES ('triage-v30', 'dm', 'message-v30', 'interested',
                     'preserved triage', 'preserved reply', 'run-v30-envoy-1', 15, NULL)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO reply_triage
               (id, channel, message_id, intent, summary, suggested_reply, task_run_id,
                created_at, acknowledged_at)
             VALUES ('triage-v30-external', 'dm', 'message-v30-external', 'question',
                     'retained triage', 'retained reply', 'run-v30-external', 9, NULL)",
            [],
        )
        .unwrap();
    let retired_before = connection
        .query_row(
            "SELECT COUNT(*) FROM task_runs WHERE origin = 'envoy'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(retired_before, 5);
    drop(connection);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&connection, 30, 31).unwrap();
    connection
        .execute_batch("PRAGMA user_version=31; COMMIT; PRAGMA foreign_keys=ON")
        .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*), GROUP_CONCAT(origin), COUNT(chat_session_id) FROM task_runs",
                [],
                |row| Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?
                )),
            )
            .unwrap(),
        (1, "external".to_string(), 0)
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM task_runs WHERE origin IN ('envoy', 'dispatcher')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    let retired_after = connection
        .query_row(
            "SELECT COUNT(*) FROM task_runs WHERE origin IN ('envoy', 'dispatcher')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let deleted_retired_count = retired_before - retired_after;
    assert_eq!(deleted_retired_count, 5);
    for (table, expected) in [
        ("tasks", 1_i64),
        ("creators", 1_i64),
        ("creator_relationships", 1_i64),
        ("account_refs", 1_i64),
        ("resolved_targets", 1_i64),
        ("conversations", 1_i64),
        ("messages", 2_i64),
        ("mailbox_refs", 1_i64),
        ("email_threads", 1_i64),
        ("emails", 1_i64),
    ] {
        let count = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert_eq!(count, expected, "{table} facts must survive migration");
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT id, scheduled_for, state, evidence_json, idempotency_key
                 FROM task_runs WHERE id = 'run-v30-external'",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?
                )),
            )
            .unwrap(),
        (
            "run-v30-external".into(),
            6,
            "expired".into(),
            None,
            "key-external".into()
        )
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT task_run_id FROM reply_triage WHERE id = 'triage-v30-external'",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap(),
        Some("run-v30-external".into())
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM task_attention WHERE id = 'attention-v30-1'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT id, channel, summary, suggested_reply, task_run_id, created_at, acknowledged_at
                 FROM reply_triage WHERE id = 'triage-v30'",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?, row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?, row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?
                )),
            )
            .unwrap(),
        (
            "triage-v30".into(), "dm".into(), "preserved triage".into(),
            "preserved reply".into(), None, 15, None
        )
    );
    for index in [
        "reply_triage_one_per_email",
        "reply_triage_one_per_message",
        "reply_triage_open",
        "task_attention_open",
        "task_attention_recency",
        "task_attention_by_task_recency",
    ] {
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1,
            "missing preserved index {index}"
        );
    }
    connection
        .execute(
            "INSERT INTO reply_triage
               (id, channel, message_id, intent, summary, suggested_reply, created_at)
             VALUES ('triage-duplicate', 'dm', 'message-v30', 'question', 'duplicate', 'duplicate', 9)",
            [],
        )
        .expect_err("message uniqueness must survive the rebuild");
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 31,
            ..
        }
    ));
}

#[test]
fn production_connection_configures_a_busy_timeout() {
    let (_directory, repository) = repository();
    let timeout_ms = repository
        .lock()
        .unwrap()
        .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
        .expect("busy timeout");
    assert_eq!(timeout_ms, 5_000);
}

#[test]
fn route_dependencies_reject_cycles_restrict_parents_and_cascade_children() {
    let (_directory, path) = current_database();
    let connection = Connection::open(path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    connection
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at)
             VALUES ('creator-1', 'Creator', NULL, 1, 1, NULL);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES ('locator-a', 'website', 'url', 'https://a.example', 'A', 1, 1),
                    ('locator-b', 'website', 'url', 'https://b.example', 'B', 1, 1),
                    ('locator-c', 'website', 'url', 'https://c.example', 'C', 1, 1);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES ('route-a', 'creator-1', 'locator-a', 'exact', 1, 1),
                    ('route-b', 'creator-1', 'locator-b', 'exact', 1, 1),
                    ('route-c', 'creator-1', 'locator-c', 'exact', 1, 1);
             INSERT INTO creator_contact_route_dependencies
               (parent_contact_route_id, child_contact_route_id, kind, created_at)
             VALUES ('route-a', 'route-b', 'link_hub', 1),
                    ('route-b', 'route-c', 'link_hub', 1);",
        )
        .unwrap();
    let cycle = connection
        .execute(
            "INSERT INTO creator_contact_route_dependencies
               (parent_contact_route_id, child_contact_route_id, kind, created_at)
             VALUES ('route-c', 'route-a', 'link_hub', 1)",
            [],
        )
        .unwrap_err();
    assert!(cycle.to_string().contains("dependency cycle"));
    assert!(
        connection
            .execute(
                "DELETE FROM creator_contact_routes WHERE id = 'route-a'",
                []
            )
            .is_err(),
        "a parent route must be removed only after all children"
    );
    connection
        .execute(
            "DELETE FROM creator_contact_routes WHERE id = 'route-c'",
            [],
        )
        .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM creator_contact_route_dependencies
                 WHERE child_contact_route_id = 'route-c'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0,
        "deleting a child must cascade only its dependency rows"
    );
}

#[test]
fn schema_v5_database_is_rejected_without_migration_or_fallback() {
    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("crm.db");
    let connection = Connection::open(&path).expect("open legacy db");
    connection
        .execute_batch(
            "CREATE TABLE leads (id TEXT PRIMARY KEY);
             PRAGMA user_version = 5;",
        )
        .expect("legacy schema");
    drop(connection);
    let error = match SqliteRepository::open(&path) {
        Ok(_) => panic!("legacy database must be rejected"),
        Err(error) => error,
    };
    assert!(
        error.contains(&format!(
            "schema version 5; expected {}",
            schema::CURRENT_SCHEMA_VERSION
        )),
        "{error}"
    );
    assert!(error.contains("keep the existing data unchanged"));
}

#[test]
fn current_schema_is_accepted_after_reopen() {
    let (_directory, path) = current_database();
    SqliteRepository::open(path).expect("current schema must remain valid");
}

#[test]
fn current_schema_inspect_and_normal_open_do_not_write_or_migrate() {
    let (directory, path) = current_database();
    let observer = Connection::open(&path).expect("open current observer");
    let before_data_version = observer
        .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
        .expect("current data version");
    let before_user_version = observer
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .expect("current schema version");

    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );
    drop(SqliteRepository::open(&path).expect("normal current open"));

    assert_eq!(
        observer
            .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
            .expect("unchanged data version"),
        before_data_version,
        "current inspect/open must not write"
    );
    assert_eq!(
        observer
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .expect("unchanged schema version"),
        before_user_version
    );
    assert_eq!(backup_count(directory.path()), 0);
}

#[test]
fn current_schema_rejects_an_archive_marker_without_inbound_membership() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('mailbox-drift', 'resend', 'Drift', 'drift@example.com', '{}', 1, 1, 1);
             INSERT INTO email_threads
               (id, mailbox_ref_id, provider_thread_key, subject, active_folder,
                archived_at, created_at, updated_at)
             VALUES ('thread-drift', 'mailbox-drift', 'drift', 'Drift', 'sent',
                     123, 1, 1);",
        )
        .unwrap();
    let error = match SqliteRepository::open(&path) {
        Ok(_) => panic!("archive marker drift must fail closed"),
        Err(error) => error,
    };
    assert!(
        error.contains("archive marker without an inbound message"),
        "{error}"
    );
}

fn backup_count(directory: &std::path::Path) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
        .count()
}

fn structurally_valid_v26_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v27(&connection).unwrap();
    connection
        .execute_batch(
            "DROP TABLE task_attention;
             DROP INDEX task_runs_task_identity;
             PRAGMA user_version = 26;",
        )
        .unwrap();
    drop(connection);
    (directory, path)
}

#[test]
fn v26_and_future_versions_are_unsupported_without_backup_or_write() {
    let (v26_directory, v26_path) = structurally_valid_v26_database();
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&v26_path),
        crate::CreatorDatabaseStatus::Unsupported { version: 26, .. }
    ));
    let error = crate::CreatorRuntime::upgrade_database(&v26_path).unwrap_err();
    assert_eq!(
        error.kind,
        crate::CreatorDatabaseUpgradeFailureKind::UnsafeData
    );
    assert!(!error.storage_upgraded);
    assert_eq!(backup_count(v26_directory.path()), 0);
    assert_eq!(
        Connection::open(&v26_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        26
    );

    let (future_directory, future_path) = current_database();
    Connection::open(&future_path)
        .unwrap()
        .pragma_update(None, "user_version", 99_i64)
        .unwrap();
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&future_path),
        crate::CreatorDatabaseStatus::Unsupported { version: 99, .. }
    ));
    let error = crate::CreatorRuntime::upgrade_database(&future_path).unwrap_err();
    assert_eq!(
        error.kind,
        crate::CreatorDatabaseUpgradeFailureKind::UnsafeData
    );
    assert!(!error.storage_upgraded);
    assert_eq!(backup_count(future_directory.path()), 0);
    assert_eq!(
        Connection::open(&future_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        99
    );
}

#[test]
fn backup_creation_failure_leaves_v27_source_untouched() {
    let (directory, path) = seeded_v27_creator_database();
    let source = Connection::open(&path).unwrap();
    let invalid_backup_target = directory.path().join("backup-target-is-a-directory");
    std::fs::create_dir(&invalid_backup_target).unwrap();
    let error = schema::create_online_backup(&source, &invalid_backup_target)
        .expect_err("directory target must reject online backup creation");
    assert!(error.to_ascii_lowercase().contains("backup"), "{error}");
    drop(source);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
}

#[test]
fn invalid_v27_recovery_point_is_removed_before_migration_starts() {
    let (directory, path) = seeded_v27_creator_database();
    let error = schema::upgrade_with_invalid_backup_for_test(&path).unwrap_err();
    assert_eq!(
        error.kind,
        crate::CreatorDatabaseUpgradeFailureKind::BackupFailed
    );
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
    assert_eq!(backup_count(directory.path()), 0);
}

#[test]
fn v27_upgrade_fails_cleanly_while_another_writer_holds_the_database() {
    let (directory, path) = seeded_v27_creator_database();
    let blocker = Connection::open(&path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let error = crate::CreatorRuntime::upgrade_database(&path)
        .expect_err("concurrent writer must prevent upgrade");
    assert_eq!(error.kind, crate::CreatorDatabaseUpgradeFailureKind::Busy);
    assert!(!error.storage_upgraded);
    blocker.execute_batch("ROLLBACK").unwrap();
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
    assert_eq!(backup_count(directory.path()), 0);
}

#[test]
fn insufficient_space_creates_no_v27_backup_or_partial_migration() {
    let (directory, path) = seeded_v27_creator_database();
    let error = schema::upgrade_with_available_space_for_test(&path, 0).unwrap_err();
    assert_eq!(
        error.kind,
        crate::CreatorDatabaseUpgradeFailureKind::InsufficientSpace
    );
    assert!(!error.storage_upgraded);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
    assert_eq!(backup_count(directory.path()), 0);
}

#[test]
fn commit_before_v27_write_lock_is_in_the_locked_recovery_snapshot() {
    let (_directory, path) = seeded_v27_creator_database();
    let writer_path = path.clone();
    let result = schema::upgrade_with_before_lock_for_test(&path, move || {
        Connection::open(writer_path)
            .unwrap()
            .execute_batch(
                "BEGIN IMMEDIATE;
                 INSERT INTO creators
                   (id, display_name, notes, created_at, updated_at, archived_at,
                    score, assessment, follower_counts_json, pinned_at)
                 VALUES ('gap-creator', 'Gap Creator', NULL, 9, 9, NULL,
                         NULL, NULL, '{}', NULL);
                 INSERT INTO creator_relationships
                   (creator_id, stage, last_touch_at, created_at, updated_at)
                 VALUES ('gap-creator', 'prospect', NULL, 9, 9);
                 COMMIT;",
            )
            .unwrap();
    })
    .unwrap();
    for source in [&path, Path::new(&result.backup_path)] {
        assert_eq!(
            Connection::open(source)
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM creators WHERE id = 'gap-creator'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1,
            "pre-lock commit must be in upgraded data and exact recovery point"
        );
    }
}

#[test]
fn same_version_schema_with_legacy_table_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("CREATE TABLE leads (id TEXT PRIMARY KEY)", [])
        .expect("pollute schema");

    let error = current_schema_error(&path);
    assert!(error.contains("unexpected: leads"), "{error}");
    assert!(
        error.contains("keep the existing data unchanged"),
        "{error}"
    );
}

#[test]
fn same_version_schema_with_missing_column_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("ALTER TABLE creators DROP COLUMN notes", [])
        .expect("remove current column");

    let error = current_schema_error(&path);
    assert!(
        error.contains("column definition differs for table creators"),
        "{error}"
    );
}

#[test]
fn same_version_schema_with_forbidden_lead_id_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("ALTER TABLE conversations ADD COLUMN lead_id TEXT", [])
        .expect("add legacy column");

    let error = current_schema_error(&path);
    assert!(error.contains("forbidden legacy column"), "{error}");
}

#[test]
fn same_version_schema_with_retired_campaign_id_column_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("ALTER TABLE conversations ADD COLUMN campaign_id TEXT", [])
        .expect("add retired attribution column");

    let error = current_schema_error(&path);
    assert!(
        error.contains("campaign_id is not allowed on table conversations"),
        "{error}"
    );
}

#[test]
fn same_version_schema_with_wrong_foreign_key_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute_batch(
            "ALTER TABLE creator_relationships RENAME TO creator_relationships_old;
             CREATE TABLE creator_relationships (
               creator_id TEXT PRIMARY KEY REFERENCES creator_groups(id) ON DELETE RESTRICT,
               stage TEXT NOT NULL CHECK(stage IN ('prospect','engaged','collaborating','archived')),
               last_touch_at INTEGER,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             DROP TABLE creator_relationships_old;",
        )
        .expect("replace foreign key");

    let error = current_schema_error(&path);
    assert!(
        error.contains("foreign key definition differs for table creator_relationships"),
        "{error}"
    );
}

#[test]
fn same_version_schema_with_missing_required_index_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("DROP INDEX platform_account_observations_latest", [])
        .expect("drop current index");

    let error = current_schema_error(&path);
    assert!(
        error.contains("platform_account_observations_latest"),
        "{error}"
    );
}

#[test]
fn fresh_current_schema_contains_v30_locator_uniqueness_and_the_v29_state() {
    let (_directory, path) = current_database();
    let connection = Connection::open(path).expect("open current db");
    let expected = [
        "creator_contact_routes_one_per_locator",
        "creator_origins_by_creator_recency",
        "email_threads_by_creator",
        "locator_observations_by_creator_source",
        "locator_observations_by_locator_recency",
        "locator_observations_by_platform_account_source",
        "touchpoints_by_creator_recency",
    ];
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'index' AND name IN (
               'creator_origins_by_creator_recency',
               'creator_contact_routes_one_per_locator',
               'email_threads_by_creator',
               'locator_observations_by_creator_source',
               'locator_observations_by_locator_recency',
               'locator_observations_by_platform_account_source',
               'touchpoints_by_creator_recency'
             ) ORDER BY name",
        )
        .expect("v29 index query");
    let actual = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("v29 indexes")
        .collect::<Result<Vec<_>, _>>()
        .expect("v29 index names");
    assert_eq!(actual, expected);
    let columns = connection
        .prepare("PRAGMA table_info(mailbox_sync_incremental_continuations)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        columns,
        vec![
            "mailbox_ref_id",
            "direction",
            "next_cursor",
            "newest_provider_message_id",
            "newest_occurred_at",
            "updated_at",
        ]
    );
    let required_columns: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('mailbox_sync_incremental_continuations')
             WHERE name IN (
               'mailbox_ref_id', 'direction', 'next_cursor',
               'newest_provider_message_id', 'newest_occurred_at', 'updated_at'
             ) AND \"notnull\" = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(required_columns, 6);
    let foreign_keys = connection
        .prepare(
            "SELECT \"from\", \"to\", \"table\", on_delete
             FROM pragma_foreign_key_list('mailbox_sync_incremental_continuations')
             ORDER BY seq",
        )
        .unwrap()
        .query_map([], |row| {
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
        foreign_keys,
        vec![
            (
                "mailbox_ref_id".into(),
                "mailbox_ref_id".into(),
                "mailbox_sync_state".into(),
                "CASCADE".into(),
            ),
            (
                "direction".into(),
                "direction".into(),
                "mailbox_sync_state".into(),
                "CASCADE".into(),
            ),
        ]
    );
    let table_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema
             WHERE type = 'table' AND name = 'mailbox_sync_incremental_continuations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(table_sql.contains("CHECK(direction IN ('inbound','outbound'))"));
    assert!(table_sql.matches("length(trim(").count() >= 2);
}

#[test]
fn frozen_v29_schema_fixture_is_upgradeable_without_schema_mismatch() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).expect("open current db");
    schema::downgrade_reference_to_v29(&connection).expect("build v29 fixture");
    let actual_task_attention = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'task_attention'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("task_attention schema");
    assert_eq!(
        schema::normalize_sql_for_test(&actual_task_attention),
        schema::normalize_sql_for_test(include_str!(
            "../../../../tests/fixtures/v29_task_attention_schema.sql"
        )),
        "v29 reference must match the frozen redacted task_attention schema"
    );
    drop(connection);

    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 29,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            ..
        }
    ));
    crate::CreatorRuntime::upgrade_database(&path).expect("v29 upgrade succeeds");
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );
}

#[test]
fn same_version_schema_with_missing_v30_locator_index_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("DROP INDEX creator_contact_routes_one_per_locator", [])
        .expect("drop v30 locator index");
    let error = current_schema_error(&path);
    assert!(
        error.contains("creator_contact_routes_one_per_locator"),
        "{error}"
    );
}

#[test]
fn v29_continuation_table_rejects_orphans_and_blank_boundaries_and_cascades() {
    let (_directory, path) = current_database();
    let connection = Connection::open(path).unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    connection
        .execute(
            "INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('mailbox-continuation-ddl', 'resend', 'Mail',
                     'mail@example.invalid', '[]', 1, 1, 1)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, backfill_complete, updated_at)
             VALUES ('mailbox-continuation-ddl', 'outbound', 0, 1)",
            [],
        )
        .unwrap();
    let orphan = connection
        .execute(
            "INSERT INTO mailbox_sync_incremental_continuations
               (mailbox_ref_id, direction, next_cursor,
                newest_provider_message_id, newest_occurred_at, updated_at)
             VALUES ('mailbox-continuation-ddl', 'inbound', 'next', 'anchor', 2, 2)",
            [],
        )
        .expect_err("continuation without the same-direction sync state must fail");
    assert!(orphan.to_string().contains("FOREIGN KEY"), "{orphan}");
    connection
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, backfill_complete, updated_at)
             VALUES ('mailbox-continuation-ddl', 'inbound', 0, 1)",
            [],
        )
        .unwrap();
    for (next_cursor, newest_anchor) in [
        ("", "anchor"),
        (" \t\r\n", "anchor"),
        ("next", ""),
        ("next", " \t\r\n"),
    ] {
        let error = connection
            .execute(
                "INSERT INTO mailbox_sync_incremental_continuations
                   (mailbox_ref_id, direction, next_cursor,
                    newest_provider_message_id, newest_occurred_at, updated_at)
                 VALUES ('mailbox-continuation-ddl', 'inbound', ?1, ?2, 2, 2)",
                rusqlite::params![next_cursor, newest_anchor],
            )
            .expect_err("blank continuation boundary must fail the DDL check");
        assert!(error.to_string().contains("CHECK"), "{error}");
    }
    connection
        .execute(
            "INSERT INTO mailbox_sync_incremental_continuations
               (mailbox_ref_id, direction, next_cursor,
                newest_provider_message_id, newest_occurred_at, updated_at)
             VALUES ('mailbox-continuation-ddl', 'inbound', '  opaque cursor  ',
                     'anchor', 2, 2)",
            [],
        )
        .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT next_cursor FROM mailbox_sync_incremental_continuations",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "  opaque cursor  "
    );
    connection
        .execute(
            "DELETE FROM mailbox_sync_state
             WHERE mailbox_ref_id = 'mailbox-continuation-ddl' AND direction = 'inbound'",
            [],
        )
        .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM mailbox_sync_incremental_continuations",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn same_version_schema_with_missing_v29_index_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("DROP INDEX locator_observations_by_creator_source", [])
        .expect("drop v29 index");
    let error = current_schema_error(&path);
    assert!(
        error.contains("locator_observations_by_creator_source"),
        "{error}"
    );
}

#[test]
fn same_version_schema_with_missing_v29_continuation_table_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("DROP TABLE mailbox_sync_incremental_continuations", [])
        .expect("drop v29 continuation table");
    let error = current_schema_error(&path);
    assert!(
        error.contains("mailbox_sync_incremental_continuations"),
        "{error}"
    );
}

#[test]
fn current_schema_carries_no_retired_campaign_objects_or_attribution_columns() {
    let (_directory, repository) = repository();
    let connection = repository.lock().expect("repository lock");
    let mut objects_statement = connection
        .prepare(
            "SELECT type, name FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .expect("objects query");
    let objects = objects_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("objects")
        .map(|row| row.expect("object"))
        .collect::<Vec<_>>();
    drop(objects_statement);
    assert!(
        objects
            .iter()
            .all(|(_, name)| !name.contains("campaign") || name.starts_with("discovery_")),
        "retired Campaign objects must not exist: {objects:?}"
    );
    for (kind, table) in objects.iter().filter(|(kind, _)| kind == "table") {
        let mut columns_statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("table info");
        let columns = columns_statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("columns")
            .map(|row| row.expect("column"))
            .collect::<Vec<_>>();
        for column in ["lead_id", "campaign_id"] {
            assert!(
                !columns.iter().any(|name| name == column),
                "{kind} {table} must not contain retired column {column}"
            );
        }
    }
}

#[test]
fn current_schema_contains_no_secret_or_transport_session_columns() {
    let (_directory, repository) = repository();
    let connection = repository.lock().expect("repository lock");
    let mut statement = connection
        .prepare(
            "SELECT sql FROM sqlite_master
             WHERE type = 'table' AND sql IS NOT NULL ORDER BY name",
        )
        .expect("schema query");
    let schema_sql = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("schema rows")
        .map(|row| row.expect("schema sql"))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    for forbidden in [
        "api_key",
        "auth_key",
        "access_hash",
        "cookie",
        "password",
        "device_state",
        "service_credential_id",
    ] {
        assert!(
            !schema_sql.contains(forbidden),
            "found forbidden {forbidden}"
        );
    }
}

#[test]
fn task_attention_schema_enforces_kind_identity_bounds_and_cascades() {
    let (_directory, path) = current_database();
    let connection = Connection::open(path).unwrap();
    schema::downgrade_reference_to_v33(&connection).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    connection
        .execute_batch(
            "INSERT INTO tasks
               (id, name, campaign_id, instruction, schedule_json, status, created_by,
                created_at, updated_at, revision, kind)
             VALUES ('task-attention-1', 'One', NULL, 'one',
                     '{\"kind\":\"once\",\"date\":\"2020-01-01\",\"time\":\"10:20\"}',
                     'active', 'ui', 1, 1, 1, 'custom'),
                    ('task-attention-2', 'Two', NULL, 'two',
                     '{\"kind\":\"once\",\"date\":\"2020-01-01\",\"time\":\"10:20\"}',
                     'active', 'ui', 1, 1, 1, 'custom');
             INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at,
                lease_expires_at, report_summary, evidence_json, finished_at, origin,
                chat_session_id)
             VALUES ('run-attention-1', 'task-attention-1', 1, 'reported_failed',
                     'claim-attention-1', 1, 10, 'failed', '{}', 2, 'external', NULL);",
        )
        .unwrap();

    for (sql, label) in [
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, occurred_at)
             VALUES ('bad-runless', 'task-attention-1', 'run_failed', NULL,
                     'run_failed:missing', 2)",
            "run_failed requires a run",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, reason_code, message, occurred_at)
             VALUES ('bad-block-run', 'task-attention-1', 'dispatch_blocked',
                     'run-attention-1', 'dispatch_blocked:run', 'codex_missing', 'safe', 2)",
            "dispatch_blocked forbids a run",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, reason_code, message, occurred_at)
             VALUES ('bad-reason', 'task-attention-1', 'dispatch_blocked', NULL,
                     'dispatch_blocked:reason', 'Bad Reason', 'safe', 2)",
            "reason code is bounded snake case",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, reason_code, message, occurred_at)
             VALUES ('bad-message', 'task-attention-1', 'dispatch_blocked', NULL,
                     'dispatch_blocked:message', 'codex_missing', '   ', 2)",
            "message cannot be blank",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, reason_code, message, occurred_at)
             VALUES ('bad-public-copy', 'task-attention-1', 'dispatch_blocked', NULL,
                     'dispatch_blocked:copy', 'codex_cli_missing', 'raw /Users/private error', 2)",
            "blocked public copy is one fixed enum-owned pair",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, occurred_at, acknowledged_at)
             VALUES ('bad-time', 'task-attention-1', 'run_failed', 'run-attention-1',
                     'run_failed:bad-time', 2, 1)",
            "ack cannot predate occurrence",
        ),
        (
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, occurred_at)
             VALUES ('bad-cross-task', 'task-attention-2', 'run_failed', 'run-attention-1',
                     'run_failed:cross-task', 2)",
            "run must belong to the same task",
        ),
    ] {
        connection.execute(sql, []).expect_err(label);
    }

    connection
        .execute_batch(
            "INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, occurred_at)
             VALUES ('attention-run', 'task-attention-1', 'run_failed', 'run-attention-1',
                     'run_failed:run-attention-1', 2);
             INSERT INTO task_attention
               (id, task_id, kind, task_run_id, occurrence_key, reason_code, message, occurred_at)
             VALUES ('attention-block', 'task-attention-1', 'dispatch_blocked', NULL,
                     'dispatch_blocked:attempt-1', 'codex_cli_missing',
                     '未找到可用的 Codex CLI', 2);
             DELETE FROM task_runs WHERE id = 'run-attention-1';",
        )
        .unwrap();
    let remaining = connection
        .prepare("SELECT id FROM task_attention ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(remaining, vec!["attention-block"]);
}

#[test]
fn same_version_schema_with_missing_reply_triage_index_is_rejected() {
    let (_directory, path) = current_database();
    Connection::open(&path)
        .expect("open current db")
        .execute("DROP INDEX reply_triage_open", [])
        .expect("drop attention index");
    let error = current_schema_error(&path);
    assert!(error.contains("reply_triage_open"), "{error}");
}

#[derive(Debug, PartialEq, Eq)]
struct V28BusinessSnapshot {
    row_counts: [u64; 6],
    digest: String,
}

fn v28_business_snapshot(connection: &Connection) -> V28BusinessSnapshot {
    use sha2::{Digest, Sha256};

    let row_counts = [
        "creators",
        "creator_origins",
        "locator_observations",
        "email_threads",
        "emails",
        "touchpoints",
    ]
    .map(|table| {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap()
    });
    let mut statement = connection
        .prepare(
            "SELECT kind, id, owner_id, related_id, payload FROM (
               SELECT 'creator' AS kind, id, '' AS owner_id, '' AS related_id,
                      display_name || ':' || COALESCE(notes, '') AS payload
               FROM creators
               UNION ALL
               SELECT 'origin', id, creator_id, source_id, CAST(discovered_at AS TEXT)
               FROM creator_origins
               UNION ALL
               SELECT 'locator_observation', id, locator_id, source_id,
                      COALESCE(creator_id, platform_account_id, '') || ':' || observed_value || ':' || observed_at
               FROM locator_observations
               UNION ALL
               SELECT 'email_thread', id, COALESCE(creator_id, ''), mailbox_ref_id,
                      provider_thread_key || ':' || subject
               FROM email_threads
               UNION ALL
               SELECT 'email', id, thread_id, provider_message_id,
                      direction || ':' || occurred_at
               FROM emails
               UNION ALL
               SELECT 'touchpoint', id, creator_id, COALESCE(email_id, ''),
                      kind || ':' || occurred_at
               FROM touchpoints
             ) ORDER BY kind, id",
        )
        .expect("business digest query");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .expect("business digest rows");
    let mut hasher = Sha256::new();
    for row in rows {
        let row = row.expect("business digest row");
        for value in [row.0, row.1, row.2, row.3, row.4] {
            hasher.update(value.len().to_be_bytes());
            hasher.update(value.as_bytes());
        }
    }
    V28BusinessSnapshot {
        row_counts,
        digest: format!("{:x}", hasher.finalize()),
    }
}

fn seeded_v28_business_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at)
             VALUES ('creator-v28', 'Preserved v28 Creator', 'keep v28 notes', 10, 20, NULL),
                    ('creator-v28-peer', 'Preserved v28 Peer', NULL, 11, 21, NULL);
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES ('creator-v28', 'engaged', 30, 10, 20),
                    ('creator-v28-peer', 'prospect', NULL, 11, 21);
             INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id,
                metadata_json, observed_at)
             VALUES ('source-v28', 'profile_bio', 'x', NULL, NULL, NULL,
                     '{\"preserved\":true}', 31);
             INSERT INTO platform_accounts
               (id, platform, target_kind, external_id, created_at, updated_at)
             VALUES ('account-v28', 'x', 'user', 'external-v28', 10, 20);
             INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
             VALUES ('link-v28', 'creator-v28', 'account-v28', 'claimed', 'asserted', 10, 20);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES ('locator-v28', 'email', 'email_address', 'preserved@example.invalid',
                     'preserved@example.invalid', 10, 20);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovered_at)
             VALUES ('origin-v28', 'manual_entry', 'creator-v28', 'source-v28', 32);
             INSERT INTO locator_observations
               (id, locator_id, source_id, creator_id, platform_account_id,
                observed_value, observed_at)
             VALUES ('locator-observation-v28-direct', 'locator-v28', 'source-v28',
                     'creator-v28', NULL, 'preserved@example.invalid', 33),
                    ('locator-observation-v28-account', 'locator-v28', 'source-v28',
                     NULL, 'account-v28', 'preserved-account', 34);
             INSERT INTO mailbox_refs
               (id, provider, label, email_address, capabilities_json, available,
                created_at, updated_at)
             VALUES ('mailbox-v28', 'resend', 'Preserved', 'mailbox@example.invalid',
                     '{}', 1, 10, 20);
             INSERT INTO email_threads
               (id, mailbox_ref_id, creator_id, provider_thread_key, subject,
                active_folder, created_at, updated_at)
             VALUES ('thread-v28', 'mailbox-v28', 'creator-v28', 'provider-thread-v28',
                     'Preserved thread', 'sent', 10, 20);
             INSERT INTO emails
               (id, thread_id, provider_message_id, direction, subject,
                references_json, delivery_status, headers_json, occurred_at, origin)
             VALUES ('email-v28', 'thread-v28', 'provider-message-v28', 'outbound',
                     'Preserved email', '[]', 'sent', '{}', 35, 'confirmed_send');
             INSERT INTO touchpoints
               (id, creator_id, kind, email_id, occurred_at, metadata_json)
             VALUES ('touchpoint-v28', 'creator-v28', 'email_sent', 'email-v28', 35,
                     '{\"preserved\":true}');",
        )
        .unwrap();
    schema::downgrade_reference_to_v28(&connection).unwrap();
    drop(connection);
    (directory, path)
}

#[test]
fn v27_reference_preflight_cannot_partially_downgrade_a_current_fixture() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "INSERT INTO creator_groups
               (id, name, revision, position, created_at, updated_at)
             VALUES ('group-current', 'Current', 1, 0, 1, 1)",
            [],
        )
        .unwrap();
    let error = schema::downgrade_reference_to_v27(&connection)
        .expect_err("nonempty v28 fact must reject a v27 reference");
    assert!(error.contains("Creator groups"), "{error}");
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name = 'creator_origins_by_creator_recency'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'table'
                   AND name = 'mailbox_sync_incremental_continuations'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1,
        "failed v27 preflight partially dropped the v29 amendment"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index'
                   AND name = 'creator_contact_routes_one_per_locator'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1,
        "failed v27 preflight partially dropped the v30 invariant"
    );
    drop(connection);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(path),
        crate::CreatorDatabaseStatus::Ready
    );
}

fn seeded_v27_creator_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v27(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at,
                score, assessment, follower_counts_json, pinned_at)
             VALUES ('creator-v27', 'Preserved Creator', 'keep notes', 10, 20,
                     NULL, 88, 'keep assessment', '{\"youtube\":1234}', 30);
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES ('creator-v27', 'engaged', 15, 10, 20);
             INSERT INTO tasks
               (id, name, campaign_id, instruction, schedule_json, status, created_by,
                created_at, updated_at, revision, kind)
             VALUES ('task-v27', 'Preserved task', NULL, 'preserve this task',
                     '{\"kind\":\"once\",\"date\":\"2020-01-01\",\"time\":\"10:20\"}',
                     'active', 'agent', 10, 20, 3, 'custom');
             INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at,
                lease_expires_at, report_summary, evidence_json, finished_at, origin,
                chat_session_id)
             VALUES ('run-v27', 'task-v27', 11, 'reported_failed', 'claim-v27', 12,
                     100, 'preserved failure', '{}', 13, 'external', NULL);",
        )
        .unwrap();
    drop(connection);
    (directory, path)
}

#[test]
fn v28_upgrade_adds_empty_v29_continuation_state_and_six_indexes_without_business_changes() {
    let (_directory, path) = seeded_v28_business_database();
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 28,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: Default::default(),
        }
    );
    let before = v28_business_snapshot(&Connection::open(&path).unwrap());

    let upgraded = crate::CreatorRuntime::upgrade_database(&path).expect("v28 upgrade succeeds");
    assert_eq!(upgraded.from_version, 28);
    assert_eq!(upgraded.to_version, schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(
        upgraded.impact,
        crate::CreatorDatabaseUpgradeImpact::default()
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );

    let upgraded_connection = Connection::open(&path).unwrap();
    assert_eq!(v28_business_snapshot(&upgraded_connection), before);
    assert_eq!(
        upgraded_connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name IN (
                   'creator_origins_by_creator_recency',
                   'email_threads_by_creator',
                   'locator_observations_by_creator_source',
                   'locator_observations_by_locator_recency',
                   'locator_observations_by_platform_account_source',
                   'touchpoints_by_creator_recency'
                 )",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        6
    );
    assert_eq!(
        upgraded_connection
            .query_row(
                "SELECT COUNT(*) FROM mailbox_sync_incremental_continuations",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );

    let backup = Connection::open(&upgraded.backup_path).expect("open verified v28 backup");
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        28
    );
    assert_eq!(v28_business_snapshot(&backup), before);
    assert_eq!(
        backup
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'table'
                   AND name = 'mailbox_sync_incremental_continuations'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&upgraded.backup_path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 28,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: Default::default(),
        }
    );
}

#[test]
fn v29_upgrade_adds_native_locator_uniqueness_and_preserves_rows() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v29(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, created_at, updated_at, follower_counts_json)
             VALUES ('creator-v29-a', 'A', 1, 1, '{}'),
                    ('creator-v29-b', 'B', 1, 1, '{}');
             INSERT INTO creator_relationships
               (creator_id, stage, created_at, updated_at)
             VALUES ('creator-v29-a', 'prospect', 1, 1),
                    ('creator-v29-b', 'prospect', 1, 1);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES ('locator-v29', 'email', 'email_address',
                     'v29@example.com', 'v29@example.com', 1, 1);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES ('route-v29', 'creator-v29-a', 'locator-v29', 'exact', 1, 1);",
        )
        .unwrap();
    drop(connection);

    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 29,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: Default::default(),
        }
    );
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).expect("v29 upgrade succeeds");
    assert_eq!(upgraded.from_version, 29);
    assert_eq!(upgraded.to_version, schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(
        upgraded.impact,
        crate::CreatorDatabaseUpgradeImpact::default()
    );

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT creator_id FROM creator_contact_routes WHERE id = 'route-v29'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "creator-v29-a"
    );
    let index = connection
        .prepare("PRAGMA index_list('creator_contact_routes')")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, bool>(2)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .find(|(name, _)| name == "creator_contact_routes_one_per_locator")
        .expect("v30 locator index");
    assert!(index.1);
    connection
        .execute_batch(
            "DROP TRIGGER prevent_shared_creator_contact_locator_insert;
             DROP TRIGGER prevent_shared_creator_contact_locator_update;",
        )
        .unwrap();
    let duplicate = connection
        .execute(
            "INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES ('route-v30-duplicate', 'creator-v29-b', 'locator-v29', 'exact', 2, 2)",
            [],
        )
        .expect_err("native v30 uniqueness must not depend on triggers");
    assert!(duplicate.to_string().contains("UNIQUE"), "{duplicate}");
    assert_eq!(
        Connection::open(upgraded.backup_path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        29
    );
}

#[test]
fn v29_shared_locator_owners_block_before_backup_or_migration_write() {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v29(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, created_at, updated_at, follower_counts_json)
             VALUES ('creator-v29-shared-a', 'A', 1, 1, '{}'),
                    ('creator-v29-shared-b', 'B', 1, 1, '{}');
             INSERT INTO creator_relationships
               (creator_id, stage, created_at, updated_at)
             VALUES ('creator-v29-shared-a', 'prospect', 1, 1),
                    ('creator-v29-shared-b', 'prospect', 1, 1);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES ('locator-v29-shared', 'email', 'email_address',
                     'shared@example.com', 'shared@example.com', 1, 1);
             DROP TRIGGER prevent_shared_creator_contact_locator_insert;
             DROP TRIGGER prevent_shared_creator_contact_locator_update;
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES ('route-v29-shared-a', 'creator-v29-shared-a',
                     'locator-v29-shared', 'exact', 1, 1),
                    ('route-v29-shared-b', 'creator-v29-shared-b',
                     'locator-v29-shared', 'exact', 1, 1);",
        )
        .unwrap();
    connection
        .execute_batch(schema::NO_SHARED_CONTACT_ROUTE_TRIGGERS_SQL)
        .unwrap();
    drop(connection);

    let status = crate::CreatorRuntime::inspect_database(&path);
    let crate::CreatorDatabaseStatus::Unsupported { version, reason } = status else {
        panic!("shared v29 locator owners must be blocked, got {status:?}");
    };
    assert_eq!(version, 29);
    assert!(reason.contains("Creator owners"), "{reason}");
    assert!(
        std::fs::read_dir(directory.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains("backup")),
        "blocked v29 preflight created a backup"
    );
}

fn explain_details(connection: &Connection, sql: &str) -> Vec<String> {
    connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("prepare access-path evidence")
        .query_map([], |row| row.get::<_, String>(3))
        .expect("query access-path evidence")
        .collect::<Result<Vec<_>, _>>()
        .expect("access-path evidence rows")
}

#[test]
fn v29_indexes_drive_the_profiled_non_q5_access_paths() {
    let (_directory, path) = seeded_v28_business_database();
    crate::CreatorRuntime::upgrade_database(&path).expect("v28 upgrade succeeds");
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch("PRAGMA automatic_index = OFF;")
        .unwrap();

    let touchpoint_plan = explain_details(
        &connection,
        "SELECT MAX(touch.occurred_at) FROM touchpoints touch
         WHERE touch.creator_id = 'creator-v28'",
    );
    assert!(
        touchpoint_plan.iter().any(|detail| detail.contains(
            "SEARCH touch USING COVERING INDEX touchpoints_by_creator_recency (creator_id=?)"
        )),
        "{touchpoint_plan:#?}"
    );

    let email_plan = explain_details(
        &connection,
        "SELECT MAX(email.occurred_at) FROM emails email
         JOIN email_threads thread ON thread.id = email.thread_id
         WHERE thread.creator_id = 'creator-v28' AND email.direction = 'outbound'",
    );
    assert!(
        email_plan.iter().any(|detail| detail.contains(
            "SEARCH thread USING COVERING INDEX email_threads_by_creator (creator_id=?)"
        )),
        "{email_plan:#?}"
    );
    assert!(
        email_plan.iter().any(|detail| {
            detail.contains("SEARCH email USING INDEX sqlite_autoindex_emails_2 (thread_id=?)")
        }),
        "the existing emails(thread_id, provider_message_id) prefix must remain sufficient: {email_plan:#?}"
    );

    let origin_plan = explain_details(
        &connection,
        "SELECT id, kind, creator_id, source_id, discovered_at
         FROM creator_origins
         WHERE creator_id = 'creator-v28' AND source_id IS NOT NULL
         ORDER BY discovered_at DESC",
    );
    assert!(
        origin_plan.iter().any(|detail| detail.contains(
            "SEARCH creator_origins USING INDEX creator_origins_by_creator_recency (creator_id=?)"
        )),
        "{origin_plan:#?}"
    );
    assert!(
        origin_plan
            .iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE")),
        "{origin_plan:#?}"
    );

    let locator_plan = explain_details(
        &connection,
        "SELECT observation.id, observation.source_id, observation.observed_at
         FROM locator_observations observation
         JOIN observation_sources source ON source.id = observation.source_id
         WHERE observation.locator_id = 'locator-v28'
           AND (observation.creator_id = 'creator-v28'
             OR observation.platform_account_id IN (
               SELECT platform_account_id FROM creator_account_links
               WHERE creator_id = 'creator-v28'
             ))
         ORDER BY observation.observed_at DESC",
    );
    assert!(
        locator_plan.iter().any(|detail| detail.contains(
            "SEARCH observation USING INDEX locator_observations_by_locator_recency (locator_id=?)"
        )),
        "{locator_plan:#?}"
    );
    assert!(
        locator_plan
            .iter()
            .all(|detail| !detail.contains("USE TEMP B-TREE")),
        "{locator_plan:#?}"
    );
}

#[test]
fn v28_pre_release_amendment_rolls_back_to_the_valid_predecessor() {
    let (_directory, path) = seeded_v28_business_database();
    let before = v28_business_snapshot(&Connection::open(&path).unwrap());
    let mut connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=OFF").unwrap();
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    schema::apply_forward_migrations(&transaction, 28, schema::CURRENT_SCHEMA_VERSION).unwrap();
    assert_eq!(
        transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name = 'creator_origins_by_creator_recency'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    assert_eq!(
        transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'table'
                   AND name = 'mailbox_sync_incremental_continuations'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    transaction.rollback().unwrap();
    drop(connection);

    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 28,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: Default::default(),
        }
    );
    let connection = Connection::open(path).unwrap();
    assert_eq!(v28_business_snapshot(&connection), before);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name = 'creator_origins_by_creator_recency'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'table'
                   AND name = 'mailbox_sync_incremental_continuations'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn precommit_business_validation_rolls_back_and_keeps_the_verified_v27_backup() {
    let (directory, path) = seeded_v27_creator_database();
    let error = schema::upgrade_with_post_migration_mutation_for_test(&path, |transaction| {
        transaction
            .execute_batch(
                "INSERT INTO mailbox_refs
                   (id, provider, label, email_address, capabilities_json, available,
                    created_at, updated_at)
                 VALUES ('mailbox-precommit-drift', 'resend', 'Drift',
                         'drift@example.com', '{}', 1, 1, 1);
                 INSERT INTO email_threads
                   (id, mailbox_ref_id, provider_thread_key, subject, active_folder,
                    archived_at, created_at, updated_at)
                 VALUES ('thread-precommit-drift', 'mailbox-precommit-drift', 'drift',
                         'Drift', 'sent', 123, 1, 1);",
            )
            .unwrap();
    })
    .expect_err("current business drift must fail before commit");
    assert_eq!(error.support_code, "LOCAL_DATA_VALIDATION_FAILED");
    assert!(!error.storage_upgraded);

    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: seeded_v27_task_retirement_impact(),
        }
    );
    let source = Connection::open(&path).unwrap();
    assert_eq!(
        source
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        27
    );
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM creators WHERE id = 'creator-v27'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    source
        .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect_err("rolled-back v27 source must not contain Creator groups");
    assert_eq!(
        source
            .query_row(
                "SELECT COUNT(*) FROM email_threads WHERE id = 'thread-precommit-drift'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        0
    );

    let backups = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".backup-"))
        })
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1, "verified recovery point must be retained");
    let backup = &backups[0];
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        27
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(backup),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: seeded_v27_task_retirement_impact(),
        }
    );
}

#[test]
fn v27_chain_upgrade_preserves_creator_facts_and_retires_tasks() {
    let (_directory, path) = seeded_v27_creator_database();
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: seeded_v27_task_retirement_impact(),
        }
    );
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).expect("v27 upgrade succeeds");
    assert_eq!(upgraded.from_version, 27);
    assert_eq!(upgraded.to_version, schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(upgraded.impact, seeded_v27_task_retirement_impact());

    let connection = Connection::open(&path).unwrap();
    let preserved = connection
        .query_row(
            "SELECT creator.display_name, creator.notes, creator.score,
                    creator.assessment, creator.follower_counts_json,
                    creator.pinned_at, creator.group_id, relationship.stage
             FROM creators creator
             JOIN creator_relationships relationship
               ON relationship.creator_id = creator.id
             WHERE creator.id = 'creator-v27'",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u8>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        preserved,
        (
            "Preserved Creator".to_string(),
            "keep notes".to_string(),
            88,
            "keep assessment".to_string(),
            "{\"youtube\":1234}".to_string(),
            30,
            None,
            "engaged".to_string(),
        )
    );
    assert_retired_task_schema_absent(&connection);
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name IN (
                   'creator_origins_by_creator_recency',
                   'email_threads_by_creator',
                   'locator_observations_by_creator_source',
                   'locator_observations_by_locator_recency',
                   'locator_observations_by_platform_account_source',
                   'touchpoints_by_creator_recency'
                 )",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        6,
        "v27 must traverse both immutable adjacent edges"
    );

    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        27
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT COUNT(*) FROM tasks task
                 JOIN task_runs run ON run.task_id = task.id
                 WHERE task.id = 'task-v27' AND run.id = 'run-v27'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    backup
        .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect_err("the v27 backup must not contain Creator groups");
}

#[test]
fn v27_full_chain_migration_rolls_back_to_the_reopenable_predecessor() {
    let (_directory, path) = seeded_v27_creator_database();
    let mut connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    schema::apply_forward_migrations(&transaction, 27, schema::CURRENT_SCHEMA_VERSION).unwrap();
    assert_eq!(
        transaction
            .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        transaction
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema
                 WHERE type = 'index' AND name = 'creator_origins_by_creator_recency'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
    transaction.rollback().unwrap();
    drop(connection);

    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: seeded_v27_task_retirement_impact(),
        }
    );
    let connection = Connection::open(path).unwrap();
    connection
        .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
            row.get::<_, u64>(0)
        })
        .expect_err("rolled-back v27 source has no Creator groups");
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM creators WHERE id = 'creator-v27'",
                [],
                |row| row.get::<_, u64>(0),
            )
            .unwrap(),
        1
    );
}
