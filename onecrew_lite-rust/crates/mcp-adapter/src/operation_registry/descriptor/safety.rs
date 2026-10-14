use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{EntryId, InstancePointer};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationSafety {
    pub(crate) confirmation: ConfirmationSafety,
    pub(crate) idempotency: IdempotencySafety,
    pub(crate) readback: ReadbackSafety,
    pub(crate) audit: AuditMode,
    pub(crate) warnings_present: bool,
    pub(crate) recovery_present: bool,
    pub(crate) warning_output_pointers: Vec<InstancePointer>,
    pub(crate) recovery_output_pointers: Vec<InstancePointer>,
}

impl OperationSafety {
    pub(crate) fn read_only() -> Self {
        Self {
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
        }
    }

    pub(crate) fn summary(&self) -> OperationSafetySummary {
        OperationSafetySummary {
            confirmation_mode: self.confirmation.mode,
            confirmation_parameters_present: !self.confirmation.parameter_pointers.is_empty(),
            idempotency_mode: self.idempotency.mode,
            idempotency_identity_present: !self.idempotency.identity_pointers.is_empty(),
            readback_mode: self.readback.mode,
            readback_operations_present: !self.readback.operation_ids.is_empty(),
            audit_mode: self.audit,
            warnings_present: self.warnings_present,
            recovery_present: self.recovery_present,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfirmationSafety {
    pub(crate) mode: ConfirmationMode,
    pub(crate) parameter_pointers: Vec<InstancePointer>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfirmationMode {
    None,
    OwnerParameters,
    AttendedPrompt,
    AttendedPromptAndOwnerParameters,
}

impl ConfirmationMode {
    pub(crate) fn has_owner_parameters(self) -> bool {
        matches!(
            self,
            Self::OwnerParameters | Self::AttendedPromptAndOwnerParameters
        )
    }

    pub(crate) fn has_attended_prompt(self) -> bool {
        matches!(
            self,
            Self::AttendedPrompt | Self::AttendedPromptAndOwnerParameters
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdempotencySafety {
    pub(crate) mode: IdempotencyMode,
    pub(crate) identity_pointers: Vec<InstancePointer>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IdempotencyMode {
    None,
    OwnerRequestId,
    OwnerInputIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadbackSafety {
    pub(crate) mode: ReadbackMode,
    pub(crate) operation_ids: Vec<EntryId>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReadbackMode {
    None,
    InlineOwnerVerified,
    FollowUpOperations,
    InlineAndFollowUp,
}

impl ReadbackMode {
    pub(crate) fn has_follow_up_operations(self) -> bool {
        matches!(self, Self::FollowUpOperations | Self::InlineAndFollowUp)
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuditMode {
    None,
    OwnerRecorded,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OperationSafetySummary {
    pub(crate) confirmation_mode: ConfirmationMode,
    pub(crate) confirmation_parameters_present: bool,
    pub(crate) idempotency_mode: IdempotencyMode,
    pub(crate) idempotency_identity_present: bool,
    pub(crate) readback_mode: ReadbackMode,
    pub(crate) readback_operations_present: bool,
    pub(crate) audit_mode: AuditMode,
    pub(crate) warnings_present: bool,
    pub(crate) recovery_present: bool,
}
