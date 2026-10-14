use onecrew_local_runtime::ToolkitId;
use std::sync::Arc;

use crate::operation_registry::approval::ApprovalRequest;
use onecrew_local_runtime::{
    AccountConnectionOperationInput, BrowserAccountConnectionCompleteInput,
    BrowserAccountConnectionStartInput, BrowserAccountConnectionStatus, ClearXChatPasscodeInput,
    DirectAccountConnectionCancelInput, OperatorAccountConnectionBeginInput,
    OperatorAccountConnectionCancelInput, OperatorAccountConnectionFinalizeInput,
    OperatorAccountConnectionIntent, OperatorAccountConnectionMethod,
    OperatorAccountConnectionPrepareInput, OperatorAccountConnectionStatus,
    OperatorAccountConnectionStatusInput, OperatorAccountDisconnectInput,
    OperatorAccountDisconnectOutcome, OperatorAccountPlatform, OperatorAccountProjection,
    SetXChatPasscodeInput, TelegramAccountConnectionOtpInput, TelegramAccountConnectionStartInput,
    TelegramAccountConnectionStatus, TelegramAccountConnectionTwoFactorInput,
};

use super::super::RegistryOwners;
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
        EffectClass, IdempotencyMode, IdempotencySafety, InstancePointer, OperationRegistration,
        OperationSafety, ReadbackMode, ReadbackSafety, SecretClass, SurfaceId, SurfaceSet,
    },
    execution::{
        ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode, RecoveryDirective,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 2] = [SurfaceId::Full, SurfaceId::Accounts];

#[derive(Debug)]
#[allow(
    dead_code,
    reason = "raw owner text is deliberately ignored when lifecycle failures are projected as secret-free codes"
)]
enum LifecycleError {
    WrongKind,
    OutreachInProgress,
    Preflight(String),
    Read(String),
    Effect(String),
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    register_browser(
        builder,
        owners.clone(),
        ToolkitId::X,
        OperatorAccountPlatform::X,
    )?;
    register_browser(
        builder,
        owners.clone(),
        ToolkitId::Instagram,
        OperatorAccountPlatform::Instagram,
    )?;
    register_telegram(builder, owners.clone())?;
    register_account_management(builder, owners.clone())?;
    register_reconnect(
        builder,
        owners.clone(),
        ToolkitId::X,
        OperatorAccountPlatform::X,
        OperatorAccountConnectionMethod::Browser,
    )?;
    register_reconnect(
        builder,
        owners.clone(),
        ToolkitId::Instagram,
        OperatorAccountPlatform::Instagram,
        OperatorAccountConnectionMethod::Browser,
    )?;
    register_reconnect(
        builder,
        owners,
        ToolkitId::Telegram,
        OperatorAccountPlatform::Telegram,
        OperatorAccountConnectionMethod::ExistingAccount,
    )
}

fn register_browser(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
    toolkit: ToolkitId,
    expected: OperatorAccountPlatform,
) -> Result<(), RegistryBuildError> {
    let status_id = match toolkit {
        ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS.as_str(),
        ToolkitId::Instagram => {
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_STATUS.as_str()
        }
        _ => unreachable!(),
    };
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<BrowserAccountConnectionStartInput, BrowserAccountConnectionStatus, LifecycleError, _, _, _>(
        metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_START, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_START, _ => unreachable!() }),
        policy(EffectClass::ExternalStep, SecretClass::Transient, &["/proxy/password", "/x_chat_passcode"], &[], &["/idempotency_key"]),
        move |_context, input| { let owner = Arc::clone(&owner); async move { if input.platform != expected { return Err(LifecycleError::WrongKind); } owner.browser_start(input).await.map_err(LifecycleError::Effect) } }, classify_with(status_id))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<AccountConnectionOperationInput, BrowserAccountConnectionStatus, LifecycleError, _, _, _>(metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_STATUS, _ => unreachable!() }), read_policy(), move |_context,input| { let owner=Arc::clone(&owner); async move { let out=owner.browser_status(input).await.map_err(LifecycleError::Read)?; (out.platform==expected).then_some(out).ok_or(LifecycleError::WrongKind) }}, classify_with(status_id))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<BrowserAccountConnectionCompleteInput, BrowserAccountConnectionStatus, LifecycleError, _, _, _>(metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_COMPLETE, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_COMPLETE, _ => unreachable!() }), policy(EffectClass::ExternalStep, SecretClass::None, &[], &["/login_completed"], &["/idempotency_key"]), move |_context,input| { let owner=Arc::clone(&owner); async move { let status=owner.browser_status(AccountConnectionOperationInput{operation_id:input.operation_id.clone()}).await.map_err(LifecycleError::Preflight)?; if status.platform!=expected{return Err(LifecycleError::WrongKind)} owner.browser_complete(input).await.map_err(LifecycleError::Effect) }}, classify_with(status_id))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<DirectAccountConnectionCancelInput, BrowserAccountConnectionStatus, LifecycleError, _, _, _>(metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_CANCEL, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_BROWSER_CANCEL, _ => unreachable!() }), policy(EffectClass::ExternalStep, SecretClass::None, &[], &["/confirmed"], &[]), move |_context,input| { let owner=Arc::clone(&owner); async move { let status=owner.browser_status(AccountConnectionOperationInput{operation_id:input.operation_id.clone()}).await.map_err(LifecycleError::Preflight)?; if status.platform!=expected{return Err(LifecycleError::WrongKind)} owner.browser_cancel(input).await.map_err(LifecycleError::Effect) }}, classify_with(status_id))
}

