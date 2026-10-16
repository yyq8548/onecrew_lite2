use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use rusqlite::Connection;

use super::*;
use crate::{
    ConfirmCreatorImportInput, ConfirmCreatorImportResult, ContactChannel, CreateCreatorGroupInput,
    CreatorGroupDestination, CreatorImportConflictKind, CreatorTransferAccount,
    CreatorTransferConfidence, CreatorTransferContactRoute, CreatorTransferCreator,
    CreatorTransferLinkState, CreatorTransferPlatform, CreatorTransferProfile,
    CreatorTransferRelationship, CreatorTransferStage, CreatorTransferTargetKind,
    CreatorWorkbookDisplay, CreatorWorkbookExport, ListCreatorGroupsInput, ListCreatorsInput,
    LocatorKind, PrepareCreatorImportResult, SetCreatorGroupInput, UpdateCreatorInput,
};

fn transfer_creator(
    _id: &str,
    display_name: &str,
    platform: CreatorTransferPlatform,
    external_id: &str,
    handle: &str,
) -> CreatorTransferCreator {
    CreatorTransferCreator {
        notes: None,
        tags: Vec::new(),
        semantic_sha256: String::new(),
        record_key: String::new(),
        display_name: display_name.into(),
        score: Some(90),
        follower_counts: BTreeMap::from([(platform, 42)]),
        pinned: true,
        relationship: CreatorTransferRelationship {
            stage: CreatorTransferStage::Engaged,
        },
        accounts: vec![CreatorTransferAccount {
            capture_order: 0,
            platform,
            target_kind: CreatorTransferTargetKind::User,
            external_id: external_id.into(),
            link_state: CreatorTransferLinkState::Confirmed,
            confidence: CreatorTransferConfidence::Exact,
            source_kind: SourceKind::InitialPlatformAccount,
            latest_profile: Some(CreatorTransferProfile {
                handle: Some(handle.into()),
                display_name: Some(format!("{display_name} profile")),
                profile_url: Some(format!("https://example.com/{handle}")),
                avatar_url: Some(format!("https://example.com/{handle}.png")),
            }),
        }],
        contact_routes: Vec::new(),
    }
}

#[test]
fn export_creator_import_node_compatibility_fixture() {
    let Some(output) = std::env::var_os("ONECREW_CREATOR_IMPORT_ORACLE_DIRECTORY") else {
        return;
    };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    for version in [2_u16, 3, 4] {
        let (directory, repository) = repository();
        let mut creator = transfer_creator(
            "source", "Oracle Creator", CreatorTransferPlatform::X, "oracle-x", "oracle_writer",
        );
        if version >= 3 { creator.tags = vec!["研究".into()]; }
        if version >= 4 {
            creator.notes = Some("Public import source evidence".into());
            creator.contact_routes.push(CreatorTransferContactRoute {
                save_order: 0, channel: ContactChannel::Wechat, locator_kind: LocatorKind::Handle,
                normalized_value: "oracle_wx".into(), display_value: "Oracle_WX".into(),
                confidence: CreatorTransferConfidence::Asserted,
                source_kinds: vec![SourceKind::CreatorImport],
            });
        }
        let current = crate::creator_transfer::build_creator_bundle(vec![creator]).unwrap();
        let staged = if version == 2 {
            crate::creator_transfer::legacy_v2_projection_for_test(current.clone())
        } else { current.clone() };
        let prepared = repository.prepare_creator_import(staged.clone()).unwrap();
        std::fs::write(output.join(format!("v{version}.bundle.json")), staged.to_staged_import_json_bytes().unwrap()).unwrap();
        std::fs::write(output.join(format!("v{version}.xlsx")), current.to_xlsx_bytes().unwrap()).unwrap();
        std::fs::write(output.join(format!("v{version}.prepared.json")), serde_json::to_vec(&prepared).unwrap()).unwrap();
        let connection = Connection::open(directory.path().join("crm.db")).unwrap();
        connection.execute("VACUUM INTO ?1", [output.join(format!("v{version}.sqlite")).to_str().unwrap()]).unwrap();
    }
}

#[test]
fn import_tags_fill_empty_preserve_absent_and_block_nonempty_conflicts() {
    let (_directory, repository) = repository();
    let mut incoming = transfer_creator(
        "tagged",
        "Tagged",
        CreatorTransferPlatform::X,
        "tagged-x",
        "tagged",
    );
    let initial = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming.clone()]))
            .unwrap(),
    );
    repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: initial,
        })
        .unwrap();
    let id = repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items[0]
        .id
        .clone();
    incoming.tags = vec!["美妆".into(), "Beauty".into()];
    let prepared = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming.clone()]))
            .unwrap(),
    );
    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: prepared,
        })
        .unwrap();
    assert!(changed);
    let ConfirmCreatorImportResult::Imported { receipt, .. } = result else {
        panic!("tag fill must be reported as imported")
    };
    assert_eq!(receipt.impact.creator_tags_filled, 1);
    assert_eq!(
        repository.get_creator(&id).unwrap().creator.tags,
        incoming.tags
    );
    incoming.display_name = "Older data".into();
    incoming.tags.clear();
    let prepared = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming.clone()]))
            .unwrap(),
    );
    repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: prepared,
        })
        .unwrap();
    assert_eq!(
        repository.get_creator(&id).unwrap().creator.tags,
        ["美妆", "Beauty"]
    );
    incoming.tags = vec!["Other".into()];
    let blocked = repository
        .prepare_creator_import(bundle(vec![incoming]))
        .unwrap();
    let PrepareCreatorImportResult::Blocked { conflicts, .. } = blocked else {
        panic!("nonempty tags must conflict")
    };
    assert!(
        conflicts
            .iter()
            .any(|conflict| conflict.kind == CreatorImportConflictKind::CreatorTagsConflict)
    );
    assert_eq!(
        repository.get_creator(&id).unwrap().creator.tags,
        ["美妆", "Beauty"]
    );
}

