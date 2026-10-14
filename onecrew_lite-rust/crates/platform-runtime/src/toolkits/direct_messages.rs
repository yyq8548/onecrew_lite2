//! Shared admission and effect classification for the existing DM contract.
use onecrew_dm_contracts::{DmDiagnostic, DmReadError, DmSendError, DmSendFailureKind};

use crate::PlatformConnectionResolutionError;

/// A DM toolkit failure retaining safe protocol evidence and no message content.
#[derive(Debug)]
pub enum DmToolkitError {
    Admission(PlatformConnectionResolutionError),
    InvalidInput(&'static str),
    Read(DmReadError),
    Send(DmSendError),
}

impl DmToolkitError {
    pub fn safe_message(&self) -> &'static str {
        match self {
            Self::Admission(error) => error.safe_message(),
            Self::InvalidInput(message) => message,
            Self::Read(error) => error.diagnostic.message,
            Self::Send(error) => error.safe_message(),
        }
    }

    /// Classify a failure returned by a send call. Failure before dispatch is
    /// a refusal; an unavailable read cannot prove the remote write absent.
    pub fn send_failure_kind(&self) -> DmSendFailureKind {
        match self {
            Self::Admission(_) | Self::InvalidInput(_) => DmSendFailureKind::Refused,
            Self::Read(_) => DmSendFailureKind::Unknown,
            Self::Send(error) => error.kind(),
        }
    }

    /// Local admission is identified by its actual runtime protocol; provider
    /// diagnostics retain the namespace supplied by the facade.
    pub fn diagnostic(&self, platform: &'static str) -> DmDiagnostic {
        match self {
            Self::Admission(error) => DmDiagnostic::local(
                platform,
                "onecrew_connection_admission",
                error.code(),
                error.safe_message(),
            ),
            Self::InvalidInput(message) => {
                DmDiagnostic::local(platform, "onecrew_dm_toolkit", "invalid_input", message)
            }
            Self::Read(error) => error.diagnostic.clone(),
            Self::Send(error) => error.diagnostic.clone(),
        }
    }
}

impl From<DmReadError> for DmToolkitError {
    fn from(error: DmReadError) -> Self {
        Self::Read(error)
    }
}

impl From<DmSendError> for DmToolkitError {
    fn from(error: DmSendError) -> Self {
        Self::Send(error)
    }
}

impl std::fmt::Display for DmToolkitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.safe_message())
    }
}
impl std::error::Error for DmToolkitError {}
