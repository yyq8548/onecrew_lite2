//! CRM v87 -> v88 (app 0.6.92): WeChat contact locators.
use rusqlite::Connection;

const OLD: &str = "'email','x','instagram','telegram','youtube','tiktok','website','link_hub'";
const NEW: &str =
    "'email','x','instagram','telegram','youtube','tiktok','website','link_hub','wechat'";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, OLD, NEW)
}

fn rebuild(db: &Connection, from: &str, to: &str) -> Result<(), String> {
    let foreign_keys: bool = db
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if foreign_keys {
        return Err(
            "channel rebuild requires the database upgrade owner's foreign-key-off transaction"
                .into(),
        );
    }
    for table in ["contact_locators", "conversations"] {
        let sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !sql.contains(from) {
            return Err(format!(
                "{table} channel constraint differs from predecessor"
            ));
        }
        let objects = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name=?1 AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name")
            .map_err(|e|e.to_string())?.query_map([table], |r| r.get::<_,String>(0)).map_err(|e|e.to_string())?
            .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        db.execute_batch(&format!(
            "CREATE TEMP TABLE channel_copy AS SELECT * FROM {table}; DROP TABLE {table};"
        ))
        .map_err(|e| e.to_string())?;
        db.execute_batch(&sql.replace(from, to))
            .map_err(|e| e.to_string())?;
        db.execute_batch(&format!(
            "INSERT INTO {table} SELECT * FROM channel_copy; DROP TABLE channel_copy;"
        ))
        .map_err(|e| e.to_string())?;
        for sql in objects {
            db.execute_batch(&sql).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Only in-memory historical reference construction; never a production downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    super::v88_to_v89::strip_reference(db)?;
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='contact_locators'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !sql.contains(NEW) {
        return Ok(());
    }
    db.execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(|e| e.to_string())?;
    rebuild(db, NEW, OLD)
}
