use rusqlite::Connection;

// Pre-release-amended v28 -> v29 edge (Desktop app 0.5.12). In addition to
// the six additive indexes, v29 owns private durable Phase A continuation.
// The table begins empty and no existing business row is transformed or
// removed. This amendment is valid only because v29 has not shipped.
//
// The six additive indexes were
// selected independently on a temporary v28 fixture with automatic indexes
// disabled and the production queries. They remove the profiled base-table
// scans for Creator origins, route observations, touchpoints, and email
// threads; no business row is transformed or removed.
//
// The wider Creator-origin key also covers Q5's source-id projection. The
// wider email-thread key covers its join id, after which the existing
// emails(thread_id, provider_message_id) unique-index prefix is sufficient.
// Consequently v29 does not add the optional emails direction/recency index.
const RESPONSIVENESS_INDEXES_V29_SQL: &str = "
CREATE INDEX creator_origins_by_creator_recency
  ON creator_origins(creator_id, discovered_at DESC, source_id);
CREATE INDEX locator_observations_by_locator_recency
  ON locator_observations(locator_id, observed_at DESC);
CREATE INDEX locator_observations_by_creator_source
  ON locator_observations(creator_id, source_id);
CREATE INDEX locator_observations_by_platform_account_source
  ON locator_observations(platform_account_id, source_id);
CREATE INDEX touchpoints_by_creator_recency
  ON touchpoints(creator_id, occurred_at DESC);
CREATE INDEX email_threads_by_creator
  ON email_threads(creator_id, id);";

const INCREMENTAL_CONTINUATIONS_V29_SQL: &str = "
CREATE TABLE mailbox_sync_incremental_continuations (
  mailbox_ref_id TEXT NOT NULL,
  direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
  next_cursor TEXT NOT NULL CHECK(length(trim(
    next_cursor, char(9) || char(10) || char(11) || char(12) || char(13) || ' '
  )) > 0),
  newest_provider_message_id TEXT NOT NULL CHECK(length(trim(
    newest_provider_message_id,
    char(9) || char(10) || char(11) || char(12) || char(13) || ' '
  )) > 0),
  newest_occurred_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(mailbox_ref_id, direction),
  FOREIGN KEY(mailbox_ref_id, direction)
    REFERENCES mailbox_sync_state(mailbox_ref_id, direction) ON DELETE CASCADE
);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(&format!(
            "{RESPONSIVENESS_INDEXES_V29_SQL}{INCREMENTAL_CONTINUATIONS_V29_SQL}"
        ))
        .map_err(|error| error.to_string())
}
