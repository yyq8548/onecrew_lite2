CREATE TEMP TABLE reply_triage_v33_copy AS
 SELECT id,channel,email_id,message_id,intent,summary,suggested_reply,created_at,acknowledged_at FROM reply_triage;
DROP TABLE reply_triage;
CREATE TABLE reply_triage (
 id TEXT PRIMARY KEY,
 channel TEXT NOT NULL CHECK(channel IN ('email','dm')),
 email_id TEXT REFERENCES emails(id) ON DELETE CASCADE,
 message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
 intent TEXT NOT NULL CHECK(intent IN ('interested','question','negotiating','declined','irrelevant','other')),
 summary TEXT NOT NULL, suggested_reply TEXT,
 created_at INTEGER NOT NULL, acknowledged_at INTEGER,
 CHECK((channel = 'email' AND email_id IS NOT NULL AND message_id IS NULL)
   OR (channel = 'dm' AND message_id IS NOT NULL AND email_id IS NULL))
);
CREATE UNIQUE INDEX reply_triage_one_per_email ON reply_triage(email_id) WHERE email_id IS NOT NULL;
CREATE UNIQUE INDEX reply_triage_one_per_message ON reply_triage(message_id) WHERE message_id IS NOT NULL;
CREATE INDEX reply_triage_open ON reply_triage(created_at DESC) WHERE acknowledged_at IS NULL;
INSERT INTO reply_triage SELECT * FROM reply_triage_v33_copy;
DROP TABLE reply_triage_v33_copy;
DROP TABLE task_attention;
DROP TABLE task_runs;
DROP TABLE tasks;
DELETE FROM idempotency_records WHERE action='task_create';
