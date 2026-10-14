use onecrew_local_domain::{
    CreatorSyncAcceptedBundle, CreatorSyncBundleProfile, CreatorSyncCapability,
    CreatorSyncClientPlatform, CreatorSyncCredentialMetadata, CreatorSyncCredentialStatus,
    CreatorSyncProtocol, Platform, ProfileSecret, ServiceCredentialSecret, ServiceProvider,
};
use rusqlite::{Connection, TransactionBehavior};
use tempfile::tempdir;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{
    EmailSecretRecordKind, EmailSecretRecordSecret, LocalVault, ServiceCredentialHealthCommit,
    VaultCreatorSyncCredentialState, VaultDataStatus, VaultDataUpgradeError,
    VaultDataUpgradeFailureKind, VaultDataUpgradeStatus,
    schema::{self, CURRENT_SCHEMA_VERSION, PREVIOUS_SCHEMA_VERSION},
    upgrade::upgrade_with_hooks,
};

fn open_error(path: &std::path::Path) -> String {
    match LocalVault::open(path) {
        Ok(_) => panic!("non-current local-vault schema must be rejected"),
        Err(error) => error.to_string(),
    }
}

fn profile_secret(name: &str) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: format!("remote-{name}"),
        cookies: Vec::new(),
        x_chat_passcode: None,
        instagram_session_settings: None,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_session: None,
        profile_name: name.into(),
        browser_profile_dir: format!("/tmp/{name}"),
        fingerprint_seed: String::new(),
        proxy: None,
        proxy_egress: None,
    }
}

fn creator_sync_metadata() -> CreatorSyncCredentialMetadata {
    CreatorSyncCredentialMetadata {
        installation_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        credential_id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        access_user_id: "access-user".into(),
        access_device_id: "access-device".into(),
        source_instance_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        access_pairing_id: "pairing-id".into(),
        client_platform: CreatorSyncClientPlatform::Macos,
        app_version: "0.5.11".into(),
        status: CreatorSyncCredentialStatus::Active,
        revision: 1,
        created_at: 100,
        updated_at: 101,
        last_seen_at: Some(101),
        revoked_at: None,
        protocol: CreatorSyncProtocol::ReacherCreatorSync,
        protocol_version: 2,
        capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
        accepted_bundle: CreatorSyncAcceptedBundle {
            format: "agent-platform.creator-snapshot".into(),
            format_version: 1,
            profile: CreatorSyncBundleProfile::PortablePublic,
            max_snapshot_bytes: 1024,
            max_chunk_bytes: 512,
            max_chunks: 2,
            max_namespace_staged_bytes: 2048,
            max_access_user_staged_bytes: 4096,
        },
    }
}

fn creator_sync_bearer(metadata: &CreatorSyncCredentialMetadata) -> ServiceCredentialSecret {
    ServiceCredentialSecret::new(format!("{}_{}", metadata.credential_id, "A".repeat(43)))
}

fn downgrade_current_to_v2(path: &std::path::Path) {
    let connection = Connection::open(path).unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
        .unwrap();
}

fn create_empty_v3(path: &std::path::Path) {
    let connection = Connection::open(path).unwrap();
    schema::create_v3_schema_for_test(&connection).unwrap();
    drop(connection);
    crate::crypto::VaultCipher::load_or_create(&path.with_file_name("vault.key")).unwrap();
}

fn create_empty_v2(path: &std::path::Path) {
    let connection = Connection::open(path).unwrap();
    schema::create_v2_schema_for_test(&connection).unwrap();
    drop(connection);
    crate::crypto::VaultCipher::load_or_create(&path.with_file_name("vault.key")).unwrap();
}

#[derive(PartialEq, Eq)]
struct StoredRows {
    profiles: Vec<StoredProfileRow>,
    credentials: Vec<StoredCredentialRow>,
}

#[derive(PartialEq, Eq)]
struct StoredProfileRow {
    id: String,
    platform: String,
    provider_account_id: String,
    username: String,
    encrypted_secret: Vec<u8>,
    created_at: String,
    updated_at: String,
}

