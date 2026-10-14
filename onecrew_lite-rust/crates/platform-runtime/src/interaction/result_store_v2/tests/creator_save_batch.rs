use std::sync::Arc;

use onecrew_creator_runtime::{
    AccountRefId, CanonicalCreatorSaveInput, ContactChannel, CreatorQualityInput,
    CreatorSaveContactEvidence, CreatorSaveProfile, CreatorSaveProvenance, LocatorKind, Platform,
    PlatformAccountCandidate, SourceKind, TargetKind,
};
use uuid::Uuid;

use super::*;

pub(in crate::interaction::result_store_v2) fn input(
    platform: &crate::LocalPlatform,
    count: usize,
    deadline: Instant,
) -> ExecutedResultInput {
    input_with_extra_contact(platform, count, deadline, None)
}

fn input_with_extra_contact(
    platform: &crate::LocalPlatform,
    count: usize,
    deadline: Instant,
    extra_contact: Option<(usize, &str)>,
) -> ExecutedResultInput {
    let operator_id = Uuid::from_u128(1);
    let operator_ref = AccountRefId::new(operator_id.to_string());
    let creators = platform.creators().unwrap();
    // Seed the real X account projection required by provenance validation.
    // No vault credentials/session are installed, so confirmation still proves
    // it can finish without a provider read.
    creators
        .sync_account_ref(
            operator_ref.clone(),
            Platform::X,
            "profile",
            "@testoperator",
        )
        .unwrap();
    let mut inputs = Vec::new();
    for index in 0..count {
        let username = format!("person{index}");
        let profile_url = format!("https://x.com/{username}");
        let account = PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some((index + 1).to_string()),
            target_kind: Some(TargetKind::User),
            handle: Some(username.clone()),
            display_name: format!("Person {index}"),
            profile_url: Some(profile_url.clone()),
        };
        inputs.push(CanonicalCreatorSaveInput {
            profile_observed_at: 1_800_000_000,
            row_id: format!("row-{index}"),
            account,
            profile: CreatorSaveProfile {
                avatar_url: Some(format!("https://images.example.com/creators/{index}/avatar.jpg")),
                description: Some(format!("{} Contact: creator-{index}@example.com", "Independent researcher publishing original weekly technology analysis, source-linked interviews, and practical product demonstrations for an active specialist audience. ".repeat(3))),
                location: None,
                website_urls: vec![format!("https://creator-{index}.example.com/contact")],
                follower_count: Some(10_000 + index as u64),
            },
            provenance: CreatorSaveProvenance::DirectSelection {
                source_ref: profile_url.clone(),
                source_url: Some(profile_url.clone()),
                description: Some("Direct user selection".into()),
                operator_account_ref_id: Some(operator_ref.clone()),
            },
            contact_evidence: vec![
                CreatorSaveContactEvidence::ProfileLink { source: SourceKind::ProfileWebsite, source_url: Some(profile_url.clone()),
                    display_text: "Website".into(), target_url: format!("https://creator-{index}.example.com/contact") },
                CreatorSaveContactEvidence::Observation { channel: ContactChannel::Email,
                    kind: LocatorKind::EmailAddress, value: format!("creator-{index}@example.com"),
                    source: SourceKind::ProfileBio, source_url: Some(profile_url) },
            ],
            notes: Some("Original reviewed notes".into()),
            tags: None,
            quality: CreatorQualityInput::default(),
        });
    }
    if let Some((index, email)) = extra_contact {
        let row = &mut inputs[index];
        row.contact_evidence
            .push(CreatorSaveContactEvidence::Observation {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                value: email.to_owned(),
                source: SourceKind::ProfileBio,
                source_url: row.account.profile_url.clone(),
            });
    }
    input_from_entries(platform, inputs, deadline)
}

