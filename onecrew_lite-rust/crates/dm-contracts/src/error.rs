use schemars::JsonSchema;
use serde::Serialize;

/// Where a diagnostic was established, not an effect or recovery classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DmDiagnosticOrigin {
    Provider,
    Local,
}

/// Safe evidence from the actual protocol. Codes have meaning only within the
/// platform, protocol and origin that supplied them. Absence is not a diagnosis.
/// Human messages must be owner-authored constants, never raw provider Display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DmDiagnostic {
    pub platform: &'static str,
    pub protocol: &'static str,
    pub origin: DmDiagnosticOrigin,
    pub code: Option<String>,
    pub http_status: Option<u16>,
    pub rpc_code: Option<i32>,
    pub message: &'static str,
}

impl DmDiagnostic {
    pub fn local(
        platform: &'static str,
        protocol: &'static str,
        code: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            platform,
            protocol,
            origin: DmDiagnosticOrigin::Local,
            code: Some(code.into()),
            http_status: None,
            rpc_code: None,
            message,
        }
    }

    pub fn provider(platform: &'static str, protocol: &'static str, message: &'static str) -> Self {
        Self {
            platform,
            protocol,
            origin: DmDiagnosticOrigin::Provider,
            code: None,
            http_status: None,
            rpc_code: None,
            message,
        }
    }
}

/// A transient diagnostic annotated by the runtime that actually ran the stage.
/// This is deliberately separate from durable receipts and effect state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DmFailure {
    pub stage: &'static str,
    pub diagnostic: DmDiagnostic,
}

/// Provider evidence about the effect of a failed direct-message write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmSendFailureKind {
    /// The provider or local preflight proves no send was accepted.
    Refused,
    /// An effect may exist, including network/protocol errors or empty IDs.
    Unknown,
}

/// A failed send keeps effect evidence independent of the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", diagnostic.message)]
pub struct DmSendError {
    kind: DmSendFailureKind,
    pub diagnostic: DmDiagnostic,
}

impl DmSendError {
    pub fn refused(diagnostic: DmDiagnostic) -> Self {
        Self {
            kind: DmSendFailureKind::Refused,
            diagnostic,
        }
    }

    pub fn unknown(diagnostic: DmDiagnostic) -> Self {
        Self {
            kind: DmSendFailureKind::Unknown,
            diagnostic,
        }
    }

    pub fn kind(&self) -> DmSendFailureKind {
        self.kind
    }

    pub fn safe_message(&self) -> &'static str {
        self.diagnostic.message
    }
}

/// A capability probe or thread read that could not answer. Only audited safe
/// facts cross this boundary; the raw provider error stays below its facade.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", diagnostic.message)]
pub struct DmReadError {
    pub diagnostic: DmDiagnostic,
}