#[derive(PartialEq, Eq)]
struct StoredCredentialRow {
    id: String,
    provider: String,
    label: String,
    metadata_json: String,
    encrypted_secret: Vec<u8>,
    disabled: i64,
    cooldown_until: Option<i64>,
    last_used_at: Option<i64>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

fn stored_rows(path: &std::path::Path) -> StoredRows {
    let connection = Connection::open(path).unwrap();
    let mut profiles = connection
        .prepare(
            "SELECT id, platform, provider_account_id, username, encrypted_secret,
                    created_at, updated_at FROM profiles ORDER BY id",
        )
        .unwrap();
    let profiles = profiles
        .query_map([], |row| {
            Ok(StoredProfileRow {
                id: row.get(0)?,
                platform: row.get(1)?,
                provider_account_id: row.get(2)?,
                username: row.get(3)?,
                encrypted_secret: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let mut credentials = connection
        .prepare(
            "SELECT id, provider, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at
             FROM service_credentials ORDER BY id",
        )
        .unwrap();
    let credentials = credentials
        .query_map([], |row| {
            Ok(StoredCredentialRow {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                metadata_json: row.get(3)?,
                encrypted_secret: row.get(4)?,
                disabled: row.get(5)?,
                cooldown_until: row.get(6)?,
                last_used_at: row.get(7)?,
                last_error: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    StoredRows {
        profiles,
        credentials,
    }
}

#[test]
fn replace_profile_preserves_id_across_username_case_changes() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let original = vault
        .upsert(Platform::X, "Creator", &profile_secret("original"))
        .unwrap();
    let mut replacement = profile_secret("replacement");
    replacement.provider_account_id = original.provider_account_id.clone();
    let replaced = vault
        .replace_profile(original.id, Platform::X, "RenamedCreator", &replacement)
        .unwrap();
    assert_eq!(replaced.id, original.id);
    assert_eq!(replaced.provider_account_id, original.provider_account_id);
    assert_eq!(vault.list(Some(Platform::X)).unwrap().len(), 1);
    let (stored, secret) = vault.get(original.id).unwrap();
    assert_eq!(stored.username, "RenamedCreator");
    assert_eq!(stored.provider_account_id, secret.provider_account_id);
    assert_eq!(secret.profile_name, "replacement");
}

#[test]
fn restore_profile_reinstates_exact_metadata_and_secret_snapshot() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let original_secret = profile_secret("original");
    let original = vault
        .upsert(Platform::Telegram, "original_name", &original_secret)
        .unwrap();
    let mut replacement = profile_secret("replacement");
    replacement.provider_account_id = original.provider_account_id.clone();
    vault
        .replace_profile(
            original.id,
            Platform::Telegram,
            "replacement_name",
            &replacement,
        )
        .unwrap();

    vault.restore_profile(&original, &original_secret).unwrap();
    let (restored, restored_secret) = vault.get(original.id).unwrap();
    assert_eq!(restored.id, original.id);
    assert_eq!(restored.platform, original.platform);
    assert_eq!(restored.provider_account_id, original.provider_account_id);
    assert_eq!(restored.username, original.username);
    assert_eq!(restored.created_at, original.created_at);
    assert_eq!(restored.updated_at, original.updated_at);
    assert_eq!(
        serde_json::to_value(restored_secret).unwrap(),
        serde_json::to_value(original_secret).unwrap()
    );
}

#[test]
fn upsert_matches_remote_identity_not_mutable_username() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let secret = profile_secret("stable");
    let first = vault.upsert(Platform::X, "old_handle", &secret).unwrap();
    let second = vault.upsert(Platform::X, "new_handle", &secret).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.username, "new_handle");
    assert_eq!(vault.list(Some(Platform::X)).unwrap().len(), 1);
}

#[test]
fn empty_database_is_initialized_to_the_only_supported_schema() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    drop(LocalVault::open(&path).expect("create current vault"));

    let connection = Connection::open(&path).expect("open vault database");
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .expect("schema version");
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    drop(connection);

    LocalVault::open(&path).expect("reopen current vault schema");
}

#[test]
fn production_connection_configures_a_busy_timeout() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let timeout_ms = vault
        .connection
        .lock()
        .unwrap()
        .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, u64>(0))
        .expect("busy timeout");
    assert_eq!(timeout_ms, 5_000);
}

#[test]
fn old_schema_is_rejected_without_migration_or_fallback() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    Connection::open(&path)
        .expect("open legacy vault")
        .execute_batch(
            "CREATE TABLE profiles (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL,
               username TEXT NOT NULL,
               encrypted_secret BLOB NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             PRAGMA user_version = 0;",
        )
        .expect("create legacy schema");

    let error = open_error(&path);
    assert!(
        error.contains("unsupported local-vault schema version 0"),
        "{error}"
    );
    assert!(!error.to_ascii_lowercase().contains("reset"), "{error}");
}

#[test]
fn inspection_distinguishes_storage_created_by_a_newer_version() {
    let root = tempdir().unwrap();
    let path = root.path().join("agent-platform.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE future (id TEXT PRIMARY KEY); PRAGMA user_version = 99;")
        .unwrap();
    std::fs::write(root.path().join("vault.key"), [7_u8; 32]).unwrap();
    assert!(matches!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::Unsupported {
            created_by_newer_version: true,
            ..
        }
    ));
}

#[test]
fn same_version_half_finished_schema_is_rejected() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    Connection::open(&path)
        .expect("open incomplete vault")
        .execute_batch(&format!(
            "CREATE TABLE profiles (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL,
               username TEXT NOT NULL,
               encrypted_secret BLOB NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"
        ))
        .expect("create incomplete schema");

    let error = open_error(&path);
    assert!(error.contains("missing: service_credentials"), "{error}");
}

#[test]
fn same_version_polluted_schema_is_rejected() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    drop(LocalVault::open(&path).expect("create current vault"));
    Connection::open(&path)
        .expect("open current vault")
        .execute("CREATE TABLE legacy_credentials (secret BLOB)", [])
        .expect("pollute current schema");

    let error = open_error(&path);
    assert!(error.contains("unexpected: legacy_credentials"), "{error}");
}

#[test]
fn same_version_schema_with_wrong_column_is_rejected() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    drop(LocalVault::open(&path).expect("create current vault"));
    Connection::open(&path)
        .expect("open current vault")
        .execute(
            "ALTER TABLE service_credentials RENAME COLUMN metadata_json TO metadata",
            [],
        )
        .expect("rename current column");

    let error = open_error(&path);
    assert!(
        error.contains("definition differs at schema object table:service_credentials"),
        "{error}"
    );
}

#[test]
fn service_credentials_round_trip_without_leaking_secret_in_list() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "default",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("AIza-test-secret"),
        )
        .unwrap();

    let listed = vault
        .list_service_credentials(ServiceProvider::Youtube, false)
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, saved.id);
    assert_eq!(listed[0].label, "default");

    let (fetched, secret) = vault.get_service_credential(saved.id).unwrap();
    assert_eq!(
        serde_json::to_value(&listed[0]).unwrap(),
        serde_json::to_value(fetched).unwrap()
    );
    assert_eq!(secret.expose_secret(), "AIza-test-secret");

    vault
        .mark_service_credential_failure(saved.id, "quota exceeded", false, Some(u64::MAX))
        .unwrap();
    assert!(
        vault
            .list_service_credentials(ServiceProvider::Youtube, true)
            .unwrap()
            .is_empty()
    );
    assert!(vault.delete_service_credential(saved.id).unwrap());
}