/// Seal the review through the production factory. Every test in this module
/// then reviews the projection the save owner really emits: a hand-built review
/// would leave the projection budgets, and so the batch limit, unproven.
fn input_from_entries(
    platform: &crate::LocalPlatform,
    inputs: Vec<CanonicalCreatorSaveInput>,
    deadline: Instant,
) -> ExecutedResultInput {
    let creators = platform.creators().unwrap();
    let prepared = creators
        .prepare_creator_save_batch(
            inputs,
            onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
        )
        .unwrap();
    let context = OwnerReviewContext::new(
        origin("origin-one"),
        "save-1".to_owned(),
        "onecrew.creator.candidate.batch.save".to_owned(),
        None,
    )
    .unwrap();
    let review = crate::creator_save::seal_prepared_review_for_test(
        prepared,
        context,
        deadline.saturating_duration_since(Instant::now()),
    )
    .unwrap();
    let mut input = review
        .into_input(
            ExecutedBatch {
                outcomes: vec![ExecutedCallOutcome {
                    call_id: "save-1".to_owned(),
                    operation_id: "onecrew.creator.candidate.batch.save".to_owned(),
                    disposition: CallDisposition::NotRun {
                        reason: "confirmation_required".to_owned(),
                    },
                    summary:
                        "Prepared fresh Creator details for review; confirmation is required to save."
                            .to_owned(),
                    resource_refs: Vec::new(),
                }],
            },
            None,
        )
        .unwrap();
    // Production sealing turns a lifetime into a deadline, which lands after the
    // instant the caller asked for. Tests assert expiry exactly at that instant.
    if let Some(PendingUserAction::CreatorSaveBatch { payload, .. }) = &mut input.pending_action {
        payload
            .set_creator_save_deadline_for_test(deadline)
            .unwrap();
    }
    input
}

fn selections(snapshot: &ResultSnapshot) -> Vec<CreatorSaveBatchSelection> {
    let Some(PendingActionView::CreatorSaveBatch { review }) = &snapshot.pending_action else {
        panic!("prepared batch review")
    };
    review
        .rows
        .iter()
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

#[test]
fn forged_generic_payload_and_changed_preview_cannot_mint_save_authority() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    for corrupt_payload in [true, false] {
        let mut value = input(&platform, 1, Instant::now() + Duration::from_secs(60));
        let Some(PendingUserAction::CreatorSaveBatch { payload, review }) =
            &mut value.pending_action
        else {
            unreachable!()
        };
        if corrupt_payload {
            *payload = OwnerActionPayload::new(b"forged canonical payload").unwrap();
        } else {
            review.rows[0].preview.notes = Some("Unreviewed replacement".into());
        }
        assert_eq!(
            ResultMemoryStore::new().mint_from_execute(value),
            Err(ResultStoreError::InvalidInput(
                "trusted_creator_save_payload_required"
            ))
        );
    }
}

