//! v37 evolution matrix: every supported source, exact impact, locked
//! revalidation, WAL recovery points, rollback and committed failure evidence.
use super::*;
use crate::{CreatorDatabaseStatus, CreatorDatabaseUpgradeBlocker, CreatorDatabaseUpgradeImpact};
use std::collections::BTreeMap;

fn backups(directory: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".backup-")
        })
        .collect()
}
fn version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}
fn count(connection: &Connection, table: &str) -> u64 {
    connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn schema_identity(connection: &Connection) -> BTreeMap<String, String> {
    connection.prepare("SELECT type || ':' || name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND sql IS NOT NULL ORDER BY type,name").unwrap().query_map([],|row|Ok((row.get(0)?,schema::normalize_sql_for_test(&row.get::<_,String>(1)?)))).unwrap().collect::<Result<_,_>>().unwrap()
}

fn source(from: i64) -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    let repository = SqliteRepository::open(&path).unwrap();
    drop(repository);
    let connection = Connection::open(&path).unwrap();
    match from {
        27 => schema::downgrade_reference_to_v27(&connection),
        28 => schema::downgrade_reference_to_v28(&connection),
        29 => schema::downgrade_reference_to_v29(&connection),
        30 => schema::downgrade_reference_to_v30(&connection),
        31 => schema::downgrade_reference_to_v31(&connection),
        32 => schema::downgrade_reference_to_v32(&connection),
        33 => schema::downgrade_reference_to_v33(&connection),
        34 => schema::downgrade_reference_to_v34(&connection),
        35 => schema::downgrade_reference_to_v35(&connection),
        36 => schema::downgrade_reference_to_v36(&connection),
        _ => panic!("unsupported fixture"),
    }
    .unwrap();
    assert_eq!(version(&connection), from);
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;
        INSERT INTO creators(id,display_name,notes,created_at,updated_at) VALUES('creator','Preserved Creator','Preserved note',1,2);
        INSERT INTO creator_relationships(creator_id,stage,created_at,updated_at) VALUES('creator','prospect',1,2);
        INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('operator','x','profile','Preserved operator',1,2);
        INSERT INTO platform_accounts(id,platform,target_kind,external_id,created_at,updated_at) VALUES('account','x','user','external',1,2);
        INSERT INTO creator_account_links(id,creator_id,platform_account_id,state,confidence,created_at,updated_at) VALUES('link','creator','account','confirmed','exact',1,2);
        INSERT INTO observation_sources(id,kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at) VALUES('source','manual_entry','x','https://example.invalid/creator','stable-origin','operator','{\"preserved\":true}',3);
        INSERT INTO platform_account_observations(id,platform_account_id,source_id,handle,display_name,profile_url,public_json,observed_at) VALUES('observation','account','source','creator','Preserved Profile','https://example.invalid/creator','{\"avatar_url\":\"https://example.invalid/avatar.png\",\"follower_count\":42000}',3);
        INSERT INTO creator_origins(id,kind,creator_id,source_id,discovered_at) VALUES('origin','manual_entry','creator','source',3);
        INSERT INTO resolved_targets(id,account_ref_id,platform,target_kind,external_target_id,platform_account_id,metadata_json,created_at,updated_at) VALUES('target','operator','x','user','external','account','{\"handle\":\"creator\"}',1,2);
        INSERT INTO conversations(id,account_ref_id,resolved_target_id,channel,provider_thread_id,created_at,updated_at) VALUES('conversation','operator','target','x','thread',1,2);
        INSERT INTO messages(id,conversation_id,external_message_id,direction,body,status,sent_at) VALUES('message','conversation','provider-message','inbound','Preserved message body','received',4);
        INSERT INTO idempotency_records(key,action,state,request_fingerprint,result_json,created_at,updated_at) VALUES('old-pending','relationship_update','remote_accepted','old fingerprint','  {\"accepted\":true}  ',1,2),('p5-receipt','creator_save_batch_v1','completed','sha256:v1:fixture',' {\"outcomes\":[]} ',1,2),('email-receipt','email_send','completed','email fingerprint',' {\"provider_message_id\":\"email-1\"} ',1,2);").unwrap();
    let kind = if from == 36 {
        "platform_search"
    } else {
        "ad_hoc_search"
    };
    let origin = serde_json::json!({"kind":kind,"discovery_id":"discovery","account_ref_id":"operator","query":"q","search_kind":"users"});
    let candidate = serde_json::json!({"candidate_id":"x:user:external","origin":origin,"account":{"platform":"x","target_kind":"user","external_id":"external","handle":"creator","display_name":"Creator","profile_url":null},"public_data":{},"contact_discovery":{"routes":[],"enrichments":[]},"identity_resolution":{"status":"new"},"allowed_actions":[]});
    connection.execute("INSERT INTO creator_candidate_snapshots(discovery_id,candidate_id,origin_kind,origin_json,candidate_json,account_platform,account_target_kind,account_external_id,account_handle,account_display_name,source_kind,account_ref_id,created_at,updated_at) VALUES('discovery','x:user:external',?1,?2,?3,'x','user','external','creator','Creator',?1,'operator',1,2)",rusqlite::params![kind,origin.to_string(),candidate.to_string()]).unwrap();
    if from >= 33 {
        connection.execute_batch("INSERT INTO account_refs(id,platform,kind,label,created_at,updated_at) VALUES('tto-account','tiktok','profile','Preserved TikTok account',1,2); INSERT INTO tiktok_one_account_bindings(account_ref_id,connection_id,cloud_account_id,installation_id,access_user_id,created_at,updated_at) VALUES('tto-account','connection','cloud','installation','access',1,2);").unwrap();
    }
    drop(connection);
    (directory, path)
}