fn register_telegram(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<TelegramAccountConnectionStartInput,TelegramAccountConnectionStatus,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_START),policy(EffectClass::ExternalStep,SecretClass::Transient,&["/api_hash","/proxy/password","/phone"],&[],&["/idempotency_key"]),move |_c,i|{let o=Arc::clone(&owner);async move{o.telegram_start(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<AccountConnectionOperationInput,TelegramAccountConnectionStatus,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS),read_policy(),move |_c,i|{let o=Arc::clone(&owner);async move{o.telegram_status(i).await.map_err(LifecycleError::Read)}},classify_with(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<TelegramAccountConnectionOtpInput,TelegramAccountConnectionStatus,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_OTP_SUBMIT),policy(EffectClass::ExternalStep,SecretClass::Transient,&["/otp"],&[],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{o.telegram_submit_otp(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<TelegramAccountConnectionTwoFactorInput,TelegramAccountConnectionStatus,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT),policy(EffectClass::ExternalStep,SecretClass::Transient,&["/password"],&[],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{o.telegram_submit_two_factor(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<DirectAccountConnectionCancelInput,TelegramAccountConnectionStatus,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_CANCEL),policy(EffectClass::ExternalStep,SecretClass::None,&[],&["/confirmed"],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{o.telegram_cancel(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()))
}

fn register_account_management(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<SetXChatPasscodeInput,OperatorAccountProjection,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::X_CHAT_PASSCODE_SET),policy(EffectClass::WriteLocal,SecretClass::VaultBound,&["/x_chat_passcode"],&[],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{o.set_x_chat_passcode(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str()))?;
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<ClearXChatPasscodeInput,OperatorAccountProjection,LifecycleError,_,_,_>(metadata(onecrew_local_runtime::OperationId::X_CHAT_PASSCODE_CLEAR),policy(EffectClass::DeleteLocal,SecretClass::None,&[],&["/confirmed"],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{o.clear_x_chat_passcode(i).await.map_err(LifecycleError::Effect)}},classify_with(onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str()))?;
    for (toolkit, expected) in [
        (ToolkitId::X, OperatorAccountPlatform::X),
        (ToolkitId::Instagram, OperatorAccountPlatform::Instagram),
        (ToolkitId::Telegram, OperatorAccountPlatform::Telegram),
    ] {
        let owner = Arc::clone(&owners.operator_accounts);
        let detail_id = match toolkit {
            ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str(),
            ToolkitId::Instagram => {
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_GET.as_str()
            }
            ToolkitId::Telegram => {
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_GET.as_str()
            }
            _ => unreachable!(),
        };
        builder.register_operation::<OperatorAccountDisconnectInput,OperatorAccountDisconnectOutcome,LifecycleError,_,_,_>(metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONNECTION_DISCONNECT, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_DISCONNECT, ToolkitId::Telegram => onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_DISCONNECT, _ => unreachable!() }),policy(EffectClass::DeleteLocal,SecretClass::None,&[],&["/confirmed"],&["/idempotency_key"]),move |_c,i|{let o=Arc::clone(&owner);async move{let account=o.list().await.map_err(LifecycleError::Preflight)?.into_iter().find(|a|a.operator_account_id==i.operator_account_id).ok_or(LifecycleError::WrongKind)?;if account.platform!=expected{return Err(LifecycleError::WrongKind)}o.disconnect(i).await.map_err(|error| match error { onecrew_local_runtime::OperatorAccountDisconnectError::OutreachInProgress => LifecycleError::OutreachInProgress, _ => LifecycleError::Effect(error.safe_message().to_owned()) })}},classify_with(detail_id))?;
    }
    Ok(())
}

fn register_reconnect(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
    toolkit: ToolkitId,
    platform: OperatorAccountPlatform,
    method: OperatorAccountConnectionMethod,
) -> Result<(), RegistryBuildError> {
    let [
        prepare_id,
        begin_id,
        status_operation_id,
        finalize_id,
        cancel_id,
    ] = match toolkit {
        ToolkitId::X => [
            onecrew_local_runtime::OperationId::X_CONNECTION_RECONNECT_PREPARE,
            onecrew_local_runtime::OperationId::X_CONNECTION_RECONNECT_BEGIN,
            onecrew_local_runtime::OperationId::X_CONNECTION_RECONNECT_STATUS,
            onecrew_local_runtime::OperationId::X_CONNECTION_RECONNECT_FINALIZE,
            onecrew_local_runtime::OperationId::X_CONNECTION_RECONNECT_CANCEL,
        ],
        ToolkitId::Instagram => [
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_RECONNECT_PREPARE,
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_RECONNECT_BEGIN,
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_RECONNECT_STATUS,
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_RECONNECT_FINALIZE,
            onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_RECONNECT_CANCEL,
        ],
        ToolkitId::Telegram => [
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_RECONNECT_PREPARE,
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_RECONNECT_BEGIN,
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_RECONNECT_STATUS,
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_RECONNECT_FINALIZE,
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_RECONNECT_CANCEL,
        ],
        _ => unreachable!(),
    };
    let status_id = status_operation_id.as_str();
    let owner = Arc::clone(&owners.operator_accounts);
    builder.register_operation::<OperatorAccountConnectionPrepareInput,OperatorAccountConnectionStatus,LifecycleError,_,_,_>(metadata(prepare_id),policy(EffectClass::WriteLocal,SecretClass::None,&[],&[],&[]),move |_c,i|{let o=Arc::clone(&owner);async move{ensure_prepare(&i,platform,method)?;o.connection_prepare(i).await.map_err(LifecycleError::Effect)}},classify_with(status_id))?;
    macro_rules! continuation { ($id:expr,$input:ty,$method_name:ident,$effect:expr,$confirm:expr,$idem:expr) => {{ let owner=Arc::clone(&owners.operator_accounts); builder.register_operation::<$input,OperatorAccountConnectionStatus,LifecycleError,_,_,_>(metadata($id),policy($effect,SecretClass::None,&[],$confirm,$idem),move |_c,i|{let o=Arc::clone(&owner);async move{let flow_id=i.flow_id.clone();let current=o.connection_status(OperatorAccountConnectionStatusInput{flow_id}).await.map_err(LifecycleError::Preflight)?;ensure_status(&current,platform,method)?;o.$method_name(i).await.map_err(LifecycleError::Effect)}},classify_with(status_id))?; }} }
    continuation!(
        begin_id,
        OperatorAccountConnectionBeginInput,
        connection_begin,
        EffectClass::ExternalStep,
        &["/confirmed"],
        &["/idempotency_key"]
    );
    continuation!(
        status_operation_id,
        OperatorAccountConnectionStatusInput,
        connection_status,
        EffectClass::ReadLocal,
        &[],
        &[]
    );
    continuation!(
        finalize_id,
        OperatorAccountConnectionFinalizeInput,
        connection_finalize,
        EffectClass::ExternalStep,
        &["/confirmed"],
        &["/idempotency_key"]
    );
    continuation!(
        cancel_id,
        OperatorAccountConnectionCancelInput,
        connection_cancel,
        EffectClass::ExternalStep,
        &["/confirmed"],
        &[]
    );
    Ok(())
}

fn ensure_prepare(
    i: &OperatorAccountConnectionPrepareInput,
    p: OperatorAccountPlatform,
    m: OperatorAccountConnectionMethod,
) -> Result<(), LifecycleError> {
    (i.platform == p && i.intent == OperatorAccountConnectionIntent::Reconnect && i.method == m)
        .then_some(())
        .ok_or(LifecycleError::WrongKind)
}
fn ensure_status(
    i: &OperatorAccountConnectionStatus,
    p: OperatorAccountPlatform,
    m: OperatorAccountConnectionMethod,
) -> Result<(), LifecycleError> {
    (i.platform == p && i.intent == OperatorAccountConnectionIntent::Reconnect && i.method == m)
        .then_some(())
        .ok_or(LifecycleError::WrongKind)
}
fn metadata(id: onecrew_local_runtime::OperationId) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: "accounts".into(),
        aliases: vec![],
        surfaces: SurfaceSet::new(SURFACES).expect("fixed surfaces"),
    }
}
fn read_policy() -> OperationPolicy {
    OperationPolicy::read_only(
        EffectClass::ReadLocal,
        ConnectionRequirement::None,
        ExecutionDeclaration::Sequential,
    )
}
fn pointer(v: &str) -> InstancePointer {
    InstancePointer::parse(v).expect("static pointer")
}
fn policy(
    effect: EffectClass,
    secret: SecretClass,
    secrets: &[&str],
    confirms: &[&str],
    idems: &[&str],
) -> OperationPolicy {
    let attended = !confirms.is_empty();
    OperationPolicy {
        effect,
        secret,
        approval: if attended {
            ApprovalClass::AttendedPrompt
        } else {
            ApprovalClass::OwnerGuarded
        },
        connection: ConnectionRequirement::None,
        sensitive_input_paths: secrets.iter().map(|v| v.to_string()).collect(),
        view_kind: if secret == SecretClass::None {
            crate::operation_registry::descriptor::OperationViewKind::SimpleOperation
        } else {
            crate::operation_registry::descriptor::OperationViewKind::ReadOnly
        },
        approval_request: attended.then(|| {
            let prompt = if confirms.contains(&"/login_completed") {
                "Confirm that the attended browser login completed?"
            } else {
                "Confirm this operator account action?"
            };
            ApprovalRequest::new(prompt).expect("static operator approval prompt is valid")
        }),
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: if attended {
                    ConfirmationMode::AttendedPromptAndOwnerParameters
                } else {
                    ConfirmationMode::None
                },
                parameter_pointers: confirms.iter().map(|v| pointer(v)).collect(),
            },
            idempotency: IdempotencySafety {
                mode: if idems.is_empty() {
                    IdempotencyMode::None
                } else {
                    IdempotencyMode::OwnerRequestId
                },
                identity_pointers: idems.iter().map(|v| pointer(v)).collect(),
            },
            readback: ReadbackSafety {
                mode: ReadbackMode::InlineOwnerVerified,
                operation_ids: vec![],
            },
            audit: AuditMode::None,
            warnings_present: false,
            recovery_present: false,
            warning_output_pointers: vec![],
            recovery_output_pointers: vec![],
        },
        execution: ExecutionDeclaration::Sequential,
    }
}
fn classify_with(
    status_operation_id: &'static str,
) -> impl Fn(LifecycleError) -> ClassifiedOwnerFailure {
    move |error| classify(error, status_operation_id)
}

