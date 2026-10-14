//! OneCrew-owned transient search plans and their operation-bound authority.
mod authority;
mod authorization;
pub(crate) use authority::SearchAuthority;
pub use authority::{SEARCH_PLAN_MAX_ACTIVE, SEARCH_PLAN_MAX_SCOPES, SEARCH_PLAN_TTL_SECONDS};
pub use authorization::{
    BoundSearchAuthorization, OperationSearchAuthorization, SearchPlanApprovalError,
    SearchPlanPolicy,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::LocalPlatform;

pub const SEARCH_PLAN_MAX_BYTES: usize = 512 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchPlanPatch {
    pub description: Option<String>,
    pub keywords: Option<Vec<String>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPlanValidationLocation {
    Draft,
    Description,
    Keywords,
    Keyword(usize),
    Scopes,
}
impl SearchPlanValidationLocation {
    pub fn parameter_pointer(self) -> String {
        match self {
            Self::Draft => String::new(),
            Self::Description => "/description".into(),
            Self::Keywords => "/keywords".into(),
            Self::Keyword(index) => format!("/keywords/{index}"),
            Self::Scopes => "/scopes".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchPlanEditorValidationReason {
    InvalidPayload,
    PayloadTooLarge,
    InvalidDescription,
    TooManyKeywords,
    InvalidKeyword,
    InvalidScopeCount,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPlanEditorValidationError {
    pub reason: SearchPlanEditorValidationReason,
    pub location: SearchPlanValidationLocation,
}

impl std::fmt::Display for SearchPlanEditorValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.reason == SearchPlanEditorValidationReason::PayloadTooLarge {
            return formatter.write_str("search_plan_payload_too_large");
        }
        write!(
            formatter,
            "invalid search plan: {:?} at {}",
            self.reason,
            self.location.parameter_pointer()
        )
    }
}
impl std::error::Error for SearchPlanEditorValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchPlanEditorDraft {
    pub description: String,
    /// Editable acceptance keywords. Changing these does not rewrite a selected
    /// operation's arguments; changed provider criteria require a new plan.
    pub keywords: Vec<String>,
    pub scopes: Vec<SearchPlanScope>,
}

/// A plan scope is bound to one registry operation and its input contract.
/// Untyped arguments are retained only at this boundary; registry validation
/// must produce the operation-owned typed input before provider invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(description = "One search the plan runs after the user confirms it.")]
pub struct SearchPlanScope {
    pub operation_id: String,
    #[schemars(description = "Copy verbatim from onecrew_tool_get for operation_id.")]
    pub schema_revision: String,
    #[schemars(
        description = "The tagged connection returned by onecrew_connection_list for the operation's toolkit."
    )]
    pub connection: SearchPlanConnection,
    #[schemars(
        description = "Search criteria for the selected operation, valid under its input_schema. Omit search_plan_policy, confirmed_plan_ref and cursor; they belong to the later search call."
    )]
    pub arguments: serde_json::Map<String, serde_json::Value>,
}

/// Public connection references are tagged and carry only registry-selected
/// identifiers. Secrets and provider credentials never cross this contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SearchPlanConnection {
    OperatorAccount {
        toolkit: String,
        operator_account_id: String,
    },
    ServiceCredential {
        toolkit: String,
        credential_id: String,
    },
}

