//! Bounded public failures preserve owner effect and recovery classification.
use crate::operation_registry::{approval::ApprovalFailure, descriptor::EffectClass};
use onecrew_local_runtime::{DmDiagnostic, DmFailure};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClassifiedOwnerFailure {
    code: PublicFailureCode,
    effect: OwnerEffectState,
    retry_safe: bool,
    recovery: Option<RecoveryDirective>,
    provider_status: Option<u16>,
    provider_evidence: Option<Box<DmDiagnostic>>,
    message: Option<BoundedPublicValue>,
    dm_failure: Option<Box<DmFailure>>,
}

impl ClassifiedOwnerFailure {
    pub(crate) fn known_no_effect(
        code: PublicFailureCode,
        retry_safe: bool,
        recovery: Option<RecoveryDirective>,
    ) -> Self {
        Self {
            code,
            effect: OwnerEffectState::KnownNoEffect,
            retry_safe,
            recovery,
            provider_status: None,
            provider_evidence: None,
            message: None,
            dm_failure: None,
        }
    }

    /// The owner proved its external effect applied and only later work
    /// failed. Retrying would duplicate the effect, so it is never retry-safe.
    pub(crate) fn applied(code: PublicFailureCode, recovery: Option<RecoveryDirective>) -> Self {
        Self {
            code,
            effect: OwnerEffectState::Applied,
            retry_safe: false,
            recovery,
            provider_status: None,
            provider_evidence: None,
            message: None,
            dm_failure: None,
        }
    }

    pub(crate) fn unknown(code: PublicFailureCode, recovery: Option<RecoveryDirective>) -> Self {
        Self {
            code,
            effect: OwnerEffectState::Unknown,
            retry_safe: false,
            recovery,
            provider_status: None,
            provider_evidence: None,
            message: None,
            dm_failure: None,
        }
    }

    pub(crate) fn with_provider_status(mut self, provider_status: Option<u16>) -> Self {
        self.provider_status = provider_status;
        self
    }

    /// Carry a platform's protocol-native evidence of a failed read without
    /// reclassifying its effects.
    pub(crate) fn with_provider_evidence(mut self, evidence: DmDiagnostic) -> Self {
        self.provider_evidence = Some(Box::new(evidence));
        self
    }

    /// Carry the owner's safe DM evidence without reclassifying its effects.
    pub(crate) fn with_dm_failure(mut self, failure: DmFailure) -> Self {
        self.dm_failure = Some(Box::new(failure));
        self
    }

    /// Attach a toolkit's already-redacted `safe_message` (ADR 0015). The
    /// text is bounded here: control characters become spaces and the value
    /// is cut to the public bound; an empty result attaches nothing.
    pub(crate) fn with_message(mut self, message: &str) -> Self {
        self.message = BoundedPublicValue::bounded(message);
        self
    }

    pub(crate) fn after_local_commit(mut self) -> Self {
        if self.effect != OwnerEffectState::Unknown {
            self.effect = OwnerEffectState::Applied;
        }
        self.retry_safe = false;
        self
    }

