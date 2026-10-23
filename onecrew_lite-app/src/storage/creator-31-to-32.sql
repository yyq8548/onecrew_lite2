CREATE TABLE email_auth_connections (
 id TEXT PRIMARY KEY,
 provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
 secret_record_id TEXT NOT NULL CHECK(length(trim(secret_record_id)) > 0),
 label TEXT NOT NULL CHECK(length(trim(label)) > 0),
 available INTEGER NOT NULL CHECK(available IN (0,1)),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 UNIQUE(id, provider_kind), UNIQUE(secret_record_id)
);
CREATE TABLE mailbox_identity_mappings (
 mailbox_ref_id TEXT PRIMARY KEY REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
 auth_connection_id TEXT NOT NULL,
 provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
 projection_kind TEXT NOT NULL CHECK(projection_kind IN ('mailbox','send_as')),
 capability_projection_revision INTEGER NOT NULL CHECK(capability_projection_revision >= 1),
 checkpoint_namespace TEXT NOT NULL CHECK(length(trim(checkpoint_namespace)) BETWEEN 1 AND 256),
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 FOREIGN KEY(auth_connection_id, provider_kind) REFERENCES email_auth_connections(id, provider_kind) ON DELETE RESTRICT
);
CREATE INDEX mailbox_identity_mappings_by_connection ON mailbox_identity_mappings(auth_connection_id, mailbox_ref_id);