#[test]
fn typed_email_secret_records_round_trip_without_secret_in_projection() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let secret = EmailSecretRecordSecret::new("re_secret_typed");
    let saved = vault
        .upsert_email_secret_record(
            EmailSecretRecordKind::ResendApiKey,
            "partnerships",
            serde_json::json!({
                "email_address": "partnerships@example.com",
                "display_name": "Reach"
            }),
            &secret,
        )
        .unwrap();
    let listed = vault
        .list_email_secret_records(EmailSecretRecordKind::ResendApiKey, false)
        .unwrap();
    assert_eq!(listed, vec![saved.clone()]);
    let encoded = serde_json::to_string(&saved).unwrap();
    assert!(!encoded.contains("re_secret_typed"));
    let (readback, readback_secret) = vault.get_email_secret_record(saved.id).unwrap();
    assert_eq!(readback, saved);
    assert_eq!(readback_secret.expose_secret(), "re_secret_typed");
    let (typed_readback, typed_secret) = vault
        .get_email_secret_record_for_connection(
            &saved.id.to_string(),
            EmailSecretRecordKind::ResendApiKey,
        )
        .unwrap();
    assert_eq!(typed_readback, saved);
    assert_eq!(typed_secret.expose_secret(), "re_secret_typed");
    assert!(matches!(
        LocalVault::inspect(root.path().join("vault.db")).status,
        VaultDataStatus::Ready
    ));
    let generic_delete = vault.delete_service_credential(saved.id).unwrap_err();
    assert!(generic_delete.to_string().contains("typed Email"));
}

#[test]
fn typed_email_secret_metadata_rejects_secret_bearing_fields() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let error = vault
        .upsert_email_secret_record(
            EmailSecretRecordKind::SmtpCredential,
            "smtp",
            serde_json::json!({"host": "smtp.example.com", "password": "secret"}),
            &EmailSecretRecordSecret::new("smtp-secret"),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("secret-bearing"), "{error}");
}

#[test]
fn generic_resend_mutation_is_not_a_runtime_typed_email_path() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let error = vault
        .upsert_service_credential(
            ServiceProvider::Resend,
            "legacy-mailbox",
            serde_json::json!({"email_address": "legacy@example.com"}),
            &ServiceCredentialSecret::new("re_legacy_ciphertext"),
        )
        .unwrap_err();
    assert!(error.to_string().contains("dedicated Email"));
    assert!(
        vault
            .get_email_secret_record_for_connection(
                "not-a-uuid",
                EmailSecretRecordKind::ResendApiKey,
            )
            .is_err()
    );
}

#[test]
fn service_credential_rename_preserves_identity_secret_metadata_and_health() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "old-label",
            serde_json::json!({"api": "youtube_data_api_v3", "project": "old"}),
            &ServiceCredentialSecret::new("old-secret"),
        )
        .unwrap();
    vault
        .mark_service_credential_failure(saved.id, "old failure", true, Some(u64::MAX))
        .unwrap();
    let (before, before_secret) = vault.get_service_credential(saved.id).unwrap();

    let renamed = vault
        .rename_service_credential(saved.id, "  new-label  ")
        .unwrap();
    let (after, after_secret) = vault.get_service_credential(saved.id).unwrap();
    assert_eq!(renamed.id, saved.id);
    assert_eq!(renamed.label, "new-label");
    assert_eq!(after.id, before.id);
    assert_eq!(after.label, "new-label");
    assert_eq!(after.provider, before.provider);
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(after.disabled, before.disabled);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.last_used_at, before.last_used_at);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after_secret.expose_secret(), before_secret.expose_secret());
}

#[test]
fn service_credential_rename_rejects_duplicate_label_without_mutating_source() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let source = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "source",
            serde_json::json!({"project": "source"}),
            &ServiceCredentialSecret::new("source-secret"),
        )
        .unwrap();
    vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "taken",
            serde_json::json!({"project": "taken"}),
            &ServiceCredentialSecret::new("taken-secret"),
        )
        .unwrap();

    assert!(vault.rename_service_credential(source.id, "taken").is_err());
    let (unchanged, secret) = vault.get_service_credential(source.id).unwrap();
    assert_eq!(unchanged.label, "source");
    assert_eq!(secret.expose_secret(), "source-secret");
}

#[test]
fn service_credential_snapshot_restore_preserves_secret_metadata_and_health() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "default",
            serde_json::json!({"project": "old"}),
            &ServiceCredentialSecret::new("old-secret"),
        )
        .unwrap();
    vault
        .mark_service_credential_failure(saved.id, "old failure", true, Some(u64::MAX))
        .unwrap();
    let (snapshot, snapshot_secret) = vault.get_service_credential(saved.id).unwrap();
    vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "default",
            serde_json::json!({"project": "candidate"}),
            &ServiceCredentialSecret::new("candidate-secret"),
        )
        .unwrap();

    vault
        .restore_service_credential(&snapshot, &snapshot_secret)
        .unwrap();
    let (restored, restored_secret) = vault.get_service_credential(saved.id).unwrap();
    assert_eq!(restored.id, snapshot.id);
    assert_eq!(restored.provider, snapshot.provider);
    assert_eq!(restored.label, snapshot.label);
    assert_eq!(restored.metadata, snapshot.metadata);
    assert_eq!(restored.disabled, snapshot.disabled);
    assert_eq!(restored.cooldown_until, snapshot.cooldown_until);
    assert_eq!(restored.last_used_at, snapshot.last_used_at);
    assert_eq!(restored.last_error, snapshot.last_error);
    assert_eq!(restored.created_at, snapshot.created_at);
    assert_eq!(restored.updated_at, snapshot.updated_at);
    assert_eq!(restored_secret.expose_secret(), "old-secret");
}