fn classify(e: LifecycleError, status_operation_id: &'static str) -> ClassifiedOwnerFailure {
    match e {
        LifecycleError::OutreachInProgress => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("outreach_action_unresolved").unwrap(),
            false,
            Some(RecoveryDirective::ReadStatus {
                reference: crate::operation_registry::execution::BoundedPublicValue::parse(
                    status_operation_id,
                )
                .unwrap(),
            }),
        )
        .with_message(
            onecrew_local_runtime::OperatorAccountDisconnectError::OutreachInProgress
                .safe_message(),
        ),
        LifecycleError::WrongKind => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("connection_kind_mismatch").unwrap(),
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        LifecycleError::Preflight(_) | LifecycleError::Read(_) => {
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse("operator_connection_failed").unwrap(),
                false,
                None,
            )
        }
        LifecycleError::Effect(_) => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("operator_connection_effect_unknown").unwrap(),
            Some(RecoveryDirective::ReadStatus {
                reference: crate::operation_registry::execution::BoundedPublicValue::parse(
                    status_operation_id,
                )
                .unwrap(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_outreach_blocks_disconnect_without_claiming_a_delete_effect() {
        let expected = ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("outreach_action_unresolved").unwrap(),
            false,
            Some(RecoveryDirective::ReadStatus {
                reference: crate::operation_registry::execution::BoundedPublicValue::parse(
                    onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str(),
                )
                .unwrap(),
            }),
        )
        .with_message(
            onecrew_local_runtime::OperatorAccountDisconnectError::OutreachInProgress
                .safe_message(),
        );
        assert_eq!(
            classify(
                LifecycleError::OutreachInProgress,
                onecrew_local_runtime::OperationId::X_CONNECTION_GET.as_str()
            ),
            expected
        );
    }

    #[test]
    fn reconnect_prepare_is_closed_over_toolkit_platform_intent_and_method() {
        let valid = OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Reconnect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: Some("opaque-account".into()),
            label: None,
        };
        assert!(
            ensure_prepare(
                &valid,
                OperatorAccountPlatform::X,
                OperatorAccountConnectionMethod::Browser
            )
            .is_ok()
        );
        assert!(
            ensure_prepare(
                &valid,
                OperatorAccountPlatform::Instagram,
                OperatorAccountConnectionMethod::Browser
            )
            .is_err()
        );
        assert!(
            ensure_prepare(
                &valid,
                OperatorAccountPlatform::X,
                OperatorAccountConnectionMethod::ExistingAccount
            )
            .is_err()
        );
    }

    #[test]
    fn secret_policies_name_only_exact_transient_or_vault_paths() {
        let telegram = policy(
            EffectClass::ExternalStep,
            SecretClass::Transient,
            &["/api_hash", "/proxy/password", "/phone"],
            &[],
            &["/idempotency_key"],
        );
        assert_eq!(
            telegram.sensitive_input_paths,
            ["/api_hash", "/proxy/password", "/phone"]
        );
        assert_eq!(telegram.secret, SecretClass::Transient);

        let passcode = policy(
            EffectClass::WriteLocal,
            SecretClass::VaultBound,
            &["/x_chat_passcode"],
            &[],
            &[],
        );
        assert_eq!(passcode.sensitive_input_paths, ["/x_chat_passcode"]);
        assert_eq!(passcode.secret, SecretClass::VaultBound);
    }

    #[test]
    fn confirmed_operator_actions_require_attended_prompt_and_owner_parameters() {
        let guarded_policy = policy(
            EffectClass::ExternalStep,
            SecretClass::None,
            &[],
            &["/confirmed"],
            &[],
        );
        assert_eq!(guarded_policy.approval, ApprovalClass::AttendedPrompt);
        assert_eq!(
            guarded_policy.safety.confirmation.mode,
            ConfirmationMode::AttendedPromptAndOwnerParameters
        );
        assert_eq!(
            guarded_policy.safety.confirmation.parameter_pointers,
            [pointer("/confirmed")]
        );
        assert!(guarded_policy.approval_request.is_some());

        let complete = policy(
            EffectClass::ExternalStep,
            SecretClass::None,
            &[],
            &["/login_completed"],
            &["/idempotency_key"],
        );
        assert_eq!(complete.approval, ApprovalClass::AttendedPrompt);
        assert_eq!(
            complete.safety.confirmation.mode,
            ConfirmationMode::AttendedPromptAndOwnerParameters
        );
        assert_eq!(
            complete.safety.confirmation.parameter_pointers,
            [pointer("/login_completed")]
        );
        assert!(complete.approval_request.is_some());
    }

    #[test]
    fn owner_failure_text_is_never_part_of_the_public_classification() {
        let failure = classify(
            LifecycleError::Effect("SECRET_SENTINEL".into()),
            onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS.as_str(),
        );
        assert!(!format!("{failure:?}").contains("SECRET_SENTINEL"));
    }

    #[test]
    fn post_invocation_failures_for_effectful_continuations_are_unknown_and_not_retry_safe() {
        for operation in [
            "browser.complete",
            "reconnect.finalize",
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_OTP_SUBMIT.as_str(),
            onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_TWO_FACTOR_SUBMIT
                .as_str(),
            "operator.disconnect",
            "connection.cancel",
        ] {
            let failure = classify(
                LifecycleError::Effect(format!("SECRET_SENTINEL:{operation}")),
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str(),
            );
            let projection = format!("{failure:?}");
            assert!(
                projection.contains("effect: Unknown"),
                "{operation}: {projection}"
            );
            assert!(
                projection.contains("retry_safe: false"),
                "{operation}: {projection}"
            );
            assert!(
                projection.contains("ReadStatus"),
                "{operation}: {projection}"
            );
            assert!(
                projection.contains(
                    onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_LOGIN_STATUS.as_str()
                ),
                "{operation}: {projection}"
            );
            assert!(!projection.contains("SECRET_SENTINEL"));
        }
    }

    #[test]
    fn adapter_preflight_and_read_failures_remain_known_no_effect() {
        for failure in [
            LifecycleError::WrongKind,
            LifecycleError::Preflight("SECRET_SENTINEL".into()),
            LifecycleError::Read("SECRET_SENTINEL".into()),
        ] {
            let projection = format!(
                "{:?}",
                classify(
                    failure,
                    onecrew_local_runtime::OperationId::X_CONNECTION_BROWSER_STATUS.as_str()
                )
            );
            assert!(projection.contains("effect: KnownNoEffect"));
            assert!(projection.contains("retry_safe: false"));
            assert!(!projection.contains("SECRET_SENTINEL"));
        }
    }
}
