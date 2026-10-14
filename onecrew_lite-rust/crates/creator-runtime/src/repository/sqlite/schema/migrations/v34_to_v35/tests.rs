use super::*;
use serde_json::{Value, json};

fn database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(
        "CREATE TABLE creators (id TEXT PRIMARY KEY, display_name TEXT, follower_counts_json TEXT);
         INSERT INTO creators VALUES ('historical-id', 'Current edited name', '{\"x\":999}');
         CREATE TABLE idempotency_records (
           key TEXT PRIMARY KEY, action TEXT, state TEXT, request_fingerprint TEXT,
           result_json TEXT, created_at INTEGER, updated_at INTEGER);
         CREATE TABLE creator_import_operations (
           id TEXT, content_sha256 TEXT, plan_fingerprint TEXT, status TEXT,
           impact_json TEXT, result_json TEXT);
         CREATE TABLE creator_import_receipts (impact_json TEXT);
         PRAGMA user_version = 34;",
    ).unwrap();
    connection
}

fn ancient_creator(with_entity_kind: bool) -> Value {
    // Frozen pre-schema18 Creator, with the pre-schema17 field optionally retained.
    let mut creator = json!({
        "id":"historical-id", "display_name":"Historical name", "notes":"Historical notes",
        "created_at":11, "updated_at":12, "archived_at":null
    });
    if with_entity_kind {
        creator["entity_kind"] = json!("person");
    }
    creator
}

fn quality_creator() -> Value {
    let mut creator = ancient_creator(false);
    creator["score"] = json!(17);
    creator["assessment"] = json!("Historical assessment");
    creator["follower_counts"] = json!({"x":123, "youtube":456});
    creator
}

fn save_payload(creator: Value) -> Value {
    json!({
        "confirmation_fingerprint":"original-confirmation",
        "result":{"creator":{"creator":creator}, "created":true}
    })
}

fn insert(connection: &Connection, key: &str, action: &str, payload: &Value) {
    connection.execute(
        "INSERT INTO idempotency_records VALUES (?1, ?2, 'completed', 'original-request', ?3, 21, 22)",
        rusqlite::params![key, action, payload.to_string()],
    ).unwrap();
}

