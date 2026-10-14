-- Mail OAuth belongs to the Access user. No device authorization job is stored.
CREATE TABLE mail_connection (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  provider_subject TEXT NOT NULL,
  email TEXT NOT NULL,
  token_ciphertext TEXT,
  token_nonce TEXT,
  token_key_version INTEGER,
  scope_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('connected','reauth_required','disconnecting','disconnected')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX mail_connection_owner ON mail_connection(user_id, created_at DESC);
CREATE INDEX mail_connection_grant ON mail_connection(user_id, provider, status);
CREATE UNIQUE INDEX mail_connection_active_subject ON mail_connection(provider, provider_subject) WHERE status != 'disconnected';

CREATE TABLE mail_oauth_transaction (
  state_hash TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  user_id TEXT NOT NULL,
  pkce_ciphertext TEXT NOT NULL,
  pkce_nonce TEXT NOT NULL,
  pkce_key_version INTEGER NOT NULL,
  nonce TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  consumed_at INTEGER
);
CREATE INDEX mail_oauth_expiry ON mail_oauth_transaction(expires_at);
