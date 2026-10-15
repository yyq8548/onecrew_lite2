//! Quarantined snapshot upload and authoritative readback lifecycle.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use onecrew_creator_runtime::CreatorBundle;
use reqwest::{Method, StatusCode};
use uuid::Uuid;

use crate::{
    http::{DEFAULT_MAX_RESPONSE_BYTES, HttpFailure, HttpRequest, MAX_PLAN_RESPONSE_BYTES},
    model::{
        CreatorSnapshotSyncResult, CreatorSnapshotSyncState, CreatorSyncAdmissionState,
        CreatorSyncIssue, sync_result,
    },
    snapshot_encode::{self, PreparedSnapshot},
    wire,
};

use super::{
    CreatorSyncRuntime,
    credential_store::{
        AdmissionValidation, StoredCredential, StoredCredentialState, ValidationFailure,
        probe_accepts_current_bundle,
    },
    parse_json,
};

impl CreatorSyncRuntime {
    /// Cheap admission pre-check for callers that must decide whether to pay
    /// for a bundle export at all: configuration, a sync already in flight,
    /// admission validation, and the stored credential are judged exactly as
    /// `sync_creator_bundle` judges them, without taking the single-flight
    /// slot. The `sync_running` read is a hint that lets the caller skip the
    /// export; `sync_creator_bundle`'s CAS remains the only admission
    /// authority, so a trigger that slips between the two at worst pays one
    /// extra export and still gets `AlreadyRunning`.
    ///
    /// Async because the validation state lives behind an async mutex that
    /// `ensure_admission_validated` holds across its remote reads; waiting for
    /// it yields the real state, while `try_lock` would turn a concurrent
    /// status poll into a spurious `ValidationRequired` (HOST-08). The flight
    /// hint is read before that wait so a caller never queues behind a
    /// validation held by the very sync it is about to be refused for.
    pub async fn can_sync_now(&self) -> Result<(), CreatorSnapshotSyncResult> {
        self.configured_for_sync()?;
        if self.inner.sync_running.load(Ordering::Acquire) {
            return Err(sync_result(CreatorSnapshotSyncState::AlreadyRunning, None));
        }
        self.admitted_credential().await.map(|_| ())
    }

