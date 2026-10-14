use crate::test_support::assert_runtime_health;
use crate::{CreatorChangeScope, CreatorGateways, CreatorRuntime};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creator_import_cpu_segment_leaves_writer_and_both_read_lanes_available() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let bundle =
        crate::creator_transfer::build_creator_bundle(vec![crate::CreatorTransferCreator {
            notes: None,
            tags: Vec::new(),
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: "Segmented import".into(),
            score: None,
            follower_counts: Default::default(),
            pinned: false,
            relationship: crate::CreatorTransferRelationship {
                stage: crate::CreatorTransferStage::Prospect,
            },
            accounts: Vec::new(),
            contact_routes: Vec::new(),
        }])
        .unwrap();
    let xlsx = bundle.to_xlsx_bytes().unwrap();
    let (cpu_started, release_cpu) = runtime.install_next_repository_cpu_barrier();
    let importing_runtime = runtime.clone();
    let importing = tokio::spawn(async move {
        importing_runtime
            .prepare_creator_import_xlsx_bytes_async(xlsx)
            .await
    });
    cpu_started.await.expect("import never entered CPU segment");

    let writer_runtime = runtime.clone();
    let writer = tokio::task::spawn_blocking(move || {
        writer_runtime.create_manual_creator(crate::ManualCreatorCreateInput {
            tags: None,
            display_name: "Writer stayed available".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
    });
    tokio::time::timeout(std::time::Duration::from_secs(1), writer)
        .await
        .expect("CPU segment held the writer")
        .unwrap()
        .unwrap();

    let first = runtime.list_creators_async(crate::ListCreatorsInput::default());
    let second = runtime.list_creators_async(crate::ListCreatorsInput::default());
    let (first, second) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::join!(first, second)
    })
    .await
    .expect("CPU segment occupied a read lane");
    assert_eq!(first.unwrap().items.len(), 1);
    assert_eq!(second.unwrap().items.len(), 1);
    assert_runtime_health().await;

    release_cpu.send(()).unwrap();
    assert!(matches!(
        importing.await.unwrap().unwrap(),
        crate::PrepareCreatorImportResult::ConfirmationRequired { .. }
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_confirm_caller_still_commits_and_publishes_exactly_once() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let bundle =
        crate::creator_transfer::build_creator_bundle(vec![crate::CreatorTransferCreator {
            notes: None,
            tags: Vec::new(),
            semantic_sha256: String::new(),
            record_key: String::new(),
            display_name: "Committed after caller drop".into(),
            score: None,
            follower_counts: Default::default(),
            pinned: false,
            relationship: crate::CreatorTransferRelationship {
                stage: crate::CreatorTransferStage::Prospect,
            },
            accounts: Vec::new(),
            contact_routes: Vec::new(),
        }])
        .unwrap();
    let operation_id = match runtime
        .prepare_creator_import_xlsx_bytes_async(bundle.to_xlsx_bytes().unwrap())
        .await
        .unwrap()
    {
        crate::PrepareCreatorImportResult::ConfirmationRequired { operation_id, .. } => {
            operation_id
        }
        other => panic!("expected prepared import, got {other:?}"),
    };
    let mut changes = runtime.subscribe_changes();
    let (writer_started, release_writer) = runtime.install_next_repository_write_barrier();
    let confirming_runtime = runtime.clone();
    let confirm_id = operation_id.clone();
    let caller = tokio::spawn(async move {
        confirming_runtime
            .confirm_creator_import_async(crate::ConfirmCreatorImportInput {
                operation_id: confirm_id,
            })
            .await
    });
    writer_started
        .await
        .expect("confirm never entered supervised writer");
    assert_runtime_health().await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    release_writer.send(()).unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
        .await
        .expect("committed import lost its Creators invalidation")
        .unwrap();
    assert_eq!(change.scopes, vec![CreatorChangeScope::Creators]);
    let creators = runtime
        .list_creators_async(crate::ListCreatorsInput::default())
        .await
        .unwrap();
    assert_eq!(creators.items.len(), 1);
    assert_eq!(
        creators.items[0].display_name,
        "Committed after caller drop"
    );

    let replay = runtime
        .confirm_creator_import_async(crate::ConfirmCreatorImportInput { operation_id })
        .await
        .unwrap();
    assert!(matches!(
        replay,
        crate::ConfirmCreatorImportResult::Imported { .. }
    ));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), changes.recv())
            .await
            .is_err(),
        "terminal replay published a second Creators invalidation"
    );
}
