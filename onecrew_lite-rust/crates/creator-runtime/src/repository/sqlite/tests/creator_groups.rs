use super::*;
use crate::{
    BulkMoveCreatorsToGroupInput, CreateCreatorGroupInput, CreatorGroupDestination,
    CreatorGroupError, CreatorGroupFilter, DeleteCreatorGroupInput, ListCreatorGroupsInput,
    ListCreatorsInput, SetCreatorGroupInput, UpdateCreatorGroupInput,
};

fn creator(repository: &SqliteRepository, name: &str) -> CreatorId {
    repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: name.to_owned(),
            notes: None,
        })
        .expect("creator")
        .creator
        .id
}

fn group(repository: &SqliteRepository, name: &str) -> crate::CreatorGroup {
    repository
        .create_creator_group(CreateCreatorGroupInput {
            name: name.to_owned(),
        })
        .expect("group")
}

#[test]
fn creator_groups_are_bounded_unique_revisioned_and_counted() {
    let (_directory, repository) = repository();
    let first = group(&repository, "  Launch  ");
    let second = group(&repository, "Long term");
    assert_eq!(first.name, "Launch");
    assert_eq!(first.revision, 1);
    assert_eq!(first.position, 0);
    assert_eq!(second.position, 1);

    assert_eq!(
        repository.create_creator_group(CreateCreatorGroupInput {
            name: "launch".into(),
        }),
        Err(CreatorGroupError::DuplicateName)
    );
    assert_eq!(
        repository.create_creator_group(CreateCreatorGroupInput { name: " ".into() }),
        Err(CreatorGroupError::InvalidInput)
    );

    let updated = repository
        .update_creator_group(UpdateCreatorGroupInput {
            group_id: first.id.clone(),
            expected_revision: 1,
            name: "Launch partners".into(),
        })
        .expect("rename");
    assert!(updated.1);
    assert_eq!(updated.0.revision, 2);
    assert_eq!(
        repository.update_creator_group(UpdateCreatorGroupInput {
            group_id: first.id.clone(),
            expected_revision: 1,
            name: "Stale".into(),
        }),
        Err(CreatorGroupError::RevisionConflict)
    );

    let listed = repository
        .list_creator_groups(ListCreatorGroupsInput::default())
        .expect("groups");
    assert_eq!(
        listed
            .items
            .iter()
            .map(|item| item.group.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Launch partners", "Long term"]
    );
    assert_eq!(listed.uncategorized_count, 0);
}

#[test]
fn move_filter_and_delete_keep_one_folder_and_preserve_creators() {
    let (_directory, repository) = repository();
    let launch = group(&repository, "Launch");
    let longterm = group(&repository, "Long term");
    let alice = creator(&repository, "Alice");
    let bob = creator(&repository, "Bob");
    let charlie = creator(&repository, "Charlie");

    let moved = repository
        .bulk_move_creators_to_group(BulkMoveCreatorsToGroupInput {
            creator_ids: vec![alice.clone(), bob.clone()],
            destination: CreatorGroupDestination::Group {
                group_id: launch.id.clone(),
            },
        })
        .expect("bulk move");
    assert_eq!((moved.moved_count, moved.unchanged_count), (2, 0));

    let launch_page = repository
        .list_creators(ListCreatorsInput {
            group: Some(CreatorGroupFilter::Group {
                group_id: launch.id.clone(),
            }),
            ..Default::default()
        })
        .expect("launch creators");
    assert_eq!(launch_page.items.len(), 2);
    assert!(
        launch_page
            .items
            .iter()
            .all(|item| { item.group.as_ref().map(|group| &group.id) == Some(&launch.id) })
    );

    let uncategorized = repository
        .list_creators(ListCreatorsInput {
            group: Some(CreatorGroupFilter::Uncategorized),
            ..Default::default()
        })
        .expect("uncategorized");
    assert_eq!(uncategorized.items[0].id, charlie);

    let moved_again = repository
        .set_creator_group(SetCreatorGroupInput {
            creator_id: alice.clone(),
            destination: CreatorGroupDestination::Group {
                group_id: longterm.id.clone(),
            },
        })
        .expect("move to another group");
    assert_eq!(
        (moved_again.moved_count, moved_again.unchanged_count),
        (1, 0)
    );
    assert_eq!(
        moved_again.assignments[0]
            .group
            .as_ref()
            .map(|group| &group.id),
        Some(&longterm.id)
    );
    assert_eq!(
        repository
            .get_creator(&alice)
            .expect("grouped Creator readback")
            .group
            .as_ref()
            .map(|group| &group.id),
        Some(&longterm.id)
    );

    let deleted = repository
        .delete_creator_group(DeleteCreatorGroupInput {
            group_id: longterm.id.clone(),
            expected_revision: longterm.revision,
        })
        .expect("delete group");
    assert_eq!(deleted.uncategorized_creator_count, 1);
    let surviving = repository.get_creator(&alice).expect("creator survives");
    assert_eq!(surviving.creator.id, alice);
    assert!(surviving.group.is_none());
    let uncategorized = repository
        .list_creators(ListCreatorsInput {
            group: Some(CreatorGroupFilter::Uncategorized),
            ..Default::default()
        })
        .expect("uncategorized after delete");
    assert!(uncategorized.items.iter().any(|item| item.id == alice));
}

#[test]
fn group_cursor_is_bound_to_its_filter_and_bulk_input_is_strict() {
    let (_directory, repository) = repository();
    for name in ["A", "B", "C"] {
        group(&repository, name);
    }
    let first_page = repository
        .list_creator_groups(ListCreatorGroupsInput {
            limit: Some(1),
            ..Default::default()
        })
        .expect("first page");
    let cursor = first_page.next_cursor.expect("cursor");
    assert_eq!(
        repository.list_creator_groups(ListCreatorGroupsInput {
            limit: Some(1),
            cursor: Some(cursor),
            query: Some("B".into()),
            include_archived: false,
        }),
        Err(CreatorGroupError::InvalidInput)
    );

    let creator_id = creator(&repository, "Creator");
    assert_eq!(
        repository.bulk_move_creators_to_group(BulkMoveCreatorsToGroupInput {
            creator_ids: vec![creator_id.clone(), creator_id],
            destination: CreatorGroupDestination::Uncategorized,
        }),
        Err(CreatorGroupError::InvalidInput)
    );
}
