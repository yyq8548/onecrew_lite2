//! Request binding for the runtime's concrete process-local search authority.
//! Neither an approval nor a bound request can be decoded from wire input.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::any::TypeId;

use super::authority::ApprovalGrant;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchPlanPolicy {
    #[default]
    RequireConfirmation,
    UserRequestedDirect,
}

/// A pure search admission refusal; no page or candidate budget is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchPlanApprovalError {
    ConfirmationRequired,
    InvalidOrExpired,
    ScopeMismatch,
    Unavailable,
}

impl SearchPlanApprovalError {
    pub const ALL: [Self; 4] = [
        Self::ConfirmationRequired,
        Self::InvalidOrExpired,
        Self::ScopeMismatch,
        Self::Unavailable,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::ConfirmationRequired => "search_plan_confirmation_required",
            Self::InvalidOrExpired => "search_plan_invalid_or_expired",
            Self::ScopeMismatch => "search_plan_scope_mismatch",
            Self::Unavailable => "search_plan_approval_unavailable",
        }
    }
}

impl std::fmt::Display for SearchPlanApprovalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}
impl std::error::Error for SearchPlanApprovalError {}

impl Serialize for SearchPlanApprovalError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code())
    }
}

impl JsonSchema for SearchPlanApprovalError {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SearchPlanApprovalError".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "enum": Self::ALL.map(Self::code) })
    }
}

/// One scope issued by the runtime's live plan authority. Creation is private
/// to that owner; inbound adapters can only bind an already issued value.
#[derive(Clone, PartialEq, Eq)]
pub struct OperationSearchAuthorization {
    grant: ApprovalGrant,
}

impl std::fmt::Debug for OperationSearchAuthorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OperationSearchAuthorization { .. }")
    }
}

impl OperationSearchAuthorization {
    pub(super) fn checked(grant: ApprovalGrant) -> Result<Self, SearchPlanApprovalError> {
        grant.validate()?;
        Ok(Self { grant })
    }
}

/// Authorization bound to one concrete toolkit request, including its selected
/// connection and page. It holds no provider, Creator, or arbitrary JSON model.
#[derive(Clone, PartialEq, Eq)]
pub struct BoundSearchAuthorization {
    approval: Option<OperationSearchAuthorization>,
    // Different operations may have the same JSON shape. This process-local
    // token must bind the actual concrete input type as well as its value.
    request_type: TypeId,
    request_digest: [u8; 32],
}

impl std::fmt::Debug for BoundSearchAuthorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BoundSearchAuthorization { .. }")
    }
}

impl BoundSearchAuthorization {
    /// Bind an issued approval, or the explicit direct-search policy, to the
    /// exact typed input that the adapter will pass to a toolkit.
    ///
    /// # Errors
    /// Rejects missing, mismatched, expired or unavailable plan authority. A
    /// direct request must not also carry a plan reference or approval.
    pub fn bind<T: Serialize + 'static>(
        request: &T,
        policy: SearchPlanPolicy,
        reference: Option<&str>,
        approval: Option<OperationSearchAuthorization>,
    ) -> Result<Self, SearchPlanApprovalError> {
        match policy {
            SearchPlanPolicy::UserRequestedDirect => {
                if reference.is_some() || approval.is_some() {
                    return Err(SearchPlanApprovalError::ScopeMismatch);
                }
            }
            SearchPlanPolicy::RequireConfirmation => {
                let approval = approval
                    .as_ref()
                    .ok_or(SearchPlanApprovalError::ConfirmationRequired)?;
                if reference != Some(approval.grant.reference()) {
                    return Err(SearchPlanApprovalError::ScopeMismatch);
                }
                approval.grant.validate()?;
            }
        }
        Ok(Self {
            approval,
            request_type: TypeId::of::<T>(),
            request_digest: digest(request)?,
        })
    }

    /// Revalidate after connection/session admission and immediately before
    /// provider I/O. Reading an approval never consumes it or extends its TTL.
    ///
    /// # Errors
    /// Rejects a different typed request or a plan cancelled, expired, or
    /// unavailable since binding. Direct calls are still request-bound.
    pub fn validate_request<T: Serialize + 'static>(
        &self,
        request: &T,
    ) -> Result<(), SearchPlanApprovalError> {
        if TypeId::of::<T>() != self.request_type || digest(request)? != self.request_digest {
            return Err(SearchPlanApprovalError::ScopeMismatch);
        }
        if let Some(approval) = &self.approval {
            approval.grant.validate()?;
        }
        Ok(())
    }
}

fn digest<T: Serialize>(request: &T) -> Result<[u8; 32], SearchPlanApprovalError> {
    let bytes = serde_json::to_vec(request).map_err(|_| SearchPlanApprovalError::ScopeMismatch)?;
    Ok(Sha256::digest(bytes).into())
}

#[cfg(test)]
mod tests;