#[test]
fn inspect_is_read_only_and_validates_all_encrypted_records() {
    let root = tempdir().unwrap();
    let path = root.path().join("agent-platform.db");
    let fresh = LocalVault::inspect(&path);
    assert_eq!(fresh.status, VaultDataStatus::Fresh);
    assert!(!path.exists());
    assert!(!root.path().join("vault.key").exists());

    let vault = LocalVault::open(&path).unwrap();
    let profile = vault
        .upsert(Platform::X, "Creator", &profile_secret("profile"))
        .unwrap();
    vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "default",
            serde_json::json!({"project": "readable"}),
            &ServiceCredentialSecret::new("secret"),
        )
        .unwrap();
    drop(vault);

    let ready = LocalVault::inspect(&path);
    assert_eq!(ready.status, VaultDataStatus::Ready);
    assert_eq!(ready.profile_directories.len(), 1);
    assert_eq!(ready.profile_directories[0].profile_id, profile.id);
    assert_eq!(
        ready.profile_directories[0].browser_profile_dir,
        "/tmp/profile"
    );

    std::fs::write(root.path().join("vault.key"), [7_u8; 32]).unwrap();
    let unreadable = LocalVault::inspect(&path);
    assert!(matches!(
        unreadable.status,
        VaultDataStatus::Unsupported { ref reason, .. } if reason.contains("encryption error")
    ));
}

#[test]
fn inspect_rejects_incomplete_store_without_creating_the_missing_peer() {
    let root = tempdir().unwrap();
    let path = root.path().join("agent-platform.db");
    Connection::open(&path).unwrap();
    let inspected = LocalVault::inspect(&path);
    assert!(matches!(
        inspected.status,
        VaultDataStatus::Unsupported { ref reason, .. } if reason.contains("credential key is missing")
    ));
    assert!(!root.path().join("vault.key").exists());
}

#[test]
fn inspect_rejects_a_non_regular_credential_key() {
    let root = tempdir().unwrap();
    let path = root.path().join("agent-platform.db");
    drop(LocalVault::open(&path).unwrap());
    std::fs::remove_file(root.path().join("vault.key")).unwrap();
    std::fs::create_dir(root.path().join("vault.key")).unwrap();
    assert!(matches!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::Unsupported { ref reason, .. } if reason.contains("regular file")
    ));
}

#[cfg(unix)]
#[test]
fn inspect_rejects_a_credential_key_with_broad_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempdir().unwrap();
    let path = root.path().join("agent-platform.db");
    drop(LocalVault::open(&path).unwrap());
    let key = root.path().join("vault.key");
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::Unsupported { ref reason, .. } if reason.contains("permissions are too broad")
    ));

    drop(LocalVault::open(&path).expect("explicit open repairs permissions"));
    assert_eq!(
        std::fs::metadata(key).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn a_late_success_never_erases_newer_cooldown_or_disable_state() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "youtube",
            serde_json::json!({"project": "cooldown-race"}),
            &ServiceCredentialSecret::new("youtube_secret"),
        )
        .unwrap();

    // A fresh rate-limit cooldown lands, then a slow in-flight operation
    // reports success: the cooldown must survive.
    vault
        .mark_service_credential_failure(saved.id, "rate limited", false, Some(u64::MAX))
        .unwrap();
    vault.mark_service_credential_success(saved.id).unwrap();
    let (credential, _) = vault.get_service_credential(saved.id).unwrap();
    assert_eq!(credential.cooldown_until, Some(i64::MAX as u64));
    assert_eq!(credential.last_error.as_deref(), Some("rate limited"));

    // Same race against a terminal disable: success must not re-enable.
    vault
        .mark_service_credential_failure(saved.id, "invalid key", true, None)
        .unwrap();
    vault.mark_service_credential_success(saved.id).unwrap();
    let (credential, _) = vault.get_service_credential(saved.id).unwrap();
    assert!(credential.disabled);
    assert_eq!(credential.last_error.as_deref(), Some("invalid key"));

    // An expired cooldown on a healthy credential is cleared by success.
    vault
        .mark_service_credential_failure(saved.id, "stale", false, Some(1))
        .unwrap();
    vault.mark_service_credential_success(saved.id).unwrap();
    let (credential, _) = vault.get_service_credential(saved.id).unwrap();
    assert!(!credential.disabled);
    assert_eq!(credential.cooldown_until, None);
    assert_eq!(credential.last_error, None);
    assert!(credential.last_used_at.is_some());
}

#[test]
fn revision_bound_health_commit_cannot_overwrite_newer_failure() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "youtube-revision",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("youtube_secret"),
        )
        .unwrap();
    let opened_revision = saved.updated_at.clone();

    vault
        .mark_service_credential_failure(saved.id, "newer transient failure", false, None)
        .unwrap();
    let (after_failure, _) = vault.get_service_credential(saved.id).unwrap();
    assert_ne!(after_failure.updated_at, opened_revision);

    assert_eq!(
        vault
            .mark_service_credential_success_if_revision(saved.id, &opened_revision)
            .unwrap(),
        ServiceCredentialHealthCommit::Superseded
    );
    let (after_stale_success, _) = vault.get_service_credential(saved.id).unwrap();
    assert_eq!(
        after_stale_success.last_error.as_deref(),
        Some("newer transient failure")
    );
    assert!(!after_stale_success.disabled);
}

#[test]
fn concurrent_revision_bound_health_observations_keep_the_newer_failure() {
    let root = tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "youtube-barrier",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("youtube_secret"),
        )
        .unwrap();
    let opened_revision = saved.updated_at.clone();
    let started = std::sync::Arc::new(std::sync::Barrier::new(2));
    let release = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_vault = vault.clone();
    let worker_started = std::sync::Arc::clone(&started);
    let worker_release = std::sync::Arc::clone(&release);
    let worker = std::thread::spawn(move || {
        worker_started.wait();
        worker_release.wait();
        worker_vault
            .mark_service_credential_success_if_revision(saved.id, &opened_revision)
            .unwrap()
    });

    started.wait();
    vault
        .mark_service_credential_failure(saved.id, "barrier newer failure", false, None)
        .unwrap();
    release.wait();
    assert_eq!(
        worker.join().unwrap(),
        ServiceCredentialHealthCommit::Superseded
    );
    assert_eq!(
        vault
            .get_service_credential(saved.id)
            .unwrap()
            .0
            .last_error
            .as_deref(),
        Some("barrier newer failure")
    );
}

