//! Test preparation uses the real Creator owner and private runtime mint path.
//! Only fresh provider network reads are replaced by canonical test evidence.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use onecrew_creator_runtime::{
    AccountRefId, CanonicalCreatorSaveInput, CreatorQualityInput, CreatorSaveContactEvidence,
    CreatorSaveProfile, CreatorSaveProvenance, Platform, PlatformAccountCandidate, SourceKind,
    TargetKind,
};
use onecrew_local_runtime::{
    CreatorSaveBatchSelection, CreatorSaveCallResult, CreatorSaveCandidateInput, CreatorSaveTarget,
    LocalPlatform, OwnerReviewContext, PendingActionView, PrepareCreatorSaveInput, ResultOrigin,
    ResultSnapshot,
};
use schemars::JsonSchema;
use serde::Serialize;

use super::fixtures::{TestOwnerError, metadata, policy};
use crate::operation_registry::{
    descriptor::{ApprovalClass, EffectClass},
    registration::{OperationRegistry, RegistryBuilder},
    v2::creator_save::{BATCH_PREPARE, PREPARE},
};

const OPERATOR_ACCOUNT_ID: &str = "00000000-0000-0000-0000-000000000001";

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NativeReadOutput {
    items: Vec<NativeReadItem>,
    next_cursor: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NativeReadItem {
    post_id: String,
    text: String,
    language: String,
    author: NativeReadAuthor,
    media: Vec<NativeReadMedia>,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NativeReadAuthor {
    candidate_id: String,
    display_name: String,
    handle: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NativeReadMedia {
    width: u32,
    height: u32,
}

fn native_read_page() -> NativeReadOutput {
    NativeReadOutput {
        items: (0..50)
            .map(|index| NativeReadItem {
                post_id: format!("post-{index}"),
                text: format!("Native read body {index} {}", "x".repeat(96)),
                language: "en".to_owned(),
                author: NativeReadAuthor {
                    candidate_id: format!("candidate-{index}"),
                    display_name: format!("Creator {index}"),
                    handle: format!("creator_{index}"),
                },
                media: vec![
                    NativeReadMedia {
                        width: 947,
                        height: 1183,
                    },
                    NativeReadMedia {
                        width: 320,
                        height: 400,
                    },
                ],
            })
            .collect(),
        next_cursor: "cursor-2".to_owned(),
    }
}

pub(super) fn entries(platform: &LocalPlatform, count: usize) -> Vec<CanonicalCreatorSaveInput> {
    let account_ref_id = AccountRefId::new(OPERATOR_ACCOUNT_ID);
    platform
        .creators()
        .unwrap()
        .sync_account_ref(
            account_ref_id.clone(),
            Platform::X,
            "profile",
            "@fixture_operator",
        )
        .unwrap();
    (0..count)
        .map(|index| {
            let handle = format!("person{index}");
            let url = format!("https://x.com/{handle}");
            CanonicalCreatorSaveInput {
                row_id: format!("row-{index}"),
                account: PlatformAccountCandidate {
                    platform: Platform::X,
                    external_id: Some((index + 1).to_string()),
                    target_kind: Some(TargetKind::User),
                    handle: Some(handle),
                    display_name: format!("Creator {index}"),
                    profile_url: Some(url.clone()),
                },
                profile: CreatorSaveProfile {
                    avatar_url: Some(format!("https://example.com/{index}.jpg")),
                    description: Some("Freshly observed biography".into()),
                    follower_count: Some(39617),
                    ..Default::default()
                },
                profile_observed_at: 1_800_000_000,
                provenance: CreatorSaveProvenance::DirectSelection {
                    source_ref: url.clone(),
                    source_url: Some(url.clone()),
                    description: Some("Explicit user selection".into()),
                    operator_account_ref_id: Some(account_ref_id.clone()),
                },
                contact_evidence: vec![CreatorSaveContactEvidence::ProfileLink {
                    source: SourceKind::ProfileWebsite,
                    source_url: Some(url),
                    display_text: "Website".into(),
                    target_url: format!("https://example.com/contact/{index}"),
                }],
                notes: Some("Reviewed 中文".into()),
                tags: Some(vec!["reviewed".into()]),
                quality: CreatorQualityInput {
                    score: Some(80),
                    assessment: Some("Relevant".into()),
                    follower_counts: [(Platform::X, 39617)].into(),
                },
            }
        })
        .collect()
}

pub(super) fn candidate(index: usize) -> CreatorSaveCandidateInput {
    serde_json::from_value(serde_json::json!({
        "intent":"save_or_update",
        "target": {"platform":"x","user_id":(index + 1).to_string(),"handle":format!("person{index}")},
        "connection": {"platform":"x","account_ref_id":OPERATOR_ACCOUNT_ID},
        "provenance": {"kind":"direct_selection","source_url":format!("https://x.com/person{index}"),"description":"Explicit user selection"},
        "notes":"Reviewed 中文","tags":["reviewed"],"quality":{"score":80,"assessment":"Relevant"}
    })).unwrap()
}

pub(super) struct Fixture {
    pub registry: OperationRegistry,
    pub candidates: Vec<CreatorSaveCandidateInput>,
    pub prepares: Arc<AtomicUsize>,
    pub contexts: Arc<Mutex<Vec<OwnerReviewContext>>>,
    pub reads: Arc<Mutex<Vec<(String, usize)>>>,
    pub generic_writes: Arc<AtomicUsize>,
}

pub(super) fn registry(platform: Arc<LocalPlatform>, count: usize) -> Fixture {
    let entries = entries(&platform, count);
    registry_with_entries(platform, entries)
}

pub(super) fn registry_with_entries(
    platform: Arc<LocalPlatform>,
    entries: Vec<CanonicalCreatorSaveInput>,
) -> Fixture {
    registry_with_entries_mode(platform, entries, false)
}

pub(super) fn registry_with_large_reads(platform: Arc<LocalPlatform>, count: usize) -> Fixture {
    let entries = entries(&platform, count);
    registry_with_entries_mode(platform, entries, true)
}

pub(super) fn registry_with_large_reads_and_names(
    platform: Arc<LocalPlatform>,
    count: usize,
    name_bytes: usize,
) -> Fixture {
    let mut entries = entries(&platform, count);
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.account.display_name = format!("Creator {index} {}", "a".repeat(name_bytes));
    }
    registry_with_entries_mode(platform, entries, true)
}

fn registry_with_entries_mode(
    platform: Arc<LocalPlatform>,
    entries: Vec<CanonicalCreatorSaveInput>,
    include_large_read: bool,
) -> Fixture {
    let count = entries.len();
    let entries = Arc::new(entries);
    let candidates = (0..count).map(candidate).collect();
    let prepares = Arc::new(AtomicUsize::new(0));
    let contexts = Arc::new(Mutex::new(Vec::new()));
    let mut catalog_ids = vec![
        PREPARE,
        BATCH_PREPARE,
        "onecrew.test.generic-write",
        "onecrew.test.read-before",
        "onecrew.test.read-between",
    ];
    if include_large_read {
        catalog_ids.push("onecrew.test.native-read");
    }
    let mut builder = RegistryBuilder::new(
        crate::operation_registry::catalog_test_support::catalog(&catalog_ids),
    );
    let reads = Arc::new(Mutex::new(Vec::new()));
    let generic_writes = Arc::new(AtomicUsize::new(0));
    let writes = Arc::clone(&generic_writes);
    let mut generic_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::ExternalWrite);
    generic_policy.approval_request = Some(
        crate::operation_registry::approval::ApprovalRequest::new(
            "Confirm the generic fixture write.",
        )
        .unwrap(),
    );
    builder.register_operation::<super::fixtures::TestInput, super::fixtures::TestOutput, TestOwnerError, _, _, _>(
        metadata("onecrew.test.generic-write"), generic_policy,
        move |_, input| { let writes = Arc::clone(&writes); async move {
            writes.fetch_add(1, Ordering::SeqCst);
            Ok(super::fixtures::TestOutput { value: input.value })
        } }, |_| unreachable!("generic fixture succeeds"),
    ).unwrap();
    for id in ["onecrew.test.read-before", "onecrew.test.read-between"] {
        let platform = Arc::clone(&platform);
        let reads = Arc::clone(&reads);
        builder.register_operation::<super::fixtures::TestInput, super::fixtures::TestOutput, TestOwnerError, _, _, _>(
            metadata(id), policy(ApprovalClass::None, EffectClass::ReadLocal),
            move |_, _| {
                let platform = Arc::clone(&platform);
                let reads = Arc::clone(&reads);
                async move {
                    let count = tokio::task::spawn_blocking(move || saved_count(&platform)).await.unwrap();
                    reads.lock().unwrap().push((id.into(), count));
                    Ok(super::fixtures::TestOutput { value: format!("{id}: {count} Creators") })
                }
            }, |_| unreachable!("read fixture succeeds"),
        ).unwrap();
    }
    if include_large_read {
        builder
            .register_operation::<
                super::fixtures::TestInput,
                NativeReadOutput,
                TestOwnerError,
                _,
                _,
                _,
            >(
                metadata("onecrew.test.native-read"),
                policy(ApprovalClass::None, EffectClass::ReadLocal),
                move |_, _| async move { Ok(native_read_page()) },
                |_| unreachable!("native read fixture succeeds"),
            )
            .unwrap();
    }
    macro_rules! register_prepare {
        ($id:expr, $input:ty, $candidates:expr) => {{
            let platform = Arc::clone(&platform);
            let entries = Arc::clone(&entries);
            let prepares = Arc::clone(&prepares);
            let contexts = Arc::clone(&contexts);
            builder
                .register_owner_review_operation::<$input, CreatorSaveCallResult, TestOwnerError, _, _, _>(
                    metadata($id),
                    review_policy(),
                    move |context, input| {
                        let platform = Arc::clone(&platform);
                        let entries = Arc::clone(&entries);
                        let prepares = Arc::clone(&prepares);
                        let contexts = Arc::clone(&contexts);
                        async move {
                            let context = context.owner_review_context().expect("server review context is bound");
                            prepares.fetch_add(1, Ordering::SeqCst);
                            let candidates: Vec<CreatorSaveCandidateInput> = ($candidates)(input);
                            let intent = candidates
                                .first()
                                .map(|candidate| candidate.intent)
                                .expect("validated save candidate batch");
                            let mut prepared_entries = Vec::new();
                            for candidate in candidates {
                                let CreatorSaveTarget::X { user_id, .. } = candidate.target else {
                                    panic!("offline fixture supplies X provider data only");
                                };
                                let index = user_id.parse::<usize>().unwrap() - 1;
                                let mut entry = entries[index].clone();
                                entry.notes = candidate.notes;
                                entry.tags = candidate.tags;
                                entry.quality = candidate.quality;
                                entry.quality.follower_counts = [(Platform::X, 39617)].into();
                                prepared_entries.push(entry);
                            }
                            let review = onecrew_local_runtime::test_support::prepare_creator_save_review(
                                &platform, prepared_entries, context.clone(), intent,
                            ).await.map_err(|_| TestOwnerError)?;
                            contexts.lock().unwrap().push(context);
                            Ok(review)
                        }
                    },
                    |_| unreachable!("valid canonical fixture prepares"),
                )
                .unwrap();
        }};
    }
    register_prepare!(PREPARE, CreatorSaveCandidateInput, |input| vec![input]);
    register_prepare!(
        BATCH_PREPARE,
        PrepareCreatorSaveInput,
        |input: PrepareCreatorSaveInput| input.candidates
    );
    Fixture {
        registry: builder.build().unwrap(),
        candidates,
        prepares,
        contexts,
        reads,
        generic_writes,
    }
}

fn review_policy() -> crate::operation_registry::registration::OperationPolicy {
    let mut policy = policy(ApprovalClass::AttendedPrompt, EffectClass::WriteLocal);
    policy.approval_request = Some(
        crate::operation_registry::approval::ApprovalRequest::new(
            "Review Creator rows before saving.",
        )
        .unwrap(),
    );
    policy
}

pub(super) async fn prepare_review(
    fixture: &Fixture,
    platform: &Arc<LocalPlatform>,
    origin: &ResultOrigin,
) -> ResultSnapshot {
    let output = fixture
        .registry
        .tool_execute_v2(
            platform,
            origin,
            crate::operation_registry::descriptor::SurfaceId::Full,
            "batch-fixture",
            &crate::operation_registry::v2::contract::ToolExecuteInput {
                calls: vec![super::fixtures::call(
                    "batch-call",
                    BATCH_PREPARE,
                    super::fixtures::revision(&fixture.registry, BATCH_PREPARE),
                    serde_json::json!({"candidates":fixture.candidates}),
                )],
                presentation:
                    crate::operation_registry::v2::contract::PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    platform
        .present_result(&output.result_ref.unwrap(), origin)
        .unwrap()
}

pub(super) async fn review(
    platform: &Arc<LocalPlatform>,
    origin: &ResultOrigin,
    count: usize,
) -> ResultSnapshot {
    let fixture = registry(Arc::clone(platform), count);
    prepare_review(&fixture, platform, origin).await
}

pub(super) fn selections(
    snapshot: &ResultSnapshot,
    count: usize,
) -> Vec<CreatorSaveBatchSelection> {
    let Some(PendingActionView::CreatorSaveBatch { review }) = &snapshot.pending_action else {
        panic!("typed batch review");
    };
    review
        .rows
        .iter()
        .take(count)
        .map(|row| CreatorSaveBatchSelection {
            row_id: row.row_id.clone(),
            confirmed_route_ids: row
                .preview
                .routes
                .iter()
                .filter(|route| route.required)
                .map(|route| route.route_id.clone())
                .collect(),
        })
        .collect()
}

pub(super) fn saved_count(platform: &LocalPlatform) -> usize {
    platform
        .creators()
        .unwrap()
        .list_creators(onecrew_creator_runtime::ListCreatorsInput {
            include_facets: true,
            ..Default::default()
        })
        .unwrap()
        .facets
        .expect("requested Creator count")
        .total
        .try_into()
        .expect("fixture count fits usize")
}