fn preserved(connection: &Connection) -> Vec<Vec<rusqlite::types::Value>> {
    let queries = [
        "SELECT id,display_name,notes,created_at,updated_at FROM creators ORDER BY id",
        "SELECT * FROM creator_relationships ORDER BY creator_id",
        "SELECT * FROM account_refs ORDER BY id",
        "SELECT * FROM platform_accounts ORDER BY id",
        "SELECT * FROM creator_account_links ORDER BY id",
        "SELECT * FROM platform_account_observations ORDER BY id",
        "SELECT * FROM observation_sources ORDER BY id",
        "SELECT * FROM resolved_targets ORDER BY id",
        "SELECT * FROM conversations ORDER BY id",
        "SELECT * FROM messages ORDER BY id",
        "SELECT key,action,state,request_fingerprint,result_json,created_at,updated_at FROM idempotency_records WHERE key IN ('p5-receipt','email-receipt') ORDER BY key",
    ];
    let mut facts = Vec::new();
    for query in queries {
        let mut statement = connection.prepare(query).unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|index| row.get(index))
                    .collect::<Result<Vec<rusqlite::types::Value>, _>>()
            })
            .unwrap();
        facts.extend(rows.map(Result::unwrap));
    }
    facts
}

#[test]
fn every_v27_through_v36_source_round_trips_to_exact_v37_with_preserved_business_rows() {
    let (_fresh_dir, fresh_path) = current_database();
    let fresh = schema_identity(&Connection::open(fresh_path).unwrap());
    for from in 27..=36 {
        let (directory, path) = source(from);
        let connection = Connection::open(&path).unwrap();
        let before = preserved(&connection);
        let source_schema = schema_identity(&connection);
        drop(connection);
        let expected = CreatorDatabaseUpgradeImpact {
            removed_candidate_snapshots: 1,
            removed_tiktok_one_account_bindings: u64::from(from >= 33),
            unresolved_retired_outreach_actions: 1,
            ..Default::default()
        };
        assert_eq!(
            crate::CreatorRuntime::inspect_database(&path),
            CreatorDatabaseStatus::UpgradeRequired {
                from_version: from,
                to_version: schema::CURRENT_SCHEMA_VERSION,
                impact: expected.clone()
            },
            "source {from}"
        );
        assert!(
            SqliteRepository::open(&path).is_err(),
            "normal open must not upgrade {from}"
        );
        assert!(backups(directory.path()).is_empty());
        let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
        assert_eq!(upgraded.impact, expected);
        assert_eq!(upgraded.to_version, schema::CURRENT_SCHEMA_VERSION);
        let current = Connection::open(&path).unwrap();
        assert_eq!(version(&current), schema::CURRENT_SCHEMA_VERSION);
        assert_eq!(schema_identity(&current), fresh, "schema {from}");
        assert_eq!(preserved(&current), before, "facts {from}");
        let backup = Connection::open(&upgraded.backup_path).unwrap();
        assert_eq!(version(&backup), from);
        assert_eq!(schema_identity(&backup), source_schema);
        assert_eq!(preserved(&backup), before);
        assert_eq!(count(&backup, "creator_candidate_snapshots"), 1);
        assert_eq!(
            backup
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&upgraded.backup_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            crate::CreatorRuntime::inspect_database(&path),
            CreatorDatabaseStatus::Ready
        );
        drop(current);
        SqliteRepository::open(&path).unwrap();
        assert!(crate::CreatorRuntime::upgrade_database(&path).is_err());
        assert_eq!(
            backups(directory.path()).len(),
            1,
            "current never backs up again"
        );
    }
}

