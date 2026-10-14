CREATE TABLE desktop_integration_authorization (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  device_id TEXT NOT NULL REFERENCES access_device(id) ON DELETE CASCADE,
  pairing_id TEXT NOT NULL,
  installation_id TEXT NOT NULL,
  credential_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  request_hash TEXT NOT NULL,
  locale TEXT NOT NULL CHECK(locale IN ('zh-CN', 'en-US')),
  status TEXT NOT NULL CHECK(status IN ('pending_browser', 'pending_provider', 'processing', 'connected', 'failed', 'cancelled', 'expired')),
  session_id TEXT,
  connection_id TEXT REFERENCES third_party_connection(id),
  error_code TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  retain_until INTEGER NOT NULL,
  UNIQUE(user_id, device_id, provider, idempotency_key)
);
CREATE INDEX desktop_authorization_pending ON desktop_integration_authorization(user_id, provider, status, expires_at);
CREATE INDEX desktop_authorization_retention ON desktop_integration_authorization(retain_until);

ALTER TABLE oauth_authorization_transaction ADD COLUMN desktop_authorization_id TEXT REFERENCES desktop_integration_authorization(id);
CREATE UNIQUE INDEX oauth_transaction_desktop_authorization ON oauth_authorization_transaction(desktop_authorization_id) WHERE desktop_authorization_id IS NOT NULL;

ALTER TABLE third_party_connection ADD COLUMN revocation_state TEXT NOT NULL DEFAULT 'none' CHECK(revocation_state IN ('none', 'revoking', 'revocation_unknown'));
CREATE INDEX third_party_connection_page ON third_party_connection(user_id, provider, created_at DESC, id DESC);
CREATE INDEX third_party_connection_revocation_progress ON third_party_connection(user_id, provider, revocation_state, updated_at);

CREATE TABLE integration_revocation (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  device_id TEXT NOT NULL REFERENCES access_device(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  connection_id TEXT NOT NULL REFERENCES third_party_connection(id),
  idempotency_key TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('running', 'revoked', 'rejected', 'unknown')),
  error_code TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  retain_until INTEGER NOT NULL,
  UNIQUE(user_id, device_id, provider, idempotency_key)
);
CREATE INDEX integration_revocation_connection ON integration_revocation(connection_id, status);
CREATE INDEX integration_revocation_retention ON integration_revocation(retain_until);

CREATE TABLE integration_read_request (
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  device_id TEXT NOT NULL REFERENCES access_device(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  request_id TEXT NOT NULL,
  operation TEXT,
  status TEXT NOT NULL CHECK(status IN ('running', 'succeeded', 'failed', 'cancelled')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  PRIMARY KEY(user_id, device_id, provider, request_id)
);
CREATE INDEX integration_read_expiry ON integration_read_request(expires_at);
CREATE INDEX integration_read_owner_created ON integration_read_request(user_id, device_id, provider, created_at);
