CREATE TABLE oauth_authorization_transaction (
  state_hash TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  session_id TEXT NOT NULL,
  locale TEXT NOT NULL CHECK(locale IN ('zh-CN', 'en-US')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  consumed_at INTEGER
);
CREATE INDEX oauth_transaction_user_provider_created ON oauth_authorization_transaction(user_id, provider, created_at);
CREATE INDEX oauth_transaction_expiry ON oauth_authorization_transaction(expires_at);
CREATE TABLE third_party_connection (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  token_ciphertext TEXT NOT NULL,
  token_nonce TEXT NOT NULL,
  token_key_version INTEGER NOT NULL,
  scope_json TEXT NOT NULL,
  account_ids_json TEXT NOT NULL DEFAULT '[]',
  status TEXT NOT NULL CHECK(status IN ('pending_verification', 'connected', 'verification_failed', 'revoked', 'cleanup_failed')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX third_party_connection_user_provider_created ON third_party_connection(user_id, provider, created_at);