    pub async fn sync_creator_bundle(
        &self,
        bundle: Arc<CreatorBundle>,
    ) -> CreatorSnapshotSyncResult {
        if let Err(result) = self.configured_for_sync() {
            return result;
        }
        if self
            .inner
            .sync_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return sync_result(CreatorSnapshotSyncState::AlreadyRunning, None);
        }
        let _flight = SyncFlight(&self.inner.sync_running);
        let stored = match self.admitted_credential().await {
            Ok(stored) => stored,
            Err(result) => return result,
        };
        // Cached admission proves identity only. Re-read the remote format
        // before every plan/upload, including credentials enrolled under v1.
        let capability = match self.read_validation_probe(&stored).await {
            Ok(probe) if probe_accepts_current_bundle(&stored.metadata, &probe) => Ok(()),
            Ok(_) | Err(ValidationFailure::Protocol) => Err(SyncFailure::Protocol),
            Err(ValidationFailure::Unauthorized) => Err(SyncFailure::Unauthorized),
            Err(ValidationFailure::Unavailable) => Err(SyncFailure::Unavailable),
        };
        if let Err(failure) = capability {
            return self.snapshot_failure(&stored, failure).await;
        }
        // Manifest planning and delta encoding are CPU passes over the whole
        // bundle; they run on the blocking pool so the async worker only
        // awaits. A join failure (panic) is a local protocol failure.
        // HOST-09 exemption (accepted, tracked in
        // docs/plans/runtime-host-responsiveness/phases/phase-1.md "Snapshot
        // sync on the blocking pool"): neither task polls a cancellation flag
        // because both are bounded CPU work over an in-memory bundle (manifest
        // <= MAX_PLAN_REQUEST_BYTES, artifact <= MAX_SNAPSHOT_BYTES) with no
        // I/O to interrupt; the previous inline encoding was equally
        // unabortable, and the host supervisor's shutdown timeout still
        // classifies a late join. `snapshot_blocking_tasks_cite_the_host_09_exemption`
        // pins this to exactly these two sites.
        let plan_bundle = Arc::clone(&bundle);
        let plan_request =
            match tokio::task::spawn_blocking(move || snapshot_encode::prepare_plan(&plan_bundle))
                .await
            {
                Ok(Ok(prepared)) => prepared,
                Ok(Err(())) | Err(_) => return protocol_mismatch_result(),
            };
        let mut replanned = false;
        loop {
            let plan = match self.plan_creator_backup(&stored, &plan_request).await {
                Ok(plan) => plan,
                Err(failure) => return self.snapshot_failure(&stored, failure).await,
            };
            let delta_bundle = Arc::clone(&bundle);
            let prepared = match tokio::task::spawn_blocking(move || {
                snapshot_encode::prepare_from_plan(&delta_bundle, &plan)
            })
            .await
            {
                Ok(Ok(Some(prepared))) => prepared,
                Ok(Ok(None)) => return self.no_changes_result(&bundle),
                Ok(Err(())) | Err(_) => return protocol_mismatch_result(),
            };
            match self.perform_snapshot_sync(&stored, prepared).await {
                Ok(result) => return result,
                Err(SyncFailure::ManifestChanged) if !replanned => replanned = true,
                Err(SyncFailure::ManifestChanged) => {
                    return sync_result(
                        CreatorSnapshotSyncState::TemporarilyUnavailable,
                        Some(CreatorSyncIssue::RemoteUnavailable),
                    );
                }
                Err(failure) => return self.snapshot_failure(&stored, failure).await,
            }
        }
    }

    fn configured_for_sync(&self) -> Result<(), CreatorSnapshotSyncResult> {
        // The production snapshot API is retired. Stop before credential
        // reads, bundle export or upload; pairing and table sync are separate.
        if self.inner.configured && self.inner.endpoints.snapshot_uploads_retired() {
            return Err(sync_result(CreatorSnapshotSyncState::Retired, None));
        }
        if self.inner.configured {
            Ok(())
        } else {
            Err(sync_result(
                CreatorSnapshotSyncState::NotPaired,
                Some(CreatorSyncIssue::ConfigurationUnavailable),
            ))
        }
    }

    /// The admission and credential judgement shared by `can_sync_now` and
    /// `sync_creator_bundle`. Reads the validation state under the async lock
    /// (copied out; the guard never crosses an await) so a validation in
    /// flight is awaited rather than mistaken for "not validated".
    async fn admitted_credential(&self) -> Result<StoredCredential, CreatorSnapshotSyncResult> {
        let validation = *self.inner.validation.lock().await;
        if validation != AdmissionValidation::Validated {
            return Err(sync_result(
                CreatorSnapshotSyncState::ValidationRequired,
                Some(CreatorSyncIssue::AdmissionValidationRequired),
            ));
        }
        match self.load_credential() {
            Ok(Some(StoredCredentialState::Active(stored))) => Ok(stored),
            Ok(Some(StoredCredentialState::NonActive(_))) | Ok(None) => {
                Err(sync_result(CreatorSnapshotSyncState::NotPaired, None))
            }
            Err(()) => Err(sync_result(
                CreatorSnapshotSyncState::TemporarilyUnavailable,
                Some(CreatorSyncIssue::CredentialUnavailable),
            )),
        }
    }

    async fn plan_creator_backup(
        &self,
        stored: &StoredCredential,
        prepared: &snapshot_encode::PreparedPlan,
    ) -> Result<wire::CreatorBackupPlanResponse, SyncFailure> {
        let request = || HttpRequest {
            creator_bundle_version: None,
            method: Method::POST,
            endpoint: self.inner.endpoints.creator_backups_plan(),
            authorization: Some(stored.bearer.authorization()),
            idempotency_key: None,
            json_body: true,
            body: prepared.request_bytes.clone(),
            max_response_bytes: MAX_PLAN_RESPONSE_BYTES,
        };
        let response = match self.send(request()).await {
            Ok(response) => response,
            Err(HttpFailure::Transport) => {
                self.send(request()).await.map_err(classify_read_failure)?
            }
            Err(HttpFailure::ResponseTooLarge) => return Err(SyncFailure::Protocol),
        };
        match response.status {
            StatusCode::OK => parse_json::<wire::CreatorBackupPlanResponse>(&response.body)
                .map_err(|_| SyncFailure::Protocol),
            status => Err(classify_snapshot_response(status, &response.body)),
        }
    }

    async fn perform_snapshot_sync(
        &self,
        stored: &StoredCredential,
        prepared: PreparedSnapshot,
    ) -> Result<CreatorSnapshotSyncResult, SyncFailure> {
        let ready = self.initialize_snapshot(stored, &prepared).await?;
        let snapshot_id = Uuid::parse_str(&ready.snapshot_id).map_err(|_| SyncFailure::Protocol)?;
        let mut replayed = ready.replayed;
        let mut status_snapshot = wire::SnapshotStatus {
            snapshot_id: ready.snapshot_id,
            content_sha256: ready.content_sha256,
            upload_status: ready.upload_status,
            review_status: ready.review_status,
            chunk_count: ready.chunk_count,
            received_chunk_count: ready.received_chunk_count,
            received_bytes: ready.received_bytes,
            missing_chunk_indices: ready.missing_chunk_indices,
        };

        if status_snapshot.upload_status != "complete" {
            for _ in 0..2 {
                let missing = validate_snapshot_status(&status_snapshot, &prepared)
                    .map_err(|_| SyncFailure::Protocol)?;
                for index in missing {
                    let was_replayed = self
                        .put_snapshot_chunk(stored, snapshot_id, &prepared, index)
                        .await?;
                    replayed |= was_replayed;
                }
                status_snapshot = self
                    .read_snapshot_status(stored, snapshot_id, &prepared)
                    .await?;
                if status_snapshot.missing_chunk_indices.is_empty() {
                    break;
                }
            }
        }

        if status_snapshot.upload_status == "complete" {
            status_snapshot = self
                .read_snapshot_status(stored, snapshot_id, &prepared)
                .await?;
            return Ok(self.completed_snapshot_result(
                &prepared,
                snapshot_id,
                replayed,
                &status_snapshot,
            ));
        }
        if !status_snapshot.missing_chunk_indices.is_empty() {
            return Err(SyncFailure::Unavailable);
        }
        let complete = match self.complete_snapshot(stored, snapshot_id, &prepared).await {
            Ok(complete) => complete,
            Err(SyncFailure::UnknownWrite) => {
                match self
                    .read_snapshot_status(stored, snapshot_id, &prepared)
                    .await
                {
                    Ok(status) if status.upload_status == "complete" => {
                        return Ok(self.completed_snapshot_result(
                            &prepared,
                            snapshot_id,
                            true,
                            &status,
                        ));
                    }
                    Ok(status) if status.upload_status == "uploading" => {
                        self.complete_snapshot(stored, snapshot_id, &prepared)
                            .await?
                    }
                    Ok(_) => return Err(SyncFailure::Protocol),
                    Err(failure) => return Err(failure),
                }
            }
            Err(failure) => return Err(failure),
        };
        replayed |= complete.replayed;
        let final_status = self
            .read_snapshot_status(stored, snapshot_id, &prepared)
            .await?;
        if complete.status != "complete"
            || complete.snapshot_id != snapshot_id.to_string()
            || complete.review_status != "quarantined"
        {
            return Err(SyncFailure::Protocol);
        }
        Ok(self.completed_snapshot_result(&prepared, snapshot_id, replayed, &final_status))
    }

    async fn initialize_snapshot(
        &self,
        stored: &StoredCredential,
        prepared: &PreparedSnapshot,
    ) -> Result<wire::SnapshotReady, SyncFailure> {
        let request = || HttpRequest {
            creator_bundle_version: None,
            method: Method::POST,
            endpoint: self.inner.endpoints.snapshots(),
            authorization: Some(stored.bearer.authorization()),
            idempotency_key: Some(prepared.content_sha256.clone()),
            json_body: true,
            body: prepared.init_bytes.clone(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        };
        let response = match self.send(request()).await {
            Ok(response) => response,
            Err(HttpFailure::Transport) => {
                self.send(request()).await.map_err(classify_write_failure)?
            }
            Err(HttpFailure::ResponseTooLarge) => return Err(SyncFailure::Protocol),
        };
        match response.status {
            StatusCode::OK | StatusCode::CREATED => {
                let expected_replayed = response.status == StatusCode::OK;
                let ready = parse_json::<wire::SnapshotReady>(&response.body)
                    .map_err(|_| SyncFailure::Protocol)?;
                if ready.status != "ready"
                    || ready.content_sha256 != prepared.content_sha256
                    || ready.chunk_count != prepared.chunks.len()
                    || ready.replayed != expected_replayed
                    || Uuid::parse_str(&ready.snapshot_id).is_err()
                    || validate_snapshot_indices(
                        &ready.missing_chunk_indices,
                        prepared.chunks.len(),
                    )
                    .is_err()
                    || ready.received_chunk_count > ready.chunk_count
                {
                    return Err(SyncFailure::Protocol);
                }
                Ok(ready)
            }
            status => Err(classify_snapshot_init_response(status, &response.body)),
        }
    }

    async fn put_snapshot_chunk(
        &self,
        stored: &StoredCredential,
        snapshot_id: Uuid,
        prepared: &PreparedSnapshot,
        index: usize,
    ) -> Result<bool, SyncFailure> {
        let chunk = prepared.chunks.get(index).ok_or(SyncFailure::Protocol)?;
        let request = || HttpRequest {
            creator_bundle_version: None,
            method: Method::PUT,
            endpoint: self
                .inner
                .endpoints
                .snapshot_chunk(snapshot_id, chunk.index),
            authorization: Some(stored.bearer.authorization()),
            idempotency_key: Some(chunk.idempotency_key.clone()),
            json_body: true,
            body: chunk.bytes.clone(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        };
        let response = match self.send(request()).await {
            Ok(response) => response,
            Err(HttpFailure::Transport) => {
                let status = self
                    .read_snapshot_status(stored, snapshot_id, prepared)
                    .await?;
                if !status.missing_chunk_indices.contains(&index) {
                    return Ok(true);
                }
                self.send(request()).await.map_err(classify_write_failure)?
            }
            Err(HttpFailure::ResponseTooLarge) => return Err(SyncFailure::Protocol),
        };
        match response.status {
            StatusCode::OK | StatusCode::CREATED => {
                let expected_replayed = response.status == StatusCode::OK;
                let stored_response = parse_json::<wire::ChunkStored>(&response.body)
                    .map_err(|_| SyncFailure::Protocol)?;
                if stored_response.status != "stored"
                    || stored_response.snapshot_id != snapshot_id.to_string()
                    || stored_response.chunk_index != index
                    || stored_response.replayed != expected_replayed
                {
                    return Err(SyncFailure::Protocol);
                }
                Ok(stored_response.replayed)
            }
            status => Err(classify_snapshot_response(status, &response.body)),
        }
    }

    async fn read_snapshot_status(
        &self,
        stored: &StoredCredential,
        snapshot_id: Uuid,
        prepared: &PreparedSnapshot,
    ) -> Result<wire::SnapshotStatus, SyncFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::GET,
                endpoint: self.inner.endpoints.snapshot(snapshot_id),
                authorization: Some(stored.bearer.authorization()),
                idempotency_key: None,
                json_body: false,
                body: Vec::new(),
                max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(classify_read_failure)?;
        match response.status {
            StatusCode::OK => {
                let status = parse_json::<wire::SnapshotStatus>(&response.body)
                    .map_err(|_| SyncFailure::Protocol)?;
                if status.snapshot_id != snapshot_id.to_string()
                    || validate_snapshot_status(&status, prepared).is_err()
                {
                    return Err(SyncFailure::Protocol);
                }
                Ok(status)
            }
            status => Err(classify_snapshot_response(status, &response.body)),
        }
    }

    async fn complete_snapshot(
        &self,
        stored: &StoredCredential,
        snapshot_id: Uuid,
        prepared: &PreparedSnapshot,
    ) -> Result<wire::SnapshotComplete, SyncFailure> {
        let response = self
            .send(HttpRequest {
                creator_bundle_version: None,
                method: Method::POST,
                endpoint: self.inner.endpoints.snapshot_complete(snapshot_id),
                authorization: Some(stored.bearer.authorization()),
                idempotency_key: Some(prepared.content_sha256.clone()),
                json_body: false,
                body: Vec::new(),
                max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            })
            .await
            .map_err(classify_write_failure)?;
        match response.status {
            StatusCode::OK | StatusCode::CREATED => {
                let expected_replayed = response.status == StatusCode::OK;
                let complete = parse_json::<wire::SnapshotComplete>(&response.body)
                    .map_err(|_| SyncFailure::Protocol)?;
                if complete.replayed != expected_replayed {
                    return Err(SyncFailure::Protocol);
                }
                Ok(complete)
            }
            status => Err(classify_snapshot_response(status, &response.body)),
        }
    }

    async fn snapshot_failure(
        &self,
        stored: &StoredCredential,
        failure: SyncFailure,
    ) -> CreatorSnapshotSyncResult {
        match failure {
            SyncFailure::Unauthorized => {
                let outcome = self
                    .handle_authoritative_non_active(
                        stored.metadata.installation_id,
                        &stored.bearer,
                        None,
                    )
                    .await;
                let issue = if outcome.cleanup_incomplete {
                    CreatorSyncIssue::SafeCleanupIncomplete
                } else {
                    match outcome.state {
                        CreatorSyncAdmissionState::Revoked => CreatorSyncIssue::RemoteRevoked,
                        _ => CreatorSyncIssue::RemoteUnauthorized,
                    }
                };
                let sync_state = match outcome.state {
                    CreatorSyncAdmissionState::Revoked => CreatorSnapshotSyncState::Revoked,
                    _ => CreatorSnapshotSyncState::Unauthorized,
                };
                sync_result(sync_state, Some(issue))
            }
            SyncFailure::Unavailable | SyncFailure::UnknownWrite | SyncFailure::ManifestChanged => {
                sync_result(
                    CreatorSnapshotSyncState::TemporarilyUnavailable,
                    Some(CreatorSyncIssue::RemoteUnavailable),
                )
            }
            SyncFailure::Protocol | SyncFailure::Conflict => sync_result(
                CreatorSnapshotSyncState::RecoveryRequired,
                Some(CreatorSyncIssue::ProtocolMismatch),
            ),
        }
    }

    fn completed_snapshot_result(
        &self,
        prepared: &PreparedSnapshot,
        snapshot_id: Uuid,
        replayed: bool,
        status: &wire::SnapshotStatus,
    ) -> CreatorSnapshotSyncResult {
        let result = completed_snapshot_result(prepared, snapshot_id, replayed, status);
        if result.state == CreatorSnapshotSyncState::Quarantined {
            self.clear_remote_override();
        }
        result
    }

    fn no_changes_result(&self, bundle: &CreatorBundle) -> CreatorSnapshotSyncResult {
        self.clear_remote_override();
        CreatorSnapshotSyncResult {
            state: CreatorSnapshotSyncState::NoChanges,
            creator_count: Some(bundle.creator_count),
            content_sha256: Some(bundle.content_sha256.clone()),
            snapshot_id: None,
            replayed: false,
            issue: None,
        }
    }

    fn clear_remote_override(&self) {
        *self
            .inner
            .remote_override
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

enum SyncFailure {
    Unavailable,
    UnknownWrite,
    Unauthorized,
    Protocol,
    Conflict,
    ManifestChanged,
}

fn protocol_mismatch_result() -> CreatorSnapshotSyncResult {
    sync_result(
        CreatorSnapshotSyncState::RecoveryRequired,
        Some(CreatorSyncIssue::ProtocolMismatch),
    )
}

struct SyncFlight<'a>(&'a AtomicBool);

impl Drop for SyncFlight<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn classify_snapshot_response(status: StatusCode, body: &[u8]) -> SyncFailure {
    if status == StatusCode::UNAUTHORIZED {
        return SyncFailure::Unauthorized;
    }
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return SyncFailure::Unavailable;
    }
    let error = parse_json::<wire::ErrorEnvelope>(body)
        .ok()
        .map(|envelope| envelope.error);
    match error.as_deref() {
        Some("upload_in_progress") => SyncFailure::Unavailable,
        Some(
            "idempotency_conflict"
            | "chunk_conflict"
            | "upload_expired"
            | "snapshot_superseded"
            | "snapshot_bytes_exceeded"
            | "stored_chunk_corrupt"
            | "chunk_range_conflict"
            | "creator_count_mismatch"
            | "digest_mismatch"
            | "artifact_mismatch",
        ) => SyncFailure::Conflict,
        _ => SyncFailure::Protocol,
    }
}

