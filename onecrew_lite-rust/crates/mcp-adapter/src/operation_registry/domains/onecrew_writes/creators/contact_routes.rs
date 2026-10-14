use crate::operation_registry::descriptor::EffectClass;
use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::domains::onecrew_writes::creators::FAILURE_CODE;
use crate::operation_registry::domains::onecrew_writes::creators::shared::{
    CREATOR_SURFACES, ConfirmationSpec, IdempotencySpec, ReadbackSpec, SafetySpec,
    classify_string_failure, metadata, policy,
};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_creator_runtime::{
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    ManualCreatorContactRouteAddInput, ManualCreatorContactRouteAddResult,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateResult,
    PrepareCreatorContactRouteDeleteInput, PrepareCreatorContactRouteDeleteResult,
};

pub(super) fn register_contact_route_writes(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<
        ManualCreatorContactRouteAddInput,
        ManualCreatorContactRouteAddResult, _,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_ADD, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&[
                    "/creator_id",
                    "/channel",
                    "/value",
                    "/source_url",
                ]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.add_manual_creator_contact_route_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<
        ConfirmCreatorContactRouteDeleteInput,
        ConfirmCreatorContactRouteDeleteResult, _,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_DELETE_CONFIRM, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::DeleteLocal,
            SafetySpec {
                confirmation: ConfirmationSpec::AttendedAndOwner {
                    prompt: "Delete the prepared Creator contact route and its local dependents?",
                    pointers: &["/operation_id"],
                },
                idempotency: IdempotencySpec::Input(&["/operation_id"]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
                warning_pointers: &["/remote_not_deleted"],
                recovery_pointers: &["/next_tool"],
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.confirm_creator_contact_route_delete_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<
        PrepareCreatorContactRouteDeleteInput,
        PrepareCreatorContactRouteDeleteResult, _,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_DELETE_PREPARE, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                warning_pointers: &["/remote_not_deleted"],
                recovery_pointers: &["/next_tool"],
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.prepare_creator_contact_route_delete_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;

    let creators = owners.creators;
    builder.register_operation::<
        ManualCreatorContactRouteUpdateInput,
        ManualCreatorContactRouteUpdateResult, _,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_UPDATE, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&[
                    "/creator_id",
                    "/contact_route_id",
                    "/channel",
                    "/value",
                    "/source_url",
                ]),
                readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str()] }),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.update_manual_creator_contact_route_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )
}
