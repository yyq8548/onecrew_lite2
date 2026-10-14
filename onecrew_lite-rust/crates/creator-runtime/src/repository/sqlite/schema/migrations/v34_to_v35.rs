//! CRM v34 -> v35 (app 0.5.23): ordered Creator text tags.
use rusqlite::Connection;

#[cfg(test)]
mod tests;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    validate_source_payloads(connection)?;
    connection.execute_batch("ALTER TABLE creators ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'
               CHECK(json_valid(tags_json) AND json_type(tags_json) = 'array' AND json_array_length(tags_json) <= 3);")
        .map_err(|error| error.to_string())?;
    for (key, payload) in replay_updates(connection)? {
        connection
            .execute(
                "UPDATE idempotency_records SET result_json = ?2 WHERE key = ?1",
                rusqlite::params![key, payload],
            )
            .map_err(|error| error.to_string())?;
    }
    // Released imports retain their operation/receipt IDs and digest keys.
    // Evolve the additive impact count explicitly rather than defaulting a missing field.
    connection
        .execute_batch(
            "UPDATE creator_import_operations
        SET impact_json = json_set(impact_json, '$.creator_tags_filled', 0);
        UPDATE creator_import_receipts
        SET impact_json = json_set(impact_json, '$.creator_tags_filled', 0);
        UPDATE creator_import_operations
        SET result_json = json_set(result_json, '$.receipt.impact.creator_tags_filled', 0)
        WHERE json_type(result_json, '$.receipt.impact') = 'object';",
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn replay_updates(connection: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut updates = Vec::new();
    // Only this released replay contract embeds a complete Creator snapshot.
    // Input omissions, request fingerprints, pending confirmations and receipts stay intact.
    let mut statement = connection
        .prepare(
            "SELECT key, result_json FROM idempotency_records
        WHERE action = 'creator_candidate_save_confirmation' AND state = 'completed'",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for (key, raw) in rows {
        let mut payload: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|_| "invalid released Creator save replay payload".to_string())?;
        let creator = payload
            .pointer_mut("/result/creator/creator")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("released Creator save replay is missing its Creator snapshot")?;
        evolve_creator_snapshot(creator)?;
        updates.push((key, payload.to_string()));
    }
    let mut statement = connection
        .prepare(
            "SELECT key, result_json FROM idempotency_records
        WHERE action = 'creator_batch_create' AND state = 'completed'",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for (key, raw) in rows {
        let mut payload: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|_| "invalid released Creator batch replay".to_string())?;
        let outcomes = payload
            .get_mut("outcomes")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or("released Creator batch replay is missing outcomes")?;
        let indexed = outcomes
            .iter()
            .filter(|outcome| outcome.get("input_index").is_some())
            .count();
        if indexed != 0 && indexed != outcomes.len() {
            return Err(
                "released Creator batch replay mixes indexed and unindexed outcomes".into(),
            );
        }
        for (input_index, outcome) in outcomes.iter_mut().enumerate() {
            // d565d98a introduced batch results in input order, without indices.
            // 31ee40c9 added input_index to every outcome; mixed shapes are not historical.
            if indexed == 0 {
                outcome
                    .as_object_mut()
                    .ok_or("released Creator batch outcome is not an object")?
                    .insert("input_index".into(), serde_json::json!(input_index));
            }
            if outcome.get("kind").and_then(serde_json::Value::as_str) == Some("accepted") {
                let creator = outcome
                    .get_mut("creator")
                    .and_then(serde_json::Value::as_object_mut)
                    .ok_or("released Creator batch result is missing its Creator")?;
                if creator
                    .insert("tags".into(), serde_json::json!([]))
                    .is_some()
                {
                    return Err("released v34 Creator batch result already contains tags".into());
                }
            }
        }
        serde_json::from_value::<crate::ManualCreatorBatchCreateResult>(payload.clone())
            .map_err(|_| "invalid released Creator batch result".to_string())?;
        updates.push((key, payload.to_string()));
    }
    Ok(updates)
}

