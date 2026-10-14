//! Shared Creator Sync runtime test support and invariant suites.

mod table_sync;
mod coordinator;
mod encoding;
mod enrollment;
mod snapshot;
mod store;

use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use onecrew_creator_runtime::CreatorBundle;
use onecrew_local_domain::{
    CreatorSyncAcceptedBundle, CreatorSyncBundleProfile, CreatorSyncCapability,
    CreatorSyncClientPlatform, CreatorSyncCredentialMetadata, CreatorSyncCredentialStatus,
    CreatorSyncProtocol,
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::sync::Notify;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{
    CreatorSnapshotSyncState, CreatorSyncAdmissionState, CreatorSyncIssue,
    CreatorSyncPairingBeginInput, CreatorSyncPairingBeginResult, CreatorSyncPairingCancelInput,
    CreatorSyncPairingCompleteInput, CreatorSyncPairingError, PairingCode, PairingState,
    snapshot_encode,
};

use super::*;

const SOURCE_INSTANCE_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";

struct FakeStore {
    value: Mutex<Option<FakeCredentialState>>,
    fail_load: AtomicBool,
    fail_save: AtomicBool,
    fail_invalidate: AtomicBool,
    fail_delete: AtomicBool,
    fail_readback_after_delete: AtomicBool,
    cleanup_calls: Mutex<Vec<&'static str>>,
}

enum FakeCredentialState {
    Active {
        metadata: CreatorSyncCredentialMetadata,
        bearer: Zeroizing<String>,
    },
    NonActive {
        metadata: CreatorSyncCredentialMetadata,
    },
}

impl Default for FakeStore {
    fn default() -> Self {
        Self {
            value: Mutex::new(None),
            fail_load: AtomicBool::new(false),
            fail_save: AtomicBool::new(false),
            fail_invalidate: AtomicBool::new(false),
            fail_delete: AtomicBool::new(false),
            fail_readback_after_delete: AtomicBool::new(false),
            cleanup_calls: Mutex::new(Vec::new()),
        }
    }
}

impl FakeStore {
    fn with_credential(metadata: CreatorSyncCredentialMetadata, bearer: BearerSecret) -> Self {
        let store = Self::default();
        *store.value.lock().unwrap() = Some(FakeCredentialState::Active {
            metadata,
            bearer: bearer.into_zeroizing(),
        });
        store
    }

    fn with_non_active(mut metadata: CreatorSyncCredentialMetadata) -> Self {
        metadata.status = CreatorSyncCredentialStatus::Invalidated;
        let store = Self::default();
        *store.value.lock().unwrap() = Some(FakeCredentialState::NonActive { metadata });
        store
    }
}

impl CreatorSyncCredentialStore for FakeStore {
    fn load(&self) -> Result<Option<StoredCredentialState>, ()> {
        if self.fail_load.load(Ordering::Acquire) {
            return Err(());
        }
        self.value
            .lock()
            .unwrap()
            .as_ref()
            .map(|stored| match stored {
                FakeCredentialState::Active { metadata, bearer } => BearerSecret::from_zeroizing(
                    metadata.credential_id,
                    Zeroizing::new(bearer.to_string()),
                )
                .map(|bearer| {
                    StoredCredentialState::Active(StoredCredential {
                        metadata: metadata.clone(),
                        bearer,
                    })
                }),
                FakeCredentialState::NonActive { metadata } => {
                    Ok(StoredCredentialState::NonActive(metadata.clone()))
                }
            })
            .transpose()
    }

    fn save(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: BearerSecret,
    ) -> Result<(), ()> {
        if self.fail_save.load(Ordering::Acquire) {
            return Err(());
        }
        *self.value.lock().unwrap() = Some(FakeCredentialState::Active {
            metadata: metadata.clone(),
            bearer: bearer.into_zeroizing(),
        });
        Ok(())
    }

    fn invalidate(&self) -> Result<(), ()> {
        self.cleanup_calls.lock().unwrap().push("invalidate");
        if self.fail_invalidate.load(Ordering::Acquire) {
            return Err(());
        }
        let mut value = self.value.lock().unwrap();
        if let Some(FakeCredentialState::Active { metadata, .. }) = value.take() {
            let mut metadata = metadata;
            metadata.status = CreatorSyncCredentialStatus::Invalidated;
            *value = Some(FakeCredentialState::NonActive { metadata });
        }
        Ok(())
    }

    fn delete(&self) -> Result<(), ()> {
        self.cleanup_calls.lock().unwrap().push("delete");
        if self.fail_delete.load(Ordering::Acquire) {
            return Err(());
        }
        *self.value.lock().unwrap() = None;
        if self.fail_readback_after_delete.load(Ordering::Acquire) {
            self.fail_load.store(true, Ordering::Release);
        }
        Ok(())
    }
}

struct CapturedRequest {
    creator_bundle_version: Option<u16>,
    method: Method,
    path: String,
    authorization: Option<String>,
    idempotency_key: Option<String>,
    json_body: bool,
    body: Vec<u8>,
}

impl From<HttpRequest> for CapturedRequest {
    fn from(mut request: HttpRequest) -> Self {
        Self {
            creator_bundle_version: request.creator_bundle_version,
            method: request.method.clone(),
            path: request.endpoint.path().to_owned(),
            authorization: request.authorization.take(),
            idempotency_key: request.idempotency_key.take(),
            json_body: request.json_body,
            body: std::mem::take(&mut request.body),
        }
    }
}

#[derive(Clone, Copy)]
enum PairingMode {
    Success,
    MismatchedReadback,
    TransportUnknownReadbackSuccess,
    TransportUnknownNotFound,
    TransportUnavailable,
    GenericUnavailableNotFound,
    InProgressThenSuccess,
    SecretLeakCreate,
    SecretLeakReadback,
    SecretLeakProbe,
    PairingRejected,
    EnrollmentRateLimited,
    StartupRevokedThenSuccess,
    EnrollmentUnauthorized,
}

struct PairingFake {
    mode: PairingMode,
    requests: Mutex<Vec<CapturedRequest>>,
    candidate: Mutex<Option<Value>>,
    post_count: AtomicUsize,
    startup_installation: Mutex<Option<Value>>,
}

impl PairingFake {
    fn new(mode: PairingMode) -> Self {
        Self {
            mode,
            requests: Mutex::new(Vec::new()),
            candidate: Mutex::new(None),
            post_count: AtomicUsize::new(0),
            startup_installation: Mutex::new(None),
        }
    }

    fn with_startup_revoked(metadata: &CreatorSyncCredentialMetadata) -> Self {
        let fake = Self::new(PairingMode::StartupRevokedThenSuccess);
        *fake.startup_installation.lock().unwrap() = Some(json!({
            "installation_id": metadata.installation_id,
            "credential_id": metadata.credential_id,
            "access_user_id": metadata.access_user_id,
            "access_device_id": metadata.access_device_id,
            "source_instance_id": metadata.source_instance_id,
            "access_pairing_id": metadata.access_pairing_id,
            "client_platform": client_platform_str(metadata.client_platform),
            "app_version": metadata.app_version,
            "status": "revoked",
            "revision": metadata.revision + 1,
            "created_at": metadata.created_at,
            "updated_at": metadata.updated_at + 1,
            "last_seen_at": metadata.last_seen_at,
            "revoked_at": metadata.updated_at + 1,
        }));
        fake
    }

    fn installation(&self, status: &str) -> Value {
        let candidate = self.candidate.lock().unwrap();
        let candidate = candidate.as_ref().unwrap();
        let access_user_id = if matches!(
            self.mode,
            PairingMode::SecretLeakCreate | PairingMode::SecretLeakReadback
        ) {
            candidate["pairing_code"].clone()
        } else {
            json!("access-user")
        };
        json!({
            "installation_id": candidate["installation_id"],
            "credential_id": candidate["credential_id"],
            "access_user_id": access_user_id,
            "access_device_id": "dev_000000000000000000000001",
            "source_instance_id": SOURCE_INSTANCE_ID,
            "access_pairing_id": "pair_000000000000000000000001",
            "client_platform": candidate["client_platform"],
            "app_version": candidate["app_version"],
            "status": status,
            "revision": 1,
            "created_at": 100,
            "updated_at": 100,
            "last_seen_at": null,
            "revoked_at": if status == "revoked" { Some(100_u64) } else { None },
        })
    }

    fn probe(&self) -> Value {
        let installation = self.installation("active");
        let access_device_id = if matches!(self.mode, PairingMode::SecretLeakProbe) {
            self.candidate.lock().unwrap().as_ref().unwrap()["code_verifier"].clone()
        } else {
            installation["access_device_id"].clone()
        };
        json!({
            "protocol": "reacher.creator_sync",
            "protocol_version": 1,
            "installation_id": installation["installation_id"],
            "credential_id": installation["credential_id"],
            "access_user_id": installation["access_user_id"],
            "access_device_id": access_device_id,
            "source_instance_id": installation["source_instance_id"],
            "access_pairing_id": installation["access_pairing_id"],
            "capabilities": ["creator_snapshots:write_chunked_quarantine"],
            "accepted_bundle": {
                "format": "reacher.creator_bundle",
                "format_version": 1,
                "profile": "portable_public",
                "max_snapshot_bytes": 32 * 1024 * 1024,
                "max_chunk_bytes": 1024 * 1024,
                "max_chunks": 128,
                "max_namespace_staged_bytes": 100 * 1024 * 1024,
                "max_access_user_staged_bytes": 400 * 1024 * 1024,
            },
        })
    }
}

#[async_trait]
impl HttpTransport for PairingFake {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        let captured = CapturedRequest::from(request);
        if captured.method == Method::POST && captured.path == "/installations" {
            let post_count = self.post_count.fetch_add(1, Ordering::AcqRel);
            let candidate: Value = serde_json::from_slice(&captured.body).unwrap();
            *self.candidate.lock().unwrap() = Some(candidate);
            if matches!(
                self.mode,
                PairingMode::TransportUnknownReadbackSuccess
                    | PairingMode::TransportUnknownNotFound
                    | PairingMode::TransportUnavailable
                    | PairingMode::SecretLeakReadback
            ) && post_count == 0
            {
                self.requests.lock().unwrap().push(captured);
                return Err(HttpFailure::Transport);
            }
            if matches!(self.mode, PairingMode::GenericUnavailableNotFound) && post_count == 0 {
                self.requests.lock().unwrap().push(captured);
                return Ok(json_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({"error": "temporarily_unavailable"}),
                ));
            }
            if matches!(self.mode, PairingMode::InProgressThenSuccess) && post_count == 0 {
                self.requests.lock().unwrap().push(captured);
                return Ok(json_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    json!({
                        "error": "enrollment_in_progress",
                        "action": "retry_same_request_for_readback",
                        "retry_safe": true,
                    }),
                ));
            }
            if matches!(self.mode, PairingMode::PairingRejected) {
                self.requests.lock().unwrap().push(captured);
                return Ok(json_response(
                    StatusCode::GONE,
                    json!({
                        "error": "pairing_consumed",
                        "action": "request_new_pairing",
                    }),
                ));
            }
            if matches!(self.mode, PairingMode::EnrollmentRateLimited) {
                self.requests.lock().unwrap().push(captured);
                return Ok(json_response(
                    StatusCode::TOO_MANY_REQUESTS,
                    json!({"error": "enrollment_attempt_quota_exceeded"}),
                ));
            }
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::CREATED,
                json!({"installation": self.installation("active"), "replayed": false}),
            ));
        }
        if captured.method == Method::GET && captured.path.starts_with("/installations/") {
            self.requests.lock().unwrap().push(captured);
            if matches!(self.mode, PairingMode::TransportUnavailable) {
                return Err(HttpFailure::Transport);
            }
            if matches!(self.mode, PairingMode::EnrollmentUnauthorized) {
                return Ok(json_response(
                    StatusCode::UNAUTHORIZED,
                    json!({"error": "unauthorized"}),
                ));
            }
            if matches!(self.mode, PairingMode::StartupRevokedThenSuccess)
                && self.post_count.load(Ordering::Acquire) == 0
            {
                return Ok(json_response(
                    StatusCode::OK,
                    json!({
                        "installation": self.startup_installation.lock().unwrap().clone().unwrap()
                    }),
                ));
            }
            if self.post_count.load(Ordering::Acquire) == 1
                && matches!(
                    self.mode,
                    PairingMode::TransportUnknownNotFound
                        | PairingMode::GenericUnavailableNotFound
                        | PairingMode::InProgressThenSuccess
                )
            {
                return Ok(json_response(
                    StatusCode::NOT_FOUND,
                    json!({"error": "not_found"}),
                ));
            }
            let mut installation = self.installation("active");
            if matches!(self.mode, PairingMode::MismatchedReadback) {
                installation["access_device_id"] = json!("dev_000000000000000000000002");
            }
            return Ok(json_response(
                StatusCode::OK,
                json!({"installation": installation}),
            ));
        }
        if captured.method == Method::GET && captured.path == "/creator-sync/credential" {
            assert_eq!(captured.creator_bundle_version, Some(3));
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(StatusCode::OK, self.probe()));
        }
        panic!("unexpected fake pairing request")
    }
}

