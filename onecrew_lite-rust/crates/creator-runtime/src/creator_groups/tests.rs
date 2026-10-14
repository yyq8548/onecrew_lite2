use crate::{
    CreateCreatorGroupInput, CreatorChangeScope, CreatorGateways, CreatorGroupDestination,
    CreatorRuntime, ManualCreatorCreateInput, SetCreatorGroupInput, UpdateCreatorGroupInput,
};

#[test]
fn creator_group_changes_publish_only_after_authoritative_changes() {
    let directory = tempfile::tempdir().expect("temp directory");
    let runtime =
        CreatorRuntime::open(directory.path(), CreatorGateways::default()).expect("runtime");
    let mut changes = runtime.subscribe_changes();

    let group = runtime
        .create_creator_group(CreateCreatorGroupInput {
            name: "Launch".into(),
        })
        .expect("group");
    assert_eq!(
        changes.try_recv().expect("group create change").scopes,
        vec![CreatorChangeScope::CreatorGroups]
    );
    let creator = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Grouped Creator".into(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .expect("creator");
    assert_eq!(
        changes.try_recv().expect("creator create change").scopes,
        vec![CreatorChangeScope::Creators]
    );

    let moved = runtime
        .set_creator_group(SetCreatorGroupInput {
            creator_id: creator.creator_id.clone(),
            destination: CreatorGroupDestination::Group {
                group_id: group.id.clone(),
            },
        })
        .expect("move");
    assert_eq!(moved.moved_count, 1);
    assert_eq!(
        changes.try_recv().expect("group assignment change").scopes,
        vec![
            CreatorChangeScope::CreatorGroups,
            CreatorChangeScope::Creators
        ]
    );

    let replay = runtime
        .set_creator_group(SetCreatorGroupInput {
            creator_id: creator.creator_id,
            destination: CreatorGroupDestination::Group {
                group_id: group.id.clone(),
            },
        })
        .expect("replay");
    assert_eq!((replay.moved_count, replay.unchanged_count), (0, 1));
    assert!(changes.try_recv().is_err(), "replay emits no change");

    let unchanged = runtime
        .update_creator_group(UpdateCreatorGroupInput {
            group_id: group.id,
            expected_revision: group.revision,
            name: group.name,
        })
        .expect("unchanged rename");
    assert_eq!(unchanged.revision, 1);
    assert!(
        changes.try_recv().is_err(),
        "unchanged rename emits no change"
    );
}
