use onecrew_creator_runtime::{AcknowledgeReplyTriageInput, RecordReplyTriageInput, ReplyTriage};

use super::{
    super::super::RegistryOwners,
    shared::{SafetySpec, metadata, owner_guarded, unknown_string_failure},
};
use crate::operation_registry::{
    descriptor::{
        AuditMode, ConfirmationMode, EffectClass, IdempotencyMode, ReadbackMode, SurfaceId,
    },
    execution::run_blocking,
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 4] = [
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Outreach,
    SurfaceId::Mail,
];
const FAILURE_CODE: &str = "triage_write_failed";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<RecordReplyTriageInput, ReplyTriage, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_TRIAGE_RECORD,
            "triage",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            triage_safety(IdempotencyMode::None, &[]),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.record_reply_triage(input)).await }
        },
        unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str()),
        ),
    )?;

    let creators = owners.creators;
    builder.register_operation::<AcknowledgeReplyTriageInput, ReplyTriage, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_TRIAGE_ACKNOWLEDGE,
            "triage",
            SURFACES,
        ),
        owner_guarded(
            EffectClass::WriteLocal,
            triage_safety(IdempotencyMode::OwnerInputIdentity, &["/triage_id"]),
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.acknowledge_reply_triage(input)).await }
        },
        unknown_string_failure(
            FAILURE_CODE,
            Some(onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str()),
        ),
    )
}

fn triage_safety(
    idempotency_mode: IdempotencyMode,
    idempotency_identity: &'static [&'static str],
) -> SafetySpec {
    SafetySpec {
        confirmation_mode: ConfirmationMode::None,
        confirmation_parameters: &[],
        idempotency_mode,
        idempotency_identity,
        readback_mode: ReadbackMode::InlineAndFollowUp,
        readback_operations: const {
            &[onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str()]
        },
        audit: AuditMode::None,
        warning_output: &[],
        recovery_output: &[],
    }
}