#[test]
fn v34_staged_v2_import_survives_upgrade_and_current_conversion_reuses_receipt() {
    let (directory, repository) = repository();
    let bytes = include_bytes!("../../../../tests/fixtures/creator_bundle_v2.json");
    let legacy = crate::CreatorBundle::from_staged_import_json_bytes(bytes).unwrap();
    let legacy_digest = legacy.content_sha256.clone();
    let prepared_id = operation_id(repository.prepare_creator_import(legacy).unwrap());
    let path = directory.path().join("crm.db");
    let connection = Connection::open(&path).unwrap();
    let fingerprint: String = connection
        .query_row(
            "SELECT plan_fingerprint FROM creator_import_operations WHERE id=?1",
            [prepared_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    connection.execute_batch("UPDATE creator_import_operations SET impact_json=json_remove(impact_json, '$.creator_tags_filled');").unwrap();
    drop(repository);
    schema::downgrade_reference_to_v34(&connection).unwrap();
    crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT plan_fingerprint FROM creator_import_operations WHERE id=?1",
                [prepared_id.as_str()],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        fingerprint
    );
    let repository = SqliteRepository::open(&path).unwrap();
    let current = crate::CreatorBundle::from_import_json_bytes(bytes).unwrap();
    assert_eq!(
        operation_id(repository.prepare_creator_import(current.clone()).unwrap()),
        prepared_id
    );
    let input = ConfirmCreatorImportInput {
        operation_id: prepared_id,
    };
    let (first, changed) = repository.confirm_creator_import(input.clone()).unwrap();
    assert!(changed);
    assert!(
        matches!(&first, ConfirmCreatorImportResult::Imported { receipt, .. } if receipt.content_sha256 == legacy_digest)
    );
    assert_eq!(
        repository.confirm_creator_import(input).unwrap(),
        (first, false)
    );
    let replay = repository.prepare_creator_import(current).unwrap();
    assert!(
        matches!(replay, PrepareCreatorImportResult::AlreadyImported { receipt } if receipt.content_sha256 == legacy_digest)
    );
    assert_eq!(
        repository
            .list_creators(ListCreatorsInput::default())
            .unwrap()
            .items
            .len(),
        1
    );
}

fn transfer_email_route(
    _route_id: &str,
    _locator_id: &str,
    save_order: u32,
    address: &str,
) -> CreatorTransferContactRoute {
    CreatorTransferContactRoute {
        save_order,
        channel: ContactChannel::Email,
        locator_kind: LocatorKind::EmailAddress,
        normalized_value: address.into(),
        display_value: address.into(),
        confidence: CreatorTransferConfidence::Exact,
        source_kinds: vec![SourceKind::ManualEntry],
    }
}

fn bundle(creators: Vec<CreatorTransferCreator>) -> crate::CreatorBundle {
    crate::creator_transfer::build_creator_bundle(creators).expect("canonical bundle")
}

fn rich_bundle(count: usize) -> crate::CreatorBundle {
    bundle(
        (0..count)
            .map(|index| {
                let mut creator = transfer_creator(
                    &format!("rich-{index}"),
                    &format!("Rich Creator {index}"),
                    CreatorTransferPlatform::X,
                    &format!("x-rich-{index}"),
                    &format!("rich_{index}"),
                );
                creator.accounts.push(CreatorTransferAccount {
                    capture_order: 1,
                    platform: CreatorTransferPlatform::Instagram,
                    target_kind: CreatorTransferTargetKind::Creator,
                    external_id: format!("ig-rich-{index}"),
                    link_state: CreatorTransferLinkState::Claimed,
                    confidence: CreatorTransferConfidence::Asserted,
                    source_kind: SourceKind::ProfileBio,
                    latest_profile: Some(CreatorTransferProfile {
                        handle: Some(format!("rich_ig_{index}")),
                        display_name: Some(format!("Rich IG {index}")),
                        profile_url: Some(format!("https://instagram.com/rich_ig_{index}")),
                        avatar_url: Some(format!("https://cdn.example/rich-ig-{index}.png")),
                    }),
                });
                creator.contact_routes.push(transfer_email_route(
                    &format!("route-rich-{index}"),
                    &format!("locator-rich-{index}"),
                    0,
                    &format!("rich-{index}@example.com"),
                ));
                creator
            })
            .collect(),
    )
}

static IMPORT_TRACE_TEST_LOCK: Mutex<()> = Mutex::new(());
static IMPORT_TRACED_STATEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static IMPORT_WRITER_PAUSE: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> =
    Mutex::new(None);

fn record_import_statement(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(statement, trigger_or_sql) = event
        && !trigger_or_sql.trim_start().starts_with("--")
    {
        IMPORT_TRACED_STATEMENTS
            .lock()
            .unwrap()
            .push(statement.sql().into_owned());
    }
}

fn pause_import_writer(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(statement, trigger_or_sql) = event else {
        return;
    };
    if trigger_or_sql.trim_start().starts_with("--") {
        return;
    }
    if !statement
        .sql()
        .trim_start()
        .starts_with("INSERT INTO creators")
    {
        return;
    }
    let Some((entered, release)) = IMPORT_WRITER_PAUSE.lock().unwrap().take() else {
        return;
    };
    entered.send(()).unwrap();
    release.recv().unwrap();
}

fn arm_import_trace(repository: &SqliteRepository, callback: fn(rusqlite::trace::TraceEvent<'_>)) {
    let events = rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT;
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    first.trace_v2(events, Some(callback));
    second.trace_v2(events, Some(callback));
    drop((first, second));
    repository.lock().unwrap().trace_v2(events, Some(callback));
}

fn clear_import_trace(repository: &SqliteRepository) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    first.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    second.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    drop((first, second));
    repository
        .lock()
        .unwrap()
        .trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
}

fn allow_import_statement(_: rusqlite::hooks::AuthContext<'_>) -> rusqlite::hooks::Authorization {
    rusqlite::hooks::Authorization::Allow
}

fn arm_import_prepare_counter(repository: &SqliteRepository, counter: &Arc<AtomicUsize>) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    for connection in [&*first, &*second] {
        let counter = Arc::clone(counter);
        connection
            .authorizer(Some(move |_: rusqlite::hooks::AuthContext<'_>| {
                counter.fetch_add(1, Ordering::Relaxed);
                rusqlite::hooks::Authorization::Allow
            }))
            .unwrap();
    }
    drop((first, second));
    let counter = Arc::clone(counter);
    repository
        .lock()
        .unwrap()
        .authorizer(Some(move |_: rusqlite::hooks::AuthContext<'_>| {
            counter.fetch_add(1, Ordering::Relaxed);
            rusqlite::hooks::Authorization::Allow
        }))
        .unwrap();
}

fn clear_import_prepare_counter(repository: &SqliteRepository) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    first
        .authorizer(Some(allow_import_statement))
        .expect("replace first read authorizer");
    second
        .authorizer(Some(allow_import_statement))
        .expect("replace second read authorizer");
    drop((first, second));
    repository
        .lock()
        .unwrap()
        .authorizer(Some(allow_import_statement))
        .expect("replace writer authorizer");
}

fn traced_rich_import(count: usize) -> (Vec<String>, usize) {
    let (_directory, repository) = repository();
    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Existing local Creator".into(),
            notes: None,
        })
        .unwrap();
    let bundle = rich_bundle(count);
    arm_import_trace(&repository, record_import_statement);
    let prepare_counter = Arc::new(AtomicUsize::new(0));
    arm_import_prepare_counter(&repository, &prepare_counter);
    IMPORT_TRACED_STATEMENTS.lock().unwrap().clear();
    let operation_id = operation_id(repository.prepare_creator_import(bundle).unwrap());
    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        result,
        ConfirmCreatorImportResult::Imported { .. }
    ));
    let statements = std::mem::take(&mut *IMPORT_TRACED_STATEMENTS.lock().unwrap());
    let preparations = prepare_counter.load(Ordering::Relaxed);
    clear_import_trace(&repository);
    clear_import_prepare_counter(&repository);
    (statements, preparations)
}

fn explain_basis_query(connection: &Connection, sql: &str, lookup_json: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .unwrap();
    statement
        .query_map([lookup_json], |row| row.get::<_, String>(3))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn operation_id(result: PrepareCreatorImportResult) -> crate::CreatorImportOperationId {
    match result {
        PrepareCreatorImportResult::ConfirmationRequired { operation_id, .. } => operation_id,
        other => panic!("expected confirmation-required import, got {other:?}"),
    }
}

#[test]
fn changing_only_the_main_creator_name_imports_with_stale_child_display_names() {
    let mut creator = transfer_creator(
        "creator-name-edit",
        "Original Name",
        CreatorTransferPlatform::X,
        "x-name-edit",
        "name_edit",
    );
    creator.contact_routes.push(transfer_email_route(
        "route-name-edit",
        "locator-name-edit",
        0,
        "rename@example.com",
    ));
    let bundle = bundle(vec![creator]);
    let record_key = bundle.creators[0].record_key.clone();
    let workbook = CreatorWorkbookExport {
        bundle,
        display_by_record_key: BTreeMap::from([(
            record_key,
            CreatorWorkbookDisplay {
                last_touch_at: Some(1_700_000_000),
                updated_at: 1_700_000_100,
                group_name: Some("Display only".into()),
            },
        )]),
    };
    let xlsx = workbook.to_xlsx_bytes().unwrap();
    let edited = crate::creator_transfer_xlsx::rewrite_xlsx_part_for_test(
        &xlsx,
        "xl/worksheets/sheet1.xml",
        |sheet| sheet.replace("Original Name", "Renamed From Main Sheet"),
    );
    let imported = crate::CreatorBundle::from_xlsx_bytes(&edited).unwrap();
    assert_eq!(imported.creators[0].display_name, "Renamed From Main Sheet");
    assert_eq!(imported.creators[0].accounts.len(), 1);
    assert_eq!(imported.creators[0].contact_routes.len(), 1);

    let (_directory, repository) = repository();
    let operation_id = operation_id(repository.prepare_creator_import(imported).unwrap());
    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        result,
        ConfirmCreatorImportResult::Imported { .. }
    ));
    let summary = &repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items[0];
    assert_eq!(summary.display_name, "Renamed From Main Sheet");
    assert_eq!(summary.platform_account_count, 1);
    assert_eq!(summary.contact_route_count, 1);
}

