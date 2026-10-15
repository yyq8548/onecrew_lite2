//! Canonical private manifest planning and delta snapshot encoding.

use onecrew_creator_runtime::{
    CREATOR_BUNDLE_FORMAT, CREATOR_BUNDLE_MAX_CREATORS, CreatorBundle, CreatorTransferCreator,
};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::wire::{
    CreatorBackupClaim, CreatorBackupKind, CreatorBackupManifestItem, CreatorBackupPlanRequest,
    CreatorBackupPlanResponse, CreatorChunk, SnapshotInit,
};

pub(super) const MAX_PLAN_REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub(super) const MAX_CHUNK_BYTES: usize = 1024 * 1024;
pub(super) const MAX_CHUNKS: usize = 128;
pub(super) const MAX_SNAPSHOT_BYTES: usize = 32 * 1024 * 1024;
// Chunk v3 carries the explicitly versioned Creator tags contract.
pub(super) const CREATOR_CHUNK_FORMAT_VERSION: u16 = 3;

pub(super) struct PreparedPlan {
    pub(super) request_bytes: Vec<u8>,
}

pub(super) struct PreparedSnapshot {
    pub(super) source_creator_count: u32,
    pub(super) source_bundle_content_sha256: String,
    pub(super) content_sha256: String,
    pub(super) chunks: Vec<PreparedChunk>,
    pub(super) init_bytes: Vec<u8>,
}

pub(super) struct PreparedChunk {
    pub(super) index: usize,
    pub(super) idempotency_key: String,
    pub(super) bytes: Vec<u8>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CreatorDeltaArtifact<'a> {
    format: &'static str,
    format_version: u16,
    profile: &'static str,
    snapshot_kind: &'static str,
    source_bundle_content_sha256: &'a str,
    manifest_sha256: &'a str,
    creator_count: u32,
    content_sha256: &'a str,
    creators: &'a [&'a CreatorTransferCreator],
}

/// Builds the private manifest request. The bundle arrives from
/// `CreatorBundle::from_json_bytes` or the Creator export (`build_creator_bundle`),
/// both of which already ran field validation; only `from_json_bytes` also
/// enforces `CREATOR_BUNDLE_MAX_BYTES`, so a locally exported bundle reaches
/// this point without a whole-artifact size check. That is accepted: the
/// remote plan request is bounded by `MAX_PLAN_REQUEST_BYTES` below and the
/// artifact actually uploaded is bounded by `prepare_delta`'s
/// `MAX_SNAPSHOT_BYTES`; re-serializing the whole bundle here only to size it
/// was a discarded pass over every byte, so only the manifest is encoded.
pub(super) fn prepare_plan(bundle: &CreatorBundle) -> Result<PreparedPlan, ()> {
    if bundle.format_version != crate::ACCESS_BUNDLE_FORMAT_VERSION {
        return Err(());
    }
    let creators = bundle
        .creators
        .iter()
        .map(|creator| CreatorBackupManifestItem {
            record_key: &creator.record_key,
            semantic_sha256: &creator.semantic_sha256,
            claims: manifest_claims(creator),
        })
        .collect::<Vec<_>>();
    let request_bytes = canonical_json(&CreatorBackupPlanRequest {
        source_bundle_format_version: crate::ACCESS_BUNDLE_FORMAT_VERSION,
        source_bundle_content_sha256: &bundle.content_sha256,
        creator_count: bundle.creator_count,
        creators: &creators,
    })?;
    if request_bytes.len() > MAX_PLAN_REQUEST_BYTES {
        return Err(());
    }
    Ok(PreparedPlan { request_bytes })
}

fn manifest_claims(creator: &CreatorTransferCreator) -> Vec<CreatorBackupClaim<'_>> {
    let mut claims = creator
        .accounts
        .iter()
        .map(|account| CreatorBackupClaim::ProviderAccount {
            platform: account.platform,
            target_kind: account.target_kind,
            external_id: &account.external_id,
            link_state: account.link_state,
        })
        .chain(
            creator
                .contact_routes
                .iter()
                .map(|route| CreatorBackupClaim::Contact {
                    channel: route.channel,
                    locator_kind: route.locator_kind,
                    normalized_value: &route.normalized_value,
                }),
        )
        .collect::<Vec<_>>();
    claims.sort_by_cached_key(|claim| serde_json::to_vec(claim).unwrap_or_default());
    claims.dedup();
    claims
}

