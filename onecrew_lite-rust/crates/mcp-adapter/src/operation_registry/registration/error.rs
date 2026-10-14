//! Closed registration failures and schema pointer roles.
use crate::operation_registry::{descriptor::EntryId, schema::SchemaBuildError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistryBuildError {
    DuplicateId(EntryId),
    InvalidCatalogReference,
    UnknownCatalogOperation(EntryId),
    OperationNotMcpSupported(EntryId),
    MissingCatalogBinding(EntryId),
    OperationToolkitMismatch(EntryId),
    DuplicateAlias(String),
    InvalidApprovalPolicy,
    InvalidSecretPolicy,
    InvalidViewKind,
    InvalidSafetyPolicy,
    InvalidExecutionDeclaration,
    SafetyPointerOutOfBounds {
        field: &'static str,
    },
    DuplicateSafetyPointer {
        field: &'static str,
    },
    SafetyPointerMismatch {
        pointer: String,
        role: ParameterPointerRole,
    },
    UnknownReadbackOperation {
        operation_id: EntryId,
        referenced_id: EntryId,
    },
    ReadbackReferenceKindMismatch {
        operation_id: EntryId,
        referenced_id: EntryId,
    },
    ReadbackSurfaceMismatch {
        operation_id: EntryId,
        referenced_id: EntryId,
    },
    MissingInputSchema,
    MissingOutputSchema,
    MissingBinding,
    InvalidPlaybook(EntryId),
    InvalidPlaybookStep(EntryId),
    InvalidToolkitGuide(EntryId),
    InvalidWorkflowGuide,
    ToolkitGuideWithoutOperations(EntryId),
    UnknownPlaybookOperation(EntryId),
    PlaybookOperationMismatch {
        playbook_id: EntryId,
        operation_id: EntryId,
    },
    SearchIndexBuild,
    /// A plan-governed search must be a read: its policy effect is
    /// `ReadOpenWorld` or `SyncOpenWorld`.
    SearchOperationNotReadOnly(EntryId),
    Schema(SchemaBuildError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParameterPointerRole {
    Input,
    Output,
}