#[test]
fn credential_revision_overflow_or_corruption_fails_closed() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let saved = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "revision-boundary",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("youtube_secret"),
        )
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE service_credentials SET updated_at = ?2 WHERE id = ?1",
            rusqlite::params![saved.id.to_string(), u64::MAX.to_string()],
        )
        .unwrap();
    let exhausted = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "revision-boundary",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("new_secret"),
        )
        .unwrap_err()
        .to_string();
    assert!(exhausted.contains("revision is exhausted"), "{exhausted}");

    connection
        .execute(
            "UPDATE service_credentials SET updated_at = 'not-a-revision' WHERE id = ?1",
            [saved.id.to_string()],
        )
        .unwrap();
    let malformed = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "revision-boundary",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new("new_secret"),
        )
        .unwrap_err()
        .to_string();
    assert!(malformed.contains("revision is invalid"), "{malformed}");
}

#[test]
fn frozen_v2_is_read_only_upgrade_required_and_normal_open_does_not_migrate() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    create_empty_v2(&path);
    let database_before = std::fs::read(&path).unwrap();
    let key_before = std::fs::read(root.path().join("vault.key")).unwrap();

    assert_eq!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::UpgradeRequired {
            from_version: 2,
            to_version: CURRENT_SCHEMA_VERSION,
        }
    );
    assert!(matches!(
        LocalVault::open(&path),
        Err(crate::VaultError::UpgradeRequired {
            from_version: 2,
            to_version: CURRENT_SCHEMA_VERSION,
        })
    ));
    assert_eq!(std::fs::read(&path).unwrap(), database_before);
    assert_eq!(
        std::fs::read(root.path().join("vault.key")).unwrap(),
        key_before
    );
    assert!(!path.with_extension("db-wal").exists());
    assert!(!path.with_extension("db-shm").exists());
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup-")
    }));
}

#[test]
fn v1_and_v2_structural_drift_fail_closed() {
    let old_root = tempdir().unwrap();
    let old_path = old_root.path().join("vault.db");
    create_empty_v2(&old_path);
    Connection::open(&old_path)
        .unwrap()
        .pragma_update(None, "user_version", 1)
        .unwrap();
    assert!(matches!(
        LocalVault::inspect(&old_path).status,
        VaultDataStatus::Unsupported {
            created_by_newer_version: false,
            ..
        }
    ));

    let drift_root = tempdir().unwrap();
    let drift_path = drift_root.path().join("vault.db");
    create_empty_v2(&drift_path);
    Connection::open(&drift_path)
        .unwrap()
        .execute("CREATE TABLE drifted (id TEXT PRIMARY KEY)", [])
        .unwrap();
    assert!(matches!(
        LocalVault::inspect(&drift_path).status,
        VaultDataStatus::Unsupported { ref reason, .. }
            if reason.contains("unexpected: drifted")
    ));
}

#[test]
fn v2_to_v4_upgrade_preserves_every_unaffected_row_and_creates_private_recovery_point() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    vault
        .upsert(Platform::X, "Creator", &profile_secret("upgrade"))
        .unwrap();
    let credential = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "default",
            serde_json::json!({"project": "preserved"}),
            &ServiceCredentialSecret::new("generated-test-value"),
        )
        .unwrap();
    vault
        .mark_service_credential_failure(credential.id, "sanitized failure", false, Some(500))
        .unwrap();
    drop(vault);
    downgrade_current_to_v2(&path);
    let before = stored_rows(&path);

    let result = LocalVault::upgrade(&path).unwrap();
    assert_eq!(result.status, VaultDataUpgradeStatus::Completed);
    assert_eq!(result.from_version, Some(2));
    assert_eq!(result.to_version, CURRENT_SCHEMA_VERSION);
    let backup = result.backup_path.unwrap();
    assert!(backup.is_file());
    assert!(stored_rows(&path) == before);
    assert!(stored_rows(&backup) == before);
    assert_eq!(LocalVault::inspect(&path).status, VaultDataStatus::Ready);
    assert_eq!(
        LocalVault::inspect(&backup).status,
        VaultDataStatus::UpgradeRequired {
            from_version: 2,
            to_version: CURRENT_SCHEMA_VERSION,
        }
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let current = LocalVault::upgrade(&path).unwrap();
    assert_eq!(current.status, VaultDataUpgradeStatus::AlreadyCurrent);
    assert!(current.backup_path.is_none());
}

#[test]
fn v3_to_v4_upgrade_types_legacy_resend_without_changing_secret_or_identity() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    create_empty_v3(&path);
    let legacy_id = Uuid::new_v4();
    let cipher = crate::crypto::VaultCipher::load_existing(&root.path().join("vault.key")).unwrap();
    let encrypted = cipher
        .encrypt(&Zeroizing::new(
            serde_json::to_vec(&serde_json::json!({"value": "re_preserved_secret"})).unwrap(),
        ))
        .unwrap();
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO service_credentials
               (id, provider, label, metadata_json, encrypted_secret, disabled,
                cooldown_until, last_used_at, last_error, created_at, updated_at)
             VALUES (?1, 'resend', 'legacy-mailbox', ?2, ?3, 0, NULL, 123, NULL, '100', '200')",
            rusqlite::params![
                legacy_id.to_string(),
                serde_json::json!({
                    "email_address": "legacy@example.com",
                    "display_name": "Legacy"
                })
                .to_string(),
                encrypted,
            ],
        )
        .unwrap();
    let before = stored_rows(&path);
    let before_row = before
        .credentials
        .iter()
        .find(|row| row.id == legacy_id.to_string())
        .unwrap();
    assert_eq!(before_row.provider, "resend");

    let result = LocalVault::upgrade(&path).unwrap();
    assert_eq!(result.from_version, Some(PREVIOUS_SCHEMA_VERSION));
    assert_eq!(result.to_version, CURRENT_SCHEMA_VERSION);
    let backup = result.backup_path.unwrap();
    assert_eq!(
        LocalVault::inspect(&backup).status,
        VaultDataStatus::UpgradeRequired {
            from_version: PREVIOUS_SCHEMA_VERSION,
            to_version: CURRENT_SCHEMA_VERSION,
        }
    );

    let after = stored_rows(&path);
    let after_row = after
        .credentials
        .iter()
        .find(|row| row.id == legacy_id.to_string())
        .unwrap();
    assert_eq!(after_row.provider, "email_resend_api_key");
    assert_eq!(after_row.id, before_row.id);
    assert_eq!(after_row.label, before_row.label);
    assert_eq!(after_row.metadata_json, before_row.metadata_json);
    assert_eq!(after_row.encrypted_secret, before_row.encrypted_secret);
    assert_eq!(after_row.disabled, before_row.disabled);
    assert_eq!(after_row.cooldown_until, before_row.cooldown_until);
    assert_eq!(after_row.last_used_at, before_row.last_used_at);
    assert_eq!(after_row.last_error, before_row.last_error);
    assert_eq!(after_row.created_at, before_row.created_at);
    assert_eq!(after_row.updated_at, before_row.updated_at);

    let reopened = LocalVault::open(&path).unwrap();
    let (record, secret) = reopened
        .get_email_secret_record_for_connection(
            &legacy_id.to_string(),
            EmailSecretRecordKind::ResendApiKey,
        )
        .unwrap();
    assert_eq!(record.id, legacy_id);
    assert_eq!(secret.expose_secret(), "re_preserved_secret");
}