#[test]
fn export_then_empty_current_restore_reconstructs_creator_list_semantics() {
    let (source_directory, source) = repository();
    let source_connection = Connection::open(source_directory.path().join("crm.db")).unwrap();
    source_connection
        .execute_batch(
            "INSERT INTO creator_groups
               (id, name, revision, position, created_at, updated_at)
             VALUES ('group-restore', 'Restore Group', 4, 2, 10, 11);
             INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at,
                score, assessment, follower_counts_json, pinned_at, group_id)
             VALUES ('creator-restore', 'Restore Creator', 'complete notes', 20, 21, NULL,
                     93, 'complete assessment', '{\"x\":321,\"instagram\":44}', 22,
                     'group-restore');
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES ('creator-restore', 'collaborating', 1, 23, 24);
             INSERT INTO account_refs
               (id, platform, kind, label, active, availability, created_at, updated_at)
             VALUES ('operator-secret-ref', 'x', 'profile', 'secret operator label', 1,
                     'available', 25, 26);
             INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id,
                metadata_json, observed_at)
             VALUES
               ('source-active', 'initial_platform_account', 'x',
                'https://x.com/restore', 'public-active', 'operator-secret-ref',
                '{\"public\":\"active\"}', 30),
               ('source-rejected', 'profile_bio', 'instagram',
                'https://instagram.com/rejected', 'public-rejected', NULL,
                '{\"public\":\"rejected\"}', 31),
               ('source-contact', 'profile_website', 'x',
                'https://restore.example/contact', 'public-contact', NULL,
                '{\"public\":\"contact\"}', 32),
               ('source-manual', 'manual_entry', NULL, NULL, NULL, NULL, '{}', 33);
             INSERT INTO platform_accounts
               (id, platform, target_kind, external_id, created_at, updated_at)
             VALUES
               ('account-active', 'x', 'user', 'restore-x-id', 40, 41),
               ('account-rejected', 'instagram', 'creator', 'restore-ig-id', 42, 43);
             INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
             VALUES
               ('link-active', 'creator-restore', 'account-active', 'confirmed', 'exact', 44, 45),
               ('link-rejected', 'creator-restore', 'account-rejected', 'rejected', 'possible', 46, 47);
             INSERT INTO platform_account_observations
               (id, platform_account_id, source_id, handle, display_name, profile_url,
                public_json, observed_at)
             VALUES
               ('observation-active', 'account-active', 'source-active', 'restore_handle',
                'Restore Profile', 'https://x.com/restore_handle',
                '{\"profile_image_url\":\"https://cdn.example/restore.png\",\"phone\":\"+10000000000\",\"accessToken\":\"must-not-export\"}', 50),
               ('observation-rejected', 'account-rejected', 'source-rejected',
                'rejected_handle', 'Rejected Evidence',
                'https://instagram.com/rejected_handle', '{\"evidence\":true}', 51);
             INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES
               ('locator-z-restore', 'email', 'email_address',
                'z-first@restore.example', 'z-first@restore.example', 60, 61),
               ('locator-a-restore', 'email', 'email_address',
                'a-second@restore.example', 'a-second@restore.example', 62, 63);
             INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES
               ('route-z-restore', 'creator-restore', 'locator-z-restore', 'exact', 64, 65),
               ('route-a-restore', 'creator-restore', 'locator-a-restore', 'exact', 66, 67);
             INSERT INTO locator_observations
               (id, locator_id, source_id, creator_id, platform_account_id,
                observed_value, observed_at)
             VALUES ('locator-observation-restore', 'locator-z-restore', 'source-contact',
                     'creator-restore', 'account-active', 'z-first@restore.example', 68);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovered_at)
             VALUES ('origin-restore', 'manual_entry', 'creator-restore', 'source-manual', 65);
             INSERT INTO touchpoints
               (id, creator_id, contact_route_id, kind, occurred_at, metadata_json)
             VALUES ('touch-secret', 'creator-restore', 'route-z-restore', 'email', 999,
                     '{\"communication_secret\":true}');
             INSERT INTO audit_events
               (id, kind, creator_id, payload_json, created_at)
             VALUES ('audit-secret', 'test', 'creator-restore',
                     '{\"audit_secret\":true}', 1000);",
        )
        .unwrap();
    drop(source_connection);

    let source_bundle = source.export_creator_bundle().unwrap();
    let source_workbook = source.export_creator_workbook().unwrap();
    assert_eq!(source_workbook.bundle, source_bundle);
    let source_xlsx = source_workbook.to_xlsx_bytes().unwrap();
    let edited_source_xlsx =
        crate::creator_transfer_xlsx::mutate_readonly_creator_cells_for_test(&source_xlsx);
    let imported_bundle = crate::CreatorBundle::from_xlsx_bytes(&edited_source_xlsx).unwrap();
    assert_eq!(imported_bundle, source_bundle);
    let source_json = String::from_utf8(source_bundle.to_json_bytes().unwrap()).unwrap();
    for excluded in [
        "group-restore",
        "Restore Group",
        "complete notes",
        "complete assessment",
        "origin-restore",
        "source-active",
        "operator-secret-ref",
        "secret operator label",
        "communication_secret",
        "audit_secret",
        "last_touch_at",
        "+10000000000",
        "must-not-export",
    ] {
        assert!(!source_json.contains(excluded), "bundle leaked {excluded}");
    }

    let (_target_directory, target) = repository();
    let operation_id = operation_id(target.prepare_creator_import(imported_bundle).unwrap());
    let (result, changed) = target
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        result,
        ConfirmCreatorImportResult::Imported { .. }
    ));
    assert_eq!(target.export_creator_bundle().unwrap(), source_bundle);

    let source_list = source.list_creators(ListCreatorsInput::default()).unwrap();
    let target_list = target.list_creators(ListCreatorsInput::default()).unwrap();
    let source_list = &source_list.items[0];
    let target_list = &target_list.items[0];
    assert_ne!(target_list.id, source_list.id);
    assert_eq!(
        source_list.avatar_url.as_deref(),
        Some("https://cdn.example/restore.png")
    );
    assert_eq!(target_list.display_name, source_list.display_name);
    assert_eq!(target_list.stage, source_list.stage);
    assert_eq!(target_list.primary_platform, source_list.primary_platform);
    assert_eq!(target_list.primary_handle, source_list.primary_handle);
    assert_eq!(target_list.avatar_url, source_list.avatar_url);
    assert_eq!(target_list.platforms, source_list.platforms);
    assert_eq!(target_list.channels, source_list.channels);
    assert_eq!(target_list.score, source_list.score);
    assert_eq!(target_list.follower_counts, source_list.follower_counts);
    assert_eq!(
        target_list.pinned_at.is_some(),
        source_list.pinned_at.is_some()
    );
    assert_eq!(
        target_list.platform_account_count,
        source_list.platform_account_count
    );
    assert_eq!(
        target_list.contact_route_count,
        source_list.contact_route_count
    );
    assert!(source_list.group.is_some());
    assert!(target_list.group.is_none());

    let target_detail = target.get_creator(&target_list.id).unwrap();
    assert!(target_detail.group.is_none());
    assert!(target_detail.creator.notes.is_none());
    assert!(target_detail.creator.assessment.is_none());
    assert!(target_detail.origins.is_empty());
    assert_ne!(target_detail.creator.created_at, 20);
    assert_ne!(target_detail.creator.updated_at, 21);
    assert!(
        target_detail
            .platform_accounts
            .iter()
            .all(|account| !matches!(account.id.as_str(), "account-active" | "account-rejected"))
    );
    assert!(
        target_detail
            .account_links
            .iter()
            .all(|link| !matches!(link.id.as_str(), "link-active" | "link-rejected"))
    );
    assert!(target_detail.contact_routes.iter().all(|route| {
        !matches!(
            route.route.id.as_str(),
            "route-z-restore" | "route-a-restore"
        ) && !matches!(
            route.locator.id.as_str(),
            "locator-z-restore" | "locator-a-restore"
        )
    }));
    assert!(
        target_detail
            .account_links
            .iter()
            .any(|link| link.state == LinkState::Rejected)
    );

    let target_counts = target
        .lock()
        .unwrap()
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM touchpoints),
               (SELECT COUNT(*) FROM audit_events),
               (SELECT COUNT(*) FROM account_refs),
               (SELECT COUNT(*) FROM creator_groups),
               (SELECT COUNT(*) FROM creator_origins)",
            [],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u64>(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(target_counts, (0, 0, 0, 0, 0));
}