#[test]
fn v37_invalid_delete_impact_is_a_typed_blocker_before_backup() {
    let (directory, path) = source(36);
    let connection = Connection::open(&path).unwrap();
    connection.execute("INSERT INTO creator_contact_route_delete_operations(id,creator_id,contact_route_id,plan_fingerprint,impact_json,status,result_json,expires_at,created_at,updated_at) VALUES('bad','creator','removed-route','old','{}','prepared',NULL,99,1,2)",[]).unwrap();
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        CreatorDatabaseStatus::Blocked {
            version: 36,
            blocker: CreatorDatabaseUpgradeBlocker::AmbiguousContactRouteDeleteImpact
        }
    );
    let error = crate::CreatorRuntime::upgrade_database(&path).unwrap_err();
    assert_eq!(
        error.support_code,
        CreatorDatabaseUpgradeBlocker::AmbiguousContactRouteDeleteImpact.support_code()
    );
    assert!(!error.storage_upgraded);
    assert!(backups(directory.path()).is_empty());
    assert_eq!(version(&connection), 36);
    assert_eq!(count(&connection, "creator_candidate_snapshots"), 1);
}

#[test]
fn v37_rechecks_impact_under_the_lock_before_backup() {
    let (directory, path) = source(36);
    let mutation = path.clone();
    let error=schema::upgrade_with_before_lock_for_test(&path,move||{Connection::open(mutation).unwrap().execute("INSERT INTO idempotency_records(key,action,state,created_at,updated_at) VALUES('racing-confirmation','creator_contact_confirmation','in_progress',1,2)",[]).unwrap();}).unwrap_err();
    assert_eq!(error.support_code, "LOCAL_DATA_SOURCE_CHANGED");
    assert!(!error.storage_upgraded);
    assert!(backups(directory.path()).is_empty());
    assert_eq!(version(&Connection::open(path).unwrap()), 36);
}

#[test]
fn v37_wal_rows_reach_both_the_current_store_and_the_independent_backup() {
    let (_directory, path) = source(36);
    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("PRAGMA wal_autocheckpoint=0; INSERT INTO messages(id,conversation_id,external_message_id,direction,body,status,sent_at) VALUES('wal-message','conversation','wal-receipt','inbound','Committed WAL content','received',5);").unwrap();
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    for path in [&path, std::path::Path::new(&upgraded.backup_path)] {
        let connection = Connection::open(path).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT body FROM messages WHERE id='wal-message'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "Committed WAL content"
        );
    }
}