struct SnapshotState {
    snapshot_id: Uuid,
    content_sha256: Option<String>,
    chunk_count: usize,
    received: BTreeMap<usize, usize>,
    completed: bool,
    pending_updates: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
enum SnapshotMode {
    Normal,
    ProbeV1,
    ProbeV2,
    ProbeWrongBinding,
    ProbeUnavailable,
    PlanUnknownField,
    PlanUnknownCreator,
    PlanDuplicateCreator,
    PlanWrongDigestEcho,
    PlanTransportOnce,
    ManifestChangedOnce,
    UnauthorizedRevoked,
    UnauthorizedUnrecognized,
    InstallationNotFound,
}

struct SnapshotFake {
    mode: SnapshotMode,
    metadata: CreatorSyncCredentialMetadata,
    requests: Mutex<Vec<CapturedRequest>>,
    state: Mutex<SnapshotState>,
    remote_manifest: Arc<Mutex<BTreeMap<String, String>>>,
    plan_count: AtomicUsize,
    manifest_conflict_count: AtomicUsize,
}

impl SnapshotFake {
    fn new(mode: SnapshotMode, metadata: CreatorSyncCredentialMetadata) -> Self {
        Self::with_remote_manifest(mode, metadata, Arc::new(Mutex::new(BTreeMap::new())))
    }

