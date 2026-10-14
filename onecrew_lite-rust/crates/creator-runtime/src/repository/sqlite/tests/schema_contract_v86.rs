use super::*;
use crate::MailboxProvider;

#[test]
fn gmail_upgrade_preserves_mailbox_graph_and_backup() {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 85).unwrap();
    db.execute_batch("PRAGMA user_version=85; COMMIT; PRAGMA foreign_keys=ON;")
        .unwrap();
    db.execute_batch("INSERT INTO mailbox_refs VALUES ('77777777-7777-4777-8777-777777777777','resend','Preserved','preserved@example.com',NULL,'[\"send\"]',1,NULL,1,1);
        INSERT INTO email_auth_connections VALUES ('preserved-auth','resend','preserved-secret','Preserved',1,1,1);
        INSERT INTO mailbox_identity_mappings VALUES ('77777777-7777-4777-8777-777777777777','preserved-auth','resend','mailbox',1,'preserved-checkpoint',1,1);").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let outcome = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(outcome.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        85
    );
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    for database in [&db, &backup] {
        assert_eq!(database.query_row("SELECT m.email_address FROM mailbox_refs m JOIN mailbox_identity_mappings p ON p.mailbox_ref_id=m.id JOIN email_auth_connections a ON a.id=p.auth_connection_id WHERE a.id='preserved-auth'", [], |r| r.get::<_,String>(0)).unwrap(), "preserved@example.com");
    }
    let repository = SqliteRepository::open(&path).unwrap();
    let id = MailboxRefId::new("99999999-9999-4999-8999-999999999999");
    repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: id.clone(),
            provider: "gmail".into(),
            label: "Gmail".into(),
            email_address: "sender@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send, EmailCapability::SentReadback],
            available: true,
            last_error: None,
        })
        .unwrap();
    repository
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: id.clone(),
            auth_connection_id: EmailAuthConnectionId::new("gmail:owner:connection"),
            provider_kind: EmailProviderKind::Remote,
            secret_record_id: EmailSecretRecordId::new("access:owner:connection"),
            connection_label: "Gmail".into(),
            projection_kind: MailboxProjectionKind::Mailbox,
            capability_projection_revision: 1,
            checkpoint_namespace: "gmail-connection".into(),
        })
        .unwrap();
    repository.reconcile_mailbox_refs(vec![]).unwrap();
    let binding = repository.resolve_mailbox_auth_binding(&id).unwrap();
    assert_eq!(binding.provider, MailboxProvider::Gmail);
    assert!(
        binding.available,
        "Resend reconciliation must preserve Gmail availability"
    );
    for available in [false, true] {
        repository
            .upsert_mailbox_ref(MailboxRefInput {
                id: id.clone(),
                provider: "gmail".into(),
                label: binding.label.clone(),
                email_address: binding.email_address.clone(),
                display_name: None,
                capabilities: binding.capabilities.clone(),
                available,
                last_error: None,
            })
            .unwrap();
        repository
            .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
                mailbox_ref_id: id.clone(),
                auth_connection_id: binding.auth_connection.auth_connection_id.clone(),
                provider_kind: EmailProviderKind::Remote,
                secret_record_id: binding.auth_connection.secret_record_id.clone(),
                connection_label: binding.label.clone(),
                projection_kind: MailboxProjectionKind::Mailbox,
                capability_projection_revision: 1,
                checkpoint_namespace: "gmail-connection".into(),
            })
            .unwrap();
        assert_eq!(
            repository
                .resolve_mailbox_auth_binding(&id)
                .unwrap()
                .auth_connection
                .available,
            available
        );
    }
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    drop(dir);
}

#[test]
fn gmail_edge_rolls_back_constraint_and_rows() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 85).unwrap();
    db.execute_batch("PRAGMA user_version=85; COMMIT;").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='mailbox_refs'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 85, 86).unwrap();
    db.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name='mailbox_refs'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        ddl
    );
}
