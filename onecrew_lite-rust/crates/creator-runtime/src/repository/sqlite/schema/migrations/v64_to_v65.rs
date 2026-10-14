//! CRM v64 -> v65 (app 0.6.54): contact-backed recommendations.
//! Existing approvals keep their legacy policy. No historical searches start.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) const LEGACY_LIMIT: &str =
    "max_calls INTEGER NOT NULL CHECK(max_calls >= 1)";
pub(in crate::repository::sqlite::schema) const UNLIMITED_LIMIT: &str =
    "max_calls INTEGER CHECK(max_calls IS NULL OR max_calls >= 1)";
pub(in crate::repository::sqlite::schema) const LEGACY_USED: &str =
    "used_calls >= 0 AND used_calls <= max_calls";
pub(in crate::repository::sqlite::schema) const UNLIMITED_USED: &str =
    "used_calls >= 0 AND (max_calls IS NULL OR used_calls <= max_calls)";

/// Rebuild the existing discovery FK graph inside the startup transaction.
/// Row IDs, approved limits, data, indexes and triggers are preserved exactly.
fn rebuild_limit(
    connection: &Connection,
    from: &str,
    to: &str,
    from_used: &str,
    to_used: &str,
) -> Result<(), String> {
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut statement = connection.prepare("SELECT name,sql FROM sqlite_master WHERE type='table' AND name GLOB 'discovery_*' ORDER BY name").map_err(|e| e.to_string())?;
    let mut tables = statement
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(statement);
    let parent = tables
        .iter_mut()
        .find(|(name, _)| name == "discovery_campaigns")
        .ok_or("contact_upgrade_campaign_missing")?;
    if parent.1.matches(from).count() != 1 || parent.1.matches(from_used).count() != 1 {
        return Err("contact_upgrade_limit_constraint_mismatch".into());
    }
    parent.1 = parent
        .1
        .replacen(from, to, 1)
        .replacen(from_used, to_used, 1);
    let names: std::collections::BTreeSet<_> =
        tables.iter().map(|(name, _)| name.clone()).collect();
    let mut dependencies = std::collections::BTreeMap::new();
    let mut secondary = Vec::new();
    for (name, _) in &tables {
        let mut query = connection
            .prepare(&format!("PRAGMA foreign_key_list({})", quote(name)))
            .map_err(|e| e.to_string())?;
        let parents = query
            .query_map([], |r| r.get::<_, String>(2))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        dependencies.insert(
            name.clone(),
            parents
                .into_iter()
                .filter(|p| p != name && names.contains(p))
                .collect::<std::collections::BTreeSet<_>>(),
        );
        let mut query = connection.prepare("SELECT sql FROM sqlite_master WHERE tbl_name=?1 AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").map_err(|e|e.to_string())?;
        secondary.extend(
            query
                .query_map([name], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    let mut ordered = Vec::new();
    while !dependencies.is_empty() {
        let next = dependencies
            .iter()
            .find(|(_, parents)| parents.is_empty())
            .map(|(name, _)| name.clone())
            .ok_or("contact_upgrade_cyclic_dependency")?;
        dependencies.remove(&next);
        for parents in dependencies.values_mut() {
            parents.remove(&next);
        }
        ordered.push(next);
    }
    for name in &ordered {
        connection
            .execute_batch(&format!(
                "CREATE TEMP TABLE {} AS SELECT rowid AS preserved_rowid,* FROM {};",
                quote(&format!("contact_upgrade_{name}")),
                quote(name)
            ))
            .map_err(|e| e.to_string())?;
    }
    for name in ordered.iter().rev() {
        connection
            .execute_batch(&format!("DROP TABLE {};", quote(name)))
            .map_err(|e| e.to_string())?;
    }
    for name in &ordered {
        let ddl = &tables
            .iter()
            .find(|(table, _)| table == name)
            .ok_or("contact_upgrade_table_missing")?
            .1;
        connection.execute_batch(ddl).map_err(|e| e.to_string())?;
        let mut query = connection
            .prepare(&format!("PRAGMA table_info({})", quote(name)))
            .map_err(|e| e.to_string())?;
        let columns = query
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(query);
        let columns = columns
            .iter()
            .map(|s| quote(s))
            .collect::<Vec<_>>()
            .join(",");
        connection.execute_batch(&format!("INSERT INTO {} (rowid,{columns}) SELECT preserved_rowid,{columns} FROM {} ORDER BY preserved_rowid; DROP TABLE {};",quote(name),quote(&format!("contact_upgrade_{name}")),quote(&format!("contact_upgrade_{name}")))).map_err(|e|e.to_string())?;
    }
    for ddl in secondary {
        connection.execute_batch(&ddl).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(in crate::repository::sqlite::schema) const SQL: &str = r#"
CREATE TABLE discovery_recommendation_policies (
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  policy TEXT NOT NULL CHECK(policy IN ('legacy','score_and_contact_v1')),
  PRIMARY KEY(discovery_campaign_id, criteria_revision),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
INSERT INTO discovery_recommendation_policies
  SELECT discovery_campaign_id, revision, 'legacy' FROM discovery_campaign_revisions;
CREATE TABLE discovery_contact_observations (
  candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 128),
  kind TEXT NOT NULL CHECK(kind IN ('email','telegram')),
  original_value TEXT NOT NULL CHECK(length(original_value) BETWEEN 1 AND 2048),
  normalized_value TEXT NOT NULL CHECK(length(normalized_value) BETWEEN 1 AND 2048),
  source_url TEXT NOT NULL CHECK(length(source_url) BETWEEN 1 AND 2048),
  source_excerpt TEXT NOT NULL CHECK(length(CAST(source_excerpt AS BLOB)) BETWEEN 1 AND 4096),
  captured_at INTEGER NOT NULL CHECK(captured_at > 0),
  attribution TEXT NOT NULL CHECK(attribution IN ('author_published','unconfirmed')),
  PRIMARY KEY(candidate_id, criteria_revision, normalized_value, source_url),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
CREATE INDEX discovery_contacts_author_revision
  ON discovery_contact_observations(discovery_campaign_id, criteria_revision, native_author_id);
"#;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    rebuild_limit(
        connection,
        LEGACY_LIMIT,
        UNLIMITED_LIMIT,
        LEGACY_USED,
        UNLIMITED_USED,
    )?;
    connection.execute_batch(SQL).map_err(|e| e.to_string())
}

/// Historical construction only. Runtime never downgrades a database.
pub(in crate::repository::sqlite) fn strip_reference(
    connection: &Connection,
) -> Result<(), String> {
    // Earlier reference bridges may invoke this helper twice; neither is a runtime downgrade.
    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_recommendation_policies')", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !exists {
        return Ok(());
    }
    connection.execute_batch("DROP TABLE discovery_contact_observations; DROP TABLE discovery_recommendation_policies;").map_err(|e| e.to_string())?;
    rebuild_limit(
        connection,
        UNLIMITED_LIMIT,
        LEGACY_LIMIT,
        UNLIMITED_USED,
        LEGACY_USED,
    )
}
