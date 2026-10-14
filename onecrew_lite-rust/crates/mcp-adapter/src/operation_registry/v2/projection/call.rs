//! Per-call display metadata and the closed adapter presentation vocabulary.
use crate::operation_registry::descriptor::{
    EntryId, OperationEntry, OperationViewKind, SecretClass,
};
use onecrew_local_runtime::ResultPresentationKind;
use serde_json::{Map, Value};
const MAX_METADATA_TEXT_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::operation_registry::v2) enum AdapterPresentationKind {
    SearchPlanEditor,
    OutreachDraft,
    RecordPreview,
    SimpleOperation,
    ReadOnly,
    Choice,
    BatchResult,
}

impl AdapterPresentationKind {
    #[cfg(test)]
    pub(in crate::operation_registry::v2) const ALL: [Self; 7] = [
        Self::SearchPlanEditor,
        Self::OutreachDraft,
        Self::RecordPreview,
        Self::SimpleOperation,
        Self::ReadOnly,
        Self::Choice,
        Self::BatchResult,
    ];

    pub(in crate::operation_registry::v2) const fn as_str(self) -> &'static str {
        match self {
            Self::SearchPlanEditor => "search_plan_editor",
            Self::OutreachDraft => "outreach_draft",
            Self::RecordPreview => "record_preview",
            Self::SimpleOperation => "simple_operation",
            Self::ReadOnly => "read_only",
            Self::Choice => "choice",
            Self::BatchResult => "batch_result",
        }
    }

    pub(in crate::operation_registry::v2) fn runtime(self) -> ResultPresentationKind {
        ResultPresentationKind::new(self.as_str())
            .expect("closed adapter presentation kind is a valid runtime identifier")
    }
}

impl From<OperationViewKind> for AdapterPresentationKind {
    fn from(value: OperationViewKind) -> Self {
        match value {
            OperationViewKind::SearchPlanEditor => Self::SearchPlanEditor,
            OperationViewKind::OutreachDraft => Self::OutreachDraft,
            OperationViewKind::RecordPreview => Self::RecordPreview,
            OperationViewKind::SimpleOperation => Self::SimpleOperation,
            OperationViewKind::ReadOnly => Self::ReadOnly,
            OperationViewKind::Choice => Self::Choice,
        }
    }
}

#[derive(Clone)]
pub(in crate::operation_registry::v2) struct ProjectionCall {
    pub(in crate::operation_registry::v2) call_id: EntryId,
    pub(in crate::operation_registry::v2) operation_id: EntryId,
    pub(in crate::operation_registry::v2) title: String,
    pub(in crate::operation_registry::v2) summary: String,
    pub(in crate::operation_registry::v2) arguments: Map<String, Value>,
    pub(in crate::operation_registry::v2) presentable: bool,
    /// Whether this call can change anything outside the read it performed.
    /// A read's output is the answer someone asked for; an effectful call's
    /// output is its own bookkeeping.
    pub(in crate::operation_registry::v2) may_have_effect: bool,
    pub(in crate::operation_registry::v2) kind: AdapterPresentationKind,
}

impl ProjectionCall {
    pub(in crate::operation_registry::v2) fn new(
        entry: &OperationEntry,
        call_id: EntryId,
        arguments: Map<String, Value>,
    ) -> Self {
        Self {
            call_id,
            operation_id: entry.metadata.id.clone(),
            title: bounded_text(&entry.metadata.title, "Operation"),
            summary: bounded_text(&entry.metadata.summary, "Registered operation"),
            arguments,
            presentable: entry.secret == SecretClass::None
                && entry.sensitive_input_paths.is_empty(),
            may_have_effect: entry.effect.may_have_effect(),
            kind: entry.view_kind.into(),
        }
    }
}
pub(super) fn bounded_text(value: &str, fallback: &str) -> String {
    let value = value.trim();
    let value = if value.is_empty() { fallback } else { value };
    if value.len() <= MAX_METADATA_TEXT_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_METADATA_TEXT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