pub(super) fn prepare_from_plan(
    bundle: &CreatorBundle,
    plan: &CreatorBackupPlanResponse,
) -> Result<Option<PreparedSnapshot>, ()> {
    if plan.status != "planned"
        || Uuid::parse_str(&plan.plan_token).is_err()
        || bundle.format_version != crate::ACCESS_BUNDLE_FORMAT_VERSION
        || plan.source_bundle_format_version != crate::ACCESS_BUNDLE_FORMAT_VERSION
        || plan.source_bundle_content_sha256 != bundle.content_sha256
        || plan.source_creator_count != bundle.creator_count
        || !is_sha256_digest(&plan.manifest_sha256)
        || plan.changed_creators.len() > CREATOR_BUNDLE_MAX_CREATORS
        || plan.changed_creators.len() > bundle.creators.len()
        || (plan.backup_kind == CreatorBackupKind::Full
            && plan.changed_creators.len() != bundle.creators.len())
    {
        return Err(());
    }
    // The bundle contract keeps Creators strictly sorted by record_key
    // (`build_creator_bundle` sorts, `from_json_bytes` validates); the fields
    // are public, so the invariant the binary search below relies on is
    // checked here rather than assumed.
    if !bundle
        .creators
        .windows(2)
        .all(|pair| pair[0].record_key < pair[1].record_key)
    {
        return Err(());
    }
    let mut previous_record_key: Option<&str> = None;
    let mut creators = Vec::with_capacity(plan.changed_creators.len());
    for changed in &plan.changed_creators {
        if !is_sha256_digest(&changed.record_key)
            || !is_sha256_digest(&changed.semantic_sha256)
            || previous_record_key.is_some_and(|previous| previous >= changed.record_key.as_str())
        {
            return Err(());
        }
        let index = bundle
            .creators
            .binary_search_by(|creator| creator.record_key.as_str().cmp(&changed.record_key))
            .map_err(|_| ())?;
        let creator = &bundle.creators[index];
        if creator.semantic_sha256 != changed.semantic_sha256 {
            return Err(());
        }
        creators.push(creator);
        previous_record_key = Some(&changed.record_key);
    }
    if creators.is_empty() {
        let receipt = plan.observation_receipt.as_ref().ok_or(())?;
        if Uuid::parse_str(&receipt.backup_id).is_err()
            || Uuid::parse_str(&receipt.receipt_id).is_err()
        {
            return Err(());
        }
        // `observed_at` and `replayed` belong to the receipt's wire contract
        // (`deny_unknown_fields`) but carry nothing this encoder acts on.
        let _wire_contract_only = (receipt.observed_at, receipt.replayed);
        return Ok(None);
    }
    if plan.observation_receipt.is_some() {
        return Err(());
    }
    prepare_delta(
        bundle,
        &plan.plan_token,
        plan.backup_kind,
        &plan.manifest_sha256,
        creators,
    )
    .map(Some)
}

fn prepare_delta(
    source: &CreatorBundle,
    plan_token: &str,
    backup_kind: CreatorBackupKind,
    manifest_sha256: &str,
    creators: Vec<&CreatorTransferCreator>,
) -> Result<PreparedSnapshot, ()> {
    let creator_count = u32::try_from(creators.len()).map_err(|_| ())?;
    let content_sha256 = delta_content_sha256(
        &source.content_sha256,
        manifest_sha256,
        creator_count,
        &creators,
    )?;
    let artifact = canonical_json(&CreatorDeltaArtifact {
        format: CREATOR_BUNDLE_FORMAT,
        format_version: crate::ACCESS_BUNDLE_FORMAT_VERSION,
        profile: "portable_public",
        snapshot_kind: "delta",
        source_bundle_content_sha256: &source.content_sha256,
        manifest_sha256,
        creator_count,
        content_sha256: &content_sha256,
        creators: &creators,
    })?;
    if artifact.len() > MAX_SNAPSHOT_BYTES {
        return Err(());
    }
    let chunks = build_chunks(&content_sha256, &creators)?;
    let artifact_sha256 = sha256_digest(&artifact);
    let init = SnapshotInit {
        format: CREATOR_BUNDLE_FORMAT,
        format_version: crate::ACCESS_BUNDLE_FORMAT_VERSION,
        profile: "portable_public",
        snapshot_kind: "delta",
        backup_kind,
        plan_token,
        source_bundle_content_sha256: &source.content_sha256,
        source_creator_count: source.creator_count,
        manifest_sha256,
        creator_count,
        content_sha256: &content_sha256,
        artifact_sha256: &artifact_sha256,
        artifact_size_bytes: artifact.len(),
        chunk_count: chunks.len(),
        max_snapshot_bytes: MAX_SNAPSHOT_BYTES,
        max_chunk_bytes: MAX_CHUNK_BYTES,
        max_chunks: MAX_CHUNKS,
    };
    let init_bytes = canonical_json(&init)?;
    if init_bytes.len() > 16 * 1024 {
        return Err(());
    }
    Ok(PreparedSnapshot {
        source_creator_count: source.creator_count,
        source_bundle_content_sha256: source.content_sha256.clone(),
        content_sha256,
        chunks,
        init_bytes,
    })
}

