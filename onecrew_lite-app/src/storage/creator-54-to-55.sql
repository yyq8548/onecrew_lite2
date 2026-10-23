CREATE TABLE discovery_target_increases (
  id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
  operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  previous_target_count INTEGER NOT NULL CHECK(previous_target_count BETWEEN 1 AND 10000),
  new_target_count INTEGER NOT NULL CHECK(new_target_count BETWEEN 1 AND 10000 AND new_target_count > previous_target_count),
  used_calls_at_decision INTEGER NOT NULL CHECK(used_calls_at_decision >= 0),
  created_at INTEGER NOT NULL CHECK(created_at >= 0),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