#[test]
fn route_and_row_choices_are_bound_atomically_into_one_consumable_payload() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let value = input_with_extra_contact(
        &platform,
        3,
        Instant::now() + Duration::from_secs(60),
        Some((1, "additional-contact@example.com")),
    );
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(value).unwrap());
    let valid = selections(&available);
    let Some(PendingActionView::CreatorSaveBatch { review }) = &available.pending_action else {
        panic!("batch review")
    };
    // Route IDs are scoped to each reviewed row. Use a real fourth route from
    // row 1, rather than mistaking equal local IDs for a cross-row reference.
    let foreign_route = review.rows[1]
        .preview
        .routes
        .iter()
        .find(|route| {
            !review.rows[0]
                .preview
                .routes
                .iter()
                .any(|own| own.route_id == route.route_id)
        })
        .expect("the extra contact creates a route absent from row 0")
        .route_id
        .clone();
    let optional_route = review.rows[0]
        .preview
        .routes
        .iter()
        .find(|route| !route.required)
        .unwrap()
        .route_id
        .clone();
    let mut cross_row = valid[0].clone();
    cross_row.confirmed_route_ids.push(foreign_route);
    let mut unknown_route = valid[0].clone();
    unknown_route
        .confirmed_route_ids
        .push("route-not-in-this-review".into());
    let mut duplicate_route = valid[0].clone();
    duplicate_route
        .confirmed_route_ids
        .push(valid[0].confirmed_route_ids[0].clone());
    for (case, invalid, expected_reason) in [
        ("empty batch", Vec::new(), "creator_save_batch_selection"),
        (
            "too many rows",
            vec![valid[0].clone(); valid.len() + 1],
            "creator_save_batch_selection",
        ),
        ("cross-row route", vec![cross_row], "creator_save_routes"),
        (
            "duplicate rows",
            vec![valid[0].clone(), valid[0].clone()],
            "creator_save_duplicate_row",
        ),
        ("unknown route", vec![unknown_route], "creator_save_routes"),
        (
            "duplicate route",
            vec![duplicate_route],
            "creator_save_routes",
        ),
        (
            "empty routes",
            vec![CreatorSaveBatchSelection {
                row_id: valid[0].row_id.clone(),
                confirmed_route_ids: Vec::new(),
            }],
            "creator_save_routes",
        ),
        (
            "missing required route",
            vec![CreatorSaveBatchSelection {
                row_id: valid[0].row_id.clone(),
                confirmed_route_ids: vec![optional_route],
            }],
            "creator_save_routes",
        ),
        (
            "unknown row",
            vec![CreatorSaveBatchSelection {
                row_id: "row-not-in-this-review".into(),
                confirmed_route_ids: valid[0].confirmed_route_ids.clone(),
            }],
            "creator_save_unknown_row",
        ),
    ] {
        let error = store
            .submit_owner_review(
                &available.binding,
                OwnerReviewSelection::CreatorSave {
                    selections: invalid,
                },
            )
            .unwrap_err();
        assert_eq!(
            error,
            ResultStoreError::InvalidInput(expected_reason),
            "{case} must retain its error classification"
        );
        assert_eq!(
            error.to_string(),
            format!("result input rejected ({expected_reason})")
        );
        assert_eq!(
            store.current(&available.binding).unwrap(),
            available,
            "{case} must leave state unchanged"
        );
    }
    let chosen = vec![valid[0].clone(), valid[2].clone()];
    let submitted = store
        .submit_owner_review(
            &available.binding,
            OwnerReviewSelection::CreatorSave {
                selections: chosen.clone(),
            },
        )
        .unwrap();
    let submitted_binding = submitted.binding().clone();
    let action = submitted.into_creator_save().unwrap();
    assert_eq!(action.selections, chosen);
    assert!(
        store
            .submit_owner_review(
                &submitted_binding,
                OwnerReviewSelection::CreatorSave { selections: valid }
            )
            .is_err()
    );
}

#[test]
fn prepare_authority_expires_absolutely_despite_result_reads() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let value = input(&platform, 1, deadline);
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(value).unwrap());
    assert!(
        store
            .current_at(&available.binding, deadline - Duration::from_secs(1))
            .is_ok()
    );
    assert_eq!(
        store.current_at(&available.binding, deadline),
        Err(ResultStoreError::Expired)
    );
}

#[tokio::test]
async fn confirmation_consumes_immutable_prepared_content_without_provider_access() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(crate::LocalPlatform::open(directory.path()).unwrap());
    let value = input(&platform, 2, Instant::now() + Duration::from_secs(60));
    let result = platform.mint_execution_result(value).unwrap();
    let mut available = platform
        .present_result_with_locale(
            &result,
            &origin("origin-one"),
            crate::PresentationLocale::SimplifiedChinese,
        )
        .unwrap();
    // The card's locale stays authoritative even if the application preference changes.
    platform
        .presentation_preferences
        .set_locale(crate::PresentationLocale::English);
    let mut choices = selections(&available);
    choices.truncate(1);
    if let Some(PendingActionView::CreatorSaveBatch { review }) = &mut available.pending_action {
        review.rows[0].preview.notes = Some("Untrusted caller-side replacement".into());
    }
    // No X connection exists in this vault: any confirmation-time provider read fails.
    let terminal = platform
        .confirm_creator_save(available.binding, choices)
        .await
        .unwrap();
    assert!(matches!(terminal.state, ResultState::Resolved(_)));
    let expected = "达人保存完成：已保存 1 个，未选择 1 个。不要重复已完成或结果不确定的保存。";
    assert_eq!(terminal.presentation.title, "达人保存结果");
    assert_eq!(terminal.presentation.summary.as_deref(), Some(expected));
    assert_eq!(terminal.batch.outcomes[0].summary, expected);
    let continuation = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: terminal.binding.clone(),
            expected_continuation_revision: terminal.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    assert_eq!(continuation.request.message, expected);
    let PresentationSection::CreatorSaveRows { rows } = &terminal.presentation.sections[0] else {
        panic!("save rows")
    };
    assert_eq!(rows[0].status, CreatorSaveBatchRowStatus::Saved);
    assert_eq!(rows[1].status, CreatorSaveBatchRowStatus::NotSelected);
    let creator_id = onecrew_creator_runtime::CreatorId::new(rows[0].creator_id.clone().unwrap());
    let creator = platform
        .creators()
        .unwrap()
        .get_creator(&creator_id)
        .unwrap();
    assert_eq!(
        creator.creator.notes.as_deref(),
        Some("Original reviewed notes")
    );
}