impl SearchPlanScope {
    /// Stable, secret-free approval token. The token binds the exact
    /// operation, schema revision, selected connection reference, and
    /// arguments; it is never sent to a provider as an untyped payload.
    pub fn approval_token(&self) -> Result<String, String> {
        let mut value =
            serde_json::to_value(self).map_err(|_| "invalid search scope".to_owned())?;
        value.sort_all_objects();
        let bytes = serde_json::to_vec(&value).map_err(|_| "invalid search scope".to_owned())?;
        Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedSearchPlanEditor {
    pub confirmed_plan_ref: String,
    pub plan: SearchPlanEditorDraft,
}

impl SearchPlanEditorDraft {
    pub fn validate(&self) -> Result<(), String> {
        self.validate_typed().map_err(|error| error.to_string())
    }

    pub fn validate_typed(&self) -> Result<(), SearchPlanEditorValidationError> {
        use SearchPlanEditorValidationReason as R;
        use SearchPlanValidationLocation as L;
        let error = |reason, location| SearchPlanEditorValidationError { reason, location };
        if serde_json::to_vec(self)
            .map_err(|_| error(R::InvalidPayload, L::Draft))?
            .len()
            > SEARCH_PLAN_MAX_BYTES
        {
            return Err(error(R::PayloadTooLarge, L::Draft));
        }
        Self::validate_criteria(&self.description, &self.keywords)?;
        if self.scopes.is_empty() || self.scopes.len() > SEARCH_PLAN_MAX_SCOPES {
            return Err(error(R::InvalidScopeCount, L::Scopes));
        }
        for scope in &self.scopes {
            if scope.operation_id.trim().is_empty() || scope.schema_revision.trim().is_empty() {
                return Err(error(R::InvalidPayload, L::Scopes));
            }
        }
        Ok(())
    }

    /// The user-editable acceptance criteria: the description and keywords
    /// rules shared by drafts and the confirmed-plan continuation.
    pub fn validate_criteria(
        description: &str,
        keywords: &[String],
    ) -> Result<(), SearchPlanEditorValidationError> {
        use SearchPlanEditorValidationReason as R;
        use SearchPlanValidationLocation as L;
        let error = |reason, location| SearchPlanEditorValidationError { reason, location };
        if description.trim().is_empty() || description.len() > 8192 {
            return Err(error(R::InvalidDescription, L::Description));
        }
        if keywords.len() > 50 {
            return Err(error(R::TooManyKeywords, L::Keywords));
        }
        for (index, keyword) in keywords.iter().enumerate() {
            if keyword.is_empty()
                || keyword.trim() != keyword
                || keyword.len() > 512
                || keyword.chars().any(char::is_control)
                || keywords[..index].contains(keyword)
            {
                return Err(error(R::InvalidKeyword, L::Keyword(index)));
            }
        }
        Ok(())
    }

    pub fn apply_patch(&self, patch: SearchPlanPatch) -> Result<Self, String> {
        let plan = Self {
            description: patch
                .description
                .unwrap_or_else(|| self.description.clone()),
            keywords: patch.keywords.unwrap_or_else(|| self.keywords.clone()),
            scopes: self.scopes.clone(),
        };
        plan.validate()?;
        Ok(plan)
    }
}

impl LocalPlatform {
    /// Mint the operation-bound authorization for one approved scope token of
    /// a confirmed plan. Minting only validates the approval; the typed owner
    /// validates the bound request again before provider I/O.
    pub fn authorize_search_scope(
        &self,
        reference: Option<&str>,
        scope: &str,
    ) -> Result<OperationSearchAuthorization, SearchPlanApprovalError> {
        self.search_authority.authorize(reference, scope)
    }
    pub fn cancel_search_plan(&self, reference: &str) -> Result<(), String> {
        self.search_authority.cancel(reference)
    }

    /// Only the trusted attended result confirmation handler calls this bridge.
    /// All external scopes are computed here from the visible typed draft.
    pub fn confirm_search_plan_editor(
        &self,
        plan: SearchPlanEditorDraft,
        approved_scopes: Vec<String>,
    ) -> Result<ConfirmedSearchPlanEditor, String> {
        plan.validate()?;
        if plan.scopes.len() != approved_scopes.len() {
            return Err("scope compilation mismatch".into());
        }
        let confirmed = self.search_authority.confirm(approved_scopes)?;
        Ok(ConfirmedSearchPlanEditor {
            confirmed_plan_ref: confirmed,
            plan,
        })
    }
}

#[cfg(test)]
mod tests;
