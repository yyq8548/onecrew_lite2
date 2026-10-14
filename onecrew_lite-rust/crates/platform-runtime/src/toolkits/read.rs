//! Platform-independent orchestration shared by every platform read.
//!
//! A platform read entry checks its input with the platform crate's own
//! validation, then delegates here once. [`operator_account_read`] admits the
//! connection, opens the session on the blocking pool, checks a bound search
//! authorization before admission and again once the session is open, and
//! reports a failed platform call as that platform's protocol-native
//! evidence. [`platform_native_data`] serializes a platform model for peers.
//! Every read reports [`PlatformReadError`], whose variants name only local
//! outcomes; OneCrew adds no category of its own to a platform failure
//! (ADR 0017).

use std::{fmt, future::Future, sync::Arc};

use onecrew_dm_contracts::DmDiagnostic;
use onecrew_sessions::PlatformSessions;
use serde::Serialize;

use crate::{
    BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform,
    PlatformConnectionResolutionError, PlatformConnectionSelector, SearchPlanApprovalError,
    credentials::services::blocking_segment,
};

/// Platform-native data: one platform model serialized into a JSON object
/// whose keys are the platform's own field names.
pub type PlatformNativeData = serde_json::Map<String, serde_json::Value>;

/// Failure of one platform read, for every platform. Each local variant names
/// an outcome of OneCrew itself; a platform failure carries the platform's
/// own evidence. No variant holds a provider body, session diagnostic, proxy
/// address or source chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformReadError {
    /// The input was rejected before admission. The explanation is a
    /// constant written by the input's owner.
    InputInvalid(&'static str),
    /// The exact search request is unapproved, changed or no longer live.
    SearchPlanRefused(SearchPlanApprovalError),
    /// Connection admission refused the selected connection.
    Connection(PlatformConnectionResolutionError),
    /// The stored operator session could not be opened.
    SessionUnavailable,
    /// The service credential could not be opened for the read.
    CredentialUnavailable,
    /// The platform answered, but its credential health outcome could not be
    /// committed, so whether the recorded health changed is unknown.
    CredentialHealthCommitUnknown,
    /// The runtime lost a blocking segment; nothing is known about the read.
    Interrupted,
    /// The platform model did not serialize to a JSON object.
    NativeDataNotObject,
    /// The platform failed the read.
    Platform {
        /// The platform's protocol-native evidence of the failure.
        evidence: DmDiagnostic,
        /// Whether the failure is reported after a local commit, such as a
        /// credential health mark.
        after_local_commit: bool,
    },
}

impl PlatformReadError {
    /// A secret-free explanation. Local outcomes use platform-independent
    /// text, except that an invalid input carries its owner's constant; a
    /// platform failure explains itself with its evidence's owner-written
    /// message.
    pub fn safe_message(&self) -> &'static str {
        match self {
            Self::InputInvalid(reason) => reason,
            Self::SearchPlanRefused(_) => "search plan authorization was not accepted",
            Self::Connection(error) => error.safe_message(),
            Self::SessionUnavailable => "operator session could not be opened",
            Self::CredentialUnavailable => "service credential could not be opened",
            Self::CredentialHealthCommitUnknown => {
                "credential health outcome could not be recorded"
            }
            Self::Interrupted => "platform read was interrupted",
            Self::NativeDataNotObject => "platform read result is not a JSON object",
            Self::Platform { evidence, .. } => evidence.message,
        }
    }

    /// The failure as direct-message evidence: a platform failure keeps its
    /// own evidence, and a local outcome becomes a local diagnostic that names
    /// it for `platform`.
    pub(crate) fn diagnostic(&self, platform: &'static str) -> DmDiagnostic {
        const PROTOCOL: &str = "onecrew_platform_read";
        let local = |code| DmDiagnostic::local(platform, PROTOCOL, code, self.safe_message());
        match self {
            Self::Platform { evidence, .. } => evidence.clone(),
            Self::Connection(error) => DmDiagnostic::local(
                platform,
                "onecrew_connection_admission",
                error.code(),
                error.safe_message(),
            ),
            Self::InputInvalid(_) => local("invalid_input"),
            Self::SearchPlanRefused(refusal) => local(refusal.code()),
            Self::SessionUnavailable => local("session_unavailable"),
            Self::CredentialUnavailable => local("credential_unavailable"),
            Self::CredentialHealthCommitUnknown => local("credential_health_commit_unknown"),
            Self::Interrupted => local("read_interrupted"),
            Self::NativeDataNotObject => local("native_data_not_object"),
        }
    }
}

