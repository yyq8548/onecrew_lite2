use rusqlite::Connection;

// v30 -> v31 (Desktop app 0.5.14) retires the two internal task-run origins.
// The table is rebuilt because SQLite cannot alter a CHECK constraint in place.
// Existing external-run identity, timestamps, state, evidence, and idempotency
// values are copied verbatim. Retired Envoy/dispatcher runs are removed with
// their dependent attention and triage attribution before the rebuild. The
// orchestrator owns the single outer transaction.
const TASK_RUNS_V31_SQL: &str = "
PRAGMA legacy_alter_table = ON;
DROP INDEX reply_triage_one_per_email;
DROP INDEX reply_triage_one_per_message;
DROP INDEX reply_triage_open;
ALTER TABLE reply_triage RENAME TO reply_triage_v30;
DROP INDEX task_attention_open;
DROP INDEX task_attention_recency;
DROP INDEX task_attention_by_task_recency;
ALTER TABLE task_attention RENAME TO task_attention_v30;
-- Envoy/dispatcher runs are retired rather than reclassified.  The upgrade
-- transaction has foreign_keys disabled while rebuilding these tables, so
-- apply the dependent-row effects explicitly before dropping the source
-- uniqueness indexes.
DELETE FROM task_attention_v30
 WHERE task_run_id IN (
   SELECT id FROM task_runs WHERE origin IN ('envoy', 'dispatcher')
 );
UPDATE reply_triage_v30
   SET task_run_id = NULL
 WHERE task_run_id IN (
   SELECT id FROM task_runs WHERE origin IN ('envoy', 'dispatcher')
 );
DELETE FROM task_runs WHERE origin IN ('envoy', 'dispatcher');
DROP INDEX task_runs_claim_key;
DROP INDEX one_open_task_run_per_task;
DROP INDEX task_runs_by_task_recency;
DROP INDEX task_runs_task_identity;
ALTER TABLE task_runs RENAME TO task_runs_v30;
CREATE TABLE task_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
  scheduled_for INTEGER NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('claimed','reported_done','reported_failed','expired')),
  idempotency_key TEXT NOT NULL,
  claimed_at INTEGER NOT NULL,
  lease_expires_at INTEGER NOT NULL,
  report_summary TEXT,
  evidence_json TEXT CHECK(evidence_json IS NULL OR (json_valid(evidence_json) AND json_type(evidence_json) = 'object')),
  finished_at INTEGER,
  origin TEXT NOT NULL CHECK(origin IN ('external')),
  chat_session_id TEXT,
  CHECK(chat_session_id IS NULL),
  CHECK((state = 'claimed' AND report_summary IS NULL AND evidence_json IS NULL AND finished_at IS NULL)
     OR (state = 'expired' AND report_summary IS NULL AND evidence_json IS NULL AND finished_at IS NOT NULL)
     OR (state IN ('reported_done','reported_failed') AND report_summary IS NOT NULL AND evidence_json IS NOT NULL AND finished_at IS NOT NULL))
);
INSERT INTO task_runs
  (id, task_id, scheduled_for, state, idempotency_key, claimed_at,
   lease_expires_at, report_summary, evidence_json, finished_at, origin,
   chat_session_id)
SELECT id, task_id, scheduled_for, state, idempotency_key, claimed_at,
       lease_expires_at, report_summary, evidence_json, finished_at,
       origin, chat_session_id
FROM task_runs_v30;
DROP TABLE task_runs_v30;
CREATE UNIQUE INDEX task_runs_claim_key ON task_runs(idempotency_key);
CREATE UNIQUE INDEX one_open_task_run_per_task
  ON task_runs(task_id) WHERE state = 'claimed';
CREATE INDEX task_runs_by_task_recency
  ON task_runs(task_id, claimed_at DESC);
CREATE UNIQUE INDEX task_runs_task_identity
  ON task_runs(task_id, id);