    fn with_remote_manifest(
        mode: SnapshotMode,
        metadata: CreatorSyncCredentialMetadata,
        remote_manifest: Arc<Mutex<BTreeMap<String, String>>>,
    ) -> Self {
        Self {
            mode,
            metadata,
            requests: Mutex::new(Vec::new()),
            state: Mutex::new(SnapshotState {
                snapshot_id: Uuid::new_v4(),
                content_sha256: None,
                chunk_count: 0,
                received: BTreeMap::new(),
                completed: false,
                pending_updates: BTreeMap::new(),
            }),
            remote_manifest,
            plan_count: AtomicUsize::new(0),
            manifest_conflict_count: AtomicUsize::new(0),
        }
    }

    fn manifest_sha256(manifest: &BTreeMap<String, String>) -> String {
        let pairs = manifest
            .iter()
            .map(|(record_key, semantic_sha256)| json!([record_key, semantic_sha256]))
            .collect::<Vec<_>>();
        snapshot_encode::sha256_digest(&serde_json::to_vec(&pairs).unwrap())
    }

    fn snapshot_projection(state: &SnapshotState) -> Value {
        let missing = (0..state.chunk_count)
            .filter(|index| !state.received.contains_key(index))
            .collect::<Vec<_>>();
        json!({
            "snapshot_id": state.snapshot_id,
            "content_sha256": state.content_sha256.as_ref().unwrap(),
            "upload_status": if state.completed { "complete" } else { "uploading" },
            "review_status": if state.completed { Some("quarantined") } else { None },
            "chunk_count": state.chunk_count,
            "received_chunk_count": state.received.len(),
            "received_bytes": state.received.values().sum::<usize>(),
            "missing_chunk_indices": missing,
        })
    }