#[test]
fn online_backup_and_upgrade_include_committed_wal_rows() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let credential = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "base",
            serde_json::json!({"project": "wal"}),
            &ServiceCredentialSecret::new("generated-wal-value"),
        )
        .unwrap();
    drop(vault);
    downgrade_current_to_v2(&path);

    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    writer
        .execute(
            "INSERT INTO service_credentials
               (id, provider, label, metadata_json, encrypted_secret, disabled,
                cooldown_until, last_used_at, last_error, created_at, updated_at)
             SELECT ?1, provider, 'wal-only', metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at
             FROM service_credentials WHERE id = ?2",
            [Uuid::new_v4().to_string(), credential.id.to_string()],
        )
        .unwrap();
    assert!(path.with_extension("db-wal").exists());

    let result = LocalVault::upgrade(&path).unwrap();
    let backup = result.backup_path.unwrap();
    let count = |candidate: &std::path::Path| {
        Connection::open(candidate)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM service_credentials", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    assert_eq!(count(&path), 2);
    assert_eq!(count(&backup), 2);
    assert_eq!(
        Connection::open(&backup)
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
}

#[cfg(windows)]
#[test]
fn private_backup_handle_allows_sqlite_open_without_releasing_identity() {
    use crate::path_security::{
        AnchoredParentPath, same_file_identity, set_private_file_permissions,
    };

    let root = tempdir().unwrap();
    let path = root.path().join("backup.db");
    let anchored = AnchoredParentPath::new(&path).unwrap();
    let created = anchored.create_final_private().unwrap();
    set_private_file_permissions(&created).unwrap();
    let connection = Connection::open(&path).expect("SQLite must coexist with the identity handle");
    let reopened = anchored.open_final_nofollow(false).unwrap();
    assert!(same_file_identity(&created, &reopened).unwrap());
    drop(connection);
    drop(reopened);
    anchored.remove_verified_final(&created).unwrap();
    assert!(!path.exists());
}

#[test]
fn locked_revalidation_rejects_a_changed_source_before_backup() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let credential = vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "before",
            serde_json::json!({}),
            &ServiceCredentialSecret::new("generated-change-value"),
        )
        .unwrap();
    drop(vault);
    downgrade_current_to_v2(&path);

    let error = upgrade_with_hooks(
        &path,
        || {
            Connection::open(&path)
                .unwrap()
                .execute(
                    "UPDATE service_credentials SET label = 'after' WHERE id = ?1",
                    [credential.id.to_string()],
                )
                .unwrap();
        },
        |_| {},
        || Ok(()),
        || Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.support_code, "LOCAL_VAULT_SOURCE_CHANGED");
    assert!(!error.storage_upgraded);
    assert_eq!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::UpgradeRequired {
            from_version: 2,
            to_version: CURRENT_SCHEMA_VERSION,
        }
    );
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup-")
    }));
}

#[test]
fn concurrent_writer_returns_busy_without_backup_or_migration() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    create_empty_v2(&path);
    let mut writer = Connection::open(&path).unwrap();
    let transaction = writer
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();

    let error = LocalVault::upgrade(&path).unwrap_err();
    assert_eq!(error.kind, VaultDataUpgradeFailureKind::Busy);
    assert_eq!(error.support_code, "LOCAL_VAULT_BUSY");
    assert!(!error.storage_upgraded);
    assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("backup-")
    }));
    transaction.rollback().unwrap();
}

#[test]
fn invalid_backup_is_cleaned_and_source_remains_v2() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    create_empty_v2(&path);
    let backup_path = std::cell::RefCell::new(None::<std::path::PathBuf>);

    let error = upgrade_with_hooks(
        &path,
        || {},
        |backup| {
            backup_path.replace(Some(backup.to_owned()));
            std::fs::write(backup, b"invalid recovery point").unwrap();
        },
        || Ok(()),
        || Ok(()),
    )
    .unwrap_err();
    assert_eq!(error.kind, VaultDataUpgradeFailureKind::BackupFailed);
    assert_eq!(error.support_code, "LOCAL_VAULT_BACKUP_INVALID");
    assert!(!error.storage_upgraded);
    let backup = backup_path.into_inner().unwrap();
    assert!(!backup.exists());
    assert!(!std::path::PathBuf::from(format!("{}-wal", backup.display())).exists());
    assert!(!std::path::PathBuf::from(format!("{}-shm", backup.display())).exists());
    assert!(matches!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::UpgradeRequired { .. }
    ));
}

