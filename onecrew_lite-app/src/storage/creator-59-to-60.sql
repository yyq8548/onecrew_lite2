CREATE TABLE discovery_manual_approvals (
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  approved_at INTEGER NOT NULL,
  PRIMARY KEY(discovery_campaign_id, criteria_revision, candidate_id),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision)
);
CREATE TRIGGER discovery_manual_approval_immutable BEFORE UPDATE ON discovery_manual_approvals
BEGIN SELECT RAISE(ABORT, 'manual_approval_is_immutable'); END;