fn delta_content_sha256(
    source_bundle_content_sha256: &str,
    manifest_sha256: &str,
    creator_count: u32,
    creators: &[&CreatorTransferCreator],
) -> Result<String, ()> {
    let canonical = json!([
        CREATOR_BUNDLE_FORMAT,
        crate::ACCESS_BUNDLE_FORMAT_VERSION,
        "portable_public",
        "delta",
        source_bundle_content_sha256,
        manifest_sha256,
        creator_count,
        creators
            .iter()
            .map(|creator| json!([creator.record_key, creator.semantic_sha256]))
            .collect::<Vec<_>>(),
    ]);
    serde_json::to_vec(&canonical)
        .map(|bytes| sha256_digest(&bytes))
        .map_err(|_| ())
}

/// Splits `creators` into consecutive chunks, each the longest prefix of the
/// remaining Creators whose encoded envelope fits `MAX_CHUNK_BYTES`.
///
/// Every Creator is serialized exactly once for its byte length; a chunk
/// envelope is the empty envelope plus the Creator payloads, the separating
/// commas, and the `creator_count` digits, so the largest fitting `end` comes
/// from the prefix sums and one `encode_chunk` produces (and verifies) the
/// bytes. The split rule is byte-identical to growing the slice one Creator
/// at a time; `runtime::tests::encoding` pins that against the reference.
pub(super) fn build_chunks<T: Serialize>(
    content_sha256: &str,
    creators: &[T],
) -> Result<Vec<PreparedChunk>, ()> {
    let mut prefix = Vec::with_capacity(creators.len() + 1);
    prefix.push(0usize);
    for creator in creators {
        let len = serde_json::to_vec(creator).map_err(|_| ())?.len();
        prefix.push(prefix[prefix.len() - 1] + len);
    }
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < creators.len() {
        let chunk_index = chunks.len();
        let empty_len =
            encode_chunk(content_sha256, chunk_index, start, &creators[start..start])?.len();
        // `empty_len` already counts the brackets and the single digit of
        // `"creator_count":0`; a non-empty slice swaps that digit for the real
        // count and adds the payloads plus `count - 1` commas.
        let predicted = |end: usize| {
            let count = end - start;
            empty_len - 1 + decimal_digits(count) + (prefix[end] - prefix[start]) + (count - 1)
        };
        if predicted(start + 1) > MAX_CHUNK_BYTES {
            return Err(());
        }
        // `predicted` is strictly increasing in `end`, so the largest fitting
        // end is found by bisection over (start, creators.len()].
        let mut fits = start + 1;
        let mut upper = creators.len();
        while fits < upper {
            let middle = fits + (upper - fits).div_ceil(2);
            if predicted(middle) <= MAX_CHUNK_BYTES {
                fits = middle;
            } else {
                upper = middle - 1;
            }
        }
        let mut end = fits;
        let mut bytes = encode_chunk(content_sha256, chunk_index, start, &creators[start..end])?;
        debug_assert_eq!(bytes.len(), predicted(end));
        if bytes.len() > MAX_CHUNK_BYTES {
            // Defensive single step back if the prediction ever drifts from
            // the encoder; the first Creator alone was predicted to fit.
            end -= 1;
            if end == start {
                return Err(());
            }
            bytes = encode_chunk(content_sha256, chunk_index, start, &creators[start..end])?;
            if bytes.len() > MAX_CHUNK_BYTES {
                return Err(());
            }
        }
        chunks.push(PreparedChunk {
            index: chunk_index,
            idempotency_key: sha256_digest(&bytes),
            bytes,
        });
        if chunks.len() > MAX_CHUNKS {
            return Err(());
        }
        start = end;
    }
    Ok(chunks)
}

fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

pub(super) fn encode_chunk<T: Serialize>(
    content_sha256: &str,
    chunk_index: usize,
    creator_start_index: usize,
    creators: &[T],
) -> Result<Vec<u8>, ()> {
    canonical_json(&CreatorChunk {
        format: "reacher.creator_chunk",
        format_version: CREATOR_CHUNK_FORMAT_VERSION,
        snapshot_content_sha256: content_sha256,
        chunk_index,
        creator_start_index,
        creator_count: creators.len(),
        creators,
    })
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, ()> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| ())?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(super) fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value.as_bytes()[7..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(super) fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(71);
    output.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}