fn read(connection: &Connection, key: &str) -> Value {
    let raw: String = connection
        .query_row(
            "SELECT result_json FROM idempotency_records WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn historical_creator_generations_share_readonly_preflight_and_explicit_migration() {
    let connection = database();
    let mut pinned = quality_creator();
    pinned["pinned_at"] = json!(31);
    let originals = [
        ancient_creator(true),
        ancient_creator(false),
        quality_creator(),
        pinned,
    ];
    for (index, creator) in originals.iter().enumerate() {
        insert(
            &connection,
            &index.to_string(),
            "creator_candidate_save_confirmation",
            &save_payload(creator.clone()),
        );
    }
    validate_source_payloads(&connection).unwrap();
    for (index, creator) in originals.iter().enumerate() {
        assert_eq!(
            read(&connection, &index.to_string()),
            save_payload(creator.clone())
        );
    }
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    migrate(&connection).unwrap();
    connection.execute_batch("COMMIT").unwrap();
    for (index, original) in originals.iter().enumerate() {
        let payload = read(&connection, &index.to_string());
        assert_eq!(payload["confirmation_fingerprint"], "original-confirmation");
        let converted = &payload["result"]["creator"]["creator"];
        for (field, value) in original.as_object().unwrap() {
            assert_eq!(
                &converted[field], value,
                "historical field changed: {field}"
            );
        }
        assert_eq!(converted["tags"], json!([]));
        if index < 2 {
            assert_eq!(converted["follower_counts"], json!({}));
            assert!(converted["score"].is_null());
            assert!(converted["assessment"].is_null());
        }
        if index < 3 {
            assert!(converted["pinned_at"].is_null());
        }
        serde_json::from_value::<crate::Creator>(converted.clone()).unwrap();
        let binding: (String, String, i64, i64) = connection.query_row(
            "SELECT request_fingerprint, state, created_at, updated_at FROM idempotency_records WHERE key=?1",
            [index.to_string()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap();
        assert_eq!(
            binding,
            ("original-request".into(), "completed".into(), 21, 22)
        );
    }
    let current: (String, String) = connection
        .query_row(
            "SELECT display_name, follower_counts_json FROM creators WHERE id='historical-id'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        current,
        ("Current edited name".into(), "{\"x\":999}".into())
    );
}

#[test]
fn malformed_historical_creator_fields_are_not_defaulted_or_written() {
    let mut cases = Vec::new();
    for value in [json!([]), Value::Null, json!("unknown"), json!({"x":-1})] {
        let mut creator = quality_creator();
        creator["follower_counts"] = value;
        cases.push(creator);
    }
    for (field, value) in [
        ("entity_kind", json!("unknown")),
        ("entity_kind", Value::Null),
        ("notes", json!([])),
        ("created_at", json!("yesterday")),
        ("pinned_at", json!("pinned")),
        ("score", json!(101)),
        ("assessment", json!([])),
    ] {
        let mut creator = quality_creator();
        creator[field] = value;
        cases.push(creator);
    }
    let mut partial = quality_creator();
    partial.as_object_mut().unwrap().remove("follower_counts");
    cases.push(partial);
    let mut missing_base = ancient_creator(false);
    missing_base.as_object_mut().unwrap().remove("notes");
    cases.push(missing_base);
    for creator in cases {
        let connection = database();
        let payload = save_payload(creator);
        insert(
            &connection,
            "invalid",
            "creator_candidate_save_confirmation",
            &payload,
        );
        assert!(validate_source_payloads(&connection).is_err());
        assert_eq!(read(&connection, "invalid"), payload);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('creators') WHERE name='tags_json'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
    }
}

fn batch_payload(indexed: bool) -> Value {
    // d565d98a introduced contact_routes and batch outcomes together.
    let mut payload = json!({"outcomes":[
        {"kind":"accepted", "creator":{"creator_id":"historical-id", "display_name":"Batch name",
            "stage":"prospect", "created_at":11, "contact_routes":[]}},
        {"kind":"rejected", "message":"Original rejection"}
    ], "replayed":false});
    if indexed {
        payload["outcomes"][0]["input_index"] = json!(0);
        payload["outcomes"][1]["input_index"] = json!(1);
    }
    payload
}

#[test]
fn historical_unindexed_batch_recovers_original_order_and_keeps_indexed_values() {
    for indexed in [false, true] {
        let connection = database();
        let payload = batch_payload(indexed);
        insert(&connection, "batch", "creator_batch_create", &payload);
        validate_source_payloads(&connection).unwrap();
        assert_eq!(read(&connection, "batch"), payload);
        migrate(&connection).unwrap();
        let result = read(&connection, "batch");
        assert_eq!(result["outcomes"][0]["input_index"], 0);
        assert_eq!(result["outcomes"][1]["input_index"], 1);
        assert_eq!(result["outcomes"][1]["message"], "Original rejection");
        assert_eq!(result["outcomes"][0]["creator"]["tags"], json!([]));
        for (field, value) in payload["outcomes"][0]["creator"].as_object().unwrap() {
            assert_eq!(&result["outcomes"][0]["creator"][field], value);
        }
        serde_json::from_value::<crate::ManualCreatorBatchCreateResult>(result).unwrap();
    }
}

#[test]
fn mixed_batch_indices_and_malformed_existing_fields_still_block() {
    let mut mixed = batch_payload(false);
    mixed["outcomes"][0]["input_index"] = json!(0);
    let mut wrong_index = batch_payload(true);
    wrong_index["outcomes"][0]["input_index"] = json!("zero");
    let mut missing_routes = batch_payload(false);
    missing_routes["outcomes"][0]["creator"]
        .as_object_mut()
        .unwrap()
        .remove("contact_routes");
    let mut wrong_routes = batch_payload(false);
    wrong_routes["outcomes"][0]["creator"]["contact_routes"] = Value::Null;
    for payload in [mixed, wrong_index, missing_routes, wrong_routes] {
        let connection = database();
        insert(&connection, "invalid", "creator_batch_create", &payload);
        assert!(validate_source_payloads(&connection).is_err());
        assert_eq!(read(&connection, "invalid"), payload);
    }
}

fn legacy_impact() -> Value {
    json!({
        "creator_inputs":7, "creators_created":1, "creators_merged_by_id":2,
        "creators_merged_by_confirmed_account":3, "creator_scores_filled":1,
        "follower_counts_filled":2, "platform_accounts_created":1,
        "claimed_links_created":1, "confirmed_links_created":2,
        "links_promoted_to_confirmed":1, "profile_observations_created":2,
        "origins_created":3, "creators_unchanged":1
    })
}

fn insert_import(connection: &Connection, status: &str, impact: &Value, result: &Value) {
    connection
        .execute(
            "INSERT INTO creator_import_operations VALUES
           ('terminal-id', 'original-digest', 'original-plan', ?1, ?2, ?3)",
            rusqlite::params![status, impact.to_string(), result.to_string()],
        )
        .unwrap();
}

#[test]
fn terminal_legacy_import_impact_retains_its_own_counts_and_replays_without_interpretation() {
    let connection = database();
    let original = legacy_impact();
    let result = json!({"operation_id":"terminal-id", "status":"cancelled"});
    insert_import(&connection, "completed", &original, &result);
    validate_source_payloads(&connection).unwrap();
    let read_import = || {
        connection.query_row(
        "SELECT content_sha256, plan_fingerprint, impact_json, result_json FROM creator_import_operations WHERE id='terminal-id'",
        [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
    ).unwrap()
    };
    assert_eq!(
        serde_json::from_str::<Value>(&read_import().2).unwrap(),
        original
    );
    migrate(&connection).unwrap();
    let (digest, fingerprint, impact_json, result_json) = read_import();
    assert_eq!(digest, "original-digest");
    assert_eq!(fingerprint, "original-plan");
    assert_eq!(serde_json::from_str::<Value>(&result_json).unwrap(), result);
    let evolved: Value = serde_json::from_str(&impact_json).unwrap();
    for (field, value) in original.as_object().unwrap() {
        assert_eq!(&evolved[field], value);
    }
    assert_eq!(evolved["creator_tags_filled"], 0);
    assert!(evolved.get("creators_merged_by_public_identity").is_none());
    assert!(evolved.get("contact_routes_created").is_none());
    let snapshot = crate::creator_import::ImportOperationSnapshot {
        operation_id: crate::CreatorImportOperationId::new("terminal-id"),
        content_sha256: digest,
        plan_fingerprint: fingerprint,
        impact_json,
        bundle_json: Some("obsolete bundle must never be interpreted".into()),
        status: "completed".into(),
        result_json: Some(result_json),
        expires_at: 0,
    };
    assert!(
        matches!(snapshot.completed_result().unwrap(), Some(crate::ConfirmCreatorImportResult::Cancelled { operation_id }) if operation_id.as_str() == "terminal-id")
    );
}

#[test]
fn active_legacy_imports_receipts_and_malformed_or_mixed_impacts_are_rejected() {
    let cancelled = json!({"operation_id":"terminal-id", "status":"cancelled"});
    let mut malformed = legacy_impact();
    malformed["origins_created"] = json!(-1);
    let mut missing = legacy_impact();
    missing
        .as_object_mut()
        .unwrap()
        .remove("creators_merged_by_id");
    let mut mixed = legacy_impact();
    mixed["creators_merged_by_public_identity"] = json!(5);
    for (status, impact, result) in [
        ("prepared", legacy_impact(), Value::Null),
        ("completed", malformed, cancelled.clone()),
        ("completed", missing, cancelled.clone()),
        ("completed", mixed, cancelled.clone()),
        (
            "completed",
            legacy_impact(),
            json!({"operation_id":"terminal-id", "status":"imported", "receipt":{}}),
        ),
    ] {
        let connection = database();
        insert_import(&connection, status, &impact, &result);
        assert!(validate_source_payloads(&connection).is_err());
    }
    let connection = database();
    connection
        .execute(
            "INSERT INTO creator_import_receipts VALUES (?1)",
            [legacy_impact().to_string()],
        )
        .unwrap();
    assert!(validate_source_payloads(&connection).is_err());
}
