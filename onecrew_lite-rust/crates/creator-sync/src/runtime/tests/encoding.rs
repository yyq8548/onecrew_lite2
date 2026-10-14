use super::*;
use crate::wire;
use onecrew_creator_runtime::CreatorTransferCreator;

const EMPTY_MANIFEST_SHA256: &str =
    "sha256:4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945";

#[test]
fn legacy_snapshot_requires_explicit_import_before_current_upload() {
    let bytes = include_bytes!("../../../tests/fixtures/creator_sync_real_sample.json");
    assert!(CreatorBundle::from_json_bytes(bytes).is_err());
    let upgraded = CreatorBundle::from_import_json_bytes(bytes).unwrap();
    assert_eq!(upgraded.format_version, 3);
    assert!(snapshot_encode::prepare_plan(&upgraded).is_ok());
}

#[test]
fn real_local_canonical_sample_uses_the_production_plan_encoder() {
    let bundle = CreatorBundle::from_json_bytes(include_bytes!(
        "../../../tests/fixtures/creator_sync_real_sample_v3.json"
    ))
    .unwrap();
    assert_eq!(bundle.creator_count, 3);
    assert_eq!(
        bundle
            .creators
            .iter()
            .map(|creator| creator.accounts.len())
            .sum::<usize>(),
        4
    );
    assert!(
        bundle
            .creators
            .iter()
            .all(|creator| creator.contact_routes.is_empty())
    );
    let plan = snapshot_encode::prepare_plan(&bundle).unwrap();
    assert_eq!(
        snapshot_encode::sha256_digest(&plan.request_bytes),
        "sha256:0ee452bc19a7a973a977d5bef3bdaa6b254cbbe8bf1c247ae0325be7fcd404e4"
    );
}

fn plan_response(
    bundle: &CreatorBundle,
    changed_record_keys: Vec<String>,
) -> wire::CreatorBackupPlanResponse {
    let changed_creators = changed_record_keys
        .into_iter()
        .map(|record_key| {
            let semantic_sha256 = bundle
                .creators
                .iter()
                .find(|creator| creator.record_key == record_key)
                .map(|creator| creator.semantic_sha256.clone())
                .unwrap_or_else(|| format!("sha256:{}", "f".repeat(64)));
            wire::CreatorBackupChangedCreator {
                record_key,
                semantic_sha256,
            }
        })
        .collect::<Vec<_>>();
    let observation_receipt =
        changed_creators
            .is_empty()
            .then(|| wire::CreatorBackupObservationReceipt {
                backup_id: "11111111-1111-4111-8111-111111111111".into(),
                receipt_id: "22222222-2222-4222-8222-222222222222".into(),
                observed_at: 1,
                replayed: false,
            });
    let backup_kind = if changed_creators.len() == bundle.creators.len() {
        wire::CreatorBackupKind::Full
    } else {
        wire::CreatorBackupKind::Delta
    };
    wire::CreatorBackupPlanResponse {
        status: "planned".into(),
        backup_kind,
        plan_token: "00000000-0000-4000-8000-000000000000".into(),
        source_bundle_format_version: bundle.format_version,
        source_bundle_content_sha256: bundle.content_sha256.clone(),
        source_creator_count: bundle.creator_count,
        manifest_sha256: EMPTY_MANIFEST_SHA256.into(),
        changed_creators,
        observation_receipt,
    }
}

#[test]
fn full_manifest_and_delta_envelopes_match_the_current_golden() {
    assert_v3_golden(include_bytes!(
        "../../../tests/fixtures/creator_sync_v3_golden.json"
    ));
}

#[test]
fn tiktok_bundle_plan_delta_and_chunk_match_shared_v3_golden() {
    assert_v3_golden(include_bytes!(
        "../../../tests/fixtures/creator_sync_tiktok_v3_golden.json"
    ));
}

