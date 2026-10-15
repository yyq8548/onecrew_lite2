//! Validated local-vault credential state and admission revalidation.

use onecrew_local_domain::{
    CreatorSyncAcceptedBundle, CreatorSyncBundleProfile, CreatorSyncCapability,
    CreatorSyncClientPlatform, CreatorSyncCredentialMetadata, CreatorSyncCredentialStatus,
    CreatorSyncProtocol, ServiceCredentialSecret,
};
use onecrew_local_vault::{LocalVault, VaultCreatorSyncCredentialState};
use reqwest::{Method, StatusCode};
use uuid::Uuid;

use crate::{
    http::HttpRequest,
    model::{
        CreatorSyncAction, CreatorSyncAdmissionState, CreatorSyncIssue, CreatorSyncStatus,
        active_status, status,
    },
    snapshot_encode::{MAX_CHUNK_BYTES, MAX_CHUNKS, MAX_SNAPSHOT_BYTES},
    wire::{self, CredentialProbe, InstallationProjection},
};

use super::{
    CreatorSyncRuntime,
    enrollment::{BearerSecret, valid_app_version},
    parse_json,
};

const PROTOCOL_VERSION: u64 = 1;
const NAMESPACE_STAGED_BYTES: u64 = 100 * 1024 * 1024;
const ACCESS_USER_STAGED_BYTES: u64 = 400 * 1024 * 1024;

pub(super) struct StoredCredential {
    pub(super) metadata: CreatorSyncCredentialMetadata,
    pub(super) bearer: BearerSecret,
}

pub(super) enum StoredCredentialState {
    Active(StoredCredential),
    NonActive(CreatorSyncCredentialMetadata),
}

pub(super) trait CreatorSyncCredentialStore: Send + Sync {
    fn load(&self) -> Result<Option<StoredCredentialState>, ()>;
    fn save(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: BearerSecret,
    ) -> Result<(), ()>;
    fn invalidate(&self) -> Result<(), ()>;
    fn delete(&self) -> Result<(), ()>;
}

pub(super) struct LocalVaultCreatorSyncStore(LocalVault);

impl LocalVaultCreatorSyncStore {
    pub(super) fn new(vault: LocalVault) -> Self {
        Self(vault)
    }
}

impl CreatorSyncCredentialStore for LocalVaultCreatorSyncStore {
    fn load(&self) -> Result<Option<StoredCredentialState>, ()> {
        self.0
            .get_creator_sync_credential()
            .map_err(|_| ())?
            .map(|state| match state {
                VaultCreatorSyncCredentialState::Active { credential, bearer } => {
                    BearerSecret::from_zeroizing(
                        credential.metadata.credential_id,
                        bearer.into_zeroizing(),
                    )
                    .map(|bearer| {
                        StoredCredentialState::Active(StoredCredential {
                            metadata: credential.metadata,
                            bearer,
                        })
                    })
                }
                VaultCreatorSyncCredentialState::NonActive { credential } => {
                    Ok(StoredCredentialState::NonActive(credential.metadata))
                }
            })
            .transpose()
    }

    fn save(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: BearerSecret,
    ) -> Result<(), ()> {
        let bearer = ServiceCredentialSecret::from_zeroizing(bearer.into_zeroizing());
        self.0
            .save_creator_sync_credential(metadata, &bearer)
            .map(|_| ())
            .map_err(|_| ())
    }

    fn invalidate(&self) -> Result<(), ()> {
        self.0
            .invalidate_creator_sync_credential()
            .map_err(|_| ())?;
        match self.0.get_creator_sync_credential().map_err(|_| ())? {
            Some(VaultCreatorSyncCredentialState::NonActive { .. }) | None => Ok(()),
            Some(VaultCreatorSyncCredentialState::Active { .. }) => Err(()),
        }
    }

