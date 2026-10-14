//! Result-reference presentation DTOs and lifecycle status vocabulary.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ResultPresentInput {
    pub(in crate::operation_registry) result_ref: onecrew_local_runtime::ResultRef,
    /// Locale for this card. The caller must choose `zh-CN` for Chinese
    /// conversations and `en-US` for all other conversations; it is kept on
    /// the result so refreshes do not follow a later global preference.
    pub(in crate::operation_registry) locale: onecrew_local_runtime::PresentationLocale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(in crate::operation_registry) enum ResultPresentStatus {
    Available,
    Submitted,
    Resolved,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::operation_registry) struct ResultPresentResult {
    pub(in crate::operation_registry) result_ref: onecrew_local_runtime::ResultRef,
    pub(in crate::operation_registry) status: ResultPresentStatus,
    /// A bounded, secret-free terminal cause for a failed presented result.
    /// Successful and non-failed states carry `null` so the model cannot
    /// mistake an absent cause for an unreported failure.
    pub(in crate::operation_registry) failure: Option<onecrew_local_runtime::ResultFailure>,
}
