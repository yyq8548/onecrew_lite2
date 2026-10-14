use onecrew_creator_runtime::{
    ContactTargetResolutionResult, DirectMessagePreparation, PrepareDirectMessageInput,
    PrepareRelationshipActionInput, RelationshipActionPreparation, RelationshipContextInput,
    RelationshipSnapshot, ResolveContactTargetInput, SendDirectMessageInput,
    UpdateRelationshipInput,
};

use super::{
    super::super::RegistryOwners,
    owner_review::{classify_prepare_failure, review_policy},
    shared::{SafetySpec, metadata, owner_guarded},
};
use crate::operation_registry::{
    descriptor::{
        AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode, SurfaceId,
    },
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder.register_operation::<
        ResolveContactTargetInput,
        ContactTargetResolutionResult,
        onecrew_local_runtime::OutreachError,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_TARGET_RESOLVE, "outreach", SURFACES),
        owner_guarded(
            EffectClass::SyncOpenWorld,
            open_world_safety(
                ReadbackMode::InlineOwnerVerified,
                &[],
                &[],
                &["/status", "/reason", "/allowed_actions", "/browser_url", "/conflict"],
            ),
        ),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.resolve_contact_target(input).await }
        },
        classify_prepare_failure,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<RelationshipContextInput, RelationshipSnapshot, onecrew_local_runtime::OutreachError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_RELATIONSHIP_GET, "outreach", SURFACES),
        owner_guarded(
            EffectClass::SyncOpenWorld,
            open_world_safety(ReadbackMode::InlineOwnerVerified, &[], &[], &[]),
        ),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.relationship_get(input).await }
        },
        classify_prepare_failure,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<
        PrepareRelationshipActionInput,
        RelationshipActionPreparation,
        onecrew_local_runtime::OutreachError,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_RELATIONSHIP_PREPARE, "outreach", SURFACES),
        owner_guarded(
            EffectClass::SyncOpenWorld,
            open_world_safety(ReadbackMode::InlineOwnerVerified, &[], &[], &[]),
        ),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.prepare_relationship_action(input).await }
        },
        classify_prepare_failure,
    )?;

    let platform = owners.platform.clone();
    builder
        .register_owner_review_operation::<UpdateRelationshipInput, onecrew_local_runtime::OutreachCompletion, onecrew_local_runtime::OutreachError, _, _, _>(
            metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_RELATIONSHIP_UPDATE, "outreach", SURFACES),
            review_policy("Approve this relationship change before applying it."),
            move |context, input| {
                let platform = platform.clone();
                async move { platform.prepare_outreach_relationship(input, context.owner_review_context()?).await }
            },
            classify_prepare_failure,
        )?;

    let platform = owners.platform.clone();
    builder
        .register_operation::<PrepareDirectMessageInput, DirectMessagePreparation, onecrew_local_runtime::OutreachError, _, _, _>(
            metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE, "outreach", SURFACES),
            owner_guarded(
                EffectClass::SyncOpenWorld,
                open_world_safety(ReadbackMode::InlineOwnerVerified, &[], &["/warnings"], &[]),
            ),
            move |_context, input| {
                let platform = platform.clone();
                async move { platform.prepare_direct_message(input).await }
            },
            classify_prepare_failure,
        )?;

    let platform = owners.platform;
    builder.register_owner_review_operation::<SendDirectMessageInput, onecrew_local_runtime::OutreachCompletion, onecrew_local_runtime::OutreachError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_MESSAGE_SEND, "outreach", SURFACES),
        review_policy("Approve this direct message before sending it."),
        move |context, input| {
            let platform = platform.clone();
            async move { platform.prepare_outreach_direct_message(input, context.owner_review_context()?).await }
        },
        classify_prepare_failure,
    )
}

pub(super) fn open_world_safety(
    readback_mode: ReadbackMode,
    readback_operations: &'static [&'static str],
    warning_output: &'static [&'static str],
    recovery_output: &'static [&'static str],
) -> SafetySpec {
    SafetySpec {
        confirmation_mode: ConfirmationMode::None,
        confirmation_parameters: &[],
        idempotency_mode: IdempotencyMode::None,
        idempotency_identity: &[],
        readback_mode,
        readback_operations,
        audit: AuditMode::None,
        warning_output,
        recovery_output,
    }
}
