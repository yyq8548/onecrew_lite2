//! CRM v81 -> v82 (app 0.6.78): canonical native X URLs may differ in handle casing.
use rusqlite::Connection;

const OLD: &str = "json_extract(native_material_json,'$.identity.source_url') IS source_url";
const NEW: &str = "json_extract(native_material_json,'$.identity.source_url') IS lower(source_url)";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    replace_constraint(db, OLD, NEW)
}

/// Only used to construct empty historical schemas for validation.
pub(in crate::repository::sqlite::schema) fn strip_reference(
    db: &Connection,
) -> Result<(), String> {
    // Composed historical builders may already have removed this constraint.
    // Forward migration validation remains strict.
    let present: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='discovery_web_source_reads' AND instr(sql,?1)>0)", [NEW], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !present {
        return Ok(());
    }
    replace_constraint(db, NEW, OLD)
}

fn replace_constraint(db: &Connection, before: &str, after: &str) -> Result<(), String> {
    let old: String = db.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='discovery_web_source_reads'", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if old.matches(before).count() != 1 {
        return Err("Native X source casing predecessor mismatch".into());
    }
    let new = old.replacen(before, after, 1);
    let mut query = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='discovery_web_source_reads' AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").map_err(|e| e.to_string())?;
    let objects = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(query);
    db.execute_batch("SAVEPOINT web_source_v82")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        db.execute_batch("CREATE TEMP TABLE web_source_v82_rows AS SELECT * FROM discovery_web_source_reads; DROP TABLE discovery_web_source_reads;").map_err(|e| e.to_string())?;
        db.execute_batch(&new).map_err(|e| e.to_string())?;
        db.execute_batch("INSERT INTO discovery_web_source_reads SELECT * FROM web_source_v82_rows; DROP TABLE web_source_v82_rows;").map_err(|e| e.to_string())?;
        for sql in objects {
            db.execute_batch(&sql).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })();
    match result {
        Ok(()) => db
            .execute_batch("RELEASE web_source_v82")
            .map_err(|e| e.to_string()),
        Err(error) => {
            db.execute_batch("ROLLBACK TO web_source_v82; RELEASE web_source_v82")
                .map_err(|e| format!("{error}; rollback failed: {e}"))?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_preserves_rows_indexes_and_triggers_without_replaying_writes() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "CREATE TABLE discovery_web_source_reads (
            source_url TEXT PRIMARY KEY, native_material_json TEXT NOT NULL CHECK({OLD}));
            CREATE INDEX source_material ON discovery_web_source_reads(native_material_json);
            CREATE TABLE observed_writes (source TEXT);
            CREATE TRIGGER observe_insert AFTER INSERT ON discovery_web_source_reads
            BEGIN INSERT INTO observed_writes VALUES(NEW.source_url); END;"
        ))
        .unwrap();
        let source = "https://x.com/creator/status/123";
        let material = serde_json::json!({"identity":{"source_url":source}}).to_string();
        db.execute(
            "INSERT INTO discovery_web_source_reads VALUES(?1,?2)",
            [&source, &material.as_str()],
        )
        .unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT native_material_json FROM discovery_web_source_reads",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            material
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM observed_writes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('source_material','observe_insert')", [], |r| r.get::<_, i64>(0)).unwrap(), 2);
        db.execute(
            "INSERT INTO discovery_web_source_reads VALUES(?1,?2)",
            [&"https://x.com/Creator/status/123", &material.as_str()],
        )
        .unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM observed_writes", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert!(
            db.execute(
                "INSERT INTO discovery_web_source_reads VALUES(?1,?2)",
                [&"https://x.com/other/status/123", &material.as_str()]
            )
            .is_err()
        );
        assert!(
            db.execute(
                "INSERT INTO discovery_web_source_reads VALUES(?1,?2)",
                [&"https://x.com/creator/status/124", &material.as_str()]
            )
            .is_err()
        );
    }
}
