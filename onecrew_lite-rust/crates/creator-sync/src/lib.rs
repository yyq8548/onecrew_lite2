//! Creator Sync browser pairing and quarantined snapshot lifecycle.
//!
//! This crate owns the strict Worker B protocol, transient secret lifecycle,
//! vault credential handoff, and process-local enrollment/snapshot exclusion.
//! It consumes the canonical Creator-owned `CreatorBundle` without defining another
//! Creator business model.

mod device_label;
// The deployed Access backup contract is v3; local file format additions must
// not silently change enrollment or upload negotiation.
pub(crate) const ACCESS_BUNDLE_FORMAT_VERSION: u16 = 3;
pub use device_label::{DeviceLabel, DeviceLabelError};

mod config;
mod http;
mod model;
mod runtime;
mod snapshot_encode;
mod wire;

pub use model::{
    CreatorSnapshotSyncResult, CreatorSnapshotSyncState, CreatorSyncAction,
    CreatorSyncAdmissionState, CreatorSyncIssue, CreatorSyncPairingError, CreatorSyncSetupError,
    CreatorSyncStatus,
};
pub use onecrew_local_domain::CreatorSyncClientPlatform;
pub use runtime::{
    CodeVerifier, CreatorSyncPairingBeginInput, CreatorSyncPairingBeginResult,
    CreatorSyncPairingCancelInput, CreatorSyncPairingCompleteInput, CreatorSyncRuntime,
    PairingAuthorizeUrl, PairingCode, PairingState, TableSyncRequest, TableSyncResponse,
};

#[cfg(feature = "test-support")]
pub use runtime::test_support;

/// Return the single compiled Access origin for enrollment and cloud integrations.
///
/// Integrations authenticate to this origin with their owning runtime's current
/// installation credential; this does not expose or issue a provider token.
pub const fn production_access_origin() -> &'static str {
    config::production_access_origin()
}