    fn delete(&self) -> Result<(), ()> {
        self.0.delete_creator_sync_credential().map_err(|_| ())?;
        if self
            .0
            .get_creator_sync_credential()
            .map_err(|_| ())?
            .is_some()
        {
            return Err(());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AdmissionValidation {
    Required,
    Validated,
    NonActive,
}

#[derive(Clone, Copy)]
pub(super) struct RemoteAdmissionOverride {
    pub(super) state: CreatorSyncAdmissionState,
    pub(super) cleanup_incomplete: bool,
}

pub(super) struct AuthoritativeNonActiveOutcome {
    pub(super) state: CreatorSyncAdmissionState,
    pub(super) cleanup_incomplete: bool,
}

impl CreatorSyncRuntime {
    /// Remove this installation's local credential after the host has drained
    /// business work and joined its pairing callback owner. This is local
    /// logout, not remote installation revocation. Both invalidation and
    /// deletion are verified by the credential store before success.
    pub async fn logout(&self) -> Result<(), crate::CreatorSyncSetupError> {
        let mut pairing = self.inner.pairing.lock().await;
        let mut validation = self.inner.validation.lock().await;
        *validation = AdmissionValidation::NonActive;
        *pairing = super::enrollment::PairingLifecycle::new();
        // Fail closed even if durable cleanup cannot complete.
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(RemoteAdmissionOverride {
            state: CreatorSyncAdmissionState::PairingRequired,
            cleanup_incomplete: true,
        });
        let credentials = self.inner.credentials.clone();
        tokio::task::spawn_blocking(move || {
            credentials.invalidate()?;
            credentials.delete()
        })
        .await
        .map_err(|_| crate::CreatorSyncSetupError::LocalVaultUnavailable)?
        .map_err(|_| crate::CreatorSyncSetupError::LocalVaultUnavailable)?;
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        Ok(())
    }

    pub(super) fn load_credential(&self) -> Result<Option<StoredCredentialState>, ()> {
        self.inner.credentials.load().and_then(|stored| {
            stored
                .map(|stored| match stored {
                    StoredCredentialState::Active(stored) => validate_active(stored)
                        .map(StoredCredentialState::Active)
                        .ok_or(()),
                    StoredCredentialState::NonActive(metadata) => validate_non_active(&metadata)
                        .then_some(StoredCredentialState::NonActive(metadata))
                        .ok_or(()),
                })
                .transpose()
        })
    }

    /// `load_credential` on the blocking pool, so a vault read (SQLite busy
    /// timeout, decryption) never stalls the async worker awaiting admission.
    /// A join failure reads as an unavailable credential.
    async fn load_credential_blocking(&self) -> Result<Option<StoredCredentialState>, ()> {
        let runtime = self.clone();
        tokio::task::spawn_blocking(move || runtime.load_credential())
            .await
            .unwrap_or(Err(()))
    }

    /// Validates the saved credential against Access. The validation lock is
    /// held across the whole call; every vault call runs on the blocking pool.
    pub async fn ensure_admission_validated(&self) -> CreatorSyncStatus {
        if !self.inner.configured {
            return crate::model::unconfigured_status();
        }
        let mut validation = self.inner.validation.lock().await;
        let stored = match self.load_credential_blocking().await {
            Ok(Some(StoredCredentialState::Active(stored))) => stored,
            Ok(Some(StoredCredentialState::NonActive(_))) | Ok(None) => {
                *validation = AdmissionValidation::NonActive;
                return pairing_required_status(None);
            }
            Err(()) => {
                *validation = AdmissionValidation::Required;
                return validation_required_status(Some(CreatorSyncIssue::CredentialUnavailable));
            }
        };
        if *validation == AdmissionValidation::Validated {
            return active_status(&stored.metadata);
        }

        let installation = match self.read_validation_installation(&stored).await {
            Ok(installation) if installation.status == "revoked" => {
                *validation = AdmissionValidation::NonActive;
                let outcome = self
                    .handle_authoritative_non_active(
                        stored.metadata.installation_id,
                        &stored.bearer,
                        Some(CreatorSyncAdmissionState::Revoked),
                    )
                    .await;
                return override_status(outcome);
            }
            Ok(installation) => installation,
            Err(ValidationFailure::Unauthorized) => {
                *validation = AdmissionValidation::NonActive;
                let outcome = self
                    .handle_authoritative_non_active(
                        stored.metadata.installation_id,
                        &stored.bearer,
                        None,
                    )
                    .await;
                return override_status(outcome);
            }
            Err(ValidationFailure::Unavailable) => {
                *validation = AdmissionValidation::Required;
                return validation_unavailable_status();
            }
            Err(ValidationFailure::Protocol) => {
                *validation = AdmissionValidation::Required;
                return validation_required_status(Some(CreatorSyncIssue::ProtocolMismatch));
            }
        };
        let probe = match self.read_validation_probe(&stored).await {
            Ok(probe) => probe,
            Err(ValidationFailure::Unauthorized) => {
                *validation = AdmissionValidation::NonActive;
                let outcome = self
                    .handle_authoritative_non_active(
                        stored.metadata.installation_id,
                        &stored.bearer,
                        None,
                    )
                    .await;
                return override_status(outcome);
            }
            Err(ValidationFailure::Unavailable) => {
                *validation = AdmissionValidation::Required;
                return validation_unavailable_status();
            }
            Err(ValidationFailure::Protocol) => {
                *validation = AdmissionValidation::Required;
                return validation_required_status(Some(CreatorSyncIssue::ProtocolMismatch));
            }
        };
        let mut verified = match verified_metadata(&installation, &probe) {
            Ok(verified) if validation_metadata_matches(&stored.metadata, &verified) => verified,
            _ => {
                *validation = AdmissionValidation::Required;
                return validation_required_status(Some(CreatorSyncIssue::ProtocolMismatch));
            }
        };
        // Remote format capability is an observation, not a credential identity
        // or a migration of the persisted enrollment contract. Uploads probe it
        // afresh; the last-seen refresh preserves the original stored version.
        verified.accepted_bundle.format_version = stored.metadata.accepted_bundle.format_version;
        if verified.last_seen_at != stored.metadata.last_seen_at {
            let credentials = self.inner.credentials.clone();
            let metadata = verified.clone();
            let bearer = stored.bearer;
            let saved = tokio::task::spawn_blocking(move || credentials.save(&metadata, bearer))
                .await
                .unwrap_or(Err(()));
            if saved.is_err() {
                *validation = AdmissionValidation::Required;
                return validation_required_status(Some(CreatorSyncIssue::LocalVaultUnavailable));
            }
        }
        *validation = AdmissionValidation::Validated;
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        active_status(&verified)
    }

    pub(super) fn admission_validation(&self) -> Option<AdmissionValidation> {
        self.inner
            .validation
            .try_lock()
            .ok()
            .map(|validation| *validation)
    }

    pub(super) async fn handle_authoritative_non_active(
        &self,
        installation_id: Uuid,
        bearer: &BearerSecret,
        known_state: Option<CreatorSyncAdmissionState>,
    ) -> AuthoritativeNonActiveOutcome {
        let initial = known_state.unwrap_or(CreatorSyncAdmissionState::Unauthorized);
        self.set_process_non_active(initial, false);

        let state = if known_state.is_some() {
            initial
        } else {
            self.read_remote_non_active_state(installation_id, bearer)
                .await
        };
        self.set_process_non_active(state, false);

        // Vault cleanup and readback run on the blocking pool. A join failure
        // counts as incomplete cleanup, which keeps the process fail-closed.
        let credentials = self.inner.credentials.clone();
        let cleanup_complete = tokio::task::spawn_blocking(move || {
            let invalidate_ok = credentials.invalidate().is_ok();
            let delete_ok = credentials.delete().is_ok();
            let readback_ok = matches!(credentials.load(), Ok(None));
            invalidate_ok && delete_ok && readback_ok
        })
        .await
        .unwrap_or(false);
        let cleanup_incomplete = !cleanup_complete;
        self.set_process_non_active(state, cleanup_incomplete);
        AuthoritativeNonActiveOutcome {
            state,
            cleanup_incomplete,
        }
    }

    pub(super) fn status_from_vault(&self, issue: Option<CreatorSyncIssue>) -> CreatorSyncStatus {
        match self.load_credential() {
            Ok(Some(StoredCredentialState::Active(stored))) => match self.admission_validation() {
                Some(AdmissionValidation::Validated) => active_status(&stored.metadata),
                Some(AdmissionValidation::Required | AdmissionValidation::NonActive) => {
                    validation_required_status(issue)
                }
                None => status(CreatorSyncAdmissionState::Validating, None, None, None),
            },
            Ok(Some(StoredCredentialState::NonActive(_))) | Ok(None) => {
                pairing_required_status(issue)
            }
            Err(()) => validation_required_status(Some(CreatorSyncIssue::CredentialUnavailable)),
        }
    }

    fn set_process_non_active(&self, state: CreatorSyncAdmissionState, cleanup_incomplete: bool) {
        if let Ok(mut validation) = self.inner.validation.try_lock() {
            *validation = AdmissionValidation::NonActive;
        }
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(RemoteAdmissionOverride {
            state,
            cleanup_incomplete,
        });
    }

    async fn read_remote_non_active_state(
        &self,
        installation_id: Uuid,
        bearer: &BearerSecret,
    ) -> CreatorSyncAdmissionState {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::GET,
                endpoint: self.inner.endpoints.installation(installation_id),
                authorization: Some(bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await;
        match response {
            Ok(response) if response.status == StatusCode::OK => parse_json::<
                wire::InstallationReadback,
            >(&response.body)
            .ok()
            .filter(|readback| readback.installation.installation_id == installation_id.to_string())
            .filter(|readback| readback.installation.status == "revoked")
            .map_or(CreatorSyncAdmissionState::Unauthorized, |_| {
                CreatorSyncAdmissionState::Revoked
            }),
            _ => CreatorSyncAdmissionState::Unauthorized,
        }
    }

    async fn read_validation_installation(
        &self,
        stored: &StoredCredential,
    ) -> Result<InstallationProjection, ValidationFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::GET,
                endpoint: self
                    .inner
                    .endpoints
                    .installation(stored.metadata.installation_id),
                authorization: Some(stored.bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(|_| ValidationFailure::Unavailable)?;
        match response.status {
            StatusCode::OK => parse_json::<wire::InstallationReadback>(&response.body)
                .map(|readback| readback.installation)
                .map_err(|_| ValidationFailure::Protocol),
            StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND => {
                Err(ValidationFailure::Unauthorized)
            }
            status if status.is_server_error() => Err(ValidationFailure::Unavailable),
            _ => Err(ValidationFailure::Protocol),
        }
    }

    pub(super) async fn read_validation_probe(
        &self,
        stored: &StoredCredential,
    ) -> Result<CredentialProbe, ValidationFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: Some(crate::ACCESS_BUNDLE_FORMAT_VERSION),
                method: Method::GET,
                endpoint: self.inner.endpoints.credential_probe(),
                authorization: Some(stored.bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: crate::http::DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(|_| ValidationFailure::Unavailable)?;
        match response.status {
            StatusCode::OK => parse_json(&response.body).map_err(|_| ValidationFailure::Protocol),
            StatusCode::UNAUTHORIZED => Err(ValidationFailure::Unauthorized),
            status if status.is_server_error() => Err(ValidationFailure::Unavailable),
            _ => Err(ValidationFailure::Protocol),
        }
    }
}

pub(super) enum ValidationFailure {
    Unavailable,
    Unauthorized,
    Protocol,
}

pub(super) fn verified_metadata(
    installation: &InstallationProjection,
    probe: &CredentialProbe,
) -> Result<CreatorSyncCredentialMetadata, ()> {
    if installation.status != "active"
        || installation.revision == 0
        || installation.updated_at < installation.created_at
        || installation.revoked_at.is_some()
        || !valid_access_user_id(&installation.access_user_id)
        || !prefixed_base64url_24(&installation.access_device_id, "dev_")
        || !prefixed_base64url_24(&installation.access_pairing_id, "pair_")
        || probe.protocol != "reacher.creator_sync"
        || probe.protocol_version != PROTOCOL_VERSION
        || probe.installation_id != installation.installation_id
        || probe.credential_id != installation.credential_id
        || probe.access_user_id != installation.access_user_id
        || probe.access_device_id != installation.access_device_id
        || probe.source_instance_id != installation.source_instance_id
        || probe.access_pairing_id != installation.access_pairing_id
        || probe.capabilities != ["creator_snapshots:write_chunked_quarantine"]
        || probe.accepted_bundle.format != "reacher.creator_bundle"
        || !matches!(probe.accepted_bundle.format_version, 1..=3)
        || probe.accepted_bundle.profile != "portable_public"
        || probe.accepted_bundle.max_snapshot_bytes != MAX_SNAPSHOT_BYTES as u64
        || probe.accepted_bundle.max_chunk_bytes != MAX_CHUNK_BYTES as u64
        || probe.accepted_bundle.max_chunks != MAX_CHUNKS as u64
        || probe.accepted_bundle.max_namespace_staged_bytes != NAMESPACE_STAGED_BYTES
        || probe.accepted_bundle.max_access_user_staged_bytes != ACCESS_USER_STAGED_BYTES
    {
        return Err(());
    }
    let installation_id = Uuid::parse_str(&installation.installation_id).map_err(|_| ())?;
    let credential_id = Uuid::parse_str(&installation.credential_id).map_err(|_| ())?;
    let source_instance_id = Uuid::parse_str(&installation.source_instance_id).map_err(|_| ())?;
    if installation_id == credential_id {
        return Err(());
    }
    Ok(CreatorSyncCredentialMetadata {
        installation_id,
        credential_id,
        access_user_id: installation.access_user_id.clone(),
        access_device_id: installation.access_device_id.clone(),
        source_instance_id,
        access_pairing_id: installation.access_pairing_id.clone(),
        client_platform: parse_client_platform(&installation.client_platform)?,
        app_version: installation.app_version.clone(),
        status: CreatorSyncCredentialStatus::Active,
        revision: installation.revision,
        created_at: installation.created_at,
        updated_at: installation.updated_at,
        last_seen_at: installation.last_seen_at,
        revoked_at: installation.revoked_at,
        protocol: CreatorSyncProtocol::ReacherCreatorSync,
        protocol_version: probe.protocol_version,
        capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
        accepted_bundle: CreatorSyncAcceptedBundle {
            format: probe.accepted_bundle.format.clone(),
            format_version: probe.accepted_bundle.format_version,
            profile: CreatorSyncBundleProfile::PortablePublic,
            max_snapshot_bytes: probe.accepted_bundle.max_snapshot_bytes,
            max_chunk_bytes: probe.accepted_bundle.max_chunk_bytes,
            max_chunks: probe.accepted_bundle.max_chunks,
            max_namespace_staged_bytes: probe.accepted_bundle.max_namespace_staged_bytes,
            max_access_user_staged_bytes: probe.accepted_bundle.max_access_user_staged_bytes,
        },
    })
}

fn validation_metadata_matches(
    stored: &CreatorSyncCredentialMetadata,
    verified: &CreatorSyncCredentialMetadata,
) -> bool {
    let last_seen_is_monotonic = match (stored.last_seen_at, verified.last_seen_at) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(stored), Some(verified)) => verified >= stored,
    };
    if !last_seen_is_monotonic {
        return false;
    }

    // Worker B advances last_seen_at after authenticated device reads. Keep
    // every binding and protocol field exact. The supported bundle version is
    // separately checked for each upload and may change without re-enrollment.
    let mut normalized = verified.clone();
    normalized.last_seen_at = stored.last_seen_at;
    normalized.accepted_bundle.format_version = stored.accepted_bundle.format_version;
    normalized == *stored
}

/// A fresh capability read must belong to the already admitted identity. A
/// server retaining only v1 is a format mismatch, never bearer revocation.
pub(super) fn probe_accepts_current_bundle(
    metadata: &CreatorSyncCredentialMetadata,
    probe: &CredentialProbe,
) -> bool {
    let accepted = &probe.accepted_bundle;
    let enrolled = &metadata.accepted_bundle;
    probe.protocol == "reacher.creator_sync"
        && probe.protocol_version == metadata.protocol_version
        && probe.installation_id == metadata.installation_id.to_string()
        && probe.credential_id == metadata.credential_id.to_string()
        && probe.access_user_id == metadata.access_user_id
        && probe.access_device_id == metadata.access_device_id
        && probe.source_instance_id == metadata.source_instance_id.to_string()
        && probe.access_pairing_id == metadata.access_pairing_id
        && probe.capabilities == ["creator_snapshots:write_chunked_quarantine"]
        && accepted.format == enrolled.format
        && accepted.format_version == u64::from(crate::ACCESS_BUNDLE_FORMAT_VERSION)
        && accepted.profile == "portable_public"
        && accepted.max_snapshot_bytes == enrolled.max_snapshot_bytes
        && accepted.max_chunk_bytes == enrolled.max_chunk_bytes
        && accepted.max_chunks == enrolled.max_chunks
        && accepted.max_namespace_staged_bytes == enrolled.max_namespace_staged_bytes
        && accepted.max_access_user_staged_bytes == enrolled.max_access_user_staged_bytes
}

fn validate_active(stored: StoredCredential) -> Option<StoredCredential> {
    let metadata = &stored.metadata;
    let expected_prefix = format!("{}_", metadata.credential_id);
    let bearer = stored.bearer.expose();
    let metadata_contains_bearer = [
        metadata.access_user_id.as_str(),
        metadata.access_device_id.as_str(),
        metadata.access_pairing_id.as_str(),
        metadata.app_version.as_str(),
        metadata.accepted_bundle.format.as_str(),
    ]
    .into_iter()
    .any(|value| value.contains(bearer));
    if metadata.status != CreatorSyncCredentialStatus::Active
        || metadata.revoked_at.is_some()
        || !bearer.starts_with(&expected_prefix)
        || metadata_contains_bearer
        || !validate_contract_metadata(metadata)
    {
        return None;
    }
    Some(stored)
}

fn validate_non_active(metadata: &CreatorSyncCredentialMetadata) -> bool {
    metadata.status != CreatorSyncCredentialStatus::Active && validate_contract_metadata(metadata)
}

fn validate_contract_metadata(metadata: &CreatorSyncCredentialMetadata) -> bool {
    metadata.installation_id != metadata.credential_id
        && valid_access_user_id(&metadata.access_user_id)
        && prefixed_base64url_24(&metadata.access_device_id, "dev_")
        && prefixed_base64url_24(&metadata.access_pairing_id, "pair_")
        && valid_app_version(&metadata.app_version)
        && metadata.revision > 0
        && metadata.updated_at >= metadata.created_at
        && metadata.protocol == CreatorSyncProtocol::ReacherCreatorSync
        && metadata.protocol_version == PROTOCOL_VERSION
        && metadata.capabilities == [CreatorSyncCapability::WriteChunkedQuarantine]
        && metadata.accepted_bundle.format == "reacher.creator_bundle"
        && matches!(metadata.accepted_bundle.format_version, 1..=3)
        && metadata.accepted_bundle.profile == CreatorSyncBundleProfile::PortablePublic
        && metadata.accepted_bundle.max_snapshot_bytes == MAX_SNAPSHOT_BYTES as u64
        && metadata.accepted_bundle.max_chunk_bytes == MAX_CHUNK_BYTES as u64
        && metadata.accepted_bundle.max_chunks == MAX_CHUNKS as u64
        && metadata.accepted_bundle.max_namespace_staged_bytes == NAMESPACE_STAGED_BYTES
        && metadata.accepted_bundle.max_access_user_staged_bytes == ACCESS_USER_STAGED_BYTES
}

fn parse_client_platform(value: &str) -> Result<CreatorSyncClientPlatform, ()> {
    match value {
        "macos" => Ok(CreatorSyncClientPlatform::Macos),
        "windows" => Ok(CreatorSyncClientPlatform::Windows),
        _ => Err(()),
    }
}

fn valid_access_user_id(value: &str) -> bool {
    (1..=128).contains(&value.encode_utf16().count())
}

fn prefixed_base64url_24(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 24
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    })
}

fn validation_required_status(issue: Option<CreatorSyncIssue>) -> CreatorSyncStatus {
    status(
        CreatorSyncAdmissionState::ValidationRequired,
        Some(CreatorSyncAction::ValidateAdmission),
        issue.or(Some(CreatorSyncIssue::AdmissionValidationRequired)),
        None,
    )
}

fn pairing_required_status(issue: Option<CreatorSyncIssue>) -> CreatorSyncStatus {
    status(
        CreatorSyncAdmissionState::PairingRequired,
        Some(CreatorSyncAction::BeginPairing),
        issue,
        None,
    )
}

fn validation_unavailable_status() -> CreatorSyncStatus {
    status(
        CreatorSyncAdmissionState::TemporarilyUnavailable,
        Some(CreatorSyncAction::ValidateAdmission),
        Some(CreatorSyncIssue::RemoteUnavailable),
        None,
    )
}

fn override_status(outcome: AuthoritativeNonActiveOutcome) -> CreatorSyncStatus {
    crate::model::status_for_remote_override(outcome.state, outcome.cleanup_incomplete)
}
