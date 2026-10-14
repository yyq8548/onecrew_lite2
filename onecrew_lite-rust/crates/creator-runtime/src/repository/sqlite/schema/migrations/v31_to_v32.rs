use rusqlite::{Connection, params};
use uuid::Uuid;

// v31 -> v32 (Desktop app 0.5.15) separates the stable mailbox projection
// from its typed auth connection and opaque encrypted-secret record. Existing
// mailbox refs and every dependent mail fact remain untouched. The existing
// Resend service-credential UUID is copied as the bare typed
// secret-record id only as migration input. Runtime Email reads require the
// typed provider row; old generic service rows are not an Email fallback.
pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             CREATE TABLE mailbox_refs_rebuild (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL CHECK(provider IN ('resend','smtp','imap')),
               label TEXT NOT NULL,
               email_address TEXT NOT NULL,
               display_name TEXT,
               capabilities_json TEXT NOT NULL,
               available INTEGER NOT NULL,
               last_error TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             INSERT INTO mailbox_refs_rebuild
               (id, provider, label, email_address, display_name, capabilities_json,
                available, last_error, created_at, updated_at)
             SELECT id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at
             FROM mailbox_refs;
             DROP TABLE mailbox_refs;
             CREATE TABLE mailbox_refs (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL CHECK(provider IN ('resend','smtp','imap')),
               label TEXT NOT NULL,
               email_address TEXT NOT NULL,
               display_name TEXT,
               capabilities_json TEXT NOT NULL,
               available INTEGER NOT NULL,
               last_error TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             INSERT INTO mailbox_refs
               (id, provider, label, email_address, display_name, capabilities_json,
                available, last_error, created_at, updated_at)
             SELECT id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at
             FROM mailbox_refs_rebuild;
             DROP TABLE mailbox_refs_rebuild;
             PRAGMA foreign_keys = ON;
             CREATE TABLE email_auth_connections (
               id TEXT PRIMARY KEY,
               provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
               secret_record_id TEXT NOT NULL CHECK(length(trim(secret_record_id)) > 0),
               label TEXT NOT NULL CHECK(length(trim(label)) > 0),
               available INTEGER NOT NULL CHECK(available IN (0,1)),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(id, provider_kind),
               UNIQUE(secret_record_id)
             );
             CREATE TABLE mailbox_identity_mappings (
               mailbox_ref_id TEXT PRIMARY KEY REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               auth_connection_id TEXT NOT NULL,
               provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
               projection_kind TEXT NOT NULL CHECK(projection_kind IN ('mailbox','send_as')),
               capability_projection_revision INTEGER NOT NULL CHECK(capability_projection_revision >= 1),
               checkpoint_namespace TEXT NOT NULL CHECK(length(trim(checkpoint_namespace)) BETWEEN 1 AND 256),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(auth_connection_id, provider_kind)
                 REFERENCES email_auth_connections(id, provider_kind) ON DELETE RESTRICT
             );
             CREATE INDEX mailbox_identity_mappings_by_connection
               ON mailbox_identity_mappings(auth_connection_id, mailbox_ref_id);",
        )
        .map_err(|error| error.to_string())?;

    let mut statement = connection
        .prepare(
            "SELECT id, provider, label, available, created_at, updated_at
             FROM mailbox_refs ORDER BY id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, u64>(4)?,
                row.get::<_, u64>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);

    for (mailbox_ref_id, provider, label, available, created_at, updated_at) in rows {
        if provider != "resend" {
            return Err(format!(
                "cannot migrate mailbox {mailbox_ref_id}: provider {provider} is unsupported"
            ));
        }
        let secret_record_id = mailbox_ref_id.clone();
        // Historical fixtures and manually inserted rows can carry a
        // non-UUID mailbox id even though the shipped Resend path uses the
        // vault credential UUID. Keep their mail history readable but mark
        // the typed connection unavailable so provider composition fails
        // closed until an explicit vault reconciliation supplies a valid
        // secret-record reference.
        let secret_uuid = Uuid::parse_str(&mailbox_ref_id).ok();
        let auth_connection_id = secret_uuid.map_or_else(
            || Uuid::new_v4().to_string(),
            |secret_id| {
                crate::resend_auth_connection_id(secret_id)
                    .as_str()
                    .to_owned()
            },
        );
        let checkpoint_namespace = secret_uuid.map_or_else(
            || format!("email-checkpoint-{}", Uuid::new_v4()),
            |_| format!("email-checkpoint-{mailbox_ref_id}"),
        );
        let auth_available = available && secret_uuid.is_some();
        connection
            .execute(
                "INSERT INTO email_auth_connections
                   (id, provider_kind, secret_record_id, label, available, created_at, updated_at)
                 VALUES (?1, 'resend', ?2, ?3, ?4, ?5, ?6)",
                params![
                    auth_connection_id,
                    secret_record_id,
                    label,
                    auth_available,
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO mailbox_identity_mappings
                   (mailbox_ref_id, auth_connection_id, provider_kind, projection_kind,
                    capability_projection_revision, checkpoint_namespace, created_at, updated_at)
                 VALUES (?1, ?2, 'resend', 'mailbox', 1, ?3, ?4, ?5)",
                params![
                    mailbox_ref_id,
                    auth_connection_id,
                    checkpoint_namespace,
                    created_at,
                    updated_at,
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
