use onecrew_local_runtime::OperationId;
use onecrew_local_runtime::ToolkitId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::super::{approval::ApprovalRequest, execution::ErasedBinding, schema::CompiledSchema};
use super::{
    ApprovalClass, ConnectionRequirement, EffectClass, EntryId, OperationSafety, SecretClass,
    SurfaceSet,
};

#[derive(Clone, Debug)]
pub(crate) struct EntryMetadata {
    pub(crate) id: EntryId,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) toolkit: ToolkitId,
    pub(crate) domain_id: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) surfaces: SurfaceSet,
}

/// An MCP binding names one catalog declaration and contributes protocol metadata only.
#[derive(Clone, Debug)]
pub(crate) struct OperationRegistration {
    pub(crate) id: OperationId,
    pub(crate) domain_id: String,
    pub(crate) aliases: Vec<String>,
    pub(crate) surfaces: SurfaceSet,
}

pub(crate) struct OperationEntry {
    pub(crate) metadata: EntryMetadata,
    pub(crate) effect: EffectClass,
    pub(crate) secret: SecretClass,
    pub(crate) approval: ApprovalClass,
    pub(crate) connection: ConnectionRequirement,
    pub(crate) sensitive_input_paths: Vec<String>,
    pub(crate) view_kind: OperationViewKind,
    pub(crate) input: CompiledSchema,
    pub(crate) output: CompiledSchema,
    pub(crate) binding: ErasedBinding,
    pub(crate) approval_request: Option<ApprovalRequest>,
    pub(crate) safety: OperationSafety,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperationViewKind {
    SearchPlanEditor,
    OutreachDraft,
    RecordPreview,
    SimpleOperation,
    ReadOnly,
    Choice,
}

#[derive(Clone, Debug)]
pub(crate) struct PlaybookEntry {
    pub(crate) metadata: EntryMetadata,
    pub(crate) use_case: String,
    pub(crate) toolkit_ids: Vec<ToolkitId>,
    pub(crate) primary_operation_ids: Vec<EntryId>,
    pub(crate) supporting_operation_ids: Vec<EntryId>,
    pub(crate) recommended_steps: Vec<PlaybookStep>,
    pub(crate) known_pitfalls: Vec<String>,
    pub(crate) difficulty: PlaybookDifficulty,
}

#[derive(Clone, Debug)]
pub(crate) struct ToolkitGuideEntry {
    pub(crate) metadata: EntryMetadata,
    pub(crate) purpose: String,
    pub(crate) connection_model: String,
    pub(crate) identifier_semantics: Vec<String>,
    pub(crate) capability_categories: Vec<String>,
    pub(crate) gotchas: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaybookStep {
    pub(crate) instruction: String,
    pub(crate) operation_ids: Vec<EntryId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PlaybookDifficulty {
    Basic,
    Intermediate,
    Advanced,
}

pub(crate) enum RegistryEntry {
    Operation(Box<OperationEntry>),
    Playbook(PlaybookEntry),
    ToolkitGuide(ToolkitGuideEntry),
}

impl RegistryEntry {
    pub(crate) fn metadata(&self) -> &EntryMetadata {
        match self {
            Self::Operation(entry) => &entry.metadata,
            Self::Playbook(entry) => &entry.metadata,
            Self::ToolkitGuide(entry) => &entry.metadata,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EntryKind {
    Operation,
    Playbook,
    ToolkitGuide,
}
