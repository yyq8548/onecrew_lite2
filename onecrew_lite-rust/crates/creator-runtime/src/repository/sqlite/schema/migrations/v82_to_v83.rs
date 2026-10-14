//! CRM v82 -> v83 (app 0.6.79): engine-origin Douyin native source receipts.
use rusqlite::Connection;

const TARGET_OLD: &str = "CHECK(target_platform IN ('youtube','twitter'))";
const TARGET_NEW: &str = "CHECK(target_platform IN ('youtube','twitter','douyin'))";
const ID_OLD: &str = "target_platform='twitter' AND length(video_id)";
const ID_NEW: &str = "target_platform IN ('twitter','douyin') AND length(video_id)";
const FAILED: &str = "OR (status='failed'";
const COMPLETED: &str = "OR (status='completed' AND target_platform='douyin'
          AND native_author_id IS NOT NULL AND length(native_author_id) BETWEEN 19 AND 99
          AND substr(native_author_id,1,12)='MS4wLjABAAAA' AND native_author_id NOT GLOB '*[^a-zA-Z0-9_-]*'
          AND native_material_json IS NOT NULL AND json_valid(native_material_json)
          AND json_type(native_material_json)='object' AND length(CAST(native_material_json AS BLOB))<=65536
          AND json_extract(native_material_json,'$.content.author_id') IS native_author_id
          AND json_extract(native_material_json,'$.profile.author_id') IS native_author_id
          AND json_extract(native_material_json,'$.content.post_id') IS video_id
          AND json_extract(native_material_json,'$.content.source_url') IS source_url
          AND json_extract(native_material_json,'$.profile.profile_url') IS ('https://www.douyin.com/user/' || native_author_id)
          AND json_extract(native_material_json,'$.content.profile_url') IS json_extract(native_material_json,'$.profile.profile_url')
          AND stop_code IS NULL)
        OR (status='failed'";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, false)
}

/// Empty historical reference construction only.
pub(in crate::repository::sqlite::schema) fn strip_reference(
    db: &Connection,
) -> Result<(), String> {
    // Composed historical builders may already have removed this constraint.
    // Forward migration validation remains strict.
    let present: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='discovery_web_source_reads' AND instr(sql,?1)>0)", [TARGET_NEW], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !present {
        return Ok(());
    }
    rebuild(db, true)
}

fn rebuild(db: &Connection, reverse: bool) -> Result<(), String> {
    let mut sql: String = db.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='discovery_web_source_reads'", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    for (old, new) in [
        (TARGET_OLD, TARGET_NEW),
        (ID_OLD, ID_NEW),
        (FAILED, COMPLETED),
    ] {
        let (from, to) = if reverse { (new, old) } else { (old, new) };
        if sql.matches(from).count() != 1 {
            return Err("Douyin engine source predecessor mismatch".into());
        }
        sql = sql.replacen(from, to, 1);
    }
    let mut query = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='discovery_web_source_reads' AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").map_err(|e| e.to_string())?;
    let objects = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(query);
    db.execute_batch("SAVEPOINT web_source_v83")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        db.execute_batch("CREATE TEMP TABLE web_source_v83_rows AS SELECT * FROM discovery_web_source_reads; DROP TABLE discovery_web_source_reads;").map_err(|e| e.to_string())?;
        db.execute_batch(&sql).map_err(|e| e.to_string())?;
        db.execute_batch("INSERT INTO discovery_web_source_reads SELECT * FROM web_source_v83_rows; DROP TABLE web_source_v83_rows;").map_err(|e| e.to_string())?;
        for object in objects {
            db.execute_batch(&object).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })();
    match result {
        Ok(()) => db
            .execute_batch("RELEASE web_source_v83")
            .map_err(|e| e.to_string()),
        Err(error) => {
            db.execute_batch("ROLLBACK TO web_source_v83; RELEASE web_source_v83")
                .map_err(|e| format!("{error}; rollback failed: {e}"))?;
            Err(error)
        }
    }
}