#[test]
fn import_prepare_is_creator_read_only_and_confirm_is_durable_and_idempotent() {
    let (directory, repository) = repository();
    let bundle = bundle(vec![transfer_creator(
        "creator-import-new",
        "Imported Creator",
        CreatorTransferPlatform::X,
        "x-import-new",
        "import_new",
    )]);

    let operation_id = operation_id(repository.prepare_creator_import(bundle.clone()).unwrap());
    assert!(
        repository
            .get_creator(&CreatorId::new("creator-import-new"))
            .is_err(),
        "prepare must not change Creator facts"
    );

    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: operation_id.clone(),
        })
        .unwrap();
    assert!(changed);
    let receipt = match &result {
        ConfirmCreatorImportResult::Imported { receipt, .. } => receipt,
        other => panic!("expected imported result, got {other:?}"),
    };
    assert_eq!(receipt.content_sha256, bundle.content_sha256);
    assert_eq!(receipt.impact.creators_created, 1);
    assert_eq!(receipt.impact.platform_accounts_created, 1);
    assert_eq!(receipt.impact.confirmed_links_created, 1);
    assert_eq!(receipt.impact.profile_observations_created, 1);

    let imported_id = repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items[0]
        .id
        .clone();
    let record = repository.get_creator(&imported_id).unwrap();
    assert_eq!(record.creator.display_name, "Imported Creator");
    assert_eq!(record.creator.score, Some(90));
    assert_eq!(record.relationship.stage, crate::CreatorStage::Engaged);
    assert_eq!(record.platform_accounts.len(), 1);
    assert!(record.origins.is_empty());

    let (replayed, replay_changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(replayed, result);
    assert!(!replay_changed);
    assert!(matches!(
        repository.prepare_creator_import(bundle).unwrap(),
        PrepareCreatorImportResult::AlreadyImported { .. }
    ));

    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let counts = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM creator_import_receipts),
               (SELECT COUNT(*) FROM observation_sources),
               (SELECT COUNT(*) FROM creator_origins WHERE kind = 'creator_import')",
            [],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 0));
}

#[test]
fn import_never_exports_moves_or_clears_creator_groups() {
    let (_directory, repository) = repository();
    let group = repository
        .create_creator_group(CreateCreatorGroupInput {
            name: "Local only".into(),
        })
        .unwrap();

    let new_bundle = bundle(vec![transfer_creator(
        "uncategorized-import",
        "Uncategorized import",
        CreatorTransferPlatform::X,
        "uncategorized-x",
        "uncategorized_handle",
    )]);
    let new_operation_id = operation_id(repository.prepare_creator_import(new_bundle).unwrap());
    repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: new_operation_id,
        })
        .unwrap();
    let uncategorized_id = repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items
        .into_iter()
        .find(|creator| creator.display_name == "Uncategorized import")
        .unwrap()
        .id;
    assert!(
        repository
            .get_creator(&uncategorized_id)
            .unwrap()
            .group
            .is_none()
    );
    assert_eq!(
        repository
            .list_creator_groups(ListCreatorGroupsInput::default())
            .unwrap()
            .items[0]
            .group
            .id,
        group.id
    );

    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Grouped local".into(),
            notes: None,
        })
        .unwrap();
    repository
        .set_creator_group(SetCreatorGroupInput {
            creator_id: existing.creator.id.clone(),
            destination: CreatorGroupDestination::Group {
                group_id: group.id.clone(),
            },
        })
        .unwrap();
    let grouped_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Instagram,
            target_kind: TargetKind::User,
            external_id: "grouped-instagram".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &grouped_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let duplicate = bundle(vec![transfer_creator(
        existing.creator.id.as_str(),
        "Incoming must not move group",
        CreatorTransferPlatform::Instagram,
        "grouped-instagram",
        "grouped_handle",
    )]);
    let operation_id = operation_id(repository.prepare_creator_import(duplicate).unwrap());
    repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(
        repository
            .get_creator(&existing.creator.id)
            .unwrap()
            .group
            .unwrap()
            .id,
        group.id
    );
}

#[test]
fn nonempty_import_preserves_new_creator_page_semantics_with_receiver_local_ids() {
    let (_directory, repository) = repository();
    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Receiver-local Creator".into(),
            notes: None,
        })
        .unwrap();

    let mut incoming = transfer_creator(
        "creator-roundtrip-new",
        "Portable new Creator",
        CreatorTransferPlatform::X,
        "portable-roundtrip-x",
        "portable_roundtrip",
    );
    incoming.contact_routes = vec![
        transfer_email_route(
            "route-z-first",
            "locator-z-first",
            0,
            "z-first@roundtrip.example",
        ),
        transfer_email_route(
            "route-a-second",
            "locator-a-second",
            1,
            "a-second@roundtrip.example",
        ),
    ];
    let incoming_bundle = bundle(vec![incoming.clone()]);
    let canonical_incoming = incoming_bundle.creators[0].clone();
    let operation_id = operation_id(
        repository
            .prepare_creator_import(incoming_bundle.clone())
            .unwrap(),
    );
    repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();

    let imported = repository
        .export_creator_bundle()
        .unwrap()
        .creators
        .into_iter()
        .find(|creator| creator.record_key == canonical_incoming.record_key)
        .unwrap();
    assert_eq!(imported, canonical_incoming);
    assert_eq!(
        bundle(vec![imported]).to_json_bytes().unwrap(),
        incoming_bundle.to_json_bytes().unwrap()
    );
    let imported_id = repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items
        .into_iter()
        .find(|creator| creator.display_name == "Portable new Creator")
        .unwrap()
        .id;
    assert_ne!(imported_id.as_str(), "creator-roundtrip-new");
    assert!(
        repository
            .get_creator(&imported_id)
            .unwrap()
            .group
            .is_none()
    );
}

#[test]
fn manual_only_creator_is_portable_and_imports_as_a_new_local_record() {
    let (_directory, repository) = repository();
    repository
        .create_creator_group(CreateCreatorGroupInput {
            name: "Receiver-only group".into(),
        })
        .unwrap();
    let mut incoming = transfer_creator(
        "unused-source-id",
        "Manual only portable Creator",
        CreatorTransferPlatform::X,
        "unused-account",
        "unused_handle",
    );
    incoming.accounts.clear();
    incoming.contact_routes.clear();
    let operation_id = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming]))
            .unwrap(),
    );
    repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();

    let imported_id = repository
        .list_creators(ListCreatorsInput::default())
        .unwrap()
        .items
        .into_iter()
        .find(|creator| creator.display_name == "Manual only portable Creator")
        .unwrap()
        .id;
    let imported = repository.get_creator(&imported_id).unwrap();
    assert!(imported.platform_accounts.is_empty());
    assert!(imported.contact_routes.is_empty());
    assert!(imported.group.is_none());
}