fn assert_v3_golden(bytes: &[u8]) {
    let golden: Value = serde_json::from_slice(bytes).unwrap();
    let expected = &golden["expected"];
    // A Value map sorts object keys and omits the canonical trailing newline.
    // Feed the frozen byte string to the strict reader, never a re-encoded map.
    let bundle =
        CreatorBundle::from_json_bytes(expected["bundle"].as_str().unwrap().as_bytes()).unwrap();
    assert_eq!(serde_json::to_value(&bundle).unwrap(), golden["bundle"]);
    let plan = snapshot_encode::prepare_plan(&bundle).unwrap();
    let expected_plan = expected["plan"].as_str().unwrap();
    assert_eq!(plan.request_bytes, expected_plan.as_bytes());
    assert_eq!(
        snapshot_encode::sha256_digest(&plan.request_bytes),
        expected["plan_sha256"]
    );
    assert!(!expected_plan.contains("access_user_id"));
    assert!(!expected_plan.contains("group"));
    assert!(!expected_plan.contains("_at"));

    let prepared = snapshot_encode::prepare_from_plan(
        &bundle,
        &plan_response(&bundle, vec![bundle.creators[0].record_key.clone()]),
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.source_creator_count, 1);
    assert_eq!(prepared.source_bundle_content_sha256, bundle.content_sha256);
    assert_eq!(
        prepared.content_sha256,
        expected["content_sha256"].as_str().unwrap()
    );
    assert_eq!(prepared.chunks.len(), 1);
    assert_eq!(prepared.chunks[0].index, 0);
    assert_eq!(
        prepared.chunks[0].bytes,
        expected["chunk"].as_str().unwrap().as_bytes()
    );
    assert_eq!(
        prepared.chunks[0].idempotency_key,
        expected["chunk_sha256"].as_str().unwrap()
    );
    let expected_init = expected["init"].as_str().unwrap();
    assert_eq!(prepared.init_bytes, expected_init.as_bytes());
}

