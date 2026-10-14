use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, AuditMode, ConfirmationMode, ConfirmationSafety, ConnectionRequirement,
        EffectClass, EntryId, IdempotencyMode, IdempotencySafety, InstancePointer,
        OperationRegistration, OperationSafety, ReadbackMode, ReadbackSafety, SecretClass,
        SurfaceId, SurfaceSet,
    },
    execution::{ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode},
    registration::OperationPolicy,
};

pub(super) const CREATOR_SURFACES: [SurfaceId; 3] =
    [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators];

pub(super) enum ConfirmationSpec {
    None,
    Attended(&'static str),
    AttendedAndOwner {
        prompt: &'static str,
        pointers: &'static [&'static str],
    },
}

pub(super) enum IdempotencySpec {
    None,
    Input(&'static [&'static str]),
}

pub(super) enum ReadbackSpec {
    None,
    Inline,
    InlineAndFollowUp(&'static [&'static str]),
}

pub(super) struct SafetySpec {
    pub(super) confirmation: ConfirmationSpec,
    pub(super) idempotency: IdempotencySpec,
    pub(super) readback: ReadbackSpec,
    pub(super) audit: AuditMode,
    pub(super) warning_pointers: &'static [&'static str],
    pub(super) recovery_pointers: &'static [&'static str],
}

impl SafetySpec {
    pub(super) const fn local() -> Self {
        Self {
            confirmation: ConfirmationSpec::None,
            idempotency: IdempotencySpec::None,
            readback: ReadbackSpec::None,
            audit: AuditMode::None,
            warning_pointers: &[],
            recovery_pointers: &[],
        }
    }
}

pub(super) fn metadata(
    id: onecrew_local_runtime::OperationId,
    domain_id: &str,
    surfaces: impl IntoIterator<Item = SurfaceId>,
) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: domain_id.to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new(surfaces).expect("OneCrew write operations have fixed surfaces"),
    }
}

pub(super) fn policy(effect: EffectClass, spec: SafetySpec) -> OperationPolicy {
    let (approval, confirmation_mode, confirmation_pointers, approval_request) =
        match spec.confirmation {
            ConfirmationSpec::None => (
                ApprovalClass::OwnerGuarded,
                ConfirmationMode::None,
                vec![],
                None,
            ),
            ConfirmationSpec::Attended(prompt) => (
                ApprovalClass::AttendedPrompt,
                ConfirmationMode::AttendedPrompt,
                vec![],
                Some(static_prompt(prompt)),
            ),
            ConfirmationSpec::AttendedAndOwner { prompt, pointers } => (
                ApprovalClass::AttendedPrompt,
                ConfirmationMode::AttendedPromptAndOwnerParameters,
                pointers_of(pointers),
                Some(static_prompt(prompt)),
            ),
        };
    let (idempotency_mode, idempotency_pointers) = match spec.idempotency {
        IdempotencySpec::None => (IdempotencyMode::None, Vec::new()),
        IdempotencySpec::Input(pointers) => {
            (IdempotencyMode::OwnerInputIdentity, pointers_of(pointers))
        }
    };
    let (readback_mode, readback_operation_ids) = match spec.readback {
        ReadbackSpec::None => (ReadbackMode::None, Vec::new()),
        ReadbackSpec::Inline => (ReadbackMode::InlineOwnerVerified, Vec::new()),
        ReadbackSpec::InlineAndFollowUp(ids) => (
            ReadbackMode::InlineAndFollowUp,
            ids.iter().map(|id| entry_id(id)).collect(),
        ),
    };
    let warning_output_pointers = pointers_of(spec.warning_pointers);
    let recovery_output_pointers = pointers_of(spec.recovery_pointers);
    OperationPolicy {
        effect,
        secret: SecretClass::None,
        approval,
        connection: ConnectionRequirement::None,
        sensitive_input_paths: Vec::new(),
        view_kind: crate::operation_registry::descriptor::OperationViewKind::SimpleOperation,
        approval_request,
        safety: OperationSafety {
            confirmation: ConfirmationSafety {
                mode: confirmation_mode,
                parameter_pointers: confirmation_pointers,
            },
            idempotency: IdempotencySafety {
                mode: idempotency_mode,
                identity_pointers: idempotency_pointers,
            },
            readback: ReadbackSafety {
                mode: readback_mode,
                operation_ids: readback_operation_ids,
            },
            audit: spec.audit,
            warnings_present: !warning_output_pointers.is_empty(),
            recovery_present: !recovery_output_pointers.is_empty(),
            warning_output_pointers,
            recovery_output_pointers,
        },
        execution: ExecutionDeclaration::Sequential,
    }
}

/// Only V2's validated call path carries the server-owned review context. A
/// rejection here happens before the review is minted, so nothing was written.
pub(super) fn review_context(
    context: crate::operation_registry::execution::BindingContext,
) -> Result<onecrew_local_runtime::OwnerReviewContext, String> {
    context
        .owner_review_context()
        .map_err(|error| error.to_string())
}

/// A review that was never minted wrote nothing: preparation runs before the
/// idempotency claim, the fingerprint, and every per-item write. Reporting it as
/// unknown would tell an Agent that a batch which never started might have
/// created Creators, and invite it to check or retry against nothing.
pub(super) fn classify_prepare_rejection(
    code: &'static str,
) -> impl Fn(String) -> ClassifiedOwnerFailure {
    move |_error| {
        ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse(code).expect("static Creator write failure code is valid"),
            true,
            None,
        )
    }
}

pub(super) fn classify_string_failure(
    code: &'static str,
) -> impl Fn(String) -> ClassifiedOwnerFailure {
    move |_error| {
        ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse(code).expect("static Creator write failure code is valid"),
            None,
        )
    }
}

/// A lost blocking-pool result during a Creator group read or write maps to
/// the owner's own internal variant, so both group classifiers (`onecrew_reads`
/// and this domain) keep projecting it as the existing internal failure.
impl From<crate::operation_registry::execution::BlockingTaskFailure>
    for onecrew_creator_runtime::CreatorGroupError
{
    fn from(_failure: crate::operation_registry::execution::BlockingTaskFailure) -> Self {
        Self::Internal
    }
}

pub(super) fn classify_creator_group_failure(
    error: onecrew_creator_runtime::CreatorGroupError,
) -> ClassifiedOwnerFailure {
    use onecrew_creator_runtime::CreatorGroupError;

    let code = match error {
        CreatorGroupError::InvalidInput => "creator_group_invalid_input",
        CreatorGroupError::DuplicateName => "creator_group_duplicate_name",
        CreatorGroupError::GroupNotFound => "creator_group_not_found",
        CreatorGroupError::CreatorNotFound => "creator_not_found",
        CreatorGroupError::RevisionConflict => "creator_group_revision_conflict",
        CreatorGroupError::Internal => {
            return ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse("creator_group_outcome_unknown")
                    .expect("static Creator group failure code is valid"),
                None,
            );
        }
    };
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static Creator group failure code is valid"),
        false,
        None,
    )
}

fn entry_id(value: &str) -> EntryId {
    EntryId::parse(value).expect("static readback operation ID is valid")
}

fn pointers_of(values: &[&str]) -> Vec<InstancePointer> {
    values
        .iter()
        .map(|value| {
            InstancePointer::parse(*value).expect("static operation safety pointer is valid")
        })
        .collect()
}

fn static_prompt(message: &'static str) -> ApprovalRequest {
    ApprovalRequest::new(message).expect("static attended prompt is valid")
}
