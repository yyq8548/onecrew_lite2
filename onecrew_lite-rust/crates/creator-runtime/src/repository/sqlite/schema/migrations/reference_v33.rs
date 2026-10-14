use crate::repository::sqlite::schema::user_version;
use rusqlite::Connection;

/// Frozen schema-33 reference reconstruction; never an application downgrade API.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v33(
    connection: &Connection,
) -> Result<(), String> {
    super::downgrade_reference_to_v34(connection)?;
    if user_version(connection)? != 34 {
        return Ok(());
    }
    connection.execute_batch(r#"
PRAGMA foreign_keys = OFF;
CREATE TEMP TABLE reply_triage_v34_copy AS SELECT * FROM reply_triage;
DROP TABLE reply_triage;
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
             CREATE UNIQUE INDEX reply_triage_one_per_email
               ON reply_triage(email_id) WHERE email_id IS NOT NULL;
             CREATE UNIQUE INDEX reply_triage_one_per_message
               ON reply_triage(message_id) WHERE message_id IS NOT NULL;
             CREATE INDEX reply_triage_open
               ON reply_triage(created_at DESC) WHERE acknowledged_at IS NULL;
             CREATE TABLE tasks (
               id TEXT PRIMARY KEY,
               name TEXT,
               campaign_id TEXT REFERENCES campaigns(id) ON DELETE RESTRICT,
               instruction TEXT NOT NULL,
               schedule_json TEXT NOT NULL CHECK(json_valid(schedule_json) AND json_type(schedule_json) = 'object'),
               status TEXT NOT NULL CHECK(status IN ('active','paused','completed','cancelled')),
               created_by TEXT NOT NULL CHECK(created_by IN ('ui','agent')),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               revision INTEGER NOT NULL DEFAULT 1,
               kind TEXT NOT NULL CHECK(kind IN ('find','enrich','outreach','dm_watch','mailbox_triage','refresh','custom'))
             );
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
               occurrence_key TEXT NOT NULL UNIQUE
                 CHECK(length(occurrence_key) BETWEEN 1 AND 192),
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
               acknowledged_at INTEGER
                 CHECK(acknowledged_at IS NULL OR acknowledged_at >= occurred_at),
               FOREIGN KEY(task_id, task_run_id)
                 REFERENCES task_runs(task_id, id) ON DELETE CASCADE,
               CHECK((kind = 'run_failed'
                      AND task_run_id IS NOT NULL
                      AND reason_code IS NULL
                      AND message IS NULL)
                  OR (kind = 'dispatch_blocked'
                      AND task_run_id IS NULL
                      AND reason_code IS NOT NULL
                      AND message IS NOT NULL
                      AND ((reason_code = 'codex_cli_missing'
                            AND message = '未找到可用的 Codex CLI')
                        OR (reason_code = 'plugin_install_failed'
                            AND message = 'Codex 插件安装或更新失败')
                        OR (reason_code = 'mcp_unavailable'
                            AND message = '本地 MCP 未就绪')
                        OR (reason_code = 'session_start_failed'
                            AND message = '任务会话启动失败'))))
             );
             CREATE INDEX task_attention_open
               ON task_attention(sequence DESC)
               WHERE acknowledged_at IS NULL AND resolved_at IS NULL;
             CREATE INDEX task_attention_recency
               ON task_attention(sequence DESC);
             CREATE INDEX task_attention_by_task_recency
               ON task_attention(task_id, sequence DESC);

INSERT INTO reply_triage (id, channel, email_id, message_id, intent, summary, suggested_reply, created_at, acknowledged_at)
SELECT id, channel, email_id, message_id, intent, summary, suggested_reply, created_at, acknowledged_at FROM reply_triage_v34_copy;
DROP TABLE reply_triage_v34_copy;
PRAGMA user_version = 33;
PRAGMA foreign_keys = ON;
"#).map_err(|error| error.to_string())
}
