//! CRM v75 -> v76 (app 0.6.72): native observations of approved web-search sources.
//! Existing search, candidate, approval and budget facts remain unchanged.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    db.execute_batch("CREATE TABLE discovery_web_source_reads (
      search_hit_id TEXT PRIMARY KEY NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
      discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
      origin_platform TEXT NOT NULL CHECK(origin_platform IN ('google','yahoo')),
      profile_ref TEXT NOT NULL CHECK(length(profile_ref) BETWEEN 1 AND 256),
      source_url TEXT NOT NULL CHECK(length(source_url) BETWEEN 1 AND 2048),
      expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
      call_number INTEGER NOT NULL CHECK(call_number > 0),
      target_platform TEXT NOT NULL CHECK(target_platform='youtube'),
      video_id TEXT NOT NULL CHECK(length(video_id)=11 AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*'),
      status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
      native_author_id TEXT,
      native_material_json TEXT,
      stop_code TEXT,
      updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
      FOREIGN KEY(discovery_campaign_id,origin_platform,profile_ref)
        REFERENCES discovery_platform_runs(discovery_campaign_id,platform,profile_ref) ON DELETE CASCADE,
      FOREIGN KEY(discovery_campaign_id,expected_revision)
        REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE,
      CHECK((status='reading' AND native_author_id IS NULL AND native_material_json IS NULL AND stop_code IS NULL)
        OR (status='completed' AND native_author_id IS NOT NULL AND length(native_author_id)=24
          AND substr(native_author_id,1,2)='UC' AND native_author_id NOT GLOB '*[^a-zA-Z0-9_-]*'
          AND native_material_json IS NOT NULL AND json_valid(native_material_json)
          AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536
          AND json_extract(native_material_json,'$.channelId') IS native_author_id
          AND json_extract(native_material_json,'$.videoId') IS video_id AND stop_code IS NULL)
        OR (status='failed' AND native_author_id IS NULL AND native_material_json IS NULL
          AND stop_code IS NOT NULL AND length(stop_code) BETWEEN 1 AND 128))
    );
    CREATE INDEX discovery_web_source_reads_campaign
      ON discovery_web_source_reads(discovery_campaign_id,expected_revision,status);")
      .map_err(|e| e.to_string())
}

/// Historical reference construction only; never a customer downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    db.execute_batch("DROP TABLE IF EXISTS discovery_web_source_reads")
        .map_err(|e| e.to_string())
}
