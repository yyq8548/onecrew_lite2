//! Browser pairing, transient PKCE secrets, and one-use Worker enrollment.

use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use onecrew_local_domain::{CreatorSyncClientPlatform, CreatorSyncCredentialMetadata};
use rand::{RngCore, rngs::OsRng};
use reqwest::{Method, StatusCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    config::CreatorSyncEndpoints,
    http::HttpRequest,
    model::{
        CreatorSyncAction, CreatorSyncAdmissionState, CreatorSyncIssue, CreatorSyncPairingError,
        CreatorSyncStatus, active_status, status,
    },
    wire::{self, CredentialProbe, InstallationProjection},
};

use super::{
    CreatorSyncRuntime,
    credential_store::{AdmissionValidation, StoredCredentialState, verified_metadata},
    parse_json,
};

const PAIRING_PENDING_TTL: Duration = Duration::from_secs(5 * 60);
const PAIRING_CODE_PREFIX: &str = "reacher_pairing_v1.";

struct ZeroizingString(Zeroizing<String>);

impl ZeroizingString {
    fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn into_zeroizing(self) -> Zeroizing<String> {
        self.0
    }
}

pub struct PairingCode(ZeroizingString);

impl PairingCode {
    pub fn parse(value: String) -> Result<Self, CreatorSyncPairingError> {
        let value = ZeroizingString::new(value);
        if value
            .expose()
            .strip_prefix(PAIRING_CODE_PREFIX)
            .is_some_and(is_base64url_43)
        {
            Ok(Self(value))
        } else {
            Err(CreatorSyncPairingError::InvalidPairingCode)
        }
    }

    fn expose(&self) -> &str {
        self.0.expose()
    }
}

pub struct CodeVerifier(ZeroizingString);

impl CodeVerifier {
    fn generate() -> Self {
        Self(random_base64url_43())
    }

    fn expose(&self) -> &str {
        self.0.expose()
    }
}

pub struct PairingState(ZeroizingString);

impl PairingState {
    pub fn parse(value: String) -> Result<Self, CreatorSyncPairingError> {
        let value = ZeroizingString::new(value);
        if is_canonical_base64url_32(value.expose()) {
            Ok(Self(value))
        } else {
            Err(CreatorSyncPairingError::InvalidPairingState)
        }
    }

    fn generate() -> Self {
        Self(random_base64url_43())
    }

    fn expose(&self) -> &str {
        self.0.expose()
    }

    fn constant_time_eq(&self, other: &Self) -> bool {
        bool::from(self.expose().as_bytes().ct_eq(other.expose().as_bytes()))
    }
}

pub struct PairingAuthorizeUrl(ZeroizingString);