#[test]
fn plan_response_is_strict_and_closed_over_the_source_manifest() {
    let bundle = fixture_bundle();
    for version in [1, 2, 4] {
        let mut unsupported = plan_response(&bundle, vec![bundle.creators[0].record_key.clone()]);
        unsupported.source_bundle_format_version = version;
        assert!(snapshot_encode::prepare_from_plan(&bundle, &unsupported).is_err());
    }
    let missing_version = json!({
        "status": "planned", "backup_kind": "full",
        "plan_token": "00000000-0000-4000-8000-000000000000",
        "source_bundle_content_sha256": bundle.content_sha256,
        "source_creator_count": 1, "manifest_sha256": EMPTY_MANIFEST_SHA256,
        "changed_creators": [], "observation_receipt": null,
    });
    assert!(serde_json::from_value::<wire::CreatorBackupPlanResponse>(missing_version).is_err());
    let unknown_field = format!(
        "{{\"status\":\"planned\",\"backup_kind\":\"full\",\"plan_token\":\"00000000-0000-4000-8000-000000000000\",\"source_bundle_content_sha256\":\"{}\",\"source_creator_count\":1,\"manifest_sha256\":\"{}\",\"changed_creators\":[],\"observation_receipt\":null,\"access_user_id\":\"forbidden\"}}",
        bundle.content_sha256, EMPTY_MANIFEST_SHA256
    );
    assert!(
        crate::runtime::parse_json::<wire::CreatorBackupPlanResponse>(unknown_field.as_bytes())
            .is_err()
    );

    for changed_record_keys in [
        vec![format!("sha256:{}", "f".repeat(64))],
        vec![
            bundle.creators[0].record_key.clone(),
            bundle.creators[0].record_key.clone(),
        ],
    ] {
        assert!(
            snapshot_encode::prepare_from_plan(
                &bundle,
                &plan_response(&bundle, changed_record_keys),
            )
            .is_err()
        );
    }

    let mut wrong_echo = plan_response(&bundle, Vec::new());
    wrong_echo.source_bundle_content_sha256 = format!("sha256:{}", "f".repeat(64));
    assert!(snapshot_encode::prepare_from_plan(&bundle, &wrong_echo).is_err());
    assert!(
        snapshot_encode::prepare_from_plan(&bundle, &plan_response(&bundle, Vec::new()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn full_v2_transition_preserves_the_existing_remote_manifest_cas() {
    let bundle = fixture_bundle();
    let mut transition = plan_response(&bundle, vec![bundle.creators[0].record_key.clone()]);
    transition.manifest_sha256 = format!("sha256:{}", "9".repeat(64));
    let prepared = snapshot_encode::prepare_from_plan(&bundle, &transition)
        .unwrap()
        .unwrap();
    let init: Value = serde_json::from_slice(&prepared.init_bytes).unwrap();
    assert_eq!(init["backup_kind"], "full");
    assert_eq!(init["manifest_sha256"], transition.manifest_sha256);
    assert_eq!(init["creator_count"], bundle.creator_count);
}

/// The pre-linearization chunker: grow the slice one Creator at a time and
/// re-encode the whole prefix until it no longer fits. Kept verbatim as the
/// reference for the split rule the production encoder must reproduce.
fn reference_build_chunks(
    content_sha256: &str,
    creators: &[CreatorTransferCreator],
) -> Result<Vec<snapshot_encode::PreparedChunk>, ()> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < creators.len() {
        let mut end = start + 1;
        let mut accepted = None;
        while end <= creators.len() {
            let bytes = snapshot_encode::encode_chunk(
                content_sha256,
                chunks.len(),
                start,
                &creators[start..end],
            )?;
            if bytes.len() > snapshot_encode::MAX_CHUNK_BYTES {
                break;
            }
            accepted = Some(bytes);
            end += 1;
        }
        let bytes = accepted.ok_or(())?;
        let creator_count = if end > creators.len() {
            creators.len() - start
        } else {
            end - start - 1
        };
        chunks.push(snapshot_encode::PreparedChunk {
            index: chunks.len(),
            idempotency_key: snapshot_encode::sha256_digest(&bytes),
            bytes,
        });
        if chunks.len() > snapshot_encode::MAX_CHUNKS {
            return Err(());
        }
        start += creator_count;
    }
    Ok(chunks)
}

/// Deterministic synthetic Creators of roughly `target_bytes` encoded size
/// with varied per-record lengths so chunk boundaries fall at different
/// remainders; record keys are unique and strictly increasing.
fn synthetic_creators(count: usize, target_bytes: usize) -> Vec<CreatorTransferCreator> {
    let template = fixture_bundle().creators[0].clone();
    let base_len = serde_json::to_vec(&template).unwrap().len();
    (0..count)
        .map(|index| {
            let mut creator = template.clone();
            creator.record_key = format!("sha256:{index:064x}");
            creator.semantic_sha256 = format!("sha256:{:064x}", index ^ 0x5a5a);
            // Vary each record by up to 255 bytes around the target so the
            // maximal-prefix rule is exercised at many different remainders.
            let padding = target_bytes.saturating_sub(base_len) + (index * 37) % 256;
            creator.display_name = "x".repeat(padding);
            creator
        })
        .collect()
}

fn assert_same_chunks(
    expected: &[snapshot_encode::PreparedChunk],
    actual: &[snapshot_encode::PreparedChunk],
) {
    assert_eq!(expected.len(), actual.len(), "chunk count differs");
    for (expected, actual) in expected.iter().zip(actual) {
        assert_eq!(expected.index, actual.index);
        assert_eq!(expected.idempotency_key, actual.idempotency_key);
        assert!(
            expected.bytes == actual.bytes,
            "chunk {} bytes differ",
            expected.index
        );
    }
}

#[test]
fn linearized_chunker_matches_the_reference_on_the_fixture_and_small_synthetic_bundles() {
    let bundle = fixture_bundle();
    let refs = bundle.creators.iter().collect::<Vec<_>>();
    let content_sha256 = &bundle.content_sha256;
    assert_same_chunks(
        &reference_build_chunks(content_sha256, &bundle.creators).unwrap(),
        &snapshot_encode::build_chunks(content_sha256, &refs).unwrap(),
    );
    assert!(
        snapshot_encode::build_chunks::<CreatorTransferCreator>(content_sha256, &[])
            .unwrap()
            .is_empty()
    );

    // Records near the chunk limit: exactly one per chunk, and one that can
    // never fit is rejected by both implementations.
    for target in [
        snapshot_encode::MAX_CHUNK_BYTES / 2 - 64,
        snapshot_encode::MAX_CHUNK_BYTES / 3 + 1,
    ] {
        let creators = synthetic_creators(7, target);
        assert_same_chunks(
            &reference_build_chunks(content_sha256, &creators).unwrap(),
            &snapshot_encode::build_chunks(content_sha256, &creators).unwrap(),
        );
    }
    let oversized = synthetic_creators(1, snapshot_encode::MAX_CHUNK_BYTES + 1);
    assert!(reference_build_chunks(content_sha256, &oversized).is_err());
    assert!(snapshot_encode::build_chunks(content_sha256, &oversized).is_err());
}

/// Checks the split rule the reference implements, in O(N): the chunks tile
/// the Creators contiguously, every chunk is exactly `encode_chunk` of its
/// slice and fits the limit, and appending the next Creator would not fit.
fn assert_maximal_prefix_chunks(
    content_sha256: &str,
    creators: &[CreatorTransferCreator],
    chunks: &[snapshot_encode::PreparedChunk],
) {
    let mut start = 0usize;
    for (position, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.index, position);
        let envelope: Value = serde_json::from_slice(&chunk.bytes).unwrap();
        assert_eq!(envelope["chunk_index"].as_u64().unwrap() as usize, position);
        assert_eq!(
            envelope["creator_start_index"].as_u64().unwrap() as usize,
            start
        );
        let count = envelope["creator_count"].as_u64().unwrap() as usize;
        let end = start + count;
        assert!(count >= 1 && end <= creators.len());
        assert!(chunk.bytes.len() <= snapshot_encode::MAX_CHUNK_BYTES);
        assert_eq!(
            chunk.idempotency_key,
            snapshot_encode::sha256_digest(&chunk.bytes)
        );
        let expected =
            snapshot_encode::encode_chunk(content_sha256, position, start, &creators[start..end])
                .unwrap();
        assert!(chunk.bytes == expected, "chunk {position} bytes differ");
        if end < creators.len() {
            let grown = snapshot_encode::encode_chunk(
                content_sha256,
                position,
                start,
                &creators[start..=end],
            )
            .unwrap();
            assert!(
                grown.len() > snapshot_encode::MAX_CHUNK_BYTES,
                "chunk {position} is not the maximal fitting prefix"
            );
        }
        start = end;
    }
    assert_eq!(start, creators.len());
}

#[test]
fn linearized_chunker_matches_the_reference_across_several_chunks() {
    // ~8KB records keep the quadratic reference cheap (about 2 full chunks)
    // while still crossing boundaries at varied remainders.
    let creators = synthetic_creators(260, 8 * 1024);
    let content_sha256 = format!("sha256:{}", "7".repeat(64));
    let actual = snapshot_encode::build_chunks(&content_sha256, &creators).unwrap();
    assert!(
        actual.len() >= 3,
        "expected several chunks, got {}",
        actual.len()
    );
    assert_same_chunks(
        &reference_build_chunks(&content_sha256, &creators).unwrap(),
        &actual,
    );
}

#[test]
fn linearized_chunker_splits_ten_thousand_two_kilobyte_creators_by_maximal_prefix() {
    let creators = synthetic_creators(10_000, 2 * 1024);
    let content_sha256 = format!("sha256:{}", "7".repeat(64));

    let started = Instant::now();
    let chunks = snapshot_encode::build_chunks(&content_sha256, &creators).unwrap();
    let elapsed = started.elapsed();

    assert!(chunks.len() > 10);
    let total = chunks.iter().map(|chunk| chunk.bytes.len()).sum::<usize>();
    assert!(total > 10_000 * 2 * 1024);
    assert_maximal_prefix_chunks(&content_sha256, &creators, &chunks);
    eprintln!(
        "build_chunks 10k x 2KB: {} chunks, {total} bytes, linearized {elapsed:?}",
        chunks.len()
    );
}

/// Full byte-for-byte comparison against the quadratic reference on the
/// 10k x 2KB bundle. The reference re-serializes ~5 GB and takes about two
/// minutes in a debug build, so it is opt-in:
/// `cargo test -p onecrew-creator-sync encoding -- --ignored`.
#[test]
#[ignore = "quadratic reference takes ~2 minutes in debug; run with --ignored"]
fn linearized_chunker_matches_the_reference_on_ten_thousand_two_kilobyte_creators() {
    let creators = synthetic_creators(10_000, 2 * 1024);
    let content_sha256 = format!("sha256:{}", "7".repeat(64));

    let started = Instant::now();
    let actual = snapshot_encode::build_chunks(&content_sha256, &creators).unwrap();
    let linear = started.elapsed();

    let started = Instant::now();
    let expected = reference_build_chunks(&content_sha256, &creators).unwrap();
    let quadratic = started.elapsed();

    assert!(actual.len() > 10);
    assert_same_chunks(&expected, &actual);
    eprintln!(
        "build_chunks 10k x 2KB: {} chunks, linearized {linear:?}, reference {quadratic:?}",
        actual.len()
    );
}

#[test]
fn prepare_from_plan_requires_a_sorted_bundle_and_avoids_cloning_the_full_bundle() {
    let bundle = fixture_bundle();
    let mut two = (*bundle).clone();
    let mut second = bundle.creators[0].clone();
    second.record_key = format!("sha256:{}", "0".repeat(64));
    second.semantic_sha256 = format!("sha256:{}", "1".repeat(64));
    two.creators.insert(0, second);
    two.creator_count = 2;
    let plan = plan_response(
        &two,
        two.creators
            .iter()
            .map(|creator| creator.record_key.clone())
            .collect(),
    );
    assert!(plan.backup_kind == wire::CreatorBackupKind::Full);
    let prepared = snapshot_encode::prepare_from_plan(&two, &plan)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.chunks.len(), 1);

    two.creators.swap(0, 1);
    assert!(snapshot_encode::prepare_from_plan(&two, &plan).is_err());
}
