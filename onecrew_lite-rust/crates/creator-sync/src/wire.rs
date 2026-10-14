//! Strict private Worker B wire projections.

use onecrew_creator_runtime::{
    ContactChannel, CreatorTransferLinkState, CreatorTransferPlatform, CreatorTransferTargetKind,
    LocatorKind,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InstallationProjection {
    pub(super) installation_id: String,
    pub(super) credential_id: String,
    pub(super) access_user_id: String,
    pub(super) access_device_id: String,
    pub(super) source_instance_id: String,
    pub(super) access_pairing_id: String,
    pub(super) client_platform: String,
    pub(super) app_version: String,
    pub(super) status: String,
    pub(super) revision: u64,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
    pub(super) last_seen_at: Option<u64>,
    pub(super) revoked_at: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrollmentResponse {
    pub(super) installation: InstallationProjection,
    pub(super) replayed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InstallationReadback {
    pub(super) installation: InstallationProjection,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialProbe {
    pub(super) protocol: String,
    pub(super) protocol_version: u64,
    pub(super) installation_id: String,
    pub(super) credential_id: String,
    pub(super) access_user_id: String,
    pub(super) access_device_id: String,
    pub(super) source_instance_id: String,
    pub(super) access_pairing_id: String,
    pub(super) capabilities: [String; 1],
    pub(super) accepted_bundle: AcceptedBundle,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AcceptedBundle {
    pub(super) format: String,
    pub(super) format_version: u64,
    pub(super) profile: String,
    pub(super) max_snapshot_bytes: u64,
    pub(super) max_chunk_bytes: u64,
    pub(super) max_chunks: u64,
    pub(super) max_namespace_staged_bytes: u64,
    pub(super) max_access_user_staged_bytes: u64,
}

#[derive(PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatorBackupManifestItem<'a> {
    pub(super) record_key: &'a str,
    pub(super) semantic_sha256: &'a str,
    pub(super) claims: Vec<CreatorBackupClaim<'a>>,
}

#[derive(PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum CreatorBackupClaim<'a> {
    ProviderAccount {
        platform: CreatorTransferPlatform,
        target_kind: CreatorTransferTargetKind,
        external_id: &'a str,
        link_state: CreatorTransferLinkState,
    },
    Contact {
        channel: ContactChannel,
        locator_kind: LocatorKind,
        normalized_value: &'a str,
    },
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatorBackupPlanRequest<'a> {
    pub(super) source_bundle_format_version: u16,
    pub(super) source_bundle_content_sha256: &'a str,
    pub(super) creator_count: u32,
    pub(super) creators: &'a [CreatorBackupManifestItem<'a>],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatorBackupPlanResponse {
    pub(super) status: String,
    pub(super) backup_kind: CreatorBackupKind,
    pub(super) plan_token: String,
    pub(super) source_bundle_format_version: u16,
    pub(super) source_bundle_content_sha256: String,
    pub(super) source_creator_count: u32,
    pub(super) manifest_sha256: String,
    pub(super) changed_creators: Vec<CreatorBackupChangedCreator>,
    pub(super) observation_receipt: Option<CreatorBackupObservationReceipt>,
}

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CreatorBackupKind {
    Full,
    Delta,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatorBackupChangedCreator {
    pub(super) record_key: String,
    pub(super) semantic_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatorBackupObservationReceipt {
    pub(super) backup_id: String,
    pub(super) receipt_id: String,
    pub(super) observed_at: u64,
    pub(super) replayed: bool,
}

#[derive(Serialize)]
pub(super) struct EnrollmentRequest<'a> {
    pub(super) pairing_code: &'a str,
    pub(super) code_verifier: &'a str,
    pub(super) installation_id: &'a str,
    pub(super) credential_id: &'a str,
    pub(super) bearer: &'a str,
    pub(super) client_platform: &'a str,
    pub(super) app_version: &'a str,
}

#[derive(Serialize)]
pub(super) struct SnapshotInit<'a> {
    pub(super) format: &'static str,
    pub(super) format_version: u16,
    pub(super) profile: &'static str,
    pub(super) snapshot_kind: &'static str,
    pub(super) backup_kind: CreatorBackupKind,
    pub(super) plan_token: &'a str,
    pub(super) source_bundle_content_sha256: &'a str,
    pub(super) source_creator_count: u32,
    pub(super) manifest_sha256: &'a str,
    pub(super) creator_count: u32,
    pub(super) content_sha256: &'a str,
    pub(super) artifact_sha256: &'a str,
    pub(super) artifact_size_bytes: usize,
    pub(super) chunk_count: usize,
    pub(super) max_snapshot_bytes: usize,
    pub(super) max_chunk_bytes: usize,
    pub(super) max_chunks: usize,
}

#[derive(Serialize)]
pub(super) struct CreatorChunk<'a, T> {
    pub(super) format: &'static str,
    pub(super) format_version: u16,
    pub(super) snapshot_content_sha256: &'a str,
    pub(super) chunk_index: usize,
    pub(super) creator_start_index: usize,
    pub(super) creator_count: usize,
    pub(super) creators: &'a [T],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotReady {
    pub(super) status: String,
    pub(super) snapshot_id: String,
    pub(super) content_sha256: String,
    pub(super) upload_status: String,
    pub(super) review_status: Option<String>,
    pub(super) chunk_count: usize,
    pub(super) received_chunk_count: usize,
    pub(super) received_bytes: u64,
    pub(super) missing_chunk_indices: Vec<usize>,
    pub(super) replayed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChunkStored {
    pub(super) status: String,
    pub(super) snapshot_id: String,
    pub(super) chunk_index: usize,
    pub(super) replayed: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotStatus {
    pub(super) snapshot_id: String,
    pub(super) content_sha256: String,
    pub(super) upload_status: String,
    pub(super) review_status: Option<String>,
    pub(super) chunk_count: usize,
    pub(super) received_chunk_count: usize,
    pub(super) received_bytes: u64,
    pub(super) missing_chunk_indices: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotComplete {
    pub(super) status: String,
    pub(super) snapshot_id: String,
    pub(super) review_status: String,
    pub(super) replayed: bool,
}

#[derive(Deserialize)]
pub(super) struct ErrorEnvelope {
    pub(super) error: String,
    #[serde(default)]
    pub(super) action: Option<String>,
    #[serde(default)]
    pub(super) retry_safe: Option<bool>,
}
