CREATE TABLE gmail_bulk_batches (
 id TEXT PRIMARY KEY,
 mailbox_ref_id TEXT NOT NULL REFERENCES mailbox_refs(id),
 subject TEXT NOT NULL,
 body TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('draft','running','paused','cancelled','completed')),
 created_at INTEGER NOT NULL
);
CREATE TABLE gmail_bulk_items (
 id TEXT PRIMARY KEY,
 batch_id TEXT NOT NULL REFERENCES gmail_bulk_batches(id),
 recipient TEXT NOT NULL,
 source TEXT NOT NULL,
 fingerprint TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','attempting','accepted','sent','failed','unknown','cancelled')),
 provider_message_id TEXT,
 detail TEXT,
 UNIQUE(batch_id,recipient)
);
CREATE INDEX gmail_bulk_pending ON gmail_bulk_items(batch_id,state);
CREATE INDEX gmail_bulk_duplicates ON gmail_bulk_items(fingerprint,state);
CREATE TABLE gmail_send_attempts (
 idempotency_key TEXT PRIMARY KEY
);
INSERT INTO gmail_send_attempts(idempotency_key)
 SELECT i.key FROM idempotency_records i JOIN mailbox_refs m ON m.id=i.mailbox_ref_id
 WHERE m.provider='gmail' AND i.action IN ('email_send','manual_email_send','email_draft_send')
 AND i.state IN ('remote_started','remote_accepted','remote_verified','completed');