#[test]
fn import_merges_by_confirmed_account_and_only_fills_missing_facts() {
    let (_directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Local name wins".into(),
            notes: Some("private note survives".into()),
        })
        .unwrap();
    repository
        .update_creator(UpdateCreatorInput {
            tags: None,
            creator_id: existing.creator.id.clone(),
            display_name: None,
            notes: None,
            score: Some(77),
            assessment: Some("private assessment survives".into()),
            follower_counts: Some(BTreeMap::from([(Platform::X, 10)])),
        })
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "shared-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: None,
            external_ref: Some("existing-profile".into()),
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .unwrap();
    let original_observation = repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: account.id,
            source_id: source.id,
            handle: Some("local_handle".into()),
            display_name: None,
            profile_url: None,
            public_data: serde_json::json!({}),
        })
        .unwrap();

    let mut incoming = transfer_creator(
        "different-portable-id",
        "Remote name must not replace",
        CreatorTransferPlatform::X,
        "shared-x-id",
        "remote_handle",
    );
    incoming.score = Some(99);
    incoming.follower_counts = BTreeMap::from([
        (CreatorTransferPlatform::X, 999),
        (CreatorTransferPlatform::Instagram, 22),
    ]);
    let bundle = bundle(vec![incoming]);
    let operation_id = operation_id(repository.prepare_creator_import(bundle).unwrap());
    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(changed);
    let receipt = match result {
        ConfirmCreatorImportResult::Imported { receipt, .. } => receipt,
        other => panic!("expected imported result, got {other:?}"),
    };
    assert_eq!(receipt.impact.creators_created, 0);
    assert_eq!(receipt.impact.creators_merged_by_public_identity, 1);
    assert_eq!(receipt.impact.creator_scores_filled, 0);
    assert_eq!(receipt.impact.follower_counts_filled, 1);
    assert_eq!(receipt.impact.profile_observations_created, 1);

    let record = repository.get_creator(&existing.creator.id).unwrap();
    assert_eq!(record.creator.display_name, "Local name wins");
    assert_eq!(
        record.creator.notes.as_deref(),
        Some("private note survives")
    );
    assert_eq!(record.creator.score, Some(77));
    assert_eq!(
        record.creator.assessment.as_deref(),
        Some("private assessment survives")
    );
    assert_eq!(record.creator.follower_counts.get(&Platform::X), Some(&10));
    assert_eq!(
        record.creator.follower_counts.get(&Platform::Instagram),
        Some(&22)
    );
    assert_eq!(record.account_observations.len(), 2);
    let added_observations = record
        .account_observations
        .iter()
        .filter(|observation| observation.id != original_observation.id)
        .collect::<Vec<_>>();
    assert_eq!(added_observations.len(), 1);
    let latest = added_observations[0];
    assert_eq!(latest.handle.as_deref(), Some("local_handle"));
    assert_eq!(
        latest.display_name.as_deref(),
        Some("Remote name must not replace profile")
    );
    assert_eq!(
        latest.profile_url.as_deref(),
        Some("https://example.com/remote_handle")
    );
}

#[test]
fn nonempty_import_never_revives_rejected_account_evidence() {
    let (_directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Existing rejection".into(),
            notes: None,
        })
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "rejected-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &account.id,
            LinkState::Rejected,
            Confidence::Possible,
        )
        .unwrap();
    let incoming = transfer_creator(
        existing.creator.id.as_str(),
        "Incoming",
        CreatorTransferPlatform::X,
        "rejected-x-id",
        "rejected_handle",
    );
    assert!(matches!(
        repository.prepare_creator_import(bundle(vec![incoming])).unwrap(),
        PrepareCreatorImportResult::Blocked { conflicts, .. }
            if conflicts.len() == 1
                && conflicts[0].kind == CreatorImportConflictKind::RejectedLinkWouldBeRevived
    ));
    let record = repository.get_creator(&existing.creator.id).unwrap();
    assert_eq!(record.account_links[0].state, LinkState::Rejected);
}

#[test]
fn rejected_only_link_creation_is_counted_and_reported_as_imported() {
    let (_directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Rejected evidence target".into(),
            notes: None,
        })
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "rejected-only-x".into(),
        })
        .unwrap();
    let anchor = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::Instagram,
            target_kind: TargetKind::User,
            external_id: "rejected-only-anchor".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &anchor.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let mut incoming = transfer_creator(
        existing.creator.id.as_str(),
        "Rejected evidence target",
        CreatorTransferPlatform::Instagram,
        "rejected-only-anchor",
        "unused_rejected_profile",
    );
    incoming.score = None;
    incoming.follower_counts.clear();
    incoming.accounts[0].latest_profile = None;
    incoming.accounts.push(CreatorTransferAccount {
        capture_order: 1,
        platform: CreatorTransferPlatform::X,
        target_kind: CreatorTransferTargetKind::User,
        external_id: "rejected-only-x".into(),
        link_state: CreatorTransferLinkState::Rejected,
        confidence: CreatorTransferConfidence::Possible,
        source_kind: SourceKind::ProfileBio,
        latest_profile: None,
    });

    let prepared = repository
        .prepare_creator_import(bundle(vec![incoming]))
        .unwrap();
    let (operation_id, impact) = match prepared {
        PrepareCreatorImportResult::ConfirmationRequired {
            operation_id,
            impact,
            ..
        } => (operation_id, impact),
        other => panic!("expected confirmation-required import, got {other:?}"),
    };
    assert_eq!(impact.rejected_links_created, 1);
    assert!(impact.changes_creator_graph());

    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(changed);
    assert!(matches!(
        result,
        ConfirmCreatorImportResult::Imported { ref receipt, .. }
            if receipt.impact.rejected_links_created == 1
    ));
    assert!(
        repository
            .get_creator(&existing.creator.id)
            .unwrap()
            .account_links
            .iter()
            .any(|link| link.platform_account_id == account.id
                && link.state == LinkState::Rejected)
    );
}

#[test]
fn nonempty_import_adds_public_contact_routes_and_blocks_cross_creator_ownership() {
    let (_directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Contact target".into(),
            notes: None,
        })
        .unwrap();
    let identity_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "contact-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &identity_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let mut incoming = transfer_creator(
        existing.creator.id.as_str(),
        "Incoming contact",
        CreatorTransferPlatform::X,
        "contact-x-id",
        "contact_handle",
    );
    incoming.contact_routes.push(CreatorTransferContactRoute {
        save_order: 0,
        channel: ContactChannel::Email,
        locator_kind: LocatorKind::EmailAddress,
        normalized_value: "contact@example.com".into(),
        display_value: "contact@example.com".into(),
        confidence: CreatorTransferConfidence::Exact,
        source_kinds: vec![SourceKind::ManualEntry],
    });
    let operation_id = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming]))
            .unwrap(),
    );
    repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(
        repository
            .get_creator(&existing.creator.id)
            .unwrap()
            .contact_routes
            .len(),
        1
    );

    let mut foreign = transfer_creator(
        "foreign-contact",
        "Foreign contact",
        CreatorTransferPlatform::Instagram,
        "foreign-contact-id",
        "foreign_contact",
    );
    foreign.contact_routes.push(CreatorTransferContactRoute {
        save_order: 0,
        channel: ContactChannel::Email,
        locator_kind: LocatorKind::EmailAddress,
        normalized_value: "contact@example.com".into(),
        display_value: "contact@example.com".into(),
        confidence: CreatorTransferConfidence::Exact,
        source_kinds: vec![SourceKind::ManualEntry],
    });
    assert!(matches!(
        repository.prepare_creator_import(bundle(vec![foreign])).unwrap(),
        PrepareCreatorImportResult::Blocked { conflicts, .. }
            if conflicts.iter().any(|conflict|
                conflict.kind == CreatorImportConflictKind::PossibleContactMatch)
    ));
}