#[tokio::test]
async fn caller_cancellation_after_submission_keeps_commit_and_terminal_owned() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(crate::LocalPlatform::open(directory.path()).unwrap());
    let value = input(&platform, 1, Instant::now() + Duration::from_secs(60));
    let result = platform.mint_execution_result(value).unwrap();
    let available = platform
        .present_result(&result, &origin("origin-one"))
        .unwrap();
    let choices = selections(&available);
    // Block the actual SQLite writer so the caller can disconnect after its
    // trusted submission but before the transaction finishes.
    let blocker = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let caller = {
        let platform = platform.clone();
        let binding = available.binding.clone();
        tokio::spawn(async move { platform.confirm_creator_save(binding, choices).await })
    };
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(
                platform
                    .read_current_result(&available.binding)
                    .unwrap()
                    .state,
                ResultState::Submitted { .. }
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    blocker.execute_batch("ROLLBACK").unwrap();
    let terminal = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let current = platform.read_current_result(&available.binding).unwrap();
            if !matches!(current.state, ResultState::Submitted { .. }) {
                break current;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(terminal.state, ResultState::Resolved(_)));
    let PresentationSection::CreatorSaveRows { rows } = &terminal.presentation.sections[0] else {
        panic!("save rows")
    };
    assert_eq!(rows[0].status, CreatorSaveBatchRowStatus::Saved);
}

#[tokio::test]
async fn full_business_batch_mints_reviews_and_selects_every_valid_row() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let value = input(&platform, count, Instant::now() + Duration::from_secs(60));
    let Some(PendingUserAction::CreatorSaveBatch { review, payload }) = &value.pending_action
    else {
        panic!("batch review")
    };
    assert!(review.rows.iter().all(|row| row.preview.routes.len() == 3));
    assert_eq!(review.rows.len(), count);
    // These are the projection the save owner really emitted, not a fixture
    // written beside it: the batch limit is only justified if this path fits.
    let review_bytes = serde_json::to_vec(review).unwrap().len();
    let projection_bytes = serde_json::to_vec(&value.presentation).unwrap().len();
    let owner_payload_bytes = payload.bytes().len();
    let sections = value.presentation.sections.len();
    assert!(
        review_bytes <= MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
        "{review_bytes}"
    );
    assert!(
        projection_bytes <= MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
        "{projection_bytes}"
    );
    assert!(sections <= MAX_CREATOR_SAVE_BATCH_SECTIONS, "{sections}");
    assert!(
        owner_payload_bytes <= MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES,
        "{owner_payload_bytes}"
    );
    eprintln!(
        "{count} materialized runtime rows: review_bytes={review_bytes}, projection_bytes={projection_bytes}, sections={sections}, owner_payload_bytes={owner_payload_bytes}, projection_cap={MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES}, owner_cap={MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES}"
    );
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(value).unwrap());
    let chosen = selections(&available);
    assert_eq!(chosen.len(), count);
    let submitted = store
        .submit_owner_review(
            &available.binding,
            OwnerReviewSelection::CreatorSave { selections: chosen },
        )
        .unwrap();
    assert_eq!(
        submitted.into_creator_save().unwrap().selections.len(),
        count
    );

    // Reject business overflow before any account or provider access.
    let request = crate::PrepareCreatorSaveInput {
        candidates: (0..=count)
            .map(|index| crate::CreatorSaveCandidateInput {
                intent: onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
                target: crate::CreatorSaveTarget::X {
                    user_id: (index + 1).to_string(),
                    handle: format!("person{index}"),
                },
                connection: crate::CreatorSaveConnection::X {
                    account_ref_id: Uuid::from_u128(1),
                },
                provenance: crate::CreatorSaveProvenanceInput::DirectSelection {
                    source_url: format!("https://x.com/person{index}"),
                    description: None,
                },
                notes: None,
                tags: None,
                quality: CreatorQualityInput::default(),
            })
            .collect(),
    };
    let context = crate::CreatorSavePreparationContext {
        origin: origin("origin-one"),
        call_id: "overflow".into(),
        operation_id: "onecrew.creator.candidate.batch.save".into(),
    };
    assert!(matches!(
        platform.prepare_creator_save(request, context).await,
        Err(crate::CreatorSaveError::InvalidBatchSize)
    ));
}

