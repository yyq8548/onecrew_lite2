use crate::operation_registry::descriptor::{AuditMode, EffectClass};
use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::domains::onecrew_writes::creators::FAILURE_CODE;
use crate::operation_registry::domains::onecrew_writes::creators::shared::{
    CREATOR_SURFACES, ConfirmationSpec, IdempotencySpec, ReadbackSpec, SafetySpec,
    classify_creator_group_failure, classify_prepare_rejection, metadata, policy, review_context,
};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_creator_runtime::{
    BulkMoveCreatorsToGroupInput, CreateCreatorGroupInput, CreatorGroup, DeleteCreatorGroupInput,
    DeleteCreatorGroupResult, ManualCreatorBatchCreateInput, ManualCreatorBatchCreateResult,
    MoveCreatorsToGroupResult, SetCreatorGroupInput, UpdateCreatorGroupInput,
};

pub(super) fn register_creator_group_writes(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<CreateCreatorGroupInput, CreatorGroup, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_CREATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                readback: ReadbackSpec::InlineAndFollowUp(const { &[
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_LIST.as_str(),
                ] }),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.create_creator_group_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(
            classify_creator_group_failure,
        ),
    )?;

    // A manual Creator batch is an owner review, exactly like an email send.
    // The confirmed intent is the sealed grant the review mints, so there is no
    // confirmation parameter: a `/confirmed` pointer here was only ever
    // validated at registration and never read at execution, which let the
    // Agent's own flag decide whether the batch was written.
    let platform = owners.platform.clone();
    builder.register_owner_review_operation::<
        ManualCreatorBatchCreateInput,
        ManualCreatorBatchCreateResult,
        String,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_BATCH_CREATE, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                confirmation: ConfirmationSpec::Attended(
                    "Confirm creating these Creator records from the submitted batch?",
                ),
                idempotency: IdempotencySpec::Input(&["/idempotency_key"]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
                audit: AuditMode::OwnerRecorded,
                ..SafetySpec::local()
            },
        ),
        move |context, input| {
            let platform = platform.clone();
            async move {
                platform
                    .prepare_manual_creator_batch(input, review_context(context)?)
                    .await
            }
        },
        classify_prepare_rejection(FAILURE_CODE),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<UpdateCreatorGroupInput, CreatorGroup, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_UPDATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/group_id", "/expected_revision", "/name"]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_LIST.as_str(),
                ] }),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.update_creator_group_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(
            classify_creator_group_failure,
        ),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<DeleteCreatorGroupInput, DeleteCreatorGroupResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_DELETE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::DeleteLocal,
            SafetySpec {
                confirmation: ConfirmationSpec::Attended(
                    "Delete this Creator group and move its members to Uncategorized?",
                ),
                idempotency: IdempotencySpec::Input(&["/group_id", "/expected_revision"]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_LIST.as_str(),
                ] }),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.delete_creator_group_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(
            classify_creator_group_failure,
        ),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<SetCreatorGroupInput, MoveCreatorsToGroupResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_SET,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/creator_id", "/destination"]),
                readback: ReadbackSpec::InlineAndFollowUp(
                    const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.set_creator_group_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(
            classify_creator_group_failure,
        ),
    )?;

    let creators = owners.creators;
    builder
        .register_operation::<BulkMoveCreatorsToGroupInput, MoveCreatorsToGroupResult, _, _, _, _>(
            metadata(
                onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_BULK_MOVE,
                "creators",
                CREATOR_SURFACES,
            ),
            policy(
                EffectClass::WriteLocal,
                SafetySpec {
                    idempotency: IdempotencySpec::Input(&["/creator_ids", "/destination"]),
                    readback: ReadbackSpec::InlineAndFollowUp(const { &[
                        onecrew_local_runtime::OperationId::ONECREW_CREATOR_LIST.as_str(),
                    ] }),
                    ..SafetySpec::local()
                },
            ),
            move |_context, input| {
                let creators = creators.clone();
                async move { creators.bulk_move_creators_to_group_async(input).await }
            },
            crate::operation_registry::execution::classify_repository_call(
                classify_creator_group_failure,
            ),
        )
}

#[cfg(test)]
mod tests;
