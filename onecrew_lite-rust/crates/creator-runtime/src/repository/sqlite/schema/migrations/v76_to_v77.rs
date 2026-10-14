//! CRM v76 -> v77 (app 0.6.73): actual search origin is distinct from creator platform.
//! All existing candidate facts and dependent rows remain unchanged.
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

const OLD_KEY: &str = "FOREIGN KEY(discovery_campaign_id, platform, source_profile_ref)";
const NEW_KEY: &str = "FOREIGN KEY(discovery_campaign_id, source_platform, source_profile_ref)";
const FIELD: &str = "\n               source_platform TEXT NOT NULL CHECK(source_platform=platform OR (source_platform IN ('google','yahoo') AND platform IN ('youtube','tiktok','twitter','bilibili','reddit','weibo','zhihu','instagram','facebook','pinterest','linkedin','douyin','pixiv'))),";
const COLUMNS: &str = "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at";

fn identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, false)
}

fn rebuild(db: &Connection, reference: bool) -> Result<(), String> {
    let mut statement = db.prepare("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(|e| e.to_string())?;
    let mut ddl: BTreeMap<String, String> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut parents = BTreeMap::<String, BTreeSet<String>>::new();
    for name in ddl.keys() {
        let mut fields = db
            .prepare(&format!("PRAGMA foreign_key_list({})", identifier(name)))
            .map_err(|e| e.to_string())?;
        parents.insert(
            name.clone(),
            fields
                .query_map([], |r| r.get::<_, String>(2))
                .map_err(|e| e.to_string())?
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    let mut affected = BTreeSet::from(["discovery_candidates".to_owned()]);
    loop {
        let before = affected.len();
        for (child, relations) in &parents {
            if relations.iter().any(|parent| affected.contains(parent)) {
                affected.insert(child.clone());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    let mut order = Vec::<String>::new();
    while order.len() < affected.len() {
        let ready = affected
            .iter()
            .find(|name| {
                !order.contains(name)
                    && parents[*name].iter().all(|parent| {
                        parent == *name || !affected.contains(parent) || order.contains(parent)
                    })
            })
            .cloned()
            .ok_or("Candidate source migration has cyclic table dependencies")?;
        order.push(ready);
    }
    let candidate = ddl
        .get_mut("discovery_candidates")
        .ok_or("Candidate source predecessor is absent")?;
    let (from, to) = if reference {
        (NEW_KEY, OLD_KEY)
    } else {
        (OLD_KEY, NEW_KEY)
    };
    if candidate.matches(from).count() != 1 || candidate.contains(FIELD) != reference {
        return Err("Candidate source predecessor constraint mismatch".into());
    }
    *candidate = candidate.replacen(from, to, 1);
    if reference {
        *candidate = candidate.replacen(FIELD, "", 1);
    } else {
        let anchor = "updated_at INTEGER NOT NULL,";
        if candidate.matches(anchor).count() != 1 {
            return Err("Candidate source column anchor mismatch".into());
        }
        *candidate = candidate.replacen(anchor, &format!("{anchor}{FIELD}"), 1);
    }
    let mut objects = Vec::<String>::new();
    for name in &order {
        let mut query = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name=?1 AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").map_err(|e| e.to_string())?;
        objects.extend(
            query
                .query_map([name], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    db.execute_batch("SAVEPOINT candidate_source_v77")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        for name in &order {
            db.execute_batch(&format!(
                "CREATE TEMP TABLE {} AS SELECT * FROM {}",
                identifier(&format!("candidate_v77_{name}")),
                identifier(name)
            ))
            .map_err(|e| e.to_string())?;
        }
        for name in order.iter().rev() {
            db.execute_batch(&format!("DROP TABLE {}", identifier(name)))
                .map_err(|e| e.to_string())?;
        }
        for name in &order {
            db.execute_batch(&ddl[name]).map_err(|e| e.to_string())?;
            let old = identifier(&format!("candidate_v77_{name}"));
            let restore = if name == "discovery_candidates" {
                if reference {
                    format!(
                        "INSERT INTO discovery_candidates({COLUMNS}) SELECT {COLUMNS} FROM {old}"
                    )
                } else {
                    format!(
                        "INSERT INTO discovery_candidates({COLUMNS},source_platform) SELECT {COLUMNS},platform FROM {old}"
                    )
                }
            } else {
                format!("INSERT INTO {} SELECT * FROM {old}", identifier(name))
            };
            db.execute_batch(&format!("{restore}; DROP TABLE {old}"))
                .map_err(|e| e.to_string())?;
        }
        // Restore triggers only after rows; historical insert effects must not run again.
        for sql in &objects {
            db.execute_batch(sql).map_err(|e| e.to_string())?;
        }
        Ok::<_, String>(())
    })();
    match result {
        Ok(()) => db
            .execute_batch("RELEASE candidate_source_v77")
            .map_err(|e| e.to_string()),
        Err(error) => {
            db.execute_batch("ROLLBACK TO candidate_source_v77; RELEASE candidate_source_v77")
                .map_err(|e| format!("{error}; migration rollback failed: {e}"))?;
            Err(error)
        }
    }
}

/// Frozen reference construction only; never a customer downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    let current: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('discovery_candidates') WHERE name='source_platform')", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if current {
        rebuild(db, true)?;
    }
    Ok(())
}