#[test]
fn v37_successor_validation_failure_rolls_back_but_retains_a_verified_recovery_point() {
    let (directory, path) = source(36);
    let before = preserved(&Connection::open(&path).unwrap());
    let error = schema::upgrade_with_post_migration_mutation_for_test(&path, |transaction| {
        transaction
            .execute_batch("CREATE TABLE unexpected_drift(id TEXT)")
            .unwrap();
    })
    .unwrap_err();
    assert_eq!(error.support_code, "LOCAL_DATA_VALIDATION_FAILED");
    assert!(!error.storage_upgraded);
    let current = Connection::open(&path).unwrap();
    assert_eq!(version(&current), 36);
    assert_eq!(preserved(&current), before);
    assert_eq!(count(&current, "creator_candidate_snapshots"), 1);
    assert_eq!(backups(directory.path()).len(), 1);
}

#[test]
fn v37_post_commit_failure_reports_committed_storage_and_preserves_the_old_recovery_point() {
    let (directory, path) = source(36);
    let error = schema::upgrade_with_post_commit_mutation_for_test(&path, |path| {
        Connection::open(path)
            .unwrap()
            .execute_batch("CREATE TABLE post_commit_drift(id TEXT)")
            .unwrap();
    })
    .unwrap_err();
    assert!(error.storage_upgraded);
    assert_eq!(error.support_code, "LOCAL_DATA_POSTFLIGHT_FAILED");
    let current = Connection::open(&path).unwrap();
    assert_eq!(version(&current), schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(backups(directory.path()).len(), 1);
    let backup = Connection::open(&backups(directory.path())[0]).unwrap();
    assert_eq!(version(&backup), 36);
    assert_eq!(count(&backup, "creator_candidate_snapshots"), 1);
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[tokio::test]
async fn v37_retired_marker_releases_the_old_route_block_and_status_keeps_remote_uncertainty() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    let repository = SqliteRepository::open(&path).unwrap();
    let creator = repository
        .create_creator(CreateCreatorInput {
            display_name: "Creator".into(),
            notes: None,
            tags: None,
        })
        .unwrap();
    let locator = repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "creator".into(),
            display_value: "@creator".into(),
        })
        .unwrap();
    let route = repository
        .link_creator_contact_route(&creator.creator.id, &locator.id, Confidence::Exact)
        .unwrap();
    {
        let connection = repository.lock().unwrap();
        schema::downgrade_reference_to_v36(&connection).unwrap();
        connection.execute("INSERT INTO idempotency_records(key,action,contact_route_id,state,result_json,created_at,updated_at) VALUES('old-route-effect','relationship_update',?1,'remote_accepted',' {\"private\":\"old provider payload\"} ',1,2)",[route.id.as_str()]).unwrap();
    }
    let input = crate::PrepareCreatorContactRouteDeleteInput {
        creator_id: creator.creator.id.clone(),
        contact_route_id: route.id.clone(),
    };
    assert!(matches!(
        repository
            .prepare_creator_contact_route_delete(input.clone())
            .unwrap(),
        crate::PrepareCreatorContactRouteDeleteResult::Blocked {
            reason: crate::CreatorContactRouteDeleteBlockReason::InFlightExternalAction,
            ..
        }
    ));
    drop(repository);
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(upgraded.impact.unresolved_retired_outreach_actions, 1);
    let runtime =
        crate::CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    let status = runtime
        .creator_outreach_status("old-route-effect")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        &status,
        crate::CreatorOutreachStatus::Retired {
            phase: crate::CreatorRetiredOutreachPhase::RetiredActionUnresolved,
            marked: true,
            ..
        }
    ));
    assert!(
        !serde_json::to_string(&status)
            .unwrap()
            .contains("old provider payload")
    );
    let crate::PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
        operation_id, ..
    } = runtime.prepare_creator_contact_route_delete(input).unwrap()
    else {
        panic!("retired action must no longer block deletion")
    };
    assert!(matches!(
        runtime
            .confirm_creator_contact_route_delete(crate::ConfirmCreatorContactRouteDeleteInput {
                operation_id
            })
            .unwrap(),
        crate::ConfirmCreatorContactRouteDeleteResult::Deleted { .. }
    ));
}
