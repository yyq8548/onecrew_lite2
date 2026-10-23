CREATE TABLE discovery_contact_lookups (
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 discovery_campaign_id TEXT NOT NULL,
 criteria_revision INTEGER NOT NULL,
 method TEXT NOT NULL CHECK(method IN ('maigret','search')),
 receipt_json TEXT NOT NULL CHECK(json_valid(receipt_json)),
 PRIMARY KEY(candidate_id,criteria_revision,method),
 FOREIGN KEY(discovery_campaign_id,criteria_revision)
 REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE
);
