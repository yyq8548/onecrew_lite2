//! Public consumer coverage for Creator invalidation through the shared runtime handle.

use onecrew_creator_runtime::{CreatorChangeScope, ManualCreatorCreateInput};
use onecrew_local_runtime::LocalPlatform;

#[test]
fn manual_creator_create_invalidates_creator_projections_once() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let creators = platform.creators().unwrap();
    let mut changes = creators.subscribe_changes();

    let created = creators
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Invalidation".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .unwrap();
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![CreatorChangeScope::Creators]
    );

    // A read through the same handle publishes nothing.
    assert_eq!(
        creators
            .get_creator(&created.creator_id)
            .unwrap()
            .creator
            .display_name,
        "Invalidation"
    );
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
