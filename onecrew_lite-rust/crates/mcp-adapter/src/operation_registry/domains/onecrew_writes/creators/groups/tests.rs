use crate::operation_registry::descriptor::SurfaceId;
use crate::operation_registry::domains::onecrew_writes::creators::shared::classify_creator_group_failure;
use crate::operation_registry::domains::onecrew_writes::creators::test_support::{
    execute_current, operation, registry,
};
use crate::operation_registry::execution::{
    ClassifiedOwnerFailure, ExecutionContext, PublicFailureCode,
};
use onecrew_creator_runtime::{
    CreateCreatorGroupInput, CreatorGroupDestination, CreatorGroupError, ManualCreatorCreateInput,
    SetCreatorGroupInput,
};
use serde_json::json;
use std::sync::Arc;

#[test]
fn creator_group_write_failures_preserve_known_and_unknown_effect_boundaries() {
    for (error, code) in [
        (
            CreatorGroupError::InvalidInput,
            "creator_group_invalid_input",
        ),
        (
            CreatorGroupError::DuplicateName,
            "creator_group_duplicate_name",
        ),
        (CreatorGroupError::GroupNotFound, "creator_group_not_found"),
        (CreatorGroupError::CreatorNotFound, "creator_not_found"),
        (
            CreatorGroupError::RevisionConflict,
            "creator_group_revision_conflict",
        ),
    ] {
        assert_eq!(
            classify_creator_group_failure(error),
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse(code).unwrap(),
                false,
                None,
            )
        );
    }
    assert_eq!(
        classify_creator_group_failure(CreatorGroupError::Internal),
        ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("creator_group_outcome_unknown").unwrap(),
            None,
        )
    );
}

#[test]
fn creator_group_set_follow_up_get_reads_the_persisted_group() {
    let directory = tempfile::tempdir().expect("temporary data directory");
    let platform = Arc::new(
        onecrew_local_runtime::LocalPlatform::open(directory.path()).expect("local platform"),
    );
    let runtime = platform.creators().expect("OneCrew runtime");
    let creator = runtime
        .create_manual_creator(ManualCreatorCreateInput {
            tags: None,
            display_name: "Grouped readback".to_owned(),
            notes: None,
            contact_routes: Vec::new(),
        })
        .expect("Creator fixture");
    let group = runtime
        .create_creator_group(CreateCreatorGroupInput {
            name: "Readback folder".to_owned(),
        })
        .expect("group fixture");
    let registry = registry(platform);

    let set_result = execute_current(
        operation(&registry, "onecrew.creator.group.set"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "creator-group-set".to_owned(),
            connection: None,
            confirmed: false,
        },
        serde_json::to_value(SetCreatorGroupInput {
            creator_id: creator.creator_id.clone(),
            destination: CreatorGroupDestination::Group {
                group_id: group.id.clone(),
            },
        })
        .expect("set input JSON"),
    )
    .expect("registered group set");
    assert_eq!(
        set_result["assignments"][0]["group"]["id"],
        group.id.as_str()
    );

    let readback = execute_current(
        operation(&registry, "onecrew.creator.get"),
        ExecutionContext {
            surface: SurfaceId::Full,
            request_id: "creator-group-get".to_owned(),
            connection: None,
            confirmed: false,
        },
        json!({"creator_id": creator.creator_id}),
    )
    .expect("registered Creator readback");
    assert_eq!(readback["group"]["id"], group.id.as_str());
    assert_eq!(readback["group"]["name"], group.name);
}
