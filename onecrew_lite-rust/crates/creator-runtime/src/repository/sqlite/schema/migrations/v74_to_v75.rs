//! CRM v74 -> v75 (app 0.6.71): author discovery from a matched native video.
//! No existing facts, reservations, approvals or historical receipts change.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) const SQL: &str = "
CREATE TABLE discovery_douyin_author_reads (
  search_hit_id TEXT PRIMARY KEY NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  platform TEXT NOT NULL CHECK(platform='douyin'),
  profile_ref TEXT NOT NULL CHECK(length(profile_ref) BETWEEN 1 AND 256),
  source_url TEXT NOT NULL CHECK(length(source_url) BETWEEN 1 AND 2048),
  post_id TEXT NOT NULL CHECK(length(post_id) BETWEEN 1 AND 20 AND post_id NOT GLOB '*[^0-9]*' AND substr(post_id,1,1)!='0'),
  expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
  author_id TEXT,
  profile_url TEXT,
  duplicate_of TEXT REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
  stop_code TEXT,
  updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
  FOREIGN KEY(discovery_campaign_id,platform,profile_ref)
    REFERENCES discovery_platform_runs(discovery_campaign_id,platform,profile_ref) ON DELETE CASCADE,
  FOREIGN KEY(discovery_campaign_id,expected_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE,
  CHECK(source_url='https://www.douyin.com/video/' || post_id),
  CHECK((status='reading' AND author_id IS NULL AND profile_url IS NULL AND duplicate_of IS NULL AND stop_code IS NULL)
    OR (status='completed' AND author_id IS NOT NULL AND profile_url IS NOT NULL AND profile_url='https://www.douyin.com/user/' || author_id
      AND length(author_id) BETWEEN 20 AND 100 AND substr(author_id,1,12)='MS4wLjABAAAA'
      AND substr(author_id,13) NOT GLOB '*[^a-zA-Z0-9_-]*' AND stop_code IS NULL)
    OR (status='failed' AND author_id IS NULL AND profile_url IS NULL AND duplicate_of IS NULL AND stop_code IS NOT NULL))
);
CREATE INDEX discovery_douyin_author_reads_campaign
  ON discovery_douyin_author_reads(discovery_campaign_id,expected_revision,status);
";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    db.execute_batch(SQL).map_err(|e| e.to_string())
}
/// Historical reference construction only; never a customer downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    db.execute_batch("DROP TABLE IF EXISTS discovery_douyin_author_reads")
        .map_err(|e| e.to_string())
}
