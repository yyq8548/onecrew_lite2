use onecrew_creator_runtime::{
    ContentReplyPreparation, CreatorGroupListResult, CreatorIdInput, CreatorListPage,
    CreatorRecord, IdentityResolution, ListCreatorGroupsInput, ListCreatorsInput,
    PrepareContentReplyInput, ResolveCreatorIdentityInput,
};

use super::{
    super::RegistryOwners,
    shared::{classify_creator_group_read_failure, classify_string_failure, metadata, read_policy},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    execution::run_blocking,
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators];
const FAILURE_CODE: &str = "creator_read_unavailable";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder
        .register_operation::<PrepareContentReplyInput, ContentReplyPreparation, onecrew_local_runtime::OutreachError, _, _, _>(
            metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTENT_REPLY_PREPARE, "creators", SURFACES),
            read_policy(),
            move |_context, input| {
                let platform = platform.clone();
                async move { platform.prepare_content_reply(input).await }
            },
            |error| crate::operation_registry::execution::ClassifiedOwnerFailure::known_no_effect(
                crate::operation_registry::execution::PublicFailureCode::parse(FAILURE_CODE).expect("static Creator read failure code"),
                false, None,
            ).with_message(&error.to_string()),
        )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListCreatorsInput, CreatorListPage, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_LIST,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_creators_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListCreatorGroupsInput, CreatorGroupListResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_LIST,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_creator_groups_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(
            classify_creator_group_read_failure,
        ),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<CreatorIdInput, CreatorRecord, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET,
            "creators",
            SURFACES,
        ),
        read_policy().with_view_kind(
            crate::operation_registry::descriptor::OperationViewKind::RecordPreview,
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.get_creator_async(input.creator_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators;
    builder.register_operation::<ResolveCreatorIdentityInput, IdentityResolution, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_IDENTITY_RESOLVE,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.resolve_creator_identity(input)).await }
        },
        classify_string_failure(FAILURE_CODE),
    )
}
