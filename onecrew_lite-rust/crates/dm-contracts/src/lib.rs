//! Provider-neutral direct-message contracts shared by the Creator owner and
//! concrete platform providers.
//!
//! The Creator owner keeps confirmation, idempotency, the relationship ledger
//! and audit; a provider owns the actual contact. Nothing here knows a Creator,
//! a route or a durable row, and nothing here reshapes a provider payload.

mod capability;
mod error;
mod message;
mod provider;
mod target;

pub use capability::{DmCapability, DmCapabilityReason, DmCapabilityScope, DmCapabilityState};
pub use error::{
    DmDiagnostic, DmDiagnosticOrigin, DmFailure, DmReadError, DmSendError, DmSendFailureKind,
};
pub use message::{DmMessage, DmReadbackCoverage, DmReceipt, DmThread, DmThreadReadback};
pub use provider::{DirectMessageProvider, DmSendRequest};
pub use target::{DmTarget, DmTargetKind};
