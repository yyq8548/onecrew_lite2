//! CRM v78 -> v79 (app 0.6.75): one-attempt native Facebook identity receipts.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    db.execute_batch("CREATE TABLE discovery_facebook_identity_reads (
      search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id),
      discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id),
      platform TEXT NOT NULL CHECK(platform='facebook'),
      profile_ref TEXT NOT NULL CHECK(length(profile_ref)>0),
      source_url TEXT NOT NULL CHECK(source_url LIKE 'https://www.facebook.com/%'),
      expected_revision INTEGER NOT NULL CHECK(expected_revision>0),
      call_number INTEGER NOT NULL CHECK(call_number BETWEEN 1 AND 4294967295),
      status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
      native_identity_json TEXT,
      stop_code TEXT,
      updated_at INTEGER NOT NULL,
      FOREIGN KEY(discovery_campaign_id,expected_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision),
      CHECK((status='reading' AND native_identity_json IS NULL AND stop_code IS NULL)
        OR (status='failed' AND native_identity_json IS NULL AND stop_code IS NOT NULL AND length(stop_code)>0)
        OR (status='completed' AND stop_code IS NULL AND native_identity_json IS NOT NULL
          AND json_valid(native_identity_json) AND json_type(native_identity_json)='object'
          AND length(CAST(native_identity_json AS BLOB))<=4000
          AND json_extract(native_identity_json,'$.adapter_version') IS '1.0.0'
          AND json_extract(native_identity_json,'$.source_url') IS source_url
          AND json_extract(native_identity_json,'$.evidence_source') IS 'native_profile_header_response'
          AND json_type(native_identity_json,'$.id') IS 'text'
          AND length(json_extract(native_identity_json,'$.id')) BETWEEN 1 AND 20
          AND substr(json_extract(native_identity_json,'$.id'),1,1)!='0'
          AND json_extract(native_identity_json,'$.id') NOT GLOB '*[^0-9]*'))
    );
    CREATE INDEX discovery_facebook_identity_campaign ON discovery_facebook_identity_reads(discovery_campaign_id,expected_revision,status);
    CREATE TRIGGER discovery_facebook_identity_source_insert BEFORE INSERT ON discovery_facebook_identity_reads
    WHEN NOT EXISTS(SELECT 1 FROM discovery_search_hits h WHERE h.id=NEW.search_hit_id
      AND h.discovery_campaign_id=NEW.discovery_campaign_id AND h.platform=NEW.platform
      AND h.profile_ref=NEW.profile_ref AND h.source_url=NEW.source_url)
    BEGIN SELECT RAISE(ABORT,'Facebook identity source mismatch'); END;
    CREATE TRIGGER discovery_facebook_identity_source_update BEFORE UPDATE ON discovery_facebook_identity_reads
    WHEN NEW.search_hit_id!=OLD.search_hit_id OR NEW.discovery_campaign_id!=OLD.discovery_campaign_id
      OR NEW.platform!=OLD.platform OR NEW.profile_ref!=OLD.profile_ref OR NEW.source_url!=OLD.source_url
      OR NEW.expected_revision!=OLD.expected_revision OR NEW.call_number!=OLD.call_number
      OR OLD.status!='reading'
    BEGIN SELECT RAISE(ABORT,'Facebook identity receipt is immutable'); END;")
      .map_err(|error|error.to_string())
}

/// Empty validation reference only. Never downgrade customer data.
pub(in crate::repository::sqlite::schema) fn strip_reference(
    db: &Connection,
) -> Result<(), String> {
    db.execute_batch("DROP TABLE IF EXISTS discovery_facebook_identity_reads")
        .map_err(|error| error.to_string())
}
