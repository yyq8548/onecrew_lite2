//! OneCrew-owned process-local scope authority. No database stores a plan.
//!
//! A confirmed plan is its approved scope tokens and an expiry. A plan-governed
//! call is admitted when the plan is live and one approved token equals the
//! call's compiled scope; nothing is recorded under the plan, so paging is
//! bounded only by each call's own count, the provider, and the plan TTL.
//! Every call takes the one store mutex for a single bounded critical section
//! with no I/O.
use super::{OperationSearchAuthorization, SearchPlanApprovalError};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Most approved scope tokens one confirmed plan may hold.
pub const SEARCH_PLAN_MAX_SCOPES: usize = 200;
/// Most unexpired confirmed plans the process holds at once.
pub const SEARCH_PLAN_MAX_ACTIVE: usize = 1024;
/// Lifetime of a confirmed plan, in seconds, from its confirmation.
pub const SEARCH_PLAN_TTL_SECONDS: u64 = 3600;

type ApprovalStore = Arc<Mutex<HashMap<String, Approval>>>;

struct Approval {
    /// Unique approved scope tokens.
    scopes: Vec<String>,
    expires: Instant,
}
impl Approval {
    fn approves(&self, token: &str) -> bool {
        self.scopes.iter().any(|scope| scope == token)
    }
}

#[derive(Clone, Default)]
pub(crate) struct SearchAuthority {
    approvals: ApprovalStore,
}
impl SearchAuthority {
    pub(super) fn confirm(&self, scopes: Vec<String>) -> Result<String, String> {
        let mut unique = HashSet::with_capacity(scopes.len());
        if scopes.is_empty()
            || scopes.len() > SEARCH_PLAN_MAX_SCOPES
            || scopes
                .iter()
                .any(|scope| scope.is_empty() || !unique.insert(scope.as_str()))
        {
            return Err("invalid operation scopes".into());
        }
        let mut store = self
            .approvals
            .lock()
            .map_err(|_| "search approval unavailable")?;
        let now = Instant::now();
        store.retain(|_, approval| approval.expires > now);
        if store.len() >= SEARCH_PLAN_MAX_ACTIVE {
            return Err("too many active search plans".into());
        }
        let reference = uuid::Uuid::new_v4().to_string();
        store.insert(
            reference.clone(),
            Approval {
                scopes,
                expires: now + Duration::from_secs(SEARCH_PLAN_TTL_SECONDS),
            },
        );
        Ok(reference)
    }
    pub(super) fn cancel(&self, reference: &str) -> Result<(), String> {
        self.approvals
            .lock()
            .map_err(|_| "search approval unavailable")?
            .remove(reference);
        Ok(())
    }
    /// Mint an authorization for one approved scope of a live plan. Minting
    /// only validates; the typed owner validates the bound request again
    /// before provider I/O.
    pub(super) fn authorize(
        &self,
        reference: Option<&str>,
        token: &str,
    ) -> Result<OperationSearchAuthorization, SearchPlanApprovalError> {
        let reference = reference
            .ok_or(SearchPlanApprovalError::ConfirmationRequired)?
            .to_owned();
        let grant = ApprovalGrant {
            store: Arc::clone(&self.approvals),
            reference: reference.clone(),
            token: token.to_owned(),
        };
        OperationSearchAuthorization::checked(grant)
    }
}

/// The live grant for one scope token of one confirmed plan.
#[derive(Clone)]
pub(super) struct ApprovalGrant {
    store: ApprovalStore,
    reference: String,
    token: String,
}
impl ApprovalGrant {
    pub(super) fn reference(&self) -> &str {
        &self.reference
    }

    /// A missing or expired plan beats an unapproved scope.
    pub(super) fn validate(&self) -> Result<(), SearchPlanApprovalError> {
        let store = self
            .store
            .lock()
            .map_err(|_| SearchPlanApprovalError::Unavailable)?;
        let approval = store
            .get(&self.reference)
            .filter(|approval| approval.expires > Instant::now())
            .ok_or(SearchPlanApprovalError::InvalidOrExpired)?;
        if approval.approves(&self.token) {
            Ok(())
        } else {
            Err(SearchPlanApprovalError::ScopeMismatch)
        }
    }
}

impl PartialEq for ApprovalGrant {
    fn eq(&self, other: &Self) -> bool {
        self.reference == other.reference
            && self.token == other.token
            && Arc::ptr_eq(&self.store, &other.store)
    }
}
impl Eq for ApprovalGrant {}

#[cfg(test)]
mod tests;
