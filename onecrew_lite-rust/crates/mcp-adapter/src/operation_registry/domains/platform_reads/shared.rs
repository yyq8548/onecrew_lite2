//! Shared protocol metadata and failure projection, without a platform business model.
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
        EffectClass, IdempotencyMode, IdempotencySafety, OperationRegistration, OperationSafety,
        ReadbackMode, ReadbackSafety, SecretClass, SurfaceId, SurfaceSet,
    },
    execution::{
        BoundedPublicValue, ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode,
        RecoveryDirective,
    },
    registration::OperationPolicy,
};
use onecrew_local_runtime::{
    PlatformConnectionResolutionError, SearchPlanApprovalError, SearchPlanPolicy,
};

pub(super) fn metadata(id: onecrew_local_runtime::OperationId) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: "reads".to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new([SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators])
            .expect("platform read operations have fixed surfaces"),
    }
}

pub(super) fn policy(
    effect: EffectClass,
    connection: ConnectionRequirement,
    execution: ExecutionDeclaration,
) -> OperationPolicy {
    if !effect.may_have_effect() {
        return OperationPolicy::read_only(effect, connection, execution);
    }
    OperationPolicy {
        effect,
        secret: SecretClass::None,
        approval: ApprovalClass::OwnerGuarded,
        connection,
        sensitive_input_paths: Vec::new(),
        view_kind: crate::operation_registry::descriptor::OperationViewKind::SimpleOperation,
        approval_request: None,
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: ConfirmationMode::None,
                parameter_pointers: Vec::new(),
            },
            idempotency: IdempotencySafety {
                mode: IdempotencyMode::None,
                identity_pointers: Vec::new(),
            },
            readback: ReadbackSafety {
                mode: ReadbackMode::None,
                operation_ids: Vec::new(),
            },
            audit: AuditMode::None,
            warnings_present: false,
            recovery_present: false,
            warning_output_pointers: Vec::new(),
            recovery_output_pointers: Vec::new(),
        },
        execution,
    }
}
pub(super) fn classify_connection_failure(
    error: PlatformConnectionResolutionError,
) -> ClassifiedOwnerFailure {
    match error {
        PlatformConnectionResolutionError::InvalidIdentifier => connection_failure(
            "connection_id_invalid",
            Some(RecoveryDirective::CorrectInput),
        ),
        PlatformConnectionResolutionError::NotFoundOrMismatch => connection_failure(
            "connection_not_found_or_mismatch",
            Some(RecoveryDirective::CorrectInput),
        ),
        PlatformConnectionResolutionError::Unavailable => connection_failure(
            "connection_unavailable",
            Some(connection_inventory_recovery()),
        ),
        PlatformConnectionResolutionError::Internal => {
            connection_failure("connection_resolution_failed", None)
        }
    }
}
pub(super) fn search_plan_refusal(reason: SearchPlanApprovalError) -> ClassifiedOwnerFailure {
    let recovery = RecoveryDirective::PrepareSearchPlan {
        operation_id: BoundedPublicValue::parse(
            onecrew_local_runtime::OperationId::ONECREW_SEARCH_PLAN_PREPARE.as_str(),
        )
        .expect("static operation ID"),
    };
    provider_known_no_effect(reason.code(), false, Some(recovery))
}

fn provider_known_no_effect(
    code: &str,
    retry_safe: bool,
    recovery: Option<RecoveryDirective>,
) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static platform failure code is valid"),
        retry_safe,
        recovery,
    )
}

fn connection_failure(code: &str, recovery: Option<RecoveryDirective>) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static connection failure code is valid"),
        false,
        recovery,
    )
}

pub(super) fn connection_inventory_recovery() -> RecoveryDirective {
    RecoveryDirective::ReadStatus {
        reference: BoundedPublicValue::parse("onecrew_connection_list")
            .expect("fixed public meta-tool reference is valid"),
    }
}

/// Direct search never carries an unrelated confirmed-plan reference.
pub(super) fn validate_search_policy(
    policy: SearchPlanPolicy,
    confirmed_plan_ref: Option<&str>,
) -> Result<(), ()> {
    if policy == SearchPlanPolicy::UserRequestedDirect && confirmed_plan_ref.is_some() {
        return Err(());
    }
    Ok(())
}

/// Normalize the already validated wire arguments for an exact plan scope.
pub(super) fn scope_arguments<T: serde::Serialize>(
    input: &T,
    count: u8,
    cursor: Option<&str>,
    purpose: crate::operation_registry::execution::SearchScopePurpose,
) -> Result<serde_json::Map<String, serde_json::Value>, ()> {
    if matches!(
        purpose,
        crate::operation_registry::execution::SearchScopePurpose::InitialPlan
    ) && cursor.is_some()
    {
        return Err(());
    }
    let mut arguments = serde_json::to_value(input)
        .map_err(|_| ())?
        .as_object()
        .cloned()
        .ok_or(())?;
    arguments.remove("cursor");
    arguments.remove("confirmed_plan_ref");
    arguments.remove("search_plan_policy");
    arguments.insert("count".into(), serde_json::json!(count));
    Ok(arguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_plan_refusal_is_known_no_effect_and_recovers_through_a_new_plan() {
        for reason in SearchPlanApprovalError::ALL {
            assert_eq!(
                search_plan_refusal(reason),
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse(reason.code()).unwrap(),
                    false,
                    Some(RecoveryDirective::PrepareSearchPlan {
                        operation_id: BoundedPublicValue::parse(
                            onecrew_local_runtime::OperationId::ONECREW_SEARCH_PLAN_PREPARE
                                .as_str()
                        )
                        .unwrap(),
                    }),
                )
            );
        }
    }
}