fn classify_snapshot_init_response(status: StatusCode, body: &[u8]) -> SyncFailure {
    if parse_json::<wire::ErrorEnvelope>(body)
        .ok()
        .is_some_and(|envelope| envelope.error == "manifest_changed")
    {
        return SyncFailure::ManifestChanged;
    }
    classify_snapshot_response(status, body)
}

fn classify_read_failure(failure: HttpFailure) -> SyncFailure {
    match failure {
        HttpFailure::Transport => SyncFailure::Unavailable,
        HttpFailure::ResponseTooLarge => SyncFailure::Protocol,
    }
}

fn classify_write_failure(failure: HttpFailure) -> SyncFailure {
    match failure {
        HttpFailure::Transport => SyncFailure::UnknownWrite,
        HttpFailure::ResponseTooLarge => SyncFailure::Protocol,
    }
}

fn validate_snapshot_indices(indices: &[usize], chunk_count: usize) -> Result<(), ()> {
    let mut previous = None;
    for index in indices {
        if *index >= chunk_count || previous.is_some_and(|previous| previous >= *index) {
            return Err(());
        }
        previous = Some(*index);
    }
    Ok(())
}

fn validate_snapshot_status(
    status: &wire::SnapshotStatus,
    prepared: &PreparedSnapshot,
) -> Result<Vec<usize>, ()> {
    if status.content_sha256 != prepared.content_sha256
        || status.chunk_count != prepared.chunks.len()
        || status.received_chunk_count > status.chunk_count
        || !matches!(
            status.upload_status.as_str(),
            "uploading" | "conflicted" | "complete"
        )
        || status.upload_status == "conflicted"
        || !matches!(
            status.review_status.as_deref(),
            None | Some("quarantined" | "superseded" | "accepted" | "rejected")
        )
        || validate_snapshot_indices(&status.missing_chunk_indices, status.chunk_count).is_err()
    {
        return Err(());
    }
    if status.upload_status == "complete"
        && (!status.missing_chunk_indices.is_empty()
            || status.received_chunk_count != status.chunk_count)
    {
        return Err(());
    }
    if status.upload_status == "uploading" && status.review_status.is_some() {
        return Err(());
    }
    Ok(status.missing_chunk_indices.clone())
}

fn completed_snapshot_result(
    prepared: &PreparedSnapshot,
    snapshot_id: Uuid,
    replayed: bool,
    status: &wire::SnapshotStatus,
) -> CreatorSnapshotSyncResult {
    let expected_received_bytes = prepared
        .chunks
        .iter()
        .map(|chunk| chunk.bytes.len() as u64)
        .sum::<u64>();
    if status.upload_status != "complete"
        || status.review_status.as_deref() != Some("quarantined")
        || !status.missing_chunk_indices.is_empty()
        || status.received_chunk_count != prepared.chunks.len()
        || status.received_bytes != expected_received_bytes
    {
        return sync_result(
            CreatorSnapshotSyncState::RecoveryRequired,
            Some(CreatorSyncIssue::ProtocolMismatch),
        );
    }
    CreatorSnapshotSyncResult {
        state: CreatorSnapshotSyncState::Quarantined,
        creator_count: Some(prepared.source_creator_count),
        content_sha256: Some(prepared.source_bundle_content_sha256.clone()),
        snapshot_id: Some(snapshot_id.to_string()),
        replayed,
        issue: None,
    }
}