fn evolve_creator_snapshot(
    creator: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    // Completed replays outlive the database schema that first serialized them.
    // a319a0c2 (schema17) removed entity_kind, but did not rewrite these snapshots.
    // Preserve that historical field and validate its released enum when present.
    if let Some(kind) = creator.get("entity_kind")
        && !matches!(kind.as_str(), Some("person" | "team" | "brand"))
    {
        return Err("invalid historical Creator entity_kind".into());
    }
    for field in [
        "id",
        "display_name",
        "notes",
        "created_at",
        "updated_at",
        "archived_at",
    ] {
        if !creator.contains_key(field) {
            return Err("historical Creator snapshot is missing a base field".into());
        }
    }
    if creator.contains_key("tags") {
        return Err("released v34 Creator save replay already contains tags".into());
    }
    let quality_fields = ["score", "assessment", "follower_counts"];
    match quality_fields
        .iter()
        .filter(|field| creator.contains_key(**field))
        .count()
    {
        0 if !creator.contains_key("pinned_at") => {
            // a131d0d8 (schema18): nullable quality fields and BTreeMap<Platform,u64>.
            // The frozen v17->v18 DDL uses NULL / NULL / '{}', never an array.
            creator.insert("score".into(), serde_json::Value::Null);
            creator.insert("assessment".into(), serde_json::Value::Null);
            creator.insert("follower_counts".into(), serde_json::json!({}));
        }
        3 => {}
        _ => return Err("historical Creator snapshot has incomplete quality fields".into()),
    }
    // 73db335c (schema21) initializes missing pin state to NULL.
    creator
        .entry("pinned_at")
        .or_insert(serde_json::Value::Null);
    creator.insert("tags".into(), serde_json::json!([]));
    let decoded =
        serde_json::from_value::<crate::Creator>(serde_json::Value::Object(creator.clone()))
            .map_err(|_| "invalid released Creator save snapshot".to_string())?;
    if decoded.score.is_some_and(|score| score > 100) {
        return Err("invalid historical Creator score".into());
    }
    Ok(())
}

/// Read-only preflight validates every released payload this edge must evolve.
pub(super) fn validate_source_payloads(connection: &Connection) -> Result<(), String> {
    replay_updates(connection)?;
    let mut statement = connection
        .prepare(
            "SELECT impact_json, status, result_json FROM creator_import_operations
        UNION ALL SELECT impact_json, NULL, NULL FROM creator_import_receipts",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (raw, status, result_json) = row.map_err(|error| error.to_string())?;
        let mut impact: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|_| "invalid released Creator import impact".to_string())?;
        let object = impact
            .as_object_mut()
            .ok_or("released Creator import impact is not an object")?;
        if object
            .insert("creator_tags_filled".into(), serde_json::json!(0))
            .is_some_and(|value| value != serde_json::json!(0))
        {
            return Err("released Creator import impact already contains tag changes".into());
        }
        if [
            "creators_merged_by_id",
            "creators_merged_by_confirmed_account",
            "origins_created",
        ]
        .iter()
        .any(|field| object.contains_key(*field))
        {
            validate_terminal_legacy_import_impact(
                object,
                status.as_deref(),
                result_json.as_deref(),
            )?;
        } else {
            serde_json::from_value::<crate::CreatorImportImpact>(impact)
                .map_err(|_| "invalid released Creator import impact fields".to_string())?;
        }
    }
    Ok(())
}

fn validate_terminal_legacy_import_impact(
    impact: &serde_json::Map<String, serde_json::Value>,
    status: Option<&str>,
    result_json: Option<&str>,
) -> Result<(), String> {
    // Frozen c5ab2394 impact contract. 2862559f replaced source-UUID/confirmed-account
    // resolution with portable public identity and retired origin creation. These
    // counts cannot be relabelled as current public-identity/contact-route counts.
    const FIELDS: &[&str] = &[
        "creator_inputs",
        "creators_created",
        "creators_merged_by_id",
        "creators_merged_by_confirmed_account",
        "creator_scores_filled",
        "follower_counts_filled",
        "platform_accounts_created",
        "claimed_links_created",
        "confirmed_links_created",
        "links_promoted_to_confirmed",
        "profile_observations_created",
        "origins_created",
        "creators_unchanged",
        "creator_tags_filled",
    ];
    if impact.len() != FIELDS.len()
        || FIELDS.iter().any(|field| {
            impact
                .get(*field)
                .and_then(serde_json::Value::as_u64)
                .is_none()
        })
    {
        return Err("invalid frozen legacy Creator import impact fields".into());
    }
    let result = result_json
        .and_then(|raw| serde_json::from_str::<crate::ConfirmCreatorImportResult>(raw).ok());
    if status != Some("completed")
        || !matches!(
            result,
            Some(
                crate::ConfirmCreatorImportResult::Cancelled { .. }
                    | crate::ConfirmCreatorImportResult::Expired { .. }
                    | crate::ConfirmCreatorImportResult::Stale { .. }
            )
        )
    {
        return Err(
            "legacy Creator import impact requires a terminal result without a receipt".into(),
        );
    }
    // Preserve all original counters; migrate only adds the universal tags=0 count.
    // The owner returns these terminal results before interpreting bundle/impact.
    Ok(())
}
