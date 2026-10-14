use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use crate::{
    ListCreatorsInput, ManualCreatorCreateInput,
    repository::CreatorRepository,
    repository_dispatch::{CreatorRepositoryDispatchFailure, RepositoryDispatch},
};

use super::repository;

#[test]
fn two_query_only_readers_are_independent_and_bounded() {
    let (_directory, repository) = repository();
    assert_eq!(repository.read_pool().state().connections, 2);
    let first_repository = repository.clone();
    let second_repository = repository.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_first_tx, release_first_rx) = mpsc::channel();
    let (release_second_tx, release_second_rx) = mpsc::channel();
    let first_tx = entered_tx.clone();
    let first = std::thread::spawn(move || {
        let connection = first_repository.read_connection().unwrap();
        connection
            .query_row("SELECT COUNT(*) FROM creators", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap();
        first_tx.send(()).unwrap();
        release_first_rx.recv().unwrap();
    });
    let second = std::thread::spawn(move || {
        let connection = second_repository.read_connection().unwrap();
        connection
            .query_row("SELECT COUNT(*) FROM creators", [], |row| {
                row.get::<_, u64>(0)
            })
            .unwrap();
        entered_tx.send(()).unwrap();
        release_second_rx.recv().unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(repository.read_pool().state().idle_connections, 0);
    assert!(repository.read_connection().is_err());
    release_first_tx.send(()).unwrap();
    release_second_tx.send(()).unwrap();
    first.join().unwrap();
    second.join().unwrap();

    let reader = repository.read_connection().unwrap();
    assert!(
        reader
            .execute(
                "INSERT INTO creators(id, display_name, created_at, updated_at,
                 follower_counts_json) VALUES ('illegal-reader-write', 'no', 1, 1, '{}')",
                [],
            )
            .is_err(),
        "query-only read connection accepted a write"
    );
}

#[test]
fn list_creators_reads_the_committed_wal_snapshot_without_waiting_for_writer_mutex() {
    let (_directory, repository) = repository();
    repository
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Committed Creator".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .unwrap();
    let mut writer = repository.lock().unwrap();
    let transaction = writer
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    transaction
        .execute_batch(
            "INSERT INTO creators
               (id, display_name, notes, created_at, updated_at, archived_at,
                score, assessment, follower_counts_json, pinned_at, group_id)
             VALUES ('uncommitted-creator', 'Uncommitted Creator', NULL, 2, 2,
                     NULL, NULL, NULL, '{}', NULL, NULL);
             INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES ('uncommitted-creator', 'prospect', NULL, 2, 2);",
        )
        .unwrap();

    let reader = repository.clone();
    let (result_tx, result_rx) = mpsc::channel();
    let read = std::thread::spawn(move || {
        result_tx
            .send(reader.list_creators(ListCreatorsInput::default()))
            .unwrap();
    });
    let page = result_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].display_name, "Committed Creator");

    let waiting_writer = repository.clone();
    let (writer_tx, writer_rx) = mpsc::channel();
    let second_write = std::thread::spawn(move || {
        writer_tx
            .send(
                waiting_writer.create_manual_creator(ManualCreatorCreateInput {
                    tags: None,
                    display_name: "Serialized Writer".into(),
                    notes: None,
                    contact_routes: Vec::new(),
                }),
            )
            .unwrap();
    });
    assert!(writer_rx.recv_timeout(Duration::from_millis(100)).is_err());
    transaction.commit().unwrap();
    drop(writer);
    read.join().unwrap();
    writer_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .unwrap();
    second_write.join().unwrap();

    assert_eq!(
        repository
            .list_creators(ListCreatorsInput::default())
            .unwrap()
            .items
            .len(),
        3
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn admission_precedes_blocking_dispatch_and_keeps_workers_responsive() {
    let (_directory, repository) = repository();
    let repository = Arc::new(repository);
    let dispatch = RepositoryDispatch::new();
    let (entered_tx, entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let entered_rx = Arc::new(tokio::sync::Mutex::new(entered_rx));
    let (release_first_tx, release_first_rx) = mpsc::channel();
    let (release_second_tx, release_second_rx) = mpsc::channel();
    let first_dispatch = dispatch.clone();
    let first_repository = Arc::clone(&repository);
    let first_entered = entered_tx.clone();
    let first = tokio::spawn(async move {
        first_dispatch
            .read(move || {
                let result = first_repository.list_creators(ListCreatorsInput::default());
                first_entered.send(()).unwrap();
                release_first_rx.recv().unwrap();
                result
            })
            .await
    });
    let second_dispatch = dispatch.clone();
    let second_repository = Arc::clone(&repository);
    let second = tokio::spawn(async move {
        second_dispatch
            .read(move || {
                let result = second_repository.list_creators(ListCreatorsInput::default());
                entered_tx.send(()).unwrap();
                release_second_rx.recv().unwrap();
                result
            })
            .await
    });
    let mut entered = entered_rx.lock().await;
    entered.recv().await.unwrap();
    entered.recv().await.unwrap();
    drop(entered);

    tokio::time::timeout(
        Duration::from_millis(50),
        tokio::spawn(async {
            tokio::task::yield_now().await;
        }),
    )
    .await
    .expect("Tokio health future was starved")
    .unwrap();
    let called = Arc::new(AtomicUsize::new(0));
    let overload_called = Arc::clone(&called);
    let overloaded = dispatch
        .read(move || {
            overload_called.fetch_add(1, Ordering::SeqCst);
        })
        .await
        .unwrap_err();
    assert_eq!(overloaded, CreatorRepositoryDispatchFailure::Overloaded);
    assert_eq!(called.load(Ordering::SeqCst), 0);

    release_first_tx.send(()).unwrap();
    release_second_tx.send(()).unwrap();
    first.await.unwrap().unwrap().unwrap();
    second.await.unwrap().unwrap().unwrap();
    dispatch.close();
    assert_eq!(
        dispatch.read(|| ()).await.unwrap_err(),
        CreatorRepositoryDispatchFailure::Closed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_before_dispatch_has_no_effect_but_started_write_finishes_for_readback() {
    let (_directory, repository) = repository();

    let dispatch = RepositoryDispatch::new();
    let (first_entered_tx, first_entered_rx) = tokio::sync::oneshot::channel();
    let (first_release_tx, first_release_rx) = mpsc::channel();
    let first_dispatch = dispatch.clone();
    let first = tokio::spawn(async move {
        first_dispatch
            .write(move || {
                first_entered_tx.send(()).unwrap();
                first_release_rx.recv().unwrap();
            })
            .await
    });
    first_entered_rx.await.unwrap();
    let called = Arc::new(AtomicUsize::new(0));
    let cancelled_called = Arc::clone(&called);
    let waiting_dispatch = dispatch.clone();
    let cancelled = tokio::spawn(async move {
        waiting_dispatch
            .write(move || {
                cancelled_called.fetch_add(1, Ordering::SeqCst);
            })
            .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    first_release_tx.send(()).unwrap();
    first.await.unwrap().unwrap();
    assert_eq!(called.load(Ordering::SeqCst), 0);

    let repository = Arc::new(repository);
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let write_repository = Arc::clone(&repository);
    let write_dispatch = dispatch.clone();
    let write = tokio::spawn(async move {
        write_dispatch
            .write(move || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                write_repository.create_manual_creator(ManualCreatorCreateInput {
                    tags: None,
                    display_name: "completed after caller cancellation".into(),
                    notes: None,
                    contact_routes: Vec::new(),
                })
            })
            .await
    });
    started_rx.await.unwrap();
    write.abort();
    assert!(write.await.unwrap_err().is_cancelled());
    release_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let creators = repository
                .list_creators(ListCreatorsInput::default())
                .unwrap();
            if creators
                .items
                .iter()
                .any(|creator| creator.display_name == "completed after caller cancellation")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("started write did not finish for durable readback");
}