#[test]
fn import_blocks_same_platform_handle_match_but_not_cross_platform_namesakes() {
    let (directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Existing".into(),
            notes: None,
        })
        .unwrap();
    let account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "existing-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let source = repository
        .create_observation_source(ObservationSourceInput {
            kind: SourceKind::InitialPlatformAccount,
            platform: Some(Platform::X),
            source_url: None,
            external_ref: Some("handle-source".into()),
            account_ref_id: None,
            metadata: serde_json::json!({}),
        })
        .unwrap();
    repository
        .append_platform_account_observation(PlatformAccountObservationInput {
            platform_account_id: account.id,
            source_id: source.id,
            handle: Some("same_name".into()),
            display_name: None,
            profile_url: None,
            public_data: serde_json::json!({}),
        })
        .unwrap();

    let blocked = repository
        .prepare_creator_import(bundle(vec![transfer_creator(
            "incoming-x",
            "Incoming X",
            CreatorTransferPlatform::X,
            "new-x-id",
            "same_name",
        )]))
        .unwrap();
    assert!(matches!(
        blocked,
        PrepareCreatorImportResult::Blocked { conflicts, .. }
            if conflicts[0].kind == CreatorImportConflictKind::PossibleHandleMatch
    ));
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let operations: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM creator_import_operations",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        operations, 0,
        "blocked prepare must not persist an operation"
    );
    drop(connection);

    let cross_platform = repository
        .prepare_creator_import(bundle(vec![transfer_creator(
            "incoming-instagram",
            "Incoming Instagram",
            CreatorTransferPlatform::Instagram,
            "new-ig-id",
            "same_name",
        )]))
        .unwrap();
    assert!(matches!(
        cross_platform,
        PrepareCreatorImportResult::ConfirmationRequired { .. }
    ));
}

#[test]
fn import_confirm_persists_stale_when_the_previewed_plan_changes() {
    let (_directory, repository) = repository();
    let existing = repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Plan target".into(),
            notes: None,
        })
        .unwrap();
    let identity_account = repository
        .upsert_platform_account(UpsertPlatformAccountInput {
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "stale-x-id".into(),
        })
        .unwrap();
    repository
        .link_creator_account(
            &existing.creator.id,
            &identity_account.id,
            LinkState::Confirmed,
            Confidence::Exact,
        )
        .unwrap();
    let mut incoming = transfer_creator(
        existing.creator.id.as_str(),
        "Remote",
        CreatorTransferPlatform::X,
        "stale-x-id",
        "stale_handle",
    );
    incoming.score = Some(60);
    let operation_id = operation_id(
        repository
            .prepare_creator_import(bundle(vec![incoming]))
            .unwrap(),
    );

    repository
        .update_creator(UpdateCreatorInput {
            tags: None,
            creator_id: existing.creator.id,
            display_name: None,
            notes: None,
            score: Some(50),
            assessment: None,
            follower_counts: None,
        })
        .unwrap();
    let (result, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: operation_id.clone(),
        })
        .unwrap();
    assert!(!changed);
    assert!(matches!(result, ConfirmCreatorImportResult::Stale { .. }));
    let (replayed, replay_changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(replayed, result);
    assert!(!replay_changed);
}

#[test]
fn confirm_writer_recheck_turns_post_plan_basis_drift_into_stale() {
    let (directory, repository) = repository();
    let incoming = rich_bundle(1);
    let operation_id = operation_id(
        repository
            .prepare_creator_import(incoming)
            .expect("prepare import"),
    );
    let input = ConfirmCreatorImportInput {
        operation_id: operation_id.clone(),
    };
    let mut operation = super::super::creator_import::load_operation(&repository, &input).unwrap();
    let artifact = Arc::new(operation.take_artifact().unwrap());
    let basis = super::super::creator_import::load_basis(&repository, &artifact).unwrap();
    let work = crate::creator_import::build_confirm_work(operation, artifact, basis).unwrap();

    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Racing writer".into(),
            notes: None,
        })
        .unwrap();
    let (result, changed, publish_change) =
        super::super::creator_import::write_confirm(&repository, work)
            .unwrap()
            .decode()
            .unwrap();
    assert!(matches!(result, ConfirmCreatorImportResult::Stale { .. }));
    assert!(!changed);
    assert!(!publish_change);
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM creator_import_receipts", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT status FROM creator_import_operations WHERE id = ?1",
                [operation_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "completed"
    );
}