    fn installation(&self, status: &str) -> Value {
        json!({
            "installation_id": self.metadata.installation_id,
            "credential_id": self.metadata.credential_id,
            "access_user_id": self.metadata.access_user_id,
            "access_device_id": self.metadata.access_device_id,
            "source_instance_id": self.metadata.source_instance_id,
            "access_pairing_id": self.metadata.access_pairing_id,
            "client_platform": client_platform_str(self.metadata.client_platform),
            "app_version": self.metadata.app_version,
            "status": status,
            "revision": if status == "revoked" {
                self.metadata.revision + 1
            } else {
                self.metadata.revision
            },
            "created_at": self.metadata.created_at,
            "updated_at": if status == "revoked" {
                self.metadata.updated_at + 1
            } else {
                self.metadata.updated_at
            },
            "last_seen_at": self.metadata.last_seen_at,
            "revoked_at": if status == "revoked" {
                Some(self.metadata.updated_at + 1)
            } else {
                None
            },
        })
    }

    fn probe(&self) -> Value {
        json!({
            "protocol": "reacher.creator_sync",
            "protocol_version": self.metadata.protocol_version,
            "installation_id": self.metadata.installation_id,
            "credential_id": self.metadata.credential_id,
            "access_user_id": self.metadata.access_user_id,
            "access_device_id": self.metadata.access_device_id,
            "source_instance_id": self.metadata.source_instance_id,
            "access_pairing_id": self.metadata.access_pairing_id,
            "capabilities": ["creator_snapshots:write_chunked_quarantine"],
            "accepted_bundle": {
                "format": self.metadata.accepted_bundle.format,
                "format_version": match self.mode { SnapshotMode::ProbeV1 => 1, SnapshotMode::ProbeV2 => 2, _ => 3 },
                "profile": "portable_public",
                "max_snapshot_bytes": self.metadata.accepted_bundle.max_snapshot_bytes,
                "max_chunk_bytes": self.metadata.accepted_bundle.max_chunk_bytes,
                "max_chunks": self.metadata.accepted_bundle.max_chunks,
                "max_namespace_staged_bytes": self.metadata.accepted_bundle.max_namespace_staged_bytes,
                "max_access_user_staged_bytes": self.metadata.accepted_bundle.max_access_user_staged_bytes,
            },
        })
    }
}

#[async_trait]
impl HttpTransport for SnapshotFake {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        let captured = CapturedRequest::from(request);
        if matches!(
            self.mode,
            SnapshotMode::UnauthorizedRevoked | SnapshotMode::UnauthorizedUnrecognized
        ) && matches!(
            captured.path.as_str(),
            "/creator-backups/plan" | "/creator-snapshots"
        ) {
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::UNAUTHORIZED,
                json!({"error": "unauthorized"}),
            ));
        }
        if matches!(self.mode, SnapshotMode::UnauthorizedRevoked)
            && captured.path.starts_with("/installations/")
        {
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::OK,
                json!({"installation": self.installation("revoked")}),
            ));
        }
        if matches!(self.mode, SnapshotMode::UnauthorizedUnrecognized)
            && captured.path.starts_with("/installations/")
        {
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::UNAUTHORIZED,
                json!({"error": "unauthorized"}),
            ));
        }
        if matches!(self.mode, SnapshotMode::InstallationNotFound)
            && captured.path.starts_with("/installations/")
        {
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::NOT_FOUND,
                json!({"error": "not_found"}),
            ));
        }
        if captured.method == Method::GET && captured.path.starts_with("/installations/") {
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                StatusCode::OK,
                json!({"installation": self.installation("active")}),
            ));
        }
        if captured.method == Method::GET && captured.path == "/creator-sync/credential" {
            assert_eq!(captured.creator_bundle_version, Some(3));
            self.requests.lock().unwrap().push(captured);
            if matches!(self.mode, SnapshotMode::ProbeUnavailable) {
                return Err(HttpFailure::Transport);
            }
            let mut probe = self.probe();
            if matches!(self.mode, SnapshotMode::ProbeWrongBinding) {
                probe["access_device_id"] = json!("dev_000000000000000000000002");
            }
            return Ok(json_response(StatusCode::OK, probe));
        }
        assert!(
            captured
                .authorization
                .as_deref()
                .is_some_and(|value| value.starts_with("Bearer "))
        );
        if captured.method == Method::POST && captured.path == "/creator-backups/plan" {
            assert!(captured.json_body);
            assert!(captured.idempotency_key.is_none());
            let plan_count = self.plan_count.fetch_add(1, Ordering::AcqRel);
            let request: Value = serde_json::from_slice(&captured.body).unwrap();
            assert_eq!(
                request
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                [
                    "creator_count",
                    "creators",
                    "source_bundle_content_sha256",
                    "source_bundle_format_version"
                ]
            );
            assert_eq!(request["source_bundle_format_version"], 3);
            assert_eq!(
                request["creator_count"].as_u64().unwrap() as usize,
                request["creators"].as_array().unwrap().len()
            );
            assert!(request.get("access_user_id").is_none());
            if matches!(self.mode, SnapshotMode::PlanTransportOnce) && plan_count == 0 {
                self.requests.lock().unwrap().push(captured);
                return Err(HttpFailure::Transport);
            }
            let local = request["creators"]
                .as_array()
                .unwrap()
                .iter()
                .map(|creator| {
                    (
                        creator["record_key"].as_str().unwrap().to_owned(),
                        creator["semantic_sha256"].as_str().unwrap().to_owned(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let remote = self.remote_manifest.lock().unwrap();
            let backup_kind = if remote.is_empty() { "full" } else { "delta" };
            let changed_record_keys = local
                .iter()
                .filter(|(record_key, semantic_sha256)| {
                    remote.get(*record_key) != Some(*semantic_sha256)
                })
                .map(|(record_key, _)| record_key.clone())
                .collect::<Vec<_>>();
            let manifest_sha256 = Self::manifest_sha256(&remote);
            drop(remote);
            let pending_updates = changed_record_keys
                .iter()
                .map(|record_key| (record_key.clone(), local[record_key].clone()))
                .collect();
            self.state.lock().unwrap().pending_updates = pending_updates;
            let mut response = json!({
                "status": "planned",
                "backup_kind": backup_kind,
                "plan_token": Uuid::new_v4(),
                "source_bundle_format_version": request["source_bundle_format_version"],
                "source_bundle_content_sha256": request["source_bundle_content_sha256"],
                "source_creator_count": request["creator_count"],
                "manifest_sha256": manifest_sha256,
                "changed_creators": changed_record_keys.iter().map(|record_key| json!({
                    "record_key": record_key,
                    "semantic_sha256": local[record_key],
                })).collect::<Vec<_>>(),
                "observation_receipt": if changed_record_keys.is_empty() {
                    Some(json!({
                        "backup_id": Uuid::new_v4(),
                        "receipt_id": Uuid::new_v4(),
                        "observed_at": 1,
                        "replayed": false,
                    }))
                } else {
                    None
                },
            });
            match self.mode {
                SnapshotMode::PlanUnknownField => {
                    response["access_user_id"] = json!(self.metadata.access_user_id);
                }
                SnapshotMode::PlanUnknownCreator => {
                    response["changed_creators"] = json!([{
                        "record_key": format!("sha256:{}", "f".repeat(64)),
                        "semantic_sha256": format!("sha256:{}", "e".repeat(64)),
                    }]);
                    response["observation_receipt"] = Value::Null;
                }
                SnapshotMode::PlanDuplicateCreator => {
                    let record_key = local.keys().next().unwrap().clone();
                    let changed = json!({
                        "record_key": record_key,
                        "semantic_sha256": local[&record_key],
                    });
                    response["changed_creators"] = json!([changed, changed]);
                    response["observation_receipt"] = Value::Null;
                }
                SnapshotMode::PlanWrongDigestEcho => {
                    response["source_bundle_content_sha256"] =
                        json!(format!("sha256:{}", "f".repeat(64)));
                }
                _ => {}
            }
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(StatusCode::OK, response));
        }
        if captured.method == Method::POST && captured.path == "/creator-snapshots" {
            assert!(captured.json_body);
            let init: Value = serde_json::from_slice(&captured.body).unwrap();
            assert_eq!(init["snapshot_kind"], "delta");
            assert_eq!(init["max_snapshot_bytes"], 32 * 1024 * 1024);
            assert_eq!(init["max_chunk_bytes"], 1024 * 1024);
            assert_eq!(init["max_chunks"], 128);
            assert_eq!(
                captured.idempotency_key.as_deref(),
                init["content_sha256"].as_str()
            );
            if matches!(self.mode, SnapshotMode::ManifestChangedOnce)
                && self.manifest_conflict_count.fetch_add(1, Ordering::AcqRel) == 0
            {
                self.requests.lock().unwrap().push(captured);
                return Ok(json_response(
                    StatusCode::CONFLICT,
                    json!({"error": "manifest_changed"}),
                ));
            }
            let mut state = self.state.lock().unwrap();
            let incoming_content_sha256 = init["content_sha256"].as_str().unwrap();
            let replayed = state.content_sha256.as_deref() == Some(incoming_content_sha256);
            if !replayed {
                state.snapshot_id = Uuid::new_v4();
                state.received.clear();
                state.completed = false;
            }
            state.content_sha256 = Some(incoming_content_sha256.to_owned());
            state.chunk_count = init["chunk_count"].as_u64().unwrap() as usize;
            let projection = Self::snapshot_projection(&state);
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                if replayed {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                json!({
                    "status": "ready",
                    "snapshot_id": projection["snapshot_id"],
                    "content_sha256": projection["content_sha256"],
                    "upload_status": projection["upload_status"],
                    "review_status": projection["review_status"],
                    "chunk_count": projection["chunk_count"],
                    "received_chunk_count": projection["received_chunk_count"],
                    "received_bytes": projection["received_bytes"],
                    "missing_chunk_indices": projection["missing_chunk_indices"],
                    "replayed": replayed,
                }),
            ));
        }
        if captured.method == Method::PUT && captured.path.contains("/chunks/") {
            assert!(captured.json_body);
            let digest = snapshot_encode::sha256_digest(&captured.body);
            assert_eq!(captured.idempotency_key.as_deref(), Some(digest.as_str()));
            let chunk: Value = serde_json::from_slice(&captured.body).unwrap();
            let index = chunk["chunk_index"].as_u64().unwrap() as usize;
            let mut state = self.state.lock().unwrap();
            let replayed = state.received.insert(index, captured.body.len()).is_some();
            let snapshot_id = state.snapshot_id;
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                if replayed {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                json!({
                    "status": "stored",
                    "snapshot_id": snapshot_id,
                    "chunk_index": index,
                    "replayed": replayed,
                }),
            ));
        }
        if captured.method == Method::GET && captured.path.starts_with("/creator-snapshots/") {
            let state = self.state.lock().unwrap();
            let projection = Self::snapshot_projection(&state);
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(StatusCode::OK, projection));
        }
        if captured.method == Method::POST && captured.path.ends_with("/complete") {
            assert!(!captured.json_body);
            assert!(captured.body.is_empty());
            let mut state = self.state.lock().unwrap();
            assert_eq!(
                captured.idempotency_key.as_deref(),
                state.content_sha256.as_deref()
            );
            assert_eq!(state.received.len(), state.chunk_count);
            let replayed = state.completed;
            state.completed = true;
            self.remote_manifest
                .lock()
                .unwrap()
                .extend(state.pending_updates.clone());
            let snapshot_id = state.snapshot_id;
            self.requests.lock().unwrap().push(captured);
            return Ok(json_response(
                if replayed {
                    StatusCode::OK
                } else {
                    StatusCode::CREATED
                },
                json!({
                    "status": "complete",
                    "snapshot_id": snapshot_id,
                    "review_status": "quarantined",
                    "replayed": replayed,
                }),
            ));
        }
        panic!("unexpected fake snapshot request")
    }
}

