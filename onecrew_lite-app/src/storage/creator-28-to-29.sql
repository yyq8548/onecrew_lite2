CREATE INDEX creator_origins_by_creator_recency ON creator_origins(creator_id, discovered_at DESC, source_id);
CREATE INDEX locator_observations_by_locator_recency ON locator_observations(locator_id, observed_at DESC);
CREATE INDEX locator_observations_by_creator_source ON locator_observations(creator_id, source_id);
CREATE INDEX locator_observations_by_platform_account_source ON locator_observations(platform_account_id, source_id);
CREATE INDEX touchpoints_by_creator_recency ON touchpoints(creator_id, occurred_at DESC);
CREATE INDEX email_threads_by_creator ON email_threads(creator_id, id);
CREATE TABLE mailbox_sync_incremental_continuations (
 mailbox_ref_id TEXT NOT NULL,
 direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
 next_cursor TEXT NOT NULL CHECK(length(trim(next_cursor, char(9) || char(10) || char(11) || char(12) || char(13) || ' ')) > 0),
 newest_provider_message_id TEXT NOT NULL CHECK(length(trim(newest_provider_message_id, char(9) || char(10) || char(11) || char(12) || char(13) || ' ')) > 0),
 newest_occurred_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 PRIMARY KEY(mailbox_ref_id, direction),
 FOREIGN KEY(mailbox_ref_id, direction) REFERENCES mailbox_sync_state(mailbox_ref_id, direction) ON DELETE CASCADE
);