#[test]
fn import_cancel_clears_the_staged_bundle_and_is_terminally_replayable() {
    let (directory, repository) = repository();
    let bundle = bundle(vec![transfer_creator(
        "cancelled-import",
        "Cancelled",
        CreatorTransferPlatform::X,
        "cancelled-x-id",
        "cancelled_handle",
    )]);
    let operation_id = operation_id(repository.prepare_creator_import(bundle.clone()).unwrap());

    let cancelled = repository
        .cancel_creator_import(ConfirmCreatorImportInput {
            operation_id: operation_id.clone(),
        })
        .unwrap();
    assert_eq!(
        cancelled,
        ConfirmCreatorImportResult::Cancelled {
            operation_id: operation_id.clone(),
        }
    );
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let operation = connection
        .query_row(
            "SELECT status, bundle_json IS NULL FROM creator_import_operations WHERE id = ?1",
            [operation_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .unwrap();
    assert_eq!(operation, ("completed".into(), true));
    drop(connection);

    let (replayed, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(replayed, cancelled);
    assert!(!changed);
    assert!(matches!(
        repository.prepare_creator_import(bundle).unwrap(),
        PrepareCreatorImportResult::ConfirmationRequired { .. }
    ));
}

#[test]
fn a_later_prepare_expires_and_clears_abandoned_operations() {
    let (directory, repository) = repository();
    let abandoned = bundle(vec![transfer_creator(
        "abandoned-import",
        "Abandoned",
        CreatorTransferPlatform::X,
        "abandoned-x-id",
        "abandoned_handle",
    )]);
    let operation_id = operation_id(repository.prepare_creator_import(abandoned).unwrap());
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute(
            "UPDATE creator_import_operations SET expires_at = 0 WHERE id = ?1",
            [operation_id.as_str()],
        )
        .unwrap();
    drop(connection);

    let next = bundle(vec![transfer_creator(
        "next-import",
        "Next",
        CreatorTransferPlatform::Instagram,
        "next-ig-id",
        "next_handle",
    )]);
    assert!(matches!(
        repository.prepare_creator_import(next).unwrap(),
        PrepareCreatorImportResult::ConfirmationRequired { .. }
    ));

    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let cleared: bool = connection
        .query_row(
            "SELECT status = 'completed' AND bundle_json IS NULL
             FROM creator_import_operations WHERE id = ?1",
            [operation_id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(cleared);
    drop(connection);
    let (expired, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert!(matches!(
        expired,
        ConfirmCreatorImportResult::Expired { .. }
    ));
    assert!(!changed);
}

#[test]
fn rich_import_planner_queries_and_prepared_statement_families_are_bounded() {
    let _trace_guard = IMPORT_TRACE_TEST_LOCK.lock().unwrap();
    let (five_hundred, five_hundred_preparations) = traced_rich_import(500);
    let (fifteen_hundred, fifteen_hundred_preparations) = traced_rich_import(1_500);
    let normalize_sql = |sql: &str| sql.split_whitespace().collect::<Vec<_>>().join(" ");
    let basis_sql = [
        super::super::creator_import::ACCOUNT_BASIS_SQL,
        super::super::creator_import::HANDLE_BASIS_SQL,
        super::super::creator_import::CONTACT_BASIS_SQL,
    ]
    .map(normalize_sql);
    let planner_queries = |statements: &[String]| {
        statements
            .iter()
            .filter(|sql| basis_sql.contains(&normalize_sql(sql)))
            .count()
    };
    assert_eq!(planner_queries(&five_hundred), 12);
    assert_eq!(planner_queries(&fifteen_hundred), 12);
    let statement_families = |statements: &[String]| {
        statements
            .iter()
            .map(|sql| sql.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<BTreeSet<_>>()
    };
    let five_hundred_families = statement_families(&five_hundred);
    let fifteen_hundred_families = statement_families(&fifteen_hundred);
    eprintln!(
        "creator_import_sql_after count=500 executions={} planner_queries={} statement_families={} prepare_authorizations={}",
        five_hundred.len(),
        planner_queries(&five_hundred),
        five_hundred_families.len(),
        five_hundred_preparations
    );
    eprintln!(
        "creator_import_sql_after count=1500 executions={} planner_queries={} statement_families={} prepare_authorizations={}",
        fifteen_hundred.len(),
        planner_queries(&fifteen_hundred),
        fifteen_hundred_families.len(),
        fifteen_hundred_preparations
    );
    assert_eq!(five_hundred_families, fifteen_hundred_families);
    assert_eq!(five_hundred_preparations, fifteen_hundred_preparations);
    assert!(five_hundred_preparations <= 700);
    assert!(
        five_hundred_families.len() <= 35,
        "import prepared too many SQL families: {five_hundred_families:#?}"
    );
    // The old 5N+7 prepare and 14N+8 confirm formulas are replaced by fixed
    // snapshot/recheck and table-oriented json_each statements.
    assert_eq!(five_hundred.len(), fifteen_hundred.len());
    assert!(five_hundred.len() <= 50);
}

#[test]
fn basis_queries_have_bounded_explicit_plans_without_automatic_indexes() {
    let (directory, repository) = repository();
    let existing = rich_bundle(30);
    let operation_id = operation_id(repository.prepare_creator_import(existing).unwrap());
    repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    drop(repository);
    let artifact = crate::creator_import::validated_artifact_from_bundle(rich_bundle(10)).unwrap();
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute_batch("PRAGMA automatic_index=OFF; ANALYZE;")
        .unwrap();
    let account = explain_basis_query(
        &connection,
        super::super::creator_import::ACCOUNT_BASIS_SQL,
        &artifact.account_lookup_json,
    );
    let handle = explain_basis_query(
        &connection,
        super::super::creator_import::HANDLE_BASIS_SQL,
        &artifact.handle_lookup_json,
    );
    let contact = explain_basis_query(
        &connection,
        super::super::creator_import::CONTACT_BASIS_SQL,
        &artifact.contact_lookup_json,
    );
    eprintln!("account basis EQP: {account:#?}");
    eprintln!("handle basis EQP: {handle:#?}");
    eprintln!("contact basis EQP: {contact:#?}");
    for (name, plan) in [
        ("account", &account),
        ("handle", &handle),
        ("contact", &contact),
    ] {
        assert!(
            plan.iter().all(|detail| !detail.contains("AUTOMATIC")),
            "{name} basis relied on an automatic index: {plan:#?}"
        );
        assert!(
            plan.iter().any(|detail| detail.contains("SCAN json_each")),
            "{name} basis did not bind the materialized lookup rowset: {plan:#?}"
        );
    }
    assert!(account.iter().any(|detail| {
        detail.contains("platform_account_observations_latest")
            && detail.contains("platform_account_id")
    }));
    assert!(
        account
            .iter()
            .filter(|detail| {
                detail.contains("SCAN link") || detail.contains("SCAN creator_account_links")
            })
            .count()
            <= 2,
        "account links must be scanned a fixed two times, never per lookup key"
    );
    assert!(account.iter().any(|detail| {
        detail.contains("SEARCH matched USING INDEX sqlite_autoindex_platform_accounts_2")
    }));
    assert!(handle.iter().any(|detail| {
        detail.contains("SCAN link") || detail.contains("SCAN creator_account_links")
    }));
    assert!(handle.iter().any(|detail| {
        detail.contains("platform_account_observations_latest")
            && detail.contains("platform_account_id")
    }));
    assert!(
        handle
            .iter()
            .filter(|detail| {
                detail.contains("SCAN link") || detail.contains("SCAN creator_account_links")
            })
            .count()
            <= 1,
        "handle candidates must scan active links once, not history per input handle"
    );
    assert!(contact.iter().any(|detail| {
        detail.contains("creator_contact_routes_one_per_locator")
            && detail.contains("locator_id=?)")
    }));
    assert!(contact.iter().any(|detail| {
        detail.contains("SEARCH matched USING INDEX sqlite_autoindex_contact_locators_2")
    }));
}

#[test]
fn rich_import_reads_back_all_identity_facts_and_replays_without_new_facts() {
    let (_directory, repository) = repository();
    let bundle = rich_bundle(20);
    let operation_id = operation_id(repository.prepare_creator_import(bundle.clone()).unwrap());
    let (first, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: operation_id.clone(),
        })
        .unwrap();
    assert!(changed);
    let readback = repository.export_creator_bundle().unwrap();
    assert_eq!(readback.creator_count, bundle.creator_count);
    let account_keys = |value: &crate::CreatorBundle| {
        value
            .creators
            .iter()
            .flat_map(|creator| &creator.accounts)
            .map(|account| {
                (
                    account.platform,
                    account.target_kind,
                    account.external_id.clone(),
                    account.link_state,
                )
            })
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(account_keys(&readback), account_keys(&bundle));
    let (replay, replay_changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(replay, first);
    assert!(!replay_changed);
}

#[test]
fn final_import_writer_keeps_wal_reader_on_last_commit_and_bounds_the_next_writer() {
    let _trace_guard = IMPORT_TRACE_TEST_LOCK.lock().unwrap();
    let (_directory, repository) = repository();
    let operation_id = operation_id(repository.prepare_creator_import(rich_bundle(1)).unwrap());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *IMPORT_WRITER_PAUSE.lock().unwrap() = Some((entered_tx, release_rx));
    arm_import_trace(&repository, pause_import_writer);

    let confirmer = repository.clone();
    let confirm = std::thread::spawn(move || {
        confirmer.confirm_creator_import(ConfirmCreatorImportInput { operation_id })
    });
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("confirm never reached its apply transaction");
    let (reader_tx, reader_rx) = mpsc::channel();
    let readers = (0..2)
        .map(|_| {
            let reader = repository.clone();
            let reader_tx = reader_tx.clone();
            std::thread::spawn(move || {
                reader_tx
                    .send(reader.list_creators(ListCreatorsInput::default()))
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for _ in 0..2 {
        assert!(
            reader_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("WAL read lane waited for the import writer")
                .unwrap()
                .items
                .is_empty(),
            "WAL reader observed uncommitted import rows"
        );
    }
    for reader in readers {
        reader.join().unwrap();
    }

    let waiting_repository = repository.clone();
    let (writer_tx, writer_rx) = mpsc::channel();
    let writer = std::thread::spawn(move || {
        writer_tx
            .send(waiting_repository.create_creator(CreateCreatorInput {
                tags: None,
                display_name: "Queued behind import".into(),
                notes: None,
            }))
            .unwrap();
    });
    assert!(writer_rx.recv_timeout(Duration::from_millis(100)).is_err());
    release_tx.send(()).unwrap();
    confirm.join().unwrap().unwrap();
    writer_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("next writer stayed blocked after import commit")
        .unwrap();
    writer.join().unwrap();
    clear_import_trace(&repository);
}

#[test]
fn every_import_mutation_family_rolls_back_and_leaves_the_operation_recoverable() {
    for table in [
        "observation_sources",
        "creators",
        "creator_relationships",
        "platform_accounts",
        "creator_account_links",
        "platform_account_observations",
        "contact_locators",
        "creator_contact_routes",
        "locator_observations",
        "creator_import_receipts",
    ] {
        let (directory, repository) = repository();
        let operation_id = operation_id(repository.prepare_creator_import(rich_bundle(1)).unwrap());
        let input = ConfirmCreatorImportInput {
            operation_id: operation_id.clone(),
        };
        let mut operation =
            super::super::creator_import::load_operation(&repository, &input).unwrap();
        let artifact = Arc::new(operation.take_artifact().unwrap());
        let basis = super::super::creator_import::load_basis(&repository, &artifact).unwrap();
        let mut work =
            crate::creator_import::build_confirm_work(operation, artifact, basis).unwrap();
        work.install_mutation_failpoint(table);
        assert!(
            super::super::creator_import::write_confirm(&repository, work).is_err(),
            "{table} failpoint did not stop confirm"
        );
        let connection = Connection::open(directory.path().join("crm.db")).unwrap();
        let deltas = connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM creators),
                   (SELECT COUNT(*) FROM creator_import_receipts),
                   (SELECT COUNT(*) FROM audit_events),
                   (SELECT COUNT(*) FROM creator_origins),
                   (SELECT status FROM creator_import_operations WHERE id = ?1)",
                [operation_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, u64>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, u64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(deltas, (0, 0, 0, 0, "prepared".into()), "{table}");
        drop(connection);
        assert!(
            repository
                .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
                .is_ok(),
            "{table} operation was not recoverable"
        );
    }
}

#[test]
fn cancel_confirm_race_has_one_terminal_winner() {
    let (_directory, repository) = repository();
    let operation_id = operation_id(repository.prepare_creator_import(rich_bundle(1)).unwrap());
    let gate = std::sync::Arc::new(Barrier::new(3));
    let confirm_repository = repository.clone();
    let confirm_id = operation_id.clone();
    let confirm_gate = gate.clone();
    let confirm = std::thread::spawn(move || {
        confirm_gate.wait();
        confirm_repository.confirm_creator_import(ConfirmCreatorImportInput {
            operation_id: confirm_id,
        })
    });
    let cancel_repository = repository.clone();
    let cancel_id = operation_id.clone();
    let cancel_gate = gate.clone();
    let cancel = std::thread::spawn(move || {
        cancel_gate.wait();
        cancel_repository.cancel_creator_import(ConfirmCreatorImportInput {
            operation_id: cancel_id,
        })
    });
    gate.wait();
    let (confirm_result, _) = confirm.join().unwrap().unwrap();
    let cancel_result = cancel.join().unwrap().unwrap();
    assert_eq!(confirm_result, cancel_result);
    let (replay, changed) = repository
        .confirm_creator_import(ConfirmCreatorImportInput { operation_id })
        .unwrap();
    assert_eq!(replay, confirm_result);
    assert!(!changed);
}

#[test]
#[ignore = "release-style diagnostic; run explicitly with --release --ignored --nocapture"]
fn creator_import_release_diagnostic_500_1500() {
    eprintln!(
        "creator_import_before source=recorded_parent_fixture count=500 parse_ms=6.95 json_encode_ms=3.47 prepare_ms=22.49 confirm_ms=43.22 total_ms=76.13 sql_executions=9515 writer_hold_proxy_ms=43.22"
    );
    eprintln!(
        "creator_import_before source=recorded_parent_fixture count=1500 parse_ms=17.36 json_encode_ms=8.76 prepare_ms=68.25 confirm_ms=157.49 total_ms=251.86 sql_executions=28515 writer_hold_proxy_ms=157.49"
    );
    for count in [500, 1_500] {
        let mut writer_holds = Vec::new();
        let mut totals = Vec::new();
        for sample in 1..=3 {
            let (_directory, repository) = repository();
            repository
                .create_creator(CreateCreatorInput {
                    tags: None,
                    display_name: "Existing local Creator".into(),
                    notes: None,
                })
                .unwrap();
            let xlsx = rich_bundle(count).to_xlsx_bytes().unwrap();
            let prepare_started = Instant::now();
            let phase = Instant::now();
            let artifact =
                Arc::new(crate::creator_import::validated_artifact_from_xlsx(xlsx).unwrap());
            let parse = phase.elapsed();
            let phase = Instant::now();
            let basis = super::super::creator_import::load_basis(&repository, &artifact).unwrap();
            let prepare_basis = phase.elapsed();
            let phase = Instant::now();
            let work = crate::creator_import::build_prepare_work(artifact, basis).unwrap();
            let prepare_plan = phase.elapsed();
            let phase = Instant::now();
            let prepared = super::super::creator_import::write_prepare(&repository, work).unwrap();
            let prepare_writer = phase.elapsed();
            let operation_id =
                operation_id(crate::creator_import::decode_prepare_outcome(prepared).unwrap());
            let prepare = prepare_started.elapsed();

            let confirm_started = Instant::now();
            let phase = Instant::now();
            let input = ConfirmCreatorImportInput {
                operation_id: operation_id.clone(),
            };
            let mut operation =
                super::super::creator_import::load_operation(&repository, &input).unwrap();
            let confirm_operation = phase.elapsed();
            let phase = Instant::now();
            let artifact = Arc::new(operation.take_artifact().unwrap());
            let staged_parse = phase.elapsed();
            let phase = Instant::now();
            let basis = super::super::creator_import::load_basis(&repository, &artifact).unwrap();
            let confirm_basis = phase.elapsed();
            let phase = Instant::now();
            let mut work =
                crate::creator_import::build_confirm_work(operation, artifact, basis).unwrap();
            let writer_timing = work.install_timing_sink();
            let confirm_plan = phase.elapsed();
            let phase = Instant::now();
            super::super::creator_import::write_confirm(&repository, work)
                .unwrap()
                .decode()
                .unwrap();
            let confirm_writer = phase.elapsed();
            let writer_timing = writer_timing
                .lock()
                .unwrap()
                .expect("confirm writer timing was not recorded");
            let confirm = confirm_started.elapsed();
            let total = prepare + confirm;
            writer_holds.push(writer_timing.writer_hold);
            totals.push(total);
            eprintln!(
                "creator_import_after count={count} sample={sample} parse_ms={} prepare_basis_ms={} prepare_plan_ms={} prepare_writer_ms={} prepare_ms={} confirm_operation_ms={} staged_parse_ms={} confirm_basis_ms={} confirm_plan_ms={} writer_wait_ms={} transaction_begin_ms={} apply_ms={} commit_ms={} writer_hold_ms={} confirm_writer_ms={} confirm_ms={} total_ms={}",
                parse.as_secs_f64() * 1_000.0,
                prepare_basis.as_secs_f64() * 1_000.0,
                prepare_plan.as_secs_f64() * 1_000.0,
                prepare_writer.as_secs_f64() * 1_000.0,
                prepare.as_secs_f64() * 1_000.0,
                confirm_operation.as_secs_f64() * 1_000.0,
                staged_parse.as_secs_f64() * 1_000.0,
                confirm_basis.as_secs_f64() * 1_000.0,
                confirm_plan.as_secs_f64() * 1_000.0,
                writer_timing.writer_wait.as_secs_f64() * 1_000.0,
                writer_timing.transaction_begin.as_secs_f64() * 1_000.0,
                writer_timing.apply.as_secs_f64() * 1_000.0,
                writer_timing.commit.as_secs_f64() * 1_000.0,
                writer_timing.writer_hold.as_secs_f64() * 1_000.0,
                confirm_writer.as_secs_f64() * 1_000.0,
                confirm.as_secs_f64() * 1_000.0,
                total.as_secs_f64() * 1_000.0
            );
        }
        writer_holds.sort_unstable();
        totals.sort_unstable();
        eprintln!(
            "creator_import_after count={count} writer_hold_median_ms={} total_median_ms={}",
            writer_holds[1].as_secs_f64() * 1_000.0,
            totals[1].as_secs_f64() * 1_000.0
        );
    }
}

#[test]
fn legacy_v1_prepared_confirmation_becomes_stale_without_importing() {
    let (directory, repository) = repository();
    let legacy_bytes = include_bytes!("../../../../tests/fixtures/creator_bundle.json");
    let legacy = crate::CreatorBundle::from_staged_import_json_bytes(legacy_bytes).unwrap();
    let current = crate::CreatorBundle::from_import_json_bytes(legacy_bytes).unwrap();
    let operation_id = operation_id(repository.prepare_creator_import(current).unwrap());
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute(
            "UPDATE creator_import_operations SET content_sha256=?1,bundle_json=?2 WHERE id=?3",
            rusqlite::params![
                legacy.content_sha256,
                std::str::from_utf8(legacy_bytes).unwrap(),
                operation_id.as_str()
            ],
        )
        .unwrap();
    let input = ConfirmCreatorImportInput { operation_id };
    let (result, changed) = repository.confirm_creator_import(input.clone()).unwrap();
    assert!(matches!(result, ConfirmCreatorImportResult::Stale { .. }));
    assert!(!changed);
    assert_eq!(
        repository.confirm_creator_import(input).unwrap(),
        (result, false)
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM creator_import_receipts", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM creators", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