#[test]
fn precommit_failure_rolls_back_and_postcommit_failure_reports_applied_storage() {
    let rollback_root = tempdir().unwrap();
    let rollback_path = rollback_root.path().join("vault.db");
    create_empty_v2(&rollback_path);
    let rollback_backup = std::cell::RefCell::new(None::<std::path::PathBuf>);
    let rollback_error = upgrade_with_hooks(
        &rollback_path,
        || {},
        |backup| {
            rollback_backup.replace(Some(backup.to_owned()));
        },
        || {
            Err(VaultDataUpgradeError {
                kind: VaultDataUpgradeFailureKind::UnsafeData,
                support_code: "TEST_PRECOMMIT_FAILURE".into(),
                storage_upgraded: false,
            })
        },
        || Ok(()),
    )
    .unwrap_err();
    assert!(!rollback_error.storage_upgraded);
    assert!(matches!(
        LocalVault::inspect(&rollback_path).status,
        VaultDataStatus::UpgradeRequired { .. }
    ));
    assert!(rollback_backup.into_inner().unwrap().is_file());

    let committed_root = tempdir().unwrap();
    let committed_path = committed_root.path().join("vault.db");
    create_empty_v2(&committed_path);
    let committed_backup = std::cell::RefCell::new(None::<std::path::PathBuf>);
    let committed_error = upgrade_with_hooks(
        &committed_path,
        || {},
        |backup| {
            committed_backup.replace(Some(backup.to_owned()));
        },
        || Ok(()),
        || {
            Err(VaultDataUpgradeError {
                kind: VaultDataUpgradeFailureKind::InitializationFailed,
                support_code: "TEST_POSTCOMMIT_FAILURE".into(),
                storage_upgraded: false,
            })
        },
    )
    .unwrap_err();
    assert!(committed_error.storage_upgraded);
    assert_eq!(
        LocalVault::inspect(&committed_path).status,
        VaultDataStatus::Ready
    );
    assert!(committed_backup.into_inner().unwrap().is_file());
}

#[test]
fn creator_sync_metadata_is_strict_and_bearer_round_trip_is_redacted() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let metadata = creator_sync_metadata();
    let bearer = creator_sync_bearer(&metadata);

    assert!(
        vault
            .upsert_service_credential(
                ServiceProvider::CreatorSync,
                "other",
                serde_json::json!({}),
                &bearer,
            )
            .is_err()
    );
    let saved = vault
        .save_creator_sync_credential(&metadata, &bearer)
        .unwrap();
    let (readback, readback_bearer) = match vault.get_creator_sync_credential().unwrap().unwrap() {
        VaultCreatorSyncCredentialState::Active { credential, bearer } => (credential, bearer),
        VaultCreatorSyncCredentialState::NonActive { .. } => {
            panic!("newly saved Creator Sync credential must be active")
        }
    };
    assert_eq!(readback, saved);
    assert_eq!(readback.metadata, metadata);
    assert_eq!(readback_bearer.expose_secret(), bearer.expose_secret());
    assert!(!format!("{readback:?}").contains(&"A".repeat(43)));
    assert!(
        vault
            .mark_service_credential_failure(
                saved.vault_id,
                bearer.expose_secret(),
                false,
                Some(u64::MAX),
            )
            .is_err()
    );

    let connection = vault.connection.lock().unwrap();
    let (label, metadata_json, encrypted, last_error) = connection
        .query_row(
            "SELECT label, metadata_json, encrypted_secret, last_error FROM service_credentials
             WHERE provider = 'creator_sync'",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .unwrap();
    drop(connection);
    assert_eq!(label, "creator-sync");
    assert!(!metadata_json.contains(bearer.expose_secret()));
    assert!(!String::from_utf8_lossy(&encrypted).contains(bearer.expose_secret()));
    assert!(last_error.is_none());
    let encoded = serde_json::to_value(&metadata).unwrap();
    assert!(encoded.get("pairing_code").is_none());
    assert!(encoded.get("pairing_verifier").is_none());
    assert!(encoded.get("pkce_verifier").is_none());
    assert!(encoded.get("pairing_state").is_none());
    assert!(encoded.get("state").is_none());
    assert!(encoded.get("admission").is_none());

    let mut missing_pairing_id = encoded.clone();
    missing_pairing_id
        .as_object_mut()
        .unwrap()
        .remove("access_pairing_id");
    assert!(serde_json::from_value::<CreatorSyncCredentialMetadata>(missing_pairing_id).is_err());
    let mut retired_activation_id = encoded.clone();
    let pairing_id = retired_activation_id
        .as_object_mut()
        .unwrap()
        .remove("access_pairing_id")
        .unwrap();
    retired_activation_id
        .as_object_mut()
        .unwrap()
        .insert("access_activation_id".into(), pairing_id);
    assert!(
        serde_json::from_value::<CreatorSyncCredentialMetadata>(retired_activation_id).is_err()
    );

    let mut unknown = encoded;
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), serde_json::Value::Bool(true));
    assert!(serde_json::from_value::<CreatorSyncCredentialMetadata>(unknown).is_err());
    let mut nested_unknown = serde_json::to_value(&metadata).unwrap();
    nested_unknown["accepted_bundle"]["unexpected"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<CreatorSyncCredentialMetadata>(nested_unknown).is_err());

    let invalid_bearer =
        ServiceCredentialSecret::new(format!("{}_{}", Uuid::new_v4(), "B".repeat(43)));
    let error = vault
        .save_creator_sync_credential(&metadata, &invalid_bearer)
        .unwrap_err();
    assert!(!error.to_string().contains(invalid_bearer.expose_secret()));
    assert!(vault.delete_creator_sync_credential().unwrap());
    assert!(vault.get_creator_sync_credential().unwrap().is_none());
}