CREATE TABLE task_attention (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  id TEXT NOT NULL UNIQUE,
  task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK(kind IN ('run_failed','dispatch_blocked')),
  task_run_id TEXT,
  occurrence_key TEXT NOT NULL UNIQUE CHECK(length(occurrence_key) BETWEEN 1 AND 192),
  reason_code TEXT CHECK(reason_code IS NULL OR (
    length(reason_code) BETWEEN 1 AND 64
    AND reason_code NOT GLOB '*[^a-z0-9_]*'
    AND substr(reason_code, 1, 1) BETWEEN 'a' AND 'z'
  )),
  message TEXT CHECK(message IS NULL OR (
    length(trim(message)) BETWEEN 1 AND 512
  )),
  occurred_at INTEGER NOT NULL CHECK(occurred_at >= 0),
  resolved_at INTEGER CHECK(resolved_at IS NULL OR resolved_at >= occurred_at),
  acknowledged_at INTEGER CHECK(acknowledged_at IS NULL OR acknowledged_at >= occurred_at),
  FOREIGN KEY(task_id, task_run_id) REFERENCES task_runs(task_id, id) ON DELETE CASCADE,
  CHECK((kind = 'run_failed' AND task_run_id IS NOT NULL AND reason_code IS NULL AND message IS NULL)
     OR (kind = 'dispatch_blocked' AND task_run_id IS NULL AND reason_code IS NOT NULL AND message IS NOT NULL
         AND ((reason_code = 'codex_cli_missing' AND message = '未找到可用的 Codex CLI')
           OR (reason_code = 'plugin_install_failed' AND message = 'Codex 插件安装或更新失败')
           OR (reason_code = 'mcp_unavailable' AND message = '本地 MCP 未就绪')
           OR (reason_code = 'session_start_failed' AND message = '任务会话启动失败'))))
);
INSERT INTO task_attention
  (sequence, id, task_id, kind, task_run_id, occurrence_key, reason_code, message,
   occurred_at, resolved_at, acknowledged_at)
SELECT sequence, id, task_id, kind, task_run_id, occurrence_key, reason_code, message,
       occurred_at, resolved_at, acknowledged_at
FROM task_attention_v30;
DROP TABLE task_attention_v30;
CREATE INDEX task_attention_open ON task_attention(sequence DESC)
  WHERE acknowledged_at IS NULL AND resolved_at IS NULL;
CREATE INDEX task_attention_recency ON task_attention(sequence DESC);
CREATE INDEX task_attention_by_task_recency ON task_attention(task_id, sequence DESC);
CREATE TABLE reply_triage (
  id TEXT PRIMARY KEY,
  channel TEXT NOT NULL CHECK(channel IN ('email','dm')),
  email_id TEXT REFERENCES emails(id) ON DELETE CASCADE,
  message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
  intent TEXT NOT NULL CHECK(intent IN ('interested','question','negotiating','declined','irrelevant','other')),
  summary TEXT NOT NULL,
  suggested_reply TEXT,
  task_run_id TEXT REFERENCES task_runs(id) ON DELETE SET NULL,
  created_at INTEGER NOT NULL,
  acknowledged_at INTEGER,
  CHECK((channel = 'email' AND email_id IS NOT NULL AND message_id IS NULL)
     OR (channel = 'dm' AND message_id IS NOT NULL AND email_id IS NULL))
);
INSERT INTO reply_triage
  (id, channel, email_id, message_id, intent, summary, suggested_reply, task_run_id,
   created_at, acknowledged_at)
SELECT id, channel, email_id, message_id, intent, summary, suggested_reply, task_run_id,
       created_at, acknowledged_at
FROM reply_triage_v30;
DROP TABLE reply_triage_v30;
CREATE UNIQUE INDEX reply_triage_one_per_email ON reply_triage(email_id) WHERE email_id IS NOT NULL;
CREATE UNIQUE INDEX reply_triage_one_per_message ON reply_triage(message_id) WHERE message_id IS NOT NULL;
CREATE INDEX reply_triage_open ON reply_triage(created_at DESC) WHERE acknowledged_at IS NULL;
PRAGMA legacy_alter_table = OFF;";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(TASK_RUNS_V31_SQL)
        .map_err(|error| error.to_string())
}
