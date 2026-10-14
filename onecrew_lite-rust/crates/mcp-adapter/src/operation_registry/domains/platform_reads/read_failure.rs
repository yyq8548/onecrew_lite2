//! The one public presentation of a platform read failure (ADR 0017).
//!
//! A platform read binding reports the runtime's [`PlatformReadError`] and
//! registers [`read_failure_projection`] for its declared connection
//! requirement. The projection follows protocol semantics and never branches
//! on the platform: local outcomes keep their public codes, a platform failure
//! without any platform response points to the toolkit's connection check,
//! and every other platform failure is `provider_read_failed`, recovered by
//! its HTTP status or RPC code. A platform failure always carries its
//! evidence as `provider_evidence`.

use onecrew_local_runtime::{DmDiagnostic, DmDiagnosticOrigin, toolkits::PlatformReadError};

use super::super::connections::CheckedToolkit;
use super::shared::{classify_connection_failure, search_plan_refusal};
use crate::operation_registry::{
    descriptor::ConnectionRequirement,
    execution::{ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective},
};

/// The failure projection of a platform read binding that declares
/// `connection`; a recovery that names a connection check names this
/// connection's toolkit.
pub(super) fn read_failure_projection(
    connection: &ConnectionRequirement,
) -> impl Fn(PlatformReadError) -> ClassifiedOwnerFailure + Send + Sync + 'static {
    let checked = CheckedToolkit::for_connection(connection);
    move |error| project(checked, error)
}

fn project(checked: Option<CheckedToolkit>, error: PlatformReadError) -> ClassifiedOwnerFailure {
    let message = error.safe_message();
    let connection_check = || checked.map(CheckedToolkit::recovery);
    match error {
        PlatformReadError::InputInvalid(_) => known_no_effect(
            "platform_read_input_invalid",
            false,
            Some(RecoveryDirective::CorrectInput),
        )
        .with_message(message),
        PlatformReadError::SearchPlanRefused(refusal) => {
            search_plan_refusal(refusal).with_message(refusal.code())
        }
        PlatformReadError::Connection(connection) => {
            classify_connection_failure(connection).with_message(message)
        }
        PlatformReadError::SessionUnavailable | PlatformReadError::CredentialUnavailable => {
            known_no_effect(
                "platform_session_unavailable",
                false,
                Some(RecoveryDirective::Reconnect),
            )
            .with_message(message)
        }
        PlatformReadError::CredentialHealthCommitUnknown => ClassifiedOwnerFailure::unknown(
            code("credential_health_commit_unknown"),
            connection_check(),
        )
        .with_message(message),
        PlatformReadError::Interrupted => {
            ClassifiedOwnerFailure::unknown(code("platform_read_internal"), None)
                .with_message(message)
        }
        PlatformReadError::NativeDataNotObject => {
            known_no_effect("provider_payload_invalid", false, None).with_message(message)
        }
        PlatformReadError::Platform {
            evidence,
            after_local_commit,
        } => platform_failure(connection_check(), evidence, after_local_commit),
    }
}

/// `retry_safe` only says that repeating the same read adds no effect: a
/// failed platform read had none, unless it followed a local commit.
fn platform_failure(
    connection_check: Option<RecoveryDirective>,
    evidence: DmDiagnostic,
    after_local_commit: bool,
) -> ClassifiedOwnerFailure {
    let failure = if without_response(&evidence) {
        known_no_effect("network_unreachable", true, connection_check)
    } else {
        known_no_effect("provider_read_failed", true, status_recovery(&evidence))
    }
    .with_provider_status(evidence.http_status)
    .with_message(evidence.message);
    let failure = if after_local_commit {
        failure.after_local_commit()
    } else {
        failure
    };
    failure.with_provider_evidence(evidence)
}

/// No platform response arrived: the evidence comes from the provider side
/// but holds no HTTP status, RPC code or native code.
fn without_response(evidence: &DmDiagnostic) -> bool {
    evidence.origin == DmDiagnosticOrigin::Provider
        && evidence.http_status.is_none()
        && evidence.rpc_code.is_none()
        && evidence.code.is_none()
}

/// An HTTP status or RPC code of 401 reconnects; 420 and 429 retry once after
/// at least 60 seconds, and 5xx after at least 30. Other codes and local
/// evidence offer no recovery.
fn status_recovery(evidence: &DmDiagnostic) -> Option<RecoveryDirective> {
    if evidence.origin == DmDiagnosticOrigin::Local {
        return None;
    }
    evidence
        .http_status
        .map(i32::from)
        .into_iter()
        .chain(evidence.rpc_code)
        .find_map(|status| match status {
            401 => Some(RecoveryDirective::Reconnect),
            420 | 429 => Some(retry_later(60)),
            500..=599 => Some(retry_later(30)),
            _ => None,
        })
}

fn retry_later(minimum_delay_seconds: u32) -> RecoveryDirective {
    RecoveryDirective::RetryLater {
        minimum_delay_seconds,
        max_attempts: 1,
    }
}

fn known_no_effect(
    code_text: &str,
    retry_safe: bool,
    recovery: Option<RecoveryDirective>,
) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(code(code_text), retry_safe, recovery)
}

fn code(code: &str) -> PublicFailureCode {
    PublicFailureCode::parse(code).expect("static platform read failure code is valid")
}