impl PairingAuthorizeUrl {
    pub fn as_str(&self) -> &str {
        self.0.expose()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSyncPairingBeginInput {
    #[schemars(with = "String")]
    pub client_platform: CreatorSyncClientPlatform,
    pub app_version: String,
    pub callback_port: u16,
    pub owner_generation: u64,
}

pub struct CreatorSyncPairingBeginResult {
    pub status: CreatorSyncStatus,
    pub flow_id: String,
    pub authorize_url: PairingAuthorizeUrl,
}

pub struct CreatorSyncPairingCompleteInput {
    flow_id: Uuid,
    owner_generation: u64,
    pairing_code: PairingCode,
    state: PairingState,
}

impl CreatorSyncPairingCompleteInput {
    pub fn new(
        flow_id: String,
        owner_generation: u64,
        pairing_code: PairingCode,
        state: PairingState,
    ) -> Result<Self, CreatorSyncPairingError> {
        Ok(Self {
            flow_id: Uuid::parse_str(&flow_id)
                .map_err(|_| CreatorSyncPairingError::InvalidFlowId)?,
            owner_generation,
            pairing_code,
            state,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSyncPairingCancelInput {
    pub flow_id: String,
    pub owner_generation: u64,
}

pub(super) trait PairingClock: Send + Sync {
    fn now(&self) -> Instant;
}

pub(super) struct SystemPairingClock;

impl PairingClock for SystemPairingClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub(super) struct PairingLifecycle {
    phase: PairingPhase,
    last_terminal: Option<TerminalFlow>,
}

enum PairingPhase {
    Idle,
    Pending(PendingPairing),
    Recovery(CreatorSyncIssue),
}

struct PendingPairing {
    flow_id: Uuid,
    owner_generation: u64,
    created_at: Instant,
    verifier: CodeVerifier,
    state: PairingState,
    candidate: InstallationCandidate,
    idempotency_key: ZeroizingString,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalReason {
    Completed,
    Cancelled,
    Expired,
    Consumed,
    OwnerChanged,
}

struct TerminalFlow {
    flow_id: Uuid,
    reason: TerminalReason,
}

impl PairingLifecycle {
    pub(super) fn new() -> Self {
        Self {
            phase: PairingPhase::Idle,
            last_terminal: None,
        }
    }

    fn expire_if_needed(&mut self, now: Instant) {
        let expired_flow = match &self.phase {
            PairingPhase::Pending(pending)
                if now
                    .checked_duration_since(pending.created_at)
                    .is_some_and(|elapsed| elapsed >= PAIRING_PENDING_TTL) =>
            {
                Some(pending.flow_id)
            }
            _ => None,
        };
        if let Some(flow_id) = expired_flow {
            self.phase = PairingPhase::Idle;
            self.last_terminal = Some(TerminalFlow {
                flow_id,
                reason: TerminalReason::Expired,
            });
        }
    }

    fn terminal_error(&self, flow_id: Uuid) -> CreatorSyncPairingError {
        match self
            .last_terminal
            .as_ref()
            .filter(|terminal| terminal.flow_id == flow_id)
            .map(|terminal| terminal.reason)
        {
            Some(TerminalReason::Cancelled) => CreatorSyncPairingError::Cancelled,
            Some(TerminalReason::Expired) => CreatorSyncPairingError::Expired,
            Some(
                TerminalReason::Completed | TerminalReason::Consumed | TerminalReason::OwnerChanged,
            ) => CreatorSyncPairingError::Replayed,
            None => CreatorSyncPairingError::InvalidFlow,
        }
    }
}

impl CreatorSyncRuntime {
    pub fn begin_pairing(
        &self,
        input: CreatorSyncPairingBeginInput,
        device_label: &crate::DeviceLabel,
    ) -> Result<CreatorSyncPairingBeginResult, CreatorSyncPairingError> {
        if !self.inner.configured {
            return Err(CreatorSyncPairingError::Unconfigured);
        }
        if !valid_app_version(&input.app_version) {
            return Err(CreatorSyncPairingError::InvalidAppVersion);
        }
        if input.callback_port < 1024 {
            return Err(CreatorSyncPairingError::InvalidCallbackPort);
        }
        match self.load_credential() {
            Ok(Some(StoredCredentialState::Active(_)))
                if self.admission_validation() == Some(AdmissionValidation::Validated) =>
            {
                return Err(CreatorSyncPairingError::AlreadyActive);
            }
            Ok(Some(StoredCredentialState::Active(_))) => {
                return Err(CreatorSyncPairingError::AdmissionValidationRequired);
            }
            Ok(Some(StoredCredentialState::NonActive(_))) | Ok(None) => {
                if let Ok(mut validation) = self.inner.validation.try_lock() {
                    *validation = AdmissionValidation::NonActive;
                }
            }
            Err(()) => return Err(CreatorSyncPairingError::RecoveryRequired),
        }

        let mut lifecycle = self
            .inner
            .pairing
            .try_lock()
            .map_err(|_| CreatorSyncPairingError::PairingAlreadyPending)?;
        lifecycle.expire_if_needed(self.inner.clock.now());
        if matches!(lifecycle.phase, PairingPhase::Pending(_)) {
            return Err(CreatorSyncPairingError::PairingAlreadyPending);
        }

        let flow_id = Uuid::new_v4();
        let verifier = CodeVerifier::generate();
        let state = PairingState::generate();
        let authorize_url = build_authorize_url(
            &self.inner.endpoints,
            &input.app_version,
            input.callback_port,
            input.client_platform,
            &verifier,
            &state,
            device_label,
        )?;
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        lifecycle.phase = PairingPhase::Pending(PendingPairing {
            flow_id,
            owner_generation: input.owner_generation,
            created_at: self.inner.clock.now(),
            verifier,
            state,
            candidate: InstallationCandidate::generate(input.client_platform, input.app_version),
            idempotency_key: random_base64url_43(),
        });
        Ok(CreatorSyncPairingBeginResult {
            status: status(
                CreatorSyncAdmissionState::PairingPending,
                Some(CreatorSyncAction::AwaitPairingCallback),
                None,
                None,
            ),
            flow_id: flow_id.to_string(),
            authorize_url,
        })
    }

    pub async fn complete_pairing(
        &self,
        input: CreatorSyncPairingCompleteInput,
    ) -> Result<CreatorSyncStatus, CreatorSyncPairingError> {
        if !self.inner.configured {
            return Err(CreatorSyncPairingError::Unconfigured);
        }
        let mut lifecycle = self.inner.pairing.lock().await;
        lifecycle.expire_if_needed(self.inner.clock.now());
        let pending = match &lifecycle.phase {
            PairingPhase::Pending(pending) if pending.flow_id == input.flow_id => pending,
            PairingPhase::Pending(_) => return Err(lifecycle.terminal_error(input.flow_id)),
            PairingPhase::Idle | PairingPhase::Recovery(_) => {
                return Err(lifecycle.terminal_error(input.flow_id));
            }
        };
        if pending.owner_generation != input.owner_generation {
            let flow_id = pending.flow_id;
            lifecycle.phase = PairingPhase::Recovery(CreatorSyncIssue::EnrollmentUnknown);
            lifecycle.last_terminal = Some(TerminalFlow {
                flow_id,
                reason: TerminalReason::OwnerChanged,
            });
            return Err(CreatorSyncPairingError::OwnerChanged);
        }
        if !pending.state.constant_time_eq(&input.state) {
            return Err(CreatorSyncPairingError::StateMismatch);
        }

        let pending = match std::mem::replace(
            &mut lifecycle.phase,
            PairingPhase::Recovery(CreatorSyncIssue::EnrollmentUnknown),
        ) {
            PairingPhase::Pending(pending) => pending,
            PairingPhase::Idle | PairingPhase::Recovery(_) => {
                return Err(CreatorSyncPairingError::NoPendingPairing);
            }
        };
        lifecycle.last_terminal = Some(TerminalFlow {
            flow_id: pending.flow_id,
            reason: TerminalReason::Consumed,
        });
        let completed_flow_id = pending.flow_id;

        match self
            .execute_pairing_enrollment(pending, &input.pairing_code)
            .await
        {
            Ok(metadata) => {
                lifecycle.phase = PairingPhase::Idle;
                lifecycle.last_terminal = Some(TerminalFlow {
                    flow_id: completed_flow_id,
                    reason: TerminalReason::Completed,
                });
                *self.inner.validation.lock().await = AdmissionValidation::Validated;
                *self
                    .inner
                    .remote_override
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                Ok(active_status(&metadata))
            }
            Err(error) => {
                lifecycle.phase = PairingPhase::Recovery(match error {
                    CreatorSyncPairingError::RemoteUnavailable => {
                        CreatorSyncIssue::RemoteUnavailable
                    }
                    CreatorSyncPairingError::RateLimited => CreatorSyncIssue::RemoteRateLimited,
                    CreatorSyncPairingError::PairingRejected => CreatorSyncIssue::PairingRejected,
                    _ => CreatorSyncIssue::EnrollmentUnknown,
                });
                Err(error)
            }
        }
    }

    pub fn cancel_pairing(
        &self,
        input: CreatorSyncPairingCancelInput,
    ) -> Result<CreatorSyncStatus, CreatorSyncPairingError> {
        if !self.inner.configured {
            return Err(CreatorSyncPairingError::Unconfigured);
        }
        let flow_id =
            Uuid::parse_str(&input.flow_id).map_err(|_| CreatorSyncPairingError::InvalidFlowId)?;
        let mut lifecycle = self
            .inner
            .pairing
            .try_lock()
            .map_err(|_| CreatorSyncPairingError::PairingAlreadyPending)?;
        lifecycle.expire_if_needed(self.inner.clock.now());
        let pending = match &lifecycle.phase {
            PairingPhase::Pending(pending) if pending.flow_id == flow_id => pending,
            PairingPhase::Pending(_) | PairingPhase::Idle | PairingPhase::Recovery(_) => {
                return Err(lifecycle.terminal_error(flow_id));
            }
        };
        if pending.owner_generation != input.owner_generation {
            lifecycle.phase = PairingPhase::Recovery(CreatorSyncIssue::EnrollmentUnknown);
            lifecycle.last_terminal = Some(TerminalFlow {
                flow_id,
                reason: TerminalReason::OwnerChanged,
            });
            return Err(CreatorSyncPairingError::OwnerChanged);
        }
        lifecycle.phase = PairingPhase::Idle;
        lifecycle.last_terminal = Some(TerminalFlow {
            flow_id,
            reason: TerminalReason::Cancelled,
        });
        Ok(status(
            CreatorSyncAdmissionState::PairingRequired,
            Some(CreatorSyncAction::BeginPairing),
            Some(CreatorSyncIssue::PairingCancelled),
            None,
        ))
    }

    async fn execute_pairing_enrollment(
        &self,
        pending: PendingPairing,
        pairing_code: &PairingCode,
    ) -> Result<CreatorSyncCredentialMetadata, CreatorSyncPairingError> {
        let installation = match self.post_pairing(&pending, pairing_code).await {
            Ok(installation) => installation,
            Err(PairingPostFailure::ReadbackOnly) => {
                let readback = match self.read_installation(&pending.candidate).await {
                    Ok(readback) => readback,
                    Err(ReadbackFailure::Unavailable) => {
                        return Err(CreatorSyncPairingError::RemoteUnavailable);
                    }
                    Err(_) => return Err(CreatorSyncPairingError::RecoveryRequired),
                };
                match readback {
                    Some(installation)
                        if candidate_matches(&pending.candidate, &installation)
                            && !projection_contains_secret(
                                &installation,
                                &pending,
                                pairing_code,
                            ) =>
                    {
                        installation
                    }
                    Some(_) | None => return Err(CreatorSyncPairingError::RecoveryRequired),
                }
            }
            Err(PairingPostFailure::ExplicitRetry) => {
                match self.read_installation(&pending.candidate).await {
                    Ok(Some(installation))
                        if candidate_matches(&pending.candidate, &installation)
                            && !projection_contains_secret(
                                &installation,
                                &pending,
                                pairing_code,
                            ) =>
                    {
                        installation
                    }
                    Ok(Some(_)) => return Err(CreatorSyncPairingError::RecoveryRequired),
                    Ok(None) => self
                        .post_pairing(&pending, pairing_code)
                        .await
                        .map_err(|_| CreatorSyncPairingError::RecoveryRequired)?,
                    Err(ReadbackFailure::Unavailable) => {
                        return Err(CreatorSyncPairingError::RemoteUnavailable);
                    }
                    Err(_) => return Err(CreatorSyncPairingError::RecoveryRequired),
                }
            }
            Err(PairingPostFailure::PairingRejected) => {
                return Err(CreatorSyncPairingError::PairingRejected);
            }
            Err(PairingPostFailure::RateLimited) => {
                return Err(CreatorSyncPairingError::RateLimited);
            }
            Err(PairingPostFailure::RecoveryRequired) => {
                return Err(CreatorSyncPairingError::RecoveryRequired);
            }
        };
        if projection_contains_secret(&installation, &pending, pairing_code) {
            return Err(CreatorSyncPairingError::RecoveryRequired);
        }
        let readback = self
            .read_installation(&pending.candidate)
            .await
            .map_err(|_| CreatorSyncPairingError::RecoveryRequired)?
            .filter(|readback| {
                *readback == installation
                    && !projection_contains_secret(readback, &pending, pairing_code)
            })
            .ok_or(CreatorSyncPairingError::RecoveryRequired)?;
        let probe = self
            .read_credential_probe(&pending.candidate)
            .await
            .map_err(|_| CreatorSyncPairingError::RecoveryRequired)?;
        if probe_contains_secret(&probe, &pending, pairing_code) {
            return Err(CreatorSyncPairingError::RecoveryRequired);
        }
        let metadata = verified_metadata(&readback, &probe)
            .map_err(|_| CreatorSyncPairingError::RecoveryRequired)?;
        self.inner
            .credentials
            .save(&metadata, pending.candidate.bearer)
            .map_err(|_| CreatorSyncPairingError::RecoveryRequired)?;
        Ok(metadata)
    }

    async fn post_pairing(
        &self,
        pending: &PendingPairing,
        pairing_code: &PairingCode,
    ) -> Result<InstallationProjection, PairingPostFailure> {
        let body = serde_json::to_vec(&wire::EnrollmentRequest {
            pairing_code: pairing_code.expose(),
            code_verifier: pending.verifier.expose(),
            installation_id: &pending.candidate.installation_id.to_string(),
            credential_id: &pending.candidate.credential_id.to_string(),
            bearer: pending.candidate.bearer.expose(),
            client_platform: client_platform_str(pending.candidate.client_platform),
            app_version: &pending.candidate.app_version,
        })
        .map_err(|_| PairingPostFailure::RecoveryRequired)?;
        if body.len() > 4 * 1024 {
            return Err(PairingPostFailure::RecoveryRequired);
        }
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::POST,
                endpoint: self.inner.endpoints.installations(),
                authorization: None,
                idempotency_key: Some(pending.idempotency_key.expose().to_owned()),
                json_body: true,
                body,
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(|_| PairingPostFailure::ReadbackOnly)?;
        match response.status {
            StatusCode::OK | StatusCode::CREATED => {
                let expected_replayed = response.status == StatusCode::OK;
                let parsed = parse_json::<wire::EnrollmentResponse>(&response.body)
                    .map_err(|_| PairingPostFailure::RecoveryRequired)?;
                if parsed.replayed != expected_replayed
                    || !candidate_matches(&pending.candidate, &parsed.installation)
                    || projection_contains_secret(&parsed.installation, pending, pairing_code)
                {
                    return Err(PairingPostFailure::RecoveryRequired);
                }
                Ok(parsed.installation)
            }
            status => Err(classify_pairing_error(status, &response.body)),
        }
    }

    async fn read_installation(
        &self,
        candidate: &InstallationCandidate,
    ) -> Result<Option<InstallationProjection>, ReadbackFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::GET,
                endpoint: self.inner.endpoints.installation(candidate.installation_id),
                authorization: Some(candidate.bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(|_| ReadbackFailure::Unavailable)?;
        match response.status {
            StatusCode::OK => parse_json::<wire::InstallationReadback>(&response.body)
                .map(|readback| Some(readback.installation))
                .map_err(|_| ReadbackFailure::Protocol),
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::UNAUTHORIZED => {
                self.handle_authoritative_non_active(
                    candidate.installation_id,
                    &candidate.bearer,
                    None,
                )
                .await;
                Err(ReadbackFailure::Unauthorized)
            }
            status if status.is_server_error() => Err(ReadbackFailure::Unavailable),
            _ => Err(ReadbackFailure::Protocol),
        }
    }

    async fn read_credential_probe(
        &self,
        candidate: &InstallationCandidate,
    ) -> Result<CredentialProbe, ReadbackFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: Some(crate::ACCESS_BUNDLE_FORMAT_VERSION),
                method: Method::GET,
                endpoint: self.inner.endpoints.credential_probe(),
                authorization: Some(candidate.bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(|_| ReadbackFailure::Unavailable)?;
        match response.status {
            StatusCode::OK => parse_json(&response.body).map_err(|_| ReadbackFailure::Protocol),
            StatusCode::UNAUTHORIZED => {
                self.handle_authoritative_non_active(
                    candidate.installation_id,
                    &candidate.bearer,
                    None,
                )
                .await;
                Err(ReadbackFailure::Unauthorized)
            }
            status if status.is_server_error() => Err(ReadbackFailure::Unavailable),
            _ => Err(ReadbackFailure::Protocol),
        }
    }
}

pub(super) struct BearerSecret(ZeroizingString);

impl BearerSecret {
    pub(super) fn generate(credential_id: Uuid) -> Self {
        Self(ZeroizingString::new(format!(
            "{credential_id}_{}",
            random_base64url_43().expose()
        )))
    }

    pub(super) fn from_zeroizing(
        credential_id: Uuid,
        value: Zeroizing<String>,
    ) -> Result<Self, ()> {
        let value = ZeroizingString(value);
        let prefix = format!("{credential_id}_");
        let secret = value.expose().strip_prefix(&prefix).ok_or(())?;
        if !(80..=165).contains(&value.expose().len())
            || !(43..=128).contains(&secret.len())
            || !secret
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(());
        }
        Ok(Self(value))
    }

    pub(super) fn expose(&self) -> &str {
        self.0.expose()
    }

    pub(super) fn authorization(&self) -> String {
        format!("Bearer {}", self.expose())
    }

    pub(super) fn into_zeroizing(self) -> Zeroizing<String> {
        self.0.into_zeroizing()
    }
}

struct InstallationCandidate {
    installation_id: Uuid,
    credential_id: Uuid,
    bearer: BearerSecret,
    client_platform: CreatorSyncClientPlatform,
    app_version: String,
}

impl InstallationCandidate {
    fn generate(client_platform: CreatorSyncClientPlatform, app_version: String) -> Self {
        let installation_id = Uuid::new_v4();
        let mut credential_id = Uuid::new_v4();
        while credential_id == installation_id {
            credential_id = Uuid::new_v4();
        }
        Self {
            installation_id,
            credential_id,
            bearer: BearerSecret::generate(credential_id),
            client_platform,
            app_version,
        }
    }
}

enum PairingPostFailure {
    ReadbackOnly,
    ExplicitRetry,
    PairingRejected,
    RateLimited,
    RecoveryRequired,
}

enum ReadbackFailure {
    Unavailable,
    Unauthorized,
    Protocol,
}

fn build_authorize_url(
    endpoints: &CreatorSyncEndpoints,
    app_version: &str,
    callback_port: u16,
    platform: CreatorSyncClientPlatform,
    verifier: &CodeVerifier,
    state: &PairingState,
    device_label: &crate::DeviceLabel,
) -> Result<PairingAuthorizeUrl, CreatorSyncPairingError> {
    let challenge = pkce_challenge(verifier);
    endpoints
        .pairing_authorize_url(crate::config::PairingAuthorizeRequest {
            app_version,
            callback_port,
            code_challenge: &challenge,
            platform: client_platform_str(platform),
            state: state.expose(),
            label: device_label,
        })
        .map(|url| PairingAuthorizeUrl(ZeroizingString::new(url)))
        .map_err(|_| CreatorSyncPairingError::RecoveryRequired)
}

fn pkce_challenge(verifier: &CodeVerifier) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose().as_bytes()))
}

fn random_base64url_43() -> ZeroizingString {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    ZeroizingString::new(encoded)
}

fn is_base64url_43(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_canonical_base64url_32(value: &str) -> bool {
    if !is_base64url_43(value) {
        return false;
    }
    let Ok(mut decoded) = URL_SAFE_NO_PAD.decode(value) else {
        return false;
    };
    let canonical = decoded.len() == 32 && URL_SAFE_NO_PAD.encode(&decoded) == value;
    decoded.zeroize();
    canonical
}

pub(super) fn client_platform_str(platform: CreatorSyncClientPlatform) -> &'static str {
    match platform {
        CreatorSyncClientPlatform::Macos => "macos",
        CreatorSyncClientPlatform::Windows => "windows",
    }
}

fn candidate_matches(
    candidate: &InstallationCandidate,
    installation: &InstallationProjection,
) -> bool {
    installation.installation_id == candidate.installation_id.to_string()
        && installation.credential_id == candidate.credential_id.to_string()
        && installation.client_platform == client_platform_str(candidate.client_platform)
        && installation.app_version == candidate.app_version
        && installation.status == "active"
        && installation.revision > 0
        && installation.revoked_at.is_none()
}

fn projection_contains_secret(
    projection: &InstallationProjection,
    pending: &PendingPairing,
    pairing_code: &PairingCode,
) -> bool {
    [
        projection.installation_id.as_str(),
        projection.credential_id.as_str(),
        projection.access_user_id.as_str(),
        projection.access_device_id.as_str(),
        projection.source_instance_id.as_str(),
        projection.access_pairing_id.as_str(),
        projection.client_platform.as_str(),
        projection.app_version.as_str(),
        projection.status.as_str(),
    ]
    .into_iter()
    .any(|value| value_contains_pairing_secret(value, pending, pairing_code))
}

fn probe_contains_secret(
    probe: &CredentialProbe,
    pending: &PendingPairing,
    pairing_code: &PairingCode,
) -> bool {
    [
        probe.protocol.as_str(),
        probe.installation_id.as_str(),
        probe.credential_id.as_str(),
        probe.access_user_id.as_str(),
        probe.access_device_id.as_str(),
        probe.source_instance_id.as_str(),
        probe.access_pairing_id.as_str(),
        probe.capabilities[0].as_str(),
        probe.accepted_bundle.format.as_str(),
        probe.accepted_bundle.profile.as_str(),
    ]
    .into_iter()
    .any(|value| value_contains_pairing_secret(value, pending, pairing_code))
}

fn value_contains_pairing_secret(
    value: &str,
    pending: &PendingPairing,
    pairing_code: &PairingCode,
) -> bool {
    value.contains(pairing_code.expose())
        || value.contains(pending.verifier.expose())
        || value.contains(pending.state.expose())
        || value.contains(pending.candidate.bearer.expose())
}

fn classify_pairing_error(status: StatusCode, body: &[u8]) -> PairingPostFailure {
    let parsed = parse_json::<wire::ErrorEnvelope>(body).ok();
    let error = parsed.as_ref().map(|value| value.error.as_str());
    match (status, error) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Some("enrollment_rate_limited" | "enrollment_attempt_quota_exceeded"),
        ) => PairingPostFailure::RateLimited,
        (StatusCode::SERVICE_UNAVAILABLE, Some("enrollment_in_progress"))
            if parsed.as_ref().is_some_and(|value| {
                value.retry_safe == Some(true)
                    && value.action.as_deref() == Some("retry_same_request_for_readback")
            }) =>
        {
            PairingPostFailure::ExplicitRetry
        }
        (
            StatusCode::GONE
            | StatusCode::UNPROCESSABLE_ENTITY
            | StatusCode::FORBIDDEN
            | StatusCode::TOO_MANY_REQUESTS,
            Some(
                "pairing_consumed"
                | "pairing_expired"
                | "pairing_invalid"
                | "pairing_rate_limited"
                | "pairing_revoked",
            ),
        ) => PairingPostFailure::PairingRejected,
        (StatusCode::SERVICE_UNAVAILABLE, Some("pairing_unavailable"))
            if parsed
                .as_ref()
                .is_some_and(|value| value.retry_safe == Some(false)) =>
        {
            PairingPostFailure::PairingRejected
        }
        (StatusCode::CONFLICT, Some("enrollment_outcome_unknown"))
            if parsed
                .as_ref()
                .is_some_and(|value| value.retry_safe == Some(false)) =>
        {
            PairingPostFailure::RecoveryRequired
        }
        (status, _) if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS => {
            PairingPostFailure::ReadbackOnly
        }
        _ => PairingPostFailure::RecoveryRequired,
    }
}

pub(super) fn valid_app_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(1..=32).contains(&bytes.len()) || !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    let mut separator = false;
    for byte in &bytes[1..] {
        if byte.is_ascii_alphanumeric() {
            separator = false;
        } else if matches!(byte, b'.' | b'+' | b'-') && !separator {
            separator = true;
        } else {
            return false;
        }
    }
    !separator
}