/// The batch limit is derived from arithmetic, so prove the arithmetic: a full
/// batch of rows that each take every bounded field to its legal maximum must
/// still project inside both 4 MiB budgets, not merely an average batch.
#[tokio::test]
async fn worst_case_legal_rows_at_the_business_limit_stay_inside_both_projection_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let routes = onecrew_creator_runtime::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR;
    let operator_ref = AccountRefId::new(Uuid::from_u128(1).to_string());
    platform
        .creators()
        .unwrap()
        .sync_account_ref(
            operator_ref.clone(),
            Platform::X,
            "profile",
            "@testoperator",
        )
        .unwrap();
    // Every bounded projected field at its 8 KiB limit, and every row offering
    // the maximum number of initial contact routes.
    let filler = "x".repeat(8 * 1024);
    let inputs = (0..count)
        .map(|index| {
            let username = format!("person{index}");
            let profile_url = format!("https://x.com/{username}");
            let mut contact_evidence = vec![CreatorSaveContactEvidence::ProfileLink {
                source: SourceKind::ProfileWebsite,
                source_url: Some(profile_url.clone()),
                display_text: "Website".into(),
                target_url: format!("https://creator-{index}.example.com/contact"),
            }];
            for slot in 0..routes {
                contact_evidence.push(CreatorSaveContactEvidence::Observation {
                    channel: ContactChannel::Email,
                    kind: LocatorKind::EmailAddress,
                    value: format!("creator-{index}-{slot}@example.com"),
                    source: SourceKind::ProfileBio,
                    source_url: Some(profile_url.clone()),
                });
            }
            CanonicalCreatorSaveInput {
                profile_observed_at: 1_800_000_000,
                row_id: format!("row-{index}"),
                account: PlatformAccountCandidate {
                    platform: Platform::X,
                    external_id: Some((index + 1).to_string()),
                    target_kind: Some(TargetKind::User),
                    handle: Some(username),
                    display_name: format!("Person {index}"),
                    profile_url: Some(profile_url.clone()),
                },
                profile: CreatorSaveProfile {
                    avatar_url: Some(format!("https://images.example.com/{index}/avatar.jpg")),
                    description: Some(filler.clone()),
                    location: None,
                    website_urls: vec![format!("https://creator-{index}.example.com/contact")],
                    follower_count: Some(10_000 + index as u64),
                },
                provenance: CreatorSaveProvenance::DirectSelection {
                    source_ref: profile_url.clone(),
                    source_url: Some(profile_url),
                    description: Some(filler.clone()),
                    operator_account_ref_id: Some(operator_ref.clone()),
                },
                contact_evidence,
                notes: Some(filler.clone()),
                tags: None,
                quality: CreatorQualityInput::default(),
            }
        })
        .collect();
    let value = input_from_entries(&platform, inputs, Instant::now() + Duration::from_secs(60));
    let Some(PendingUserAction::CreatorSaveBatch { review, payload }) = &value.pending_action
    else {
        panic!("batch review")
    };
    let review_bytes = serde_json::to_vec(review).unwrap().len();
    let projection_bytes = serde_json::to_vec(&value.presentation).unwrap().len();
    let owner_payload_bytes = payload.bytes().len();
    let sections = value.presentation.sections.len();
    eprintln!(
        "worst-case {count} rows: review_bytes={review_bytes}, projection_bytes={projection_bytes}, sections={sections}, owner_payload_bytes={owner_payload_bytes}, projection_cap={MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES}, owner_cap={MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES}"
    );
    assert_eq!(review.rows.len(), count);
    assert!(
        review_bytes <= MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
        "{review_bytes}"
    );
    assert!(
        projection_bytes <= MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES,
        "{projection_bytes}"
    );
    assert!(sections <= MAX_CREATOR_SAVE_BATCH_SECTIONS, "{sections}");
    assert!(
        owner_payload_bytes <= MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES,
        "{owner_payload_bytes}"
    );
    // The store must accept exactly what the owner emitted for this batch.
    let store = ResultMemoryStore::new();
    let available = present(&store, store.mint_from_execute(value).unwrap());
    assert_eq!(selections(&available).len(), count);
}

