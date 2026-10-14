//! CRM v33 -> v34 (app 0.5.22): retire local Task scheduling and its ledger.
//! The product explicitly deletes Task rows; reply business facts retain their IDs and provenance.
use crate::CreatorDatabaseUpgradeImpact;
use rusqlite::Connection;

pub(super) fn impact(connection: &Connection) -> Result<CreatorDatabaseUpgradeImpact, String> {
    let count = |sql: &str| {
        connection
            .query_row(sql, [], |row| row.get::<_, u64>(0))
            .map_err(|error| error.to_string())
    };
    Ok(CreatorDatabaseUpgradeImpact {
        deleted_tasks: count("SELECT COUNT(*) FROM tasks")?,
        deleted_task_runs: count("SELECT COUNT(*) FROM task_runs")?,
        deleted_task_attention: count("SELECT COUNT(*) FROM task_attention")?,
        deleted_task_idempotency_records: count(
            "SELECT COUNT(*) FROM idempotency_records WHERE action = 'task_create'",
        )?,
        cleared_reply_triage_task_refs: count(
            "SELECT COUNT(*) FROM reply_triage WHERE task_run_id IS NOT NULL",
        )?,
        // Counters owned by later edges stay zero here; the chain planner
        // fills them from their own preflights.
        ..Default::default()
    })
}

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    // The upgrade owner holds the transaction and disables FK enforcement for table rebuilds.
    connection.execute_batch(r#"
CREATE TEMP TABLE reply_triage_v33_copy AS
SELECT id, channel, email_id, message_id, intent, summary, suggested_reply, created_at, acknowledged_at FROM reply_triage;
DROP TABLE reply_triage;
             CREATE TABLE reply_triage (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','dm')),
               email_id TEXT REFERENCES emails(id) ON DELETE CASCADE,
               message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
               intent TEXT NOT NULL CHECK(intent IN ('interested','question','negotiating','declined','irrelevant','other')),
               summary TEXT NOT NULL,
               suggested_reply TEXT,
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

INSERT INTO reply_triage SELECT * FROM reply_triage_v33_copy;
DROP TABLE reply_triage_v33_copy;
DROP TABLE task_attention;
DROP TABLE task_runs;
DROP TABLE tasks;
DELETE FROM idempotency_records WHERE action = 'task_create';
"#).map_err(|error| error.to_string())
}
