use super::*;
use crate::repository::sqlite::schema;
use serde_json::json;

const IMPACT: &str = r#"{"contact_routes":1,"dependencies":0,"email_threads":0,"email_messages":0,"email_recipients":0,"email_drafts":0,"direct_message_conversations":0,"direct_messages":0,"outreach_touchpoints":0,"audit_events":0,"idempotency_records":0,"target_resolutions":0,"identity_conflicts":0,"observations":0,"contact_locators":0,"observation_sources":0,"resolved_targets":0,"candidate_snapshots_detached":2,"creator_origins_detached":0,"local_records_total":3}"#;

fn fixture() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    schema::current::create_current_schema_unchecked(&connection).unwrap();
    schema::downgrade_reference_to_v36(&connection).unwrap();
    connection.execute_batch("INSERT INTO creators(id,display_name,created_at,updated_at) VALUES('creator','Preserved Creator',1,2);
        INSERT INTO creator_relationships(creator_id,stage,created_at,updated_at) VALUES('creator','prospect',1,2);
        INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator','x','profile','Operator',1,2),('telegram','telegram','profile','Telegram',1,2),('tto-account','tiktok','profile','Preserved TikTok account',1,2);
        INSERT INTO contact_locators(id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES('locator','x','handle','creator','@creator',1,2);
        INSERT INTO creator_contact_routes(id,creator_id,locator_id,confidence,created_at,updated_at) VALUES('route','creator','locator','exact',1,2);
        INSERT INTO tiktok_one_account_bindings(account_ref_id,connection_id,cloud_account_id,installation_id,access_user_id,created_at,updated_at) VALUES('tto-account','connection','cloud','installation','access',1,2);").unwrap();
    for kind in ["platform_search", "community_administrator"] {
        let platform = if kind == "platform_search" {
            "x"
        } else {
            "telegram"
        };
        let candidate = json!({"candidate_id":kind,"origin":{"kind":kind,"discovery_id":"discovery"},"account":{"platform":platform,"target_kind":"user","external_id":kind}}).to_string();
        connection.execute("INSERT INTO creator_candidate_snapshots(discovery_id,candidate_id,origin_kind,origin_json,candidate_json,account_platform,account_target_kind,account_external_id,account_display_name,administrator_role,community_external_id,community_title,community_target_kind,source_kind,account_ref_id,created_at,updated_at)
            VALUES('discovery',?1,?1,'{}',?2,?3,'user',?1,'Name',CASE WHEN ?1='community_administrator' THEN 'owner' END,CASE WHEN ?1='community_administrator' THEN 'community' END,CASE WHEN ?1='community_administrator' THEN 'Community' END,CASE WHEN ?1='community_administrator' THEN 'channel' END,CASE WHEN ?1='platform_search' THEN 'platform_search' ELSE 'operator_community' END,CASE WHEN ?1='platform_search' THEN 'operator' ELSE 'telegram' END,1,2)", params![kind,candidate,platform]).unwrap();
    }
    connection
}

fn operation(connection: &Connection, id: &str, completed: bool, impact: &str) {
    let result = completed.then(|| format!(r#"{{"status":"deleted","operation_id":"{id}","creator_id":"creator","contact_route_id":"route","deleted":{impact},"remote_not_deleted":true}}"#));
    connection.execute("INSERT INTO creator_contact_route_delete_operations(id,creator_id,contact_route_id,plan_fingerprint,impact_json,status,result_json,expires_at,created_at,updated_at) VALUES(?1,'creator','route','released-fingerprint',?2,?3,?4,9999999999,1,2)", params![id,impact,if completed {"completed"} else {"prepared"},result]).unwrap();
}

type StoredReceiptRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    u64,
    u64,
);

fn receipt_rows(connection: &Connection) -> Vec<StoredReceiptRow> {
    connection.prepare("SELECT key,action,state,request_fingerprint,result_json,created_at,updated_at FROM idempotency_records ORDER BY key").unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).unwrap().collect::<Result<_,_>>().unwrap()
}

#[test]
fn v37_retirement_counts_objects_and_preserves_unrelated_receipt_bytes() {
    let connection = fixture();
    operation(&connection, "prepared", false, IMPACT);
    operation(&connection, "completed", true, IMPACT);
    let zero: String = IMPACT
        .replace(
            "\"candidate_snapshots_detached\":2",
            "\"candidate_snapshots_detached\":0",
        )
        .replace("\"local_records_total\":3", "\"local_records_total\":1");
    operation(&connection, "zero", false, &zero);
    for action in [
        "relationship_update",
        "content_reply_send",
        "direct_message_send",
        "relationship_update_v1",
        "content_reply_send_v1",
        "direct_message_send_v1",
        "creator_candidate_save_confirmation",
        "creator_contact_confirmation",
    ] {
        for state in [
            "in_progress",
            "remote_started",
            "remote_accepted",
            "remote_verified",
            "completed",
            "failed",
        ] {
            let key = format!("{action}:{state}");
            let payload = if state == "in_progress" {
                None
            } else {
                Some(format!(
                    "  {{\"frozen\":\"{action}:{state}\",\"body\":\"synthetic private payload\"}}\n"
                ))
            };
            connection.execute("INSERT INTO idempotency_records(key,action,contact_route_id,state,request_fingerprint,result_json,created_at,updated_at) VALUES(?1,?2,'route',?3,'frozen fingerprint bytes',?4,1,2)",params![key,action,state,payload]).unwrap();
        }
    }
    for (key, action, payload) in [
        (
            "creator-save:result:call",
            "creator_save_batch_v1",
            r#" {"version":1,"outcomes":[]} "#,
        ),
        (
            "email",
            "email_send",
            r#" {"provider_message_id":"email-1"} "#,
        ),
    ] {
        connection.execute("INSERT INTO idempotency_records(key,action,state,request_fingerprint,result_json,created_at,updated_at) VALUES(?1,?2,'completed','exact fingerprint',?3,1,2)",params![key,action,payload]).unwrap();
    }
    let before = receipt_rows(&connection);
    let mut impact = CreatorDatabaseUpgradeImpact::default();
    preflight(&connection, 36, &mut impact).unwrap();
    assert_eq!(impact.removed_candidate_snapshots, 2);
    assert_eq!(
        impact.rewritten_contact_route_delete_impacts, 4,
        "prepared + completed plan/result + zero-count plan"
    );
    assert_eq!(impact.removed_tiktok_one_account_bindings, 1);
    assert_eq!(impact.removed_creator_save_confirmations, 6);
    assert_eq!(impact.removed_contact_confirmations, 6);
    assert_eq!(impact.unresolved_retired_outreach_actions, 12);
    assert_eq!(
        before,
        receipt_rows(&connection),
        "preflight must not write"
    );
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    migrate(&connection).unwrap();
    schema::validate_database_for_version(&connection, 37).unwrap();
    connection.execute_batch("COMMIT").unwrap();
    let after = receipt_rows(&connection);
    for old in before {
        if [
            "creator_candidate_save_confirmation",
            "creator_contact_confirmation",
        ]
        .contains(&old.1.as_str())
        {
            assert!(!after.iter().any(|row| row.0 == old.0));
            continue;
        }
        let new = after.iter().find(|row| row.0 == old.0).unwrap();
        if [
            "relationship_update",
            "content_reply_send",
            "direct_message_send",
        ]
        .contains(&old.1.as_str())
            && !["completed", "failed"].contains(&old.2.as_str())
        {
            assert_eq!(new.2, "failed");
            assert_eq!(new.3, old.3);
            assert_eq!(new.5, old.5);
            let marker: serde_json::Value =
                serde_json::from_str(new.4.as_deref().unwrap()).unwrap();
            assert_eq!(marker["kind"], "retired_action_unresolved");
            assert_eq!(marker["action"], old.1);
            assert_eq!(marker["state_before"], old.2);
            assert_eq!(
                marker["recovery"].as_str(),
                old.4.as_deref(),
                "old payload text/NULL survives exactly"
            );
        } else {
            assert_eq!(new, &old, "preserved receipt changed");
        }
    }
    for table in ["creator_candidate_snapshots", "tiktok_one_account_bindings"] {
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE name=?1",
                    [table],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT label FROM account_refs WHERE id='tto-account'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Preserved TikTok account"
    );
    let (plan,result):(String,String)=connection.query_row("SELECT impact_json,result_json FROM creator_contact_route_delete_operations WHERE id='completed'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&plan).unwrap();
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(plan.get("candidate_snapshots_detached").is_none());
    assert_eq!(plan["local_records_total"], 1);
    assert_eq!(result["deleted"], plan);
    assert_eq!(connection.query_row("SELECT plan_fingerprint FROM creator_contact_route_delete_operations WHERE id='prepared'",[],|row|row.get::<_,String>(0)).unwrap(),"released-fingerprint");
}

#[test]
fn v37_preflight_rejects_ambiguous_impacts_and_result_attribution_without_writes() {
    for malformed in [
        "{}".to_owned(),
        IMPACT.replace("\"local_records_total\":3", "\"local_records_total\":1"),
        IMPACT.replace("\"dependencies\":0", "\"dependencies\":-1"),
        IMPACT.replace("\"dependencies\":0", "\"unknown\":0"),
    ] {
        let connection = fixture();
        operation(&connection, "bad", false, &malformed);
        let error = preflight(&connection, 36, &mut Default::default()).unwrap_err();
        assert!(matches!(
            error,
            MigrationPreflightError::Blocked(
                CreatorDatabaseUpgradeBlocker::AmbiguousContactRouteDeleteImpact
            )
        ));
        assert_eq!(connection.query_row("SELECT impact_json FROM creator_contact_route_delete_operations WHERE id='bad'",[],|row|row.get::<_,String>(0)).unwrap(),malformed);
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            36
        );
    }
    let connection = fixture();
    operation(&connection, "bad", true, IMPACT);
    connection.execute("UPDATE creator_contact_route_delete_operations SET result_json=json_set(result_json,'$.creator_id','another-creator')",[]).unwrap();
    assert!(matches!(
        preflight(&connection, 36, &mut Default::default()),
        Err(MigrationPreflightError::Blocked(_))
    ));
}

#[test]
fn v37_retirement_is_inside_the_callers_transaction() {
    let connection = fixture();
    operation(&connection, "prepared", false, IMPACT);
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    migrate(&connection).unwrap();
    connection.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        36
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM creator_candidate_snapshots",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT impact_json FROM creator_contact_route_delete_operations",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        IMPACT
    );
}