struct AlwaysUnavailable;

#[async_trait]
impl HttpTransport for AlwaysUnavailable {
    async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        Err(HttpFailure::Transport)
    }
}

struct BlockingUnavailable {
    entered: Notify,
    release: Notify,
    first: AtomicBool,
}

#[async_trait]
impl HttpTransport for BlockingUnavailable {
    async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        if self
            .first
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Err(HttpFailure::Transport)
    }
}

struct FakeClock {
    now: Mutex<Instant>,
}

impl FakeClock {
    fn new(now: Instant) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    fn advance(&self, duration: Duration) {
        *self.now.lock().unwrap() += duration;
    }
}

impl PairingClock for FakeClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }
}

fn runtime(
    transport: Arc<dyn HttpTransport>,
    store: Arc<dyn CreatorSyncCredentialStore>,
) -> CreatorSyncRuntime {
    CreatorSyncRuntime::from_parts(true, Some(transport), store)
}

fn runtime_with_clock(
    transport: Arc<dyn HttpTransport>,
    store: Arc<dyn CreatorSyncCredentialStore>,
    clock: Arc<dyn PairingClock>,
) -> CreatorSyncRuntime {
    CreatorSyncRuntime::from_parts_with_clock(true, Some(transport), store, clock)
}

fn mark_admission_validated(runtime: &CreatorSyncRuntime) {
    *runtime.inner.validation.try_lock().unwrap() = AdmissionValidation::Validated;
}

