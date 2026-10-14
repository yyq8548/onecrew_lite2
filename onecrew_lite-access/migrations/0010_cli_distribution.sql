CREATE TABLE cli_install_ticket (
  token_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  version TEXT NOT NULL,
  manifest_hash TEXT NOT NULL,
  entitlement_revision INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);
CREATE INDEX cli_install_ticket_user_created ON cli_install_ticket(user_id, created_at);
