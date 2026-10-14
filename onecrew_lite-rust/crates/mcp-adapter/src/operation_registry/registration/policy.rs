//! Registration-time execution, approval, secret, and safety policy.
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{
        ApprovalClass, ConnectionRequirement, EffectClass, OperationSafety, OperationViewKind,
        SecretClass,
    },
    execution::ExecutionDeclaration,
};

pub(crate) struct OperationPolicy {
    pub(crate) effect: crate::operation_registry::descriptor::EffectClass,
    pub(crate) secret: crate::operation_registry::descriptor::SecretClass,
    pub(crate) approval: ApprovalClass,
    pub(crate) connection: crate::operation_registry::descriptor::ConnectionRequirement,
    pub(crate) sensitive_input_paths: Vec<String>,
    pub(crate) view_kind: OperationViewKind,
    pub(crate) approval_request: Option<ApprovalRequest>,
    pub(crate) safety: OperationSafety,
    pub(crate) execution: ExecutionDeclaration,
}

impl OperationPolicy {
    /// Build a policy with an explicit closed execution declaration. The
    /// declaration is copied into the sealed typed binding and validated
    /// against effect, approval, secret, and output classes before sealing.
    pub(crate) fn read_only(
        effect: EffectClass,
        connection: ConnectionRequirement,
        execution: ExecutionDeclaration,
    ) -> Self {
        assert!(
            !effect.may_have_effect(),
            "read-only operation policy requires a read effect"
        );
        Self {
            effect,
            secret: SecretClass::None,
            approval: ApprovalClass::None,
            connection,
            sensitive_input_paths: Vec::new(),
            view_kind: OperationViewKind::SimpleOperation,
            approval_request: None,
            safety: OperationSafety::read_only(),
            execution,
        }
    }

    pub(crate) fn with_view_kind(mut self, view_kind: OperationViewKind) -> Self {
        self.view_kind = view_kind;
        self
    }
}