fn begin_input(owner_generation: u64) -> CreatorSyncPairingBeginInput {
    CreatorSyncPairingBeginInput {
        client_platform: CreatorSyncClientPlatform::Macos,
        app_version: "0.5.11".into(),
        callback_port: 49152,
        owner_generation,
    }
}

fn pairing_fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/desktop_pairing_admission_v1.json"
    ))
    .unwrap()
}

fn pairing_code() -> PairingCode {
    PairingCode::parse(
        pairing_fixture()["request"]["code"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
    .unwrap()
}

fn pairing_state(begin: &CreatorSyncPairingBeginResult) -> PairingState {
    let url = url::Url::parse(begin.authorize_url.as_str()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    PairingState::parse(state).unwrap()
}

fn complete_input(
    begin: &CreatorSyncPairingBeginResult,
    owner_generation: u64,
) -> CreatorSyncPairingCompleteInput {
    CreatorSyncPairingCompleteInput::new(
        begin.flow_id.clone(),
        owner_generation,
        pairing_code(),
        pairing_state(begin),
    )
    .unwrap()
}

fn sample_credential() -> (CreatorSyncCredentialMetadata, BearerSecret) {
    let installation_id = Uuid::new_v4();
    let credential_id = Uuid::new_v4();
    (
        CreatorSyncCredentialMetadata {
            installation_id,
            credential_id,
            access_user_id: "access-user".into(),
            access_device_id: "dev_000000000000000000000001".into(),
            source_instance_id: Uuid::parse_str(SOURCE_INSTANCE_ID).unwrap(),
            access_pairing_id: "pair_000000000000000000000001".into(),
            client_platform: CreatorSyncClientPlatform::Macos,
            app_version: "0.5.11".into(),
            status: CreatorSyncCredentialStatus::Active,
            revision: 1,
            created_at: 100,
            updated_at: 100,
            last_seen_at: None,
            revoked_at: None,
            protocol: CreatorSyncProtocol::ReacherCreatorSync,
            protocol_version: 1,
            capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
            accepted_bundle: CreatorSyncAcceptedBundle {
                format: "reacher.creator_bundle".into(),
                format_version: 1,
                profile: CreatorSyncBundleProfile::PortablePublic,
                max_snapshot_bytes: 32 * 1024 * 1024,
                max_chunk_bytes: 1024 * 1024,
                max_chunks: 128,
                max_namespace_staged_bytes: 100 * 1024 * 1024,
                max_access_user_staged_bytes: 400 * 1024 * 1024,
            },
        },
        BearerSecret::generate(credential_id),
    )
}

fn fixture_bundle() -> Arc<CreatorBundle> {
    Arc::new(
        CreatorBundle::from_import_json_bytes(include_bytes!(
            "../../../../creator-runtime/tests/fixtures/creator_bundle.json"
        ))
        .unwrap(),
    )
}

fn empty_bundle() -> Arc<CreatorBundle> {
    Arc::new(
        CreatorBundle::from_import_json_bytes(
            concat!(
                "{\"format\":\"reacher.creator_bundle\",\"format_version\":1,",
                "\"profile\":\"portable_public\",\"snapshot_kind\":\"full\",",
                "\"creator_count\":0,\"content_sha256\":",
                "\"sha256:ef47efc1f35a66ceda152fc02c469c594578d146ace60d939ab99f18c0f8ecd6\",",
                "\"creators\":[]}\n"
            )
            .as_bytes(),
        )
        .unwrap(),
    )
}

fn json_response(status: StatusCode, value: Value) -> HttpResponse {
    HttpResponse {
        status,
        body: serde_json::to_vec(&value).unwrap(),
    }
}
