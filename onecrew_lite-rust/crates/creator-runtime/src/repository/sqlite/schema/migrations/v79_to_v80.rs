//! CRM v79 -> v80 (app 0.6.76): native vanity profiles require their identity receipt.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    for operation in ["INSERT", "UPDATE"] {
        let name = operation.to_ascii_lowercase();
        db.execute_batch(&format!(r#"CREATE TRIGGER discovery_facebook_native_profile_{name}
          BEFORE {operation} ON discovery_profile_reads
          WHEN NEW.platform='facebook' AND NEW.status='completed'
            AND json_extract(NEW.native_profile_json,'$.adapter_version') IS '2.0.0'
            AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i
              JOIN discovery_search_authors a ON a.search_hit_id=i.search_hit_id
              WHERE i.search_hit_id=NEW.search_hit_id AND i.discovery_campaign_id=NEW.discovery_campaign_id
                AND i.profile_ref=NEW.profile_ref AND i.expected_revision=NEW.expected_revision
                AND i.status='completed' AND i.stop_code IS NULL AND i.call_number<NEW.call_number
                AND json_extract(i.native_identity_json,'$.id') IS NEW.author_id AND a.author_id=NEW.author_id
                AND a.author_url='https://www.facebook.com/profile.php?id='||NEW.author_id
                AND json_extract(NEW.native_profile_json,'$.id') IS NEW.author_id
                AND json_extract(NEW.native_profile_json,'$.profile_url') IS a.author_url
                AND json_extract(NEW.native_profile_json,'$.evidence_source') IS 'native_profile_header_response'
                AND lower(json_extract(NEW.native_profile_json,'$.observed_profile_url')) IS lower(rtrim(i.source_url,'/')))
          BEGIN SELECT RAISE(ABORT,'Facebook native profile requires source identity'); END;"#))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Empty validation reference only. Never downgrade customer data.
pub(in crate::repository::sqlite::schema) fn strip_reference(
    db: &Connection,
) -> Result<(), String> {
    db.execute_batch(
        "DROP TRIGGER IF EXISTS discovery_facebook_native_profile_insert;
      DROP TRIGGER IF EXISTS discovery_facebook_native_profile_update;",
    )
    .map_err(|error| error.to_string())
}