impl fmt::Display for PlatformReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.safe_message())
    }
}

impl std::error::Error for PlatformReadError {}

/// Serializes one platform model into platform-native data.
///
/// # Errors
///
/// Returns [`PlatformReadError::NativeDataNotObject`] when the model does not
/// serialize to a JSON object.
pub(crate) fn platform_native_data<T: Serialize>(
    value: &T,
) -> Result<PlatformNativeData, PlatformReadError> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(data)) => Ok(data),
        _ => Err(PlatformReadError::NativeDataNotObject),
    }
}

/// The session of one operator account on one platform.
pub(crate) struct OperatorSession<'a, S> {
    pub(crate) platform: OperatorAccountPlatform,
    pub(crate) operator_account_id: &'a str,
    /// Opens the session for the admitted account reference on the blocking
    /// pool. Its diagnostic is discarded.
    pub(crate) open: fn(&PlatformSessions, &str) -> Result<S, String>,
}

/// A search authorization bound to its exact request.
pub(crate) struct BoundSearch<'a> {
    validate: Box<dyn Fn() -> Result<(), SearchPlanApprovalError> + Send + Sync + 'a>,
}

impl<'a> BoundSearch<'a> {
    pub(crate) fn new<R: Serialize + Sync + 'static>(
        authorization: &'a BoundSearchAuthorization,
        request: &'a R,
    ) -> Self {
        Self {
            validate: Box::new(move || authorization.validate_request(request)),
        }
    }

    /// A bound search whose check the test decides.
    #[cfg(test)]
    pub(crate) fn from_check(
        validate: impl Fn() -> Result<(), SearchPlanApprovalError> + Send + Sync + 'a,
    ) -> Self {
        Self {
            validate: Box::new(validate),
        }
    }

    pub(super) fn validate(&self) -> Result<(), PlatformReadError> {
        (self.validate)().map_err(PlatformReadError::SearchPlanRefused)
    }
}

/// Connection admission, then the session opened on the blocking pool.
///
/// # Errors
///
/// Returns [`PlatformReadError::Connection`] when admission refuses the
/// account, [`PlatformReadError::SessionUnavailable`] when its session cannot
/// be opened, and [`PlatformReadError::Interrupted`] when the blocking
/// segment is lost.
pub(crate) async fn admitted_session<S: Send + 'static>(
    platform: &LocalPlatform,
    session: OperatorSession<'_, S>,
) -> Result<S, PlatformReadError> {
    let account_ref_id = platform
        .resolve_platform_connection(PlatformConnectionSelector::OperatorAccount {
            platform: session.platform,
            operator_account_id: session.operator_account_id.to_owned(),
        })
        .await
        .map_err(PlatformReadError::Connection)?;
    let sessions = Arc::clone(&platform.sessions);
    let open = session.open;
    let profile_id = account_ref_id.as_str().to_owned();
    blocking_segment(PlatformReadError::Interrupted, move || {
        open(&sessions, &profile_id).map_err(|_| PlatformReadError::SessionUnavailable)
    })
    .await
}

/// One operator-account platform read after its input was validated: a bound
/// search is checked before admission, the session is admitted and opened,
/// the search is checked again, and `read` runs with the session. A failed
/// platform call crosses as `evidence` of the platform's own error.
///
/// # Errors
///
/// Returns every [`PlatformReadError`] local outcome of these steps, or
/// [`PlatformReadError::Platform`] when `read` fails.
pub(crate) async fn operator_account_read<S, T, E, Fut>(
    platform: &LocalPlatform,
    session: OperatorSession<'_, S>,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(S) -> Fut,
    evidence: fn(&E) -> DmDiagnostic,
) -> Result<T, PlatformReadError>
where
    S: Send + 'static,
    Fut: Future<Output = Result<T, E>>,
{
    if let Some(search) = &search {
        search.validate()?;
    }
    let session = admitted_session(platform, session).await?;
    if let Some(search) = &search {
        search.validate()?;
    }
    read(session)
        .await
        .map_err(|error| PlatformReadError::Platform {
            evidence: evidence(&error),
            after_local_commit: false,
        })
}

#[cfg(test)]
mod tests;
