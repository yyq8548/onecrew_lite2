-- Success receipts are internal deduplication records, never GA4 event parameters.
CREATE TABLE analytics_success_event (
  receipt_key TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  event_name TEXT NOT NULL CHECK (event_name IN ('sign_up', 'generate_lead')),
  created_at INTEGER NOT NULL,
  claimed_at INTEGER
);
CREATE INDEX analytics_success_event_pending
  ON analytics_success_event(user_id, created_at) WHERE claimed_at IS NULL;
