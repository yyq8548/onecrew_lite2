use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    ContactChannel, ContactLocatorInput, CreatorStage, LocatorKind, ManualCreatorCreateInput,
    repository::{ContactRepository, CreatorRepository},
};

use super::repository;

static BATCH_TRANSACTION_STATEMENTS: AtomicUsize = AtomicUsize::new(0);
static OWNER_LOOKUP_STATEMENTS: AtomicUsize = AtomicUsize::new(0);

fn count_batch_transaction_statements(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(statement, _) = event else {
        return;
    };
    let sql = statement.sql().trim().to_ascii_uppercase();
    if sql.starts_with("BEGIN IMMEDIATE") || sql == "COMMIT" {
        BATCH_TRANSACTION_STATEMENTS.fetch_add(1, Ordering::Relaxed);
    }
}

fn count_owner_lookup_statements(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(statement, _) = event else {
        return;
    };
    let sql = statement
        .expanded_sql()
        .unwrap_or_else(|| statement.sql().to_string())
        .to_ascii_uppercase();
    if sql.contains("FROM CONTACT_LOCATORS LOCATOR") && sql.contains("BATCH_ROUTE_") {
        OWNER_LOOKUP_STATEMENTS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn manual_creator_batch_repository_rolls_back_when_a_later_item_fails() {
    let (_directory, repository) = repository();
    let error = repository
        .create_manual_creator_batch(vec![
            ManualCreatorCreateInput {
                tags: None,
                display_name: "committed only if the batch commits".into(),
                notes: None,
                contact_routes: Vec::new(),
            },
            ManualCreatorCreateInput {
                tags: None,
                display_name: "  ".into(),
                notes: None,
                contact_routes: Vec::new(),
            },
        ])
        .expect_err("a later item failure must roll back the earlier insert");
    assert_eq!(error, "creator display_name is required");

    let connection = repository.lock().expect("repository lock");
    let creators: u64 = connection
        .query_row("SELECT COUNT(*) FROM creators", [], |row| row.get(0))
        .expect("creator count");
    let sources: u64 = connection
        .query_row("SELECT COUNT(*) FROM observation_sources", [], |row| {
            row.get(0)
        })
        .expect("source count");
    assert_eq!(creators, 0);
    assert_eq!(sources, 0);
}

#[test]
fn set_based_route_owner_lookup_handles_1000_unique_creator_routes() {
    let (_directory, repository) = repository();
    let inputs = (0..1000)
        .map(|index| ManualCreatorCreateInput {
            tags: None,
            display_name: format!("Batch Creator {index}"),
            notes: None,
            contact_routes: vec![crate::ManualCreatorInitialContactRouteInput {
                channel: ContactChannel::X,
                value: format!("batch_route_{index}"),
                source_url: None,
            }],
        })
        .collect::<Vec<_>>();
    BATCH_TRANSACTION_STATEMENTS.store(0, Ordering::Relaxed);
    repository.lock().expect("repository lock").trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(count_batch_transaction_statements),
    );
    let created = repository
        .create_manual_creator_batch(inputs)
        .expect("1000 unique Creator routes should commit as one batch");
    repository
        .lock()
        .expect("repository lock")
        .trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    assert_eq!(
        BATCH_TRANSACTION_STATEMENTS.load(Ordering::Relaxed),
        2,
        "the repository batch must emit exactly one BEGIN IMMEDIATE and one COMMIT"
    );
    assert_eq!(created.len(), 1000);
    assert!(created.iter().all(|creator| {
        creator.stage == CreatorStage::Prospect && creator.contact_routes.len() == 1
    }));

    let locators = (0..1000)
        .map(|index| ContactLocatorInput {
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: format!("batch_route_{index}"),
            display_value: format!("batch_route_{index}"),
        })
        .collect::<Vec<_>>();
    OWNER_LOOKUP_STATEMENTS.store(0, Ordering::Relaxed);
    let first_read = repository.read_connection().expect("first read connection");
    let second_read = repository
        .read_connection()
        .expect("second read connection");
    for connection in [&*first_read, &*second_read] {
        connection.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_owner_lookup_statements),
        );
    }
    drop((first_read, second_read));
    let owners = repository
        .find_contact_route_owner_keys(&locators)
        .expect("set-based owner lookup");
    let first_read = repository.read_connection().expect("first read connection");
    let second_read = repository
        .read_connection()
        .expect("second read connection");
    for connection in [&*first_read, &*second_read] {
        connection.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    }
    drop((first_read, second_read));
    assert_eq!(
        OWNER_LOOKUP_STATEMENTS.load(Ordering::Relaxed),
        4,
        "1000 locators should use four 333-row parameter-safe lookup statements"
    );
    assert_eq!(owners.len(), 1000);
    for locator in locators {
        assert!(owners.contains(&crate::contact_routes::locator_key(&locator)));
    }
}

#[test]
fn route_owner_lookup_plan_uses_persistent_unique_indexes_without_scans() {
    let (_directory, repository) = repository();
    let connection = repository.lock().expect("repository lock");
    connection
        .execute_batch("PRAGMA automatic_index = OFF;")
        .expect("disable automatic indexes");
    let sql = super::super::contacts::contact_route_owner_lookup_sql(1);
    let plan = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("prepare owner lookup plan")
        .query_map(rusqlite::params!["x", "handle", "known"], |row| {
            row.get::<_, String>(3)
        })
        .expect("query owner lookup plan")
        .collect::<Result<Vec<_>, _>>()
        .expect("owner lookup plan rows");
    assert!(
        plan.iter()
            .any(|detail| detail.contains("contact_locators") && detail.contains("INDEX")),
        "locator lookup did not use a persistent index: {plan:#?}"
    );
    assert!(
        plan.iter()
            .any(|detail| detail.contains("creator_contact_routes_one_per_locator")),
        "route lookup did not use the unique locator index: {plan:#?}"
    );
    assert!(
        plan.iter()
            .all(|detail| !detail.contains("SCAN contact_locators"))
            && plan
                .iter()
                .all(|detail| !detail.contains("SCAN creator_contact_routes")),
        "owner lookup scanned a route table: {plan:#?}"
    );
}
