//! CRM v77 -> v78 (app 0.6.74): source-bound native X observations.
//! Preserve every old row, explicit index and trigger without replaying inserts.
use rusqlite::Connection;

const TARGET: &str = "CHECK(target_platform='youtube')";
const LOCATOR: &str = "CHECK(length(video_id)=11 AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*')";
const OLD_COMPLETED: &str =
    "OR (status='completed' AND native_author_id IS NOT NULL AND length(native_author_id)=24";
const NEW_COMPLETED: &str = "OR (status='completed' AND target_platform='youtube' AND native_author_id IS NOT NULL AND length(native_author_id)=24";
const X_COMPLETED: &str = "OR (status='completed' AND target_platform='twitter'
          AND native_author_id IS NOT NULL AND length(native_author_id) BETWEEN 1 AND 20
          AND substr(native_author_id,1,1)!='0' AND native_author_id NOT GLOB '*[^0-9]*'
          AND native_material_json IS NOT NULL AND json_valid(native_material_json)
          AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536
          AND json_extract(native_material_json,'$.identity.account_id') IS native_author_id
          AND json_extract(native_material_json,'$.identity.post_id') IS video_id
          AND json_extract(native_material_json,'$.identity.source_url') IS source_url
          AND json_extract(native_material_json,'$.profile.account_id') IS native_author_id
          AND json_extract(native_material_json,'$.content.identity') IS json_extract(native_material_json,'$.identity')
          AND stop_code IS NULL)
        OR (status='failed'";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    let old: String = db.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='discovery_web_source_reads'", [], |r| r.get(0)).map_err(|e|e.to_string())?;
    for anchor in [TARGET, LOCATOR, OLD_COMPLETED, "OR (status='failed'"] {
        if old.matches(anchor).count() != 1 {
            return Err("Native X source predecessor mismatch".into());
        }
    }
    let new = old.replacen(TARGET,"CHECK(target_platform IN ('youtube','twitter'))",1)
        .replacen(LOCATOR,"CHECK((target_platform='youtube' AND length(video_id)=11 AND video_id NOT GLOB '*[^a-zA-Z0-9_-]*') OR (target_platform='twitter' AND length(video_id) BETWEEN 1 AND 20 AND substr(video_id,1,1)!='0' AND video_id NOT GLOB '*[^0-9]*'))",1)
        .replacen(OLD_COMPLETED,NEW_COMPLETED,1).replacen("OR (status='failed'",X_COMPLETED,1);
    let mut query=db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='discovery_web_source_reads' AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").map_err(|e|e.to_string())?;
    let objects = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(query);
    db.execute_batch("SAVEPOINT web_source_v78")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        db.execute_batch("CREATE TEMP TABLE web_source_v78_rows AS SELECT * FROM discovery_web_source_reads; DROP TABLE discovery_web_source_reads;").map_err(|e|e.to_string())?;
        db.execute_batch(&new).map_err(|e| e.to_string())?;
        db.execute_batch("INSERT INTO discovery_web_source_reads SELECT * FROM web_source_v78_rows; DROP TABLE web_source_v78_rows").map_err(|e|e.to_string())?;
        for sql in objects {
            db.execute_batch(&sql).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })();
    match result {
        Ok(()) => db
            .execute_batch("RELEASE web_source_v78")
            .map_err(|e| e.to_string()),
        Err(error) => {
            db.execute_batch("ROLLBACK TO web_source_v78; RELEASE web_source_v78")
                .map_err(|e| format!("{error}; rollback failed: {e}"))?;
            Err(error)
        }
    }
}

/// Build the empty historical reference only; never downgrade customer data.
pub(in crate::repository::sqlite::schema) fn strip_reference(
    db: &Connection,
) -> Result<(), String> {
    db.execute_batch("DROP TABLE discovery_web_source_reads")
        .map_err(|e| e.to_string())?;
    super::v75_to_v76::migrate(db)
}