    #[cfg(test)]
    pub(crate) fn recovery(&self) -> Option<&RecoveryDirective> {
        self.recovery.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct PublicFailureCode(String);

impl PublicFailureCode {
    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, InvalidPublicFailureCode> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        valid.then_some(Self(value)).ok_or(InvalidPublicFailureCode)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl JsonSchema for PublicFailureCode {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PublicFailureCode".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 64,
            "pattern": "^[a-z0-9_]+$"
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvalidPublicFailureCode;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "variants are consumed by Serialize and JsonSchema for the public recovery contract"
)]
pub(crate) enum RecoveryDirective {
    PrepareSearchPlan {
        operation_id: BoundedPublicValue,
    },
    CorrectInput,
    Confirm {
        parameter: BoundedPublicValue,
    },
    ReuseIdempotencyKey,
    ReadStatus {
        reference: BoundedPublicValue,
    },
    Reconnect,
    RetryLater {
        minimum_delay_seconds: u32,
        max_attempts: u8,
    },
    ContactSupport {
        audit_id: BoundedPublicValue,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct BoundedPublicValue(String);

impl BoundedPublicValue {
    const MAX_BYTES: usize = 256;

    pub(crate) fn parse(value: impl Into<String>) -> Result<Self, InvalidBoundedPublicValue> {
        let value = value.into();
        let valid = !value.trim().is_empty()
            && value.len() <= Self::MAX_BYTES
            && !value.chars().any(char::is_control);
        valid
            .then_some(Self(value))
            .ok_or(InvalidBoundedPublicValue)
    }

    /// Free text fitted to the bound: control characters become spaces, the
    /// text is trimmed and cut on a character boundary. `None` when nothing
    /// printable remains.
    pub(crate) fn bounded(value: &str) -> Option<Self> {
        let cleaned = value
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>();
        let trimmed = cleaned.trim();
        let mut end = trimmed.len().min(Self::MAX_BYTES);
        while !trimmed.is_char_boundary(end) {
            end -= 1;
        }
        Self::parse(trimmed[..end].trim_end()).ok()
    }

    #[cfg(test)]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl JsonSchema for BoundedPublicValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "BoundedPublicValue".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 256
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvalidBoundedPublicValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerEffectState {
    KnownNoEffect,
    Applied,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecuteFailurePhase {
    BeforeInvocation,
    AfterInvocation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExecuteFailureKind {
    UnknownOperation,
    SurfaceDenied,
    SchemaRevisionMismatch,
    ConnectionMismatch,
    InputSchemaRejected,
    TypedDecodeRejected,
    Approval(ApprovalFailure),
    OwnerRejected {
        code: PublicFailureCode,
        effect: OwnerEffectState,
        recovery: Option<RecoveryDirective>,
        provider_status: Option<u16>,
        provider_evidence: Option<Box<DmDiagnostic>>,
        message: Option<BoundedPublicValue>,
        dm_failure: Option<Box<DmFailure>>,
    },
    OutputSerializationFailed,
    OutputContractFailed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecuteFailure {
    pub(crate) phase: ExecuteFailurePhase,
    pub(crate) kind: ExecuteFailureKind,
    pub(crate) retry_safe: bool,
    pub(crate) effect_may_have_occurred: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecuteFailurePresentation {
    pub(crate) code: PublicFailureCode,
    pub(crate) phase: ExecuteFailurePhase,
    pub(crate) retry_safe: bool,
    pub(crate) effect: ExecuteEffectState,
    pub(crate) recovery: Option<RecoveryDirective>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_status: Option<u16>,
    /// The platform's protocol-native evidence of a failed platform read, in
    /// the direct-message diagnostic shape; never an effect or retry
    /// classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_evidence: Option<Box<DmDiagnostic>>,
    /// The owner's already-safe explanation, when one is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<BoundedPublicValue>,
    /// Evidence from the DM owner; never an effect or retry classification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dm_failure: Option<Box<DmFailure>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExecuteEffectState {
    NotInvoked,
    KnownNoEffect,
    Applied,
    Unknown,
}

pub(crate) fn project_classified_owner_failure(
    failure: ClassifiedOwnerFailure,
) -> ExecuteFailurePresentation {
    project_execute_failure(&ExecuteFailure::owner(failure))
}

pub(crate) fn project_execute_failure(failure: &ExecuteFailure) -> ExecuteFailurePresentation {
    let message = match &failure.kind {
        ExecuteFailureKind::OwnerRejected { message, .. } => message.clone(),
        _ => None,
    };
    let dm_failure = match &failure.kind {
        ExecuteFailureKind::OwnerRejected { dm_failure, .. } => dm_failure.clone(),
        _ => None,
    };
    let provider_evidence = match &failure.kind {
        ExecuteFailureKind::OwnerRejected {
            provider_evidence, ..
        } => provider_evidence.clone(),
        _ => None,
    };
    let (code, effect, recovery, provider_status) = match &failure.kind {
        ExecuteFailureKind::UnknownOperation => (
            static_failure_code("unknown_operation"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::SurfaceDenied => (
            static_failure_code("surface_denied"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::SchemaRevisionMismatch => (
            static_failure_code("schema_revision_mismatch"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::ConnectionMismatch => (
            static_failure_code("connection_mismatch"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::InputSchemaRejected => (
            static_failure_code("input_schema_rejected"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::TypedDecodeRejected => (
            static_failure_code("typed_decode_rejected"),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::Approval(approval) => (
            approval_failure_code(*approval),
            ExecuteEffectState::NotInvoked,
            None,
            None,
        ),
        ExecuteFailureKind::OwnerRejected {
            code,
            effect,
            recovery,
            provider_status,
            ..
        } => (
            code.clone(),
            match effect {
                OwnerEffectState::KnownNoEffect => ExecuteEffectState::KnownNoEffect,
                OwnerEffectState::Applied => ExecuteEffectState::Applied,
                OwnerEffectState::Unknown => ExecuteEffectState::Unknown,
            },
            recovery.clone(),
            *provider_status,
        ),
        ExecuteFailureKind::OutputSerializationFailed => (
            static_failure_code("output_serialization_failed"),
            post_invocation_effect(failure),
            None,
            None,
        ),
        ExecuteFailureKind::OutputContractFailed => (
            static_failure_code("output_contract_failed"),
            post_invocation_effect(failure),
            None,
            None,
        ),
    };
    ExecuteFailurePresentation {
        code,
        phase: failure.phase,
        retry_safe: failure.retry_safe,
        effect,
        recovery,
        provider_status,
        provider_evidence,
        message,
        dm_failure,
    }
}

fn approval_failure_code(failure: ApprovalFailure) -> PublicFailureCode {
    static_failure_code(match failure {
        ApprovalFailure::PolicyInvalid => "approval_policy_invalid",
        ApprovalFailure::ChannelUnavailable => "approval_channel_unavailable",
    })
}

fn post_invocation_effect(failure: &ExecuteFailure) -> ExecuteEffectState {
    if failure.effect_may_have_occurred {
        ExecuteEffectState::Unknown
    } else {
        ExecuteEffectState::KnownNoEffect
    }
}

fn static_failure_code(code: &'static str) -> PublicFailureCode {
    PublicFailureCode::parse(code).expect("static execute failure code is bounded")
}

impl ExecuteFailure {
    pub(crate) fn unknown_operation() -> Self {
        Self::before(ExecuteFailureKind::UnknownOperation)
    }

    pub(super) fn before(kind: ExecuteFailureKind) -> Self {
        Self {
            phase: ExecuteFailurePhase::BeforeInvocation,
            kind,
            retry_safe: true,
            effect_may_have_occurred: false,
        }
    }

    pub(super) fn approval(failure: ApprovalFailure) -> Self {
        Self::before(ExecuteFailureKind::Approval(failure))
    }

    pub(super) fn owner(failure: ClassifiedOwnerFailure) -> Self {
        let effect_may_have_occurred = matches!(
            failure.effect,
            OwnerEffectState::Applied | OwnerEffectState::Unknown
        );
        Self {
            phase: ExecuteFailurePhase::AfterInvocation,
            kind: ExecuteFailureKind::OwnerRejected {
                code: failure.code,
                effect: failure.effect,
                recovery: failure.recovery,
                provider_status: failure.provider_status,
                provider_evidence: failure.provider_evidence,
                message: failure.message,
                dm_failure: failure.dm_failure,
            },
            retry_safe: failure.retry_safe,
            effect_may_have_occurred,
        }
    }

    pub(super) fn after(kind: ExecuteFailureKind, effect: EffectClass) -> Self {
        Self {
            phase: ExecuteFailurePhase::AfterInvocation,
            kind,
            retry_safe: false,
            effect_may_have_occurred: effect.may_have_effect(),
        }
    }
}