#[tokio::test]
async fn saved_resources_are_deduplicated_by_creator_id_and_match_rows_and_continuation() {
    use std::collections::BTreeSet;
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(crate::LocalPlatform::open(directory.path()).unwrap());
    let operator = AccountRefId::new(Uuid::from_u128(1).to_string());
    platform
        .creators()
        .unwrap()
        .sync_account_ref(operator.clone(), Platform::X, "profile", "@operator")
        .unwrap();
    let inputs: Vec<_> = [100, 200, 100, 300]
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let url = format!("https://x.com/person{id}");
            CanonicalCreatorSaveInput {
                row_id: format!("row-{index}"),
                account: PlatformAccountCandidate {
                    platform: Platform::X,
                    external_id: Some(id.to_string()),
                    target_kind: Some(TargetKind::User),
                    handle: Some(format!("person{id}")),
                    display_name: "Shared name".into(),
                    profile_url: Some(url.clone()),
                },
                profile: CreatorSaveProfile::default(),
                profile_observed_at: 1_800_000_000,
                provenance: CreatorSaveProvenance::DirectSelection {
                    source_ref: url.clone(),
                    source_url: Some(url),
                    description: None,
                    operator_account_ref_id: Some(operator.clone()),
                },
                contact_evidence: Vec::new(),
                notes: None,
                tags: None,
                quality: CreatorQualityInput::default(),
            }
        })
        .collect();
    let seed = input_from_entries(
        &platform,
        vec![inputs[0].clone()],
        Instant::now() + Duration::from_secs(60),
    );
    let seed_ref = platform.mint_execution_result(seed).unwrap();
    let seed_review = platform
        .present_result(&seed_ref, &origin("origin-one"))
        .unwrap();
    let seed_selection = selections(&seed_review);
    platform
        .confirm_creator_save(seed_review.binding, seed_selection)
        .await
        .unwrap();
    let value = input_from_entries(&platform, inputs, Instant::now() + Duration::from_secs(60));
    let reference = platform.mint_execution_result(value).unwrap();
    let available = platform
        .present_result(&reference, &origin("origin-one"))
        .unwrap();
    let mut chosen = selections(&available);
    chosen.truncate(3);
    let terminal = platform
        .confirm_creator_save(available.binding, chosen)
        .await
        .unwrap();
    let PresentationSection::CreatorSaveRows { rows } = &terminal.presentation.sections[0] else {
        panic!("save rows")
    };
    assert!(
        rows[..3]
            .iter()
            .all(|row| row.status == CreatorSaveBatchRowStatus::Saved)
    );
    assert_eq!(rows[3].status, CreatorSaveBatchRowStatus::NotSelected);
    assert_eq!(rows[0].creator_id, rows[2].creator_id);
    assert_ne!(rows[0].creator_id, rows[1].creator_id);
    let refs = &terminal.batch.outcomes[0].resource_refs;
    assert_eq!(refs.len(), 2);
    assert!(
        refs.iter()
            .all(|resource| resource.kind == ResultResourceKind::Creator
                && resource.label.as_deref() == Some("Shared name"))
    );
    let saved_ids: BTreeSet<_> = rows
        .iter()
        .filter_map(|row| row.creator_id.clone())
        .collect();
    assert_eq!(
        refs.iter()
            .map(|resource| resource.id.clone())
            .collect::<BTreeSet<_>>(),
        saved_ids
    );
    let continuation = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: terminal.binding.clone(),
            expected_continuation_revision: terminal.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::CreatorSaveBatch { outcomes, .. } = continuation.request.context
    else {
        panic!("creator continuation")
    };
    assert_eq!(
        outcomes,
        rows.iter()
            .map(ResultContinuationCreatorSaveOutcome::from)
            .collect::<Vec<_>>()
    );
}