pub(super) enum PairingStatus {
    DeferToVault(Option<CreatorSyncIssue>),
    Status(CreatorSyncStatus),
}

pub(super) fn pairing_status(lifecycle: &mut PairingLifecycle, now: Instant) -> PairingStatus {
    lifecycle.expire_if_needed(now);
    match &lifecycle.phase {
        PairingPhase::Idle => {
            let issue =
                lifecycle
                    .last_terminal
                    .as_ref()
                    .and_then(|terminal| match terminal.reason {
                        TerminalReason::Expired => Some(CreatorSyncIssue::PairingExpired),
                        TerminalReason::Cancelled => Some(CreatorSyncIssue::PairingCancelled),
                        TerminalReason::OwnerChanged | TerminalReason::Consumed => {
                            Some(CreatorSyncIssue::EnrollmentUnknown)
                        }
                        TerminalReason::Completed => None,
                    });
            PairingStatus::DeferToVault(issue)
        }
        PairingPhase::Pending(_) => PairingStatus::Status(status(
            CreatorSyncAdmissionState::PairingPending,
            Some(CreatorSyncAction::AwaitPairingCallback),
            None,
            None,
        )),
        PairingPhase::Recovery(issue) => PairingStatus::Status(status(
            if matches!(
                issue,
                CreatorSyncIssue::RemoteUnavailable | CreatorSyncIssue::RemoteRateLimited
            ) {
                CreatorSyncAdmissionState::TemporarilyUnavailable
            } else {
                CreatorSyncAdmissionState::RecoveryRequired
            },
            Some(
                if matches!(
                    issue,
                    CreatorSyncIssue::RemoteUnavailable | CreatorSyncIssue::RemoteRateLimited
                ) {
                    CreatorSyncAction::Retry
                } else {
                    CreatorSyncAction::BeginPairing
                },
            ),
            Some(*issue),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_s256_golden_matches() {
        let verifier = CodeVerifier(ZeroizingString::new(
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into(),
        ));
        assert_eq!(
            pkce_challenge(&verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }
}
