use crate::operation_registry::descriptor::EffectClass;
use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::domains::onecrew_writes::creators::FAILURE_CODE;
use crate::operation_registry::domains::onecrew_writes::creators::shared::{
    CREATOR_SURFACES, ConfirmationSpec, IdempotencySpec, ReadbackSpec, SafetySpec,
    classify_string_failure, metadata, policy,
};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_creator_runtime::{
    Creator, CreatorDeleteResult, CreatorIdInput, CreatorRecord, CreatorRelationship,
    DeleteCreatorInput, ManualCreatorCreateInput, ManualCreatorCreateResult, UpdateCreatorInput,
    UpdateCreatorStageInput,
};

pub(super) fn register_local_creator_writes(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<CreatorIdInput, Creator, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_ARCHIVE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/creator_id"]),
                readback: ReadbackSpec::InlineAndFollowUp(
                    const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.archive_creator_async(input.creator_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<DeleteCreatorInput, CreatorDeleteResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_DELETE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::DeleteLocal,
            SafetySpec {
                confirmation: ConfirmationSpec::Attended(
                    "Permanently delete the Creator and all locally derived facts?",
                ),
                readback: ReadbackSpec::Inline,
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.delete_creator_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ManualCreatorCreateInput, ManualCreatorCreateResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_MANUAL_CREATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                readback: ReadbackSpec::InlineAndFollowUp(
                    const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.create_manual_creator_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<UpdateCreatorInput, CreatorRecord, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                readback: ReadbackSpec::InlineAndFollowUp(
                    const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.update_creator_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators;
    builder.register_operation::<UpdateCreatorStageInput, CreatorRelationship, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_STAGE_UPDATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/creator_id", "/stage"]),
                readback: ReadbackSpec::InlineAndFollowUp(
                    const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.update_creator_stage_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )
}