#[test]
fn creator_sync_rejects_revoked_save_unknown_metadata_and_ambiguous_rows() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let metadata = creator_sync_metadata();
    let bearer = creator_sync_bearer(&metadata);
    let saved = vault
        .save_creator_sync_credential(&metadata, &bearer)
        .unwrap();

    let mut revoked = metadata.clone();
    revoked.status = CreatorSyncCredentialStatus::Revoked;
    revoked.revoked_at = Some(102);
    assert!(
        vault
            .save_creator_sync_credential(&revoked, &bearer)
            .is_err()
    );

    let connection = vault.connection.lock().unwrap();
    connection
        .execute(
            "INSERT INTO service_credentials
               (id, provider, label, metadata_json, encrypted_secret, disabled,
                cooldown_until, last_used_at, last_error, created_at, updated_at)
             SELECT ?1, provider, 'ambiguous', metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at
             FROM service_credentials WHERE id = ?2",
            [Uuid::new_v4().to_string(), saved.vault_id.to_string()],
        )
        .unwrap();
    drop(connection);
    let error = match vault.get_creator_sync_credential() {
        Ok(_) => panic!("ambiguous Creator Sync rows must be rejected"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("ambiguous"));
    drop(vault);
    assert!(matches!(
        LocalVault::inspect(&path).status,
        VaultDataStatus::Unsupported { .. }
    ));
}

#[test]
fn crypto_plaintext_buffers_are_zeroizing_and_round_trip() {
    let root = tempdir().unwrap();
    let cipher =
        crate::crypto::VaultCipher::load_or_create(&root.path().join("vault.key")).unwrap();
    let plaintext = Zeroizing::new(b"generated plaintext".to_vec());
    let encrypted = cipher.encrypt(&plaintext).unwrap();
    let decrypted: Zeroizing<Vec<u8>> = cipher.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted.as_slice(), plaintext.as_slice());
}

#[test]
fn creator_sync_invalidation_is_durable_and_never_returns_a_bearer() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let metadata = creator_sync_metadata();
    let bearer = creator_sync_bearer(&metadata);
    let saved = vault
        .save_creator_sync_credential(&metadata, &bearer)
        .unwrap();
    assert!(vault.get_service_credential(saved.vault_id).is_err());
    assert!(vault.delete_service_credential(saved.vault_id).is_err());
    let encrypted_before = vault
        .connection
        .lock()
        .unwrap()
        .query_row(
            "SELECT encrypted_secret FROM service_credentials WHERE id = ?1",
            [saved.vault_id.to_string()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .unwrap();

    let tombstone = vault.invalidate_creator_sync_credential().unwrap().unwrap();
    assert_eq!(
        tombstone.metadata.status,
        CreatorSyncCredentialStatus::Invalidated
    );
    let (disabled, encrypted_after) = vault
        .connection
        .lock()
        .unwrap()
        .query_row(
            "SELECT disabled, encrypted_secret FROM service_credentials WHERE id = ?1",
            [saved.vault_id.to_string()],
            |row| Ok((row.get::<_, bool>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .unwrap();
    assert!(disabled);
    assert_eq!(encrypted_after, encrypted_before);
    match vault.get_creator_sync_credential().unwrap().unwrap() {
        VaultCreatorSyncCredentialState::NonActive { credential } => {
            assert_eq!(credential, tombstone);
        }
        VaultCreatorSyncCredentialState::Active { .. } => {
            panic!("invalidated credential must not release a bearer")
        }
    }

    drop(vault);
    let reopened = LocalVault::open(&path).unwrap();
    match reopened.get_creator_sync_credential().unwrap().unwrap() {
        VaultCreatorSyncCredentialState::NonActive { credential } => {
            assert_eq!(
                credential.metadata.status,
                CreatorSyncCredentialStatus::Invalidated
            );
        }
        VaultCreatorSyncCredentialState::Active { .. } => {
            panic!("restart must not reactivate an invalidated credential")
        }
    }
    assert!(reopened.delete_creator_sync_credential().unwrap());
    assert!(reopened.get_creator_sync_credential().unwrap().is_none());
}

#[test]
fn creator_sync_delete_readback_failure_preserves_the_durable_tombstone() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let metadata = creator_sync_metadata();
    let bearer = creator_sync_bearer(&metadata);
    vault
        .save_creator_sync_credential(&metadata, &bearer)
        .unwrap();

    let error = vault
        .delete_creator_sync_credential_with_readback_hook(|| {
            Err(crate::VaultError::Database(
                "injected delete readback failure".into(),
            ))
        })
        .unwrap_err();
    assert!(!error.to_string().contains(bearer.expose_secret()));
    drop(vault);

    let reopened = LocalVault::open(&path).unwrap();
    match reopened.get_creator_sync_credential().unwrap().unwrap() {
        VaultCreatorSyncCredentialState::NonActive { credential } => {
            assert_eq!(
                credential.metadata.status,
                CreatorSyncCredentialStatus::Invalidated
            );
        }
        VaultCreatorSyncCredentialState::Active { .. } => {
            panic!("failed deletion must preserve the non-active tombstone")
        }
    }
    assert!(reopened.delete_creator_sync_credential().unwrap());
}

#[test]
fn creator_sync_invalidation_readback_failure_rolls_back_to_active() {
    let root = tempdir().unwrap();
    let path = root.path().join("vault.db");
    let vault = LocalVault::open(&path).unwrap();
    let metadata = creator_sync_metadata();
    let bearer = creator_sync_bearer(&metadata);
    vault
        .save_creator_sync_credential(&metadata, &bearer)
        .unwrap();

    let error = vault
        .invalidate_creator_sync_credential_with_readback_hook(|| {
            Err(crate::VaultError::Database(
                "injected invalidation readback failure".into(),
            ))
        })
        .unwrap_err();
    assert!(!error.to_string().contains(bearer.expose_secret()));
    match vault.get_creator_sync_credential().unwrap().unwrap() {
        VaultCreatorSyncCredentialState::Active {
            credential,
            bearer: readback,
        } => {
            assert_eq!(
                credential.metadata.status,
                CreatorSyncCredentialStatus::Active
            );
            assert_eq!(readback.expose_secret(), bearer.expose_secret());
        }
        VaultCreatorSyncCredentialState::NonActive { .. } => {
            panic!("failed invalidation must roll back the tombstone transaction")
        }
    }
}
