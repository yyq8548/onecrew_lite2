use std::{
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

use super::super::{
    CallDisposition, CreatorSaveBatchSelection, ExecutedCallOutcome, PendingActionView,
    ResultResolution, ResultState, ResultTerminalUpdate, TerminalResult,
    memory::{ResultMemoryStore, StoreLimits},
};
use super::*;

const SAVE_OPERATION: &str = "onecrew.creator.candidate.batch.save";

fn setup() -> (tempfile::TempDir, crate::LocalPlatform) {
    let directory = tempfile::tempdir().unwrap();
    let platform = crate::LocalPlatform::open(directory.path()).unwrap();
    (directory, platform)
}

fn prepared(
    platform: &crate::LocalPlatform,
    call_id: &str,
    deadline: Instant,
    peer_call: Option<&[u8]>,
) -> PreparedOwnerReview {
    prepared_count(platform, call_id, deadline, peer_call, 1)
}

fn prepared_count(
    platform: &crate::LocalPlatform,
    call_id: &str,
    deadline: Instant,
    peer_call: Option<&[u8]>,
    count: usize,
) -> PreparedOwnerReview {
    // Reuse the real canonical Creator preparation, including observed routes,
    // without any provider session or synthetic authority implementation.
    let input = super::super::tests::creator_save_batch::input(platform, count, deadline);
    let context = OwnerReviewContext::new(
        input.origin,
        call_id,
        SAVE_OPERATION,
        peer_call.map(OwnerCallDigest::from_bytes),
    )
    .unwrap();
    let PendingUserAction::CreatorSaveBatch { payload, .. } = input.pending_action.unwrap() else {
        unreachable!()
    };
    let mut action = payload.into_prepared_save_for_test();
    // Change the identity before calling the real sealed review constructor,
    // which re-encodes the authority including that exact call identity.
    action.context.call_id = call_id.into();
    PreparedOwnerReview::creator_save(action, input.presentation, context).unwrap()
}

fn outcome(call_id: &str, operation_id: &str, disposition: CallDisposition) -> ExecutedCallOutcome {
    ExecutedCallOutcome {
        call_id: call_id.into(),
        operation_id: operation_id.into(),
        disposition,
        summary: "Observed call status".into(),
        resource_refs: vec![],
    }
}

fn pending(call_id: &str) -> ExecutedCallOutcome {
    outcome(
        call_id,
        SAVE_OPERATION,
        CallDisposition::NotRun {
            reason: "confirmation_required".into(),
        },
    )
}

fn frame() -> ExecutedBatch {
    ExecutedBatch {
        outcomes: vec![
            outcome("read-1", "x.user.get", CallDisposition::Succeeded),
            pending("save-1"),
            outcome(
                "read-2",
                "x.user.get",
                CallDisposition::NotRun {
                    reason: "earlier_call_requires_attention".into(),
                },
            ),
            pending("save-2"),
        ],
    }
}

fn singleton() -> ExecutedBatch {
    ExecutedBatch {
        outcomes: vec![pending("save-1")],
    }
}

fn completed_prefix() -> ExecutedBatch {
    let mut frame = frame();
    frame.outcomes[1].disposition = CallDisposition::Succeeded;
    frame.outcomes[2].disposition = CallDisposition::Succeeded;
    frame
}

fn peer(bytes: &[u8]) -> Option<PeerResumePayload> {
    Some(PeerResumePayload::new(bytes.to_vec()).unwrap())
}

fn selection(snapshot: &super::super::ResultSnapshot) -> OwnerReviewSelection {
    let Some(PendingActionView::CreatorSaveBatch { review }) = &snapshot.pending_action else {
        panic!("save review")
    };
    OwnerReviewSelection::CreatorSave {
        selections: review
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
            .collect(),
    }
}

fn mint_present(
    platform: &crate::LocalPlatform,
    store: &ResultMemoryStore,
) -> super::super::ResultSnapshot {
    let review = prepared(
        platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        Some(b"exact first call"),
    );
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, frame(), peer(b"opaque complete batch"), Vec::new())
        .unwrap();
    store.present(&reference, &origin).unwrap()
}

#[test]
fn peer_payload_is_bounded_and_debug_hides_body() {
    assert!(PeerResumePayload::new(Vec::new()).is_err());
    assert!(PeerResumePayload::new(vec![1; super::super::MAX_PEER_RESUME_BYTES + 1]).is_err());
    let peer = PeerResumePayload::new(b"exact private message body".to_vec()).unwrap();
    assert!(!format!("{peer:?}").contains("private message"));
    assert!(PeerResumePayload::new(vec![1; super::super::MAX_PEER_RESUME_BYTES]).is_ok());
}

#[test]
fn digest_binds_typed_intent_context_and_every_peer_byte() {
    let (_directory, platform) = setup();
    let deadline = Instant::now() + Duration::from_secs(900);
    let first = prepared(&platform, "save-1", deadline, Some(b"call one"))
        .into_input(frame(), peer(b"remaining one"))
        .unwrap();
    let changed_call = prepared(&platform, "save-1", deadline, Some(b"call two"))
        .into_input(frame(), peer(b"remaining one"))
        .unwrap();
    let changed_remaining = prepared(&platform, "save-1", deadline, Some(b"call one"))
        .into_input(frame(), peer(b"remaining two"))
        .unwrap();
    let digest = first.pending_action.unwrap().payload().digest();
    assert_ne!(
        digest,
        changed_call.pending_action.unwrap().payload().digest()
    );
    assert_ne!(
        digest,
        changed_remaining.pending_action.unwrap().payload().digest()
    );
}

#[test]
fn mint_rejects_wrong_call_and_missing_peer_without_allocating() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let deadline = Instant::now() + Duration::from_secs(900);
    let mut changed = frame();
    changed.outcomes[1].operation_id = "another.operation".into();
    assert!(
        store
            .mint_owner_review(
                prepared(&platform, "save-1", deadline, Some(b"call")),
                changed,
                peer(b"resume"),
                Vec::new()
            )
            .is_err()
    );
    assert!(
        store
            .mint_owner_review(
                prepared(&platform, "save-1", deadline, Some(b"call")),
                frame(),
                None,
                Vec::new()
            )
            .is_err()
    );
    assert!(
        store
            .mint_owner_review(
                prepared(&platform, "save-1", deadline, None),
                frame(),
                peer(b"resume"),
                Vec::new()
            )
            .is_err()
    );
    assert_eq!(store.cleanup().unwrap().active, 0);
}

#[test]
fn generic_confirmation_cannot_consume_typed_review() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    assert_eq!(
        store.submit(&current.binding).unwrap_err(),
        ResultStoreError::InvalidState("typed_owner_review_required")
    );
    assert!(matches!(
        store.read(&current.binding).unwrap().state,
        ResultState::Available
    ));
    assert!(
        store
            .submit_owner_review(&current.binding, OwnerReviewSelection::Confirm)
            .is_err()
    );
    assert!(
        store
            .submit_owner_review(
                &current.binding,
                OwnerReviewSelection::CreatorSave { selections: vec![] }
            )
            .is_err()
    );
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    assert_eq!(grant.kind(), OwnerReviewKind::CreatorSave);
    assert_eq!(
        grant.context().call_digest().unwrap().as_str(),
        OwnerCallDigest::from_bytes(b"exact first call").as_str()
    );
    assert_eq!(
        grant.peer_resume().unwrap().as_bytes(),
        b"opaque complete batch"
    );
    assert!(grant.into_creator_save().is_ok());
}

#[test]
fn every_binding_component_and_presentation_gate_are_checked_before_consumption() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let review = prepared(
        &platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        None,
    );
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, singleton(), None, Vec::new())
        .unwrap();
    let unpresented = store.test_binding(&reference);
    assert_eq!(
        store
            .submit_owner_review(&unpresented, OwnerReviewSelection::Confirm)
            .unwrap_err(),
        ResultStoreError::InvalidState("result_not_presented")
    );
    let current = store.present(&reference, &origin).unwrap();
    let mut bad = current.binding.clone();
    bad.origin = ResultOrigin::new("other-origin").unwrap();
    assert_eq!(
        store
            .submit_owner_review(&bad, selection(&current))
            .unwrap_err(),
        ResultStoreError::OriginMismatch
    );
    bad = current.binding.clone();
    bad.view.kind = ResultViewKind::Choice;
    assert_eq!(
        store
            .submit_owner_review(&bad, selection(&current))
            .unwrap_err(),
        ResultStoreError::ViewMismatch
    );
    bad = current.binding.clone();
    bad.fingerprint = super::super::ResultFingerprint::digest(b"forged");
    assert_eq!(
        store
            .submit_owner_review(&bad, selection(&current))
            .unwrap_err(),
        ResultStoreError::FingerprintMismatch
    );
    bad = current.binding.clone();
    bad.revision += 1;
    assert!(matches!(
        store.submit_owner_review(&bad, selection(&current)),
        Err(ResultStoreError::RevisionConflict { .. })
    ));
    assert!(
        store
            .submit_owner_review(&current.binding, selection(&current))
            .is_ok()
    );
}

#[test]
fn concurrent_submissions_mint_exactly_one_grant() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let barrier = Arc::new(Barrier::new(3));
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let store = store.clone();
            let current = current.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.submit_owner_review(&current.binding, selection(&current))
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ResultStoreError::RevisionConflict { .. })))
            .count(),
        1
    );
}

#[test]
fn second_review_keeps_identity_locale_and_requires_a_new_submission() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let submitted = grant.binding().clone();
    let next = store
        .advance_owner_review(
            &submitted,
            prepared(
                &platform,
                "save-2",
                Instant::now() + Duration::from_secs(900),
                Some(b"exact second call"),
            ),
            completed_prefix(),
            peer(b"second call remainder"),
            Vec::new(),
        )
        .unwrap();
    assert_eq!(next.binding.result_ref, current.binding.result_ref);
    assert_eq!(next.binding.view, current.binding.view);
    assert_eq!(next.binding.fingerprint, current.binding.fingerprint);
    assert_eq!(next.binding.origin, current.binding.origin);
    assert_eq!(next.locale, current.locale);
    assert!(next.presented);
    assert_eq!(next.binding.revision, submitted.revision + 1);
    assert_eq!(next.batch, completed_prefix());
    assert_eq!(store.current(&current.binding).unwrap(), next);
    assert!(matches!(
        store.advance_owner_review(
            &submitted,
            prepared(
                &platform,
                "save-2",
                Instant::now() + Duration::from_secs(900),
                Some(b"second")
            ),
            completed_prefix(),
            peer(b"second"),
            Vec::new()
        ),
        Err(ResultStoreError::RevisionConflict { .. })
    ));
    assert!(
        store
            .submit_owner_review(&current.binding, selection(&current))
            .is_err()
    );
    let second = store
        .submit_owner_review(&next.binding, selection(&next))
        .unwrap();
    assert_eq!(second.context().call_id(), "save-2");
    assert_eq!(
        second.peer_resume().unwrap().as_bytes(),
        b"second call remainder"
    );
    assert!(
        store
            .submit_owner_review(&next.binding, selection(&next))
            .is_err()
    );
}

#[test]
fn full_business_batch_review_retains_its_materialized_budget_with_peer_resume() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let review = prepared_count(
        &platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        Some(b"batch call"),
        onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS,
    );
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, frame(), peer(&vec![b'x'; 1024 * 1024]), Vec::new())
        .unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let selected = grant.into_creator_save().unwrap();
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    assert_eq!(selected.rows.len(), count);
    assert_eq!(selected.selections.len(), count);
}

#[test]
fn advance_cannot_erase_completed_prefix_or_continue_after_unknown() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    for corrupt_prefix in [false, true] {
        let mut next = completed_prefix();
        if corrupt_prefix {
            next.outcomes[0].summary = "changed old evidence".into();
        } else {
            next.outcomes[1].disposition = CallDisposition::Unknown {
                code: "remote_unknown".into(),
            };
        }
        assert!(
            store
                .advance_owner_review(
                    grant.binding(),
                    prepared(
                        &platform,
                        "save-2",
                        Instant::now() + Duration::from_secs(900),
                        Some(b"next")
                    ),
                    next,
                    peer(b"next"),
                    Vec::new()
                )
                .is_err()
        );
        assert!(matches!(
            store.current(&current.binding).unwrap().state,
            ResultState::Submitted { .. }
        ));
    }
}

#[test]
fn advance_requires_submission_and_a_later_pending_call() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let deadline = Instant::now() + Duration::from_secs(900);
    assert_eq!(
        store
            .advance_owner_review(
                &current.binding,
                prepared(&platform, "save-2", deadline, Some(b"second")),
                completed_prefix(),
                peer(b"second"),
                Vec::new()
            )
            .unwrap_err(),
        ResultStoreError::InvalidState("advance_requires_submitted")
    );
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    assert_eq!(
        store
            .advance_owner_review(
                grant.binding(),
                prepared(&platform, "save-1", deadline, Some(b"first again")),
                frame(),
                peer(b"first again"),
                Vec::new()
            )
            .unwrap_err(),
        ResultStoreError::InvalidInput("owner_review_did_not_advance")
    );
    assert!(matches!(
        store.current(&current.binding).unwrap().state,
        ResultState::Submitted { .. }
    ));
}

#[test]
fn capacity_failure_is_atomic_and_terminal_recovery_keeps_real_progress() {
    let (_directory, platform) = setup();
    let deadline = Instant::now() + Duration::from_secs(900);
    let initial = prepared(&platform, "save-1", deadline, Some(b"call"))
        .into_input(frame(), peer(b"resume"))
        .unwrap();
    let bytes = super::super::validation::result_payload_bytes(&initial).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_total_payload_bytes: bytes + 512,
        ..StoreLimits::default()
    });
    let origin = initial.origin.clone();
    let reference = store.mint_from_execute(initial).unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let metrics = store.cleanup().unwrap();
    assert_eq!(
        store
            .advance_owner_review(
                grant.binding(),
                prepared(&platform, "save-2", deadline, Some(b"next")),
                completed_prefix(),
                peer(&vec![b'x'; 4096]),
                Vec::new()
            )
            .unwrap_err(),
        ResultStoreError::Capacity("result_payload_limit")
    );
    let after = store.current(&current.binding).unwrap();
    assert_eq!(after.binding, *grant.binding());
    assert_eq!(after.batch, current.batch);
    assert_eq!(
        store.cleanup().unwrap().payload_bytes,
        metrics.payload_bytes
    );
    let terminal = store
        .terminal(
            grant.binding(),
            ResultTerminalUpdate {
                terminal: TerminalResult::Resolved(ResultResolution {
                    code: "presentation_unavailable".into(),
                    summary: "Earlier calls completed; next call was not run.".into(),
                }),
                batch: completed_prefix(),
                presentation: current.presentation,
                continuation: None,
            },
        )
        .unwrap();
    assert_eq!(
        terminal.batch.outcomes[1].disposition,
        CallDisposition::Succeeded
    );
    assert!(matches!(
        terminal.batch.outcomes[3].disposition,
        CallDisposition::NotRun { .. }
    ));
}

#[test]
fn absolute_deadline_is_not_extended_by_reading_or_advancing() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let now = Instant::now();
    let review = prepared(&platform, "save-1", now + Duration::from_secs(900), None);
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, singleton(), None, Vec::new())
        .unwrap();
    let snapshot = store.present(&reference, &origin).unwrap();
    store
        .current_at(&snapshot.binding, now + Duration::from_secs(899))
        .unwrap();
    assert_eq!(
        store
            .current_at(&snapshot.binding, now + Duration::from_secs(901))
            .unwrap_err(),
        ResultStoreError::Expired
    );
}

fn generic_input(frame: ExecutedBatch) -> ExecutedResultInput {
    ExecutedResultInput {
        origin: ResultOrigin::new("origin-one").unwrap(),
        view_kind: ResultViewKind::Confirmation,
        batch: frame,
        presentation: PresentationProjection {
            kind: super::super::ResultPresentationKind::new("generic_confirmation").unwrap(),
            title: "Review the next action".into(),
            summary: None,
            sections: vec![],
        },
        pending_action: Some(PendingUserAction::Confirm {
            prompt: "Confirm the exact action?".into(),
            label: "Confirm".into(),
            payload: OwnerActionPayload::new(b"opaque generic continuation").unwrap(),
        }),
        initial_state: InitialResultState::Available,
    }
}

#[test]
fn unified_submit_and_advance_support_all_owner_and_generic_pairs() {
    let (_directory, platform) = setup();
    for first_is_owner in [false, true] {
        for next_is_owner in [false, true] {
            let store = ResultMemoryStore::new();
            let current = if first_is_owner {
                mint_present(&platform, &store)
            } else {
                let input = generic_input(frame());
                let origin = input.origin.clone();
                let reference = store.mint_from_execute(input).unwrap();
                store.present(&reference, &origin).unwrap()
            };
            let choice = if first_is_owner {
                selection(&current)
            } else {
                OwnerReviewSelection::Confirm
            };
            let submitted = store
                .submit_result_review(&current.binding, choice)
                .unwrap();
            let binding = match submitted {
                ResultConfirmationSubmission::Owner(grant) => {
                    assert!(first_is_owner);
                    grant.binding().clone()
                }
                ResultConfirmationSubmission::Generic(submitted) => {
                    assert!(!first_is_owner);
                    let (binding, _, bytes) = submitted.into_generic_parts().unwrap();
                    assert_eq!(bytes, b"opaque generic continuation");
                    binding
                }
            };
            let next = if next_is_owner {
                store.advance_owner_review(
                    &binding,
                    prepared(
                        &platform,
                        "save-2",
                        Instant::now() + Duration::from_secs(900),
                        Some(b"second call"),
                    ),
                    completed_prefix(),
                    peer(b"second call"),
                    Vec::new(),
                )
            } else {
                store.advance_result_confirmation(
                    &binding,
                    generic_input(completed_prefix()),
                    Vec::new(),
                )
            }
            .unwrap();
            assert_eq!(next.binding.fingerprint, current.binding.fingerprint);
            assert_eq!(next.binding.result_ref, current.binding.result_ref);
            assert_eq!(next.binding.revision, binding.revision + 1);
            assert_eq!(next.batch, completed_prefix());
            let choice = if next_is_owner {
                selection(&next)
            } else {
                OwnerReviewSelection::Confirm
            };
            let submitted = store.submit_result_review(&next.binding, choice).unwrap();
            assert_eq!(
                matches!(submitted, ResultConfirmationSubmission::Owner(_)),
                next_is_owner
            );
        }
    }
}

#[test]
fn generic_advance_cannot_wrap_typed_business_authority() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let typed = prepared(
        &platform,
        "save-2",
        Instant::now() + Duration::from_secs(900),
        Some(b"next"),
    )
    .into_input(completed_prefix(), peer(b"next"))
    .unwrap();
    assert_eq!(
        store
            .advance_result_confirmation(grant.binding(), typed, Vec::new())
            .unwrap_err(),
        ResultStoreError::InvalidInput("generic_confirmation_required")
    );
    assert!(matches!(
        store.current(&current.binding).unwrap().state,
        ResultState::Submitted { .. }
    ));
}

#[test]
fn retained_creator_refs_survive_a_later_generic_confirmation_and_terminal() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let current = mint_present(&platform, &store);
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let refs = |count: usize| {
        (0..count)
            .map(|index| super::super::ResultResourceRef {
                kind: super::super::ResultResourceKind::Creator,
                id: format!("creator-{index}"),
                label: None,
            })
            .collect::<Vec<_>>()
    };
    let retained = super::super::MAX_OWNER_FRAME_RESOURCE_REFS;
    let mut progress = completed_prefix();
    // An ordinary initial generic result still has its existing 64-ref budget.
    progress.outcomes[1].resource_refs = refs(retained + 1);
    assert_eq!(
        store
            .mint_from_execute(generic_input(progress.clone()))
            .unwrap_err(),
        ResultStoreError::InvalidInput("resource_refs")
    );
    progress.outcomes[1].resource_refs = refs(retained);
    let next = store
        .advance_result_confirmation(grant.binding(), generic_input(progress.clone()), Vec::new())
        .unwrap();
    let ResultConfirmationSubmission::Generic(submitted) = store
        .submit_result_review(&next.binding, OwnerReviewSelection::Confirm)
        .unwrap()
    else {
        panic!("generic continuation remains generic");
    };
    let (binding, _, _) = submitted.into_generic_parts().unwrap();
    progress.outcomes[3].disposition = CallDisposition::Succeeded;
    let terminal = store
        .terminal(
            &binding,
            ResultTerminalUpdate {
                terminal: TerminalResult::Resolved(ResultResolution {
                    code: "completed".into(),
                    summary: "All calls completed.".into(),
                }),
                batch: progress,
                presentation: next.presentation,
                continuation: None,
            },
        )
        .unwrap();
    assert_eq!(terminal.batch.outcomes[1].resource_refs.len(), retained);
}

#[tokio::test]
async fn two_real_saves_in_one_result_use_distinct_stable_call_keys() {
    let (_directory, platform) = setup();
    let store = platform.result_store.clone();
    let review = prepared_count(
        &platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        Some(b"first exact call"),
        2,
    );
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, frame(), peer(b"complete remainder"), Vec::new())
        .unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let OwnerReviewSelection::CreatorSave { mut selections } = selection(&current) else {
        unreachable!()
    };
    selections.truncate(1);
    let grant = store
        .submit_owner_review(
            &current.binding,
            OwnerReviewSelection::CreatorSave { selections },
        )
        .unwrap();
    let binding = grant.binding().clone();
    let first = platform
        .execute_creator_save_submitted(grant)
        .await
        .unwrap();
    assert!(first.continue_batch);
    let OwnerCallOutput::CreatorSave(first_result) = &first.output else {
        unreachable!()
    };
    assert_eq!(
        first_result.rows[1].status,
        super::super::CreatorSaveBatchRowStatus::NotSelected
    );
    assert!(!first_result.rows[0].confirmed_route_ids.is_empty());
    let completed_sections = first.presentation.sections.clone();
    let mut progress = completed_prefix();
    progress.outcomes[1] = first.outcome;
    let next = store
        .advance_owner_review(
            &binding,
            prepared_count(
                &platform,
                "save-2",
                Instant::now() + Duration::from_secs(900),
                Some(b"second exact call"),
                2,
            ),
            progress.clone(),
            peer(b"second remaining call"),
            completed_sections.clone(),
        )
        .unwrap();
    assert_eq!(next.completed_sections_count, completed_sections.len());
    assert_eq!(
        store.completed_result_sections(&next.binding).unwrap(),
        completed_sections
    );
    assert_eq!(
        next.presentation.sections[..next.completed_sections_count],
        completed_sections
    );
    let grant = store
        .submit_owner_review(&next.binding, selection(&next))
        .unwrap();
    let binding = grant.binding().clone();
    let second = platform
        .execute_creator_save_submitted(grant)
        .await
        .unwrap();
    assert!(
        second.continue_batch,
        "same result must not collide with the first save's idempotency key"
    );
    let OwnerCallOutput::CreatorSave(result) = &second.output else {
        panic!("typed save output")
    };
    assert_eq!(result.rows.len(), 2);
    assert!(
        result
            .rows
            .iter()
            .all(|row| row.status == super::super::CreatorSaveBatchRowStatus::Saved)
    );
    assert!(result.rows.iter().all(|row| row.creator_id.is_some()));
    progress.outcomes[3] = second.outcome;
    assert_eq!(
        store
            .terminal(
                &binding,
                ResultTerminalUpdate {
                    batch: progress.clone(),
                    presentation: second.presentation.clone(),
                    terminal: second.terminal.clone(),
                    continuation: None,
                }
            )
            .unwrap_err(),
        ResultStoreError::InvalidInput("completed_sections_mismatch")
    );
    let mut presentation = second.presentation;
    let mut all_completed = completed_sections.clone();
    all_completed.append(&mut presentation.sections);
    presentation.sections = all_completed;
    let terminal = store
        .terminal(
            &binding,
            ResultTerminalUpdate {
                batch: progress,
                presentation,
                terminal: second.terminal,
                continuation: None,
            },
        )
        .unwrap();
    assert_eq!(
        terminal.presentation.sections[..completed_sections.len()],
        completed_sections
    );
    assert_eq!(
        terminal.completed_sections_count,
        terminal.presentation.sections.len()
    );
    assert!(
        terminal
            .batch
            .outcomes
            .iter()
            .all(|call| matches!(call.disposition, CallDisposition::Succeeded))
    );
}

fn large_completed_section() -> super::super::PresentationSection {
    super::super::PresentationSection::Fields {
        heading: None,
        fields: (0..100)
            .map(|_| super::super::PresentationField {
                label: "Details".into(),
                value: "x".repeat(8000),
            })
            .collect(),
    }
}

#[test]
fn accumulated_sections_have_eight_owner_budgets_and_exceed_the_old_section_limit() {
    let section = large_completed_section();
    // Each owner can produce five such sections within its 4 MiB limit.
    // Eight complete calls require forty sections and about 32 MB of JSON.
    let mut projection = generic_input(frame()).presentation;
    projection.sections = vec![section.clone(); 8 * 5];
    assert!(
        serde_json::to_vec(&projection).unwrap().len()
            > super::super::MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES
    );
    super::super::validation::validate_batch_projection(&projection).unwrap();
    projection.sections.extend([section.clone(), section]);
    assert_eq!(
        super::super::validation::validate_batch_projection(&projection).unwrap_err(),
        ResultStoreError::InvalidInput("projection_bytes")
    );
}

#[test]
fn only_submitted_progress_can_admit_a_large_completed_prefix() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let deadline = Instant::now() + Duration::from_secs(900);
    let sections = vec![large_completed_section()];
    assert_eq!(
        store
            .mint_owner_review(
                prepared(&platform, "save-1", deadline, Some(b"initial call")),
                frame(),
                peer(b"resume"),
                sections.clone(),
            )
            .unwrap_err(),
        ResultStoreError::InvalidInput("projection_bytes")
    );
    assert_eq!(store.cleanup().unwrap().active, 0);
    let current = mint_present(&platform, &store);
    let grant = store
        .submit_owner_review(&current.binding, selection(&current))
        .unwrap();
    let next = store
        .advance_owner_review(
            grant.binding(),
            prepared(&platform, "save-2", deadline, Some(b"next call")),
            completed_prefix(),
            peer(b"next"),
            sections.clone(),
        )
        .unwrap();
    assert_eq!(next.completed_sections_count, 1);
    assert_eq!(
        store.completed_result_sections(&next.binding).unwrap(),
        sections
    );
    let grant = store
        .submit_owner_review(&next.binding, selection(&next))
        .unwrap();
    let mut without_prefix = next.presentation;
    without_prefix
        .sections
        .drain(..next.completed_sections_count);
    assert_eq!(
        store
            .terminal(
                grant.binding(),
                ResultTerminalUpdate {
                    batch: next.batch,
                    presentation: without_prefix,
                    terminal: TerminalResult::Resolved(ResultResolution {
                        code: "completed".into(),
                        summary: "Completed".into()
                    }),
                    continuation: None,
                }
            )
            .unwrap_err(),
        ResultStoreError::InvalidInput("completed_sections_mismatch")
    );
}

#[test]
fn generic_attended_peer_keeps_the_full_large_batch_without_creating_owner_authority() {
    let store = ResultMemoryStore::new();
    let bytes = vec![b'x'; 1024 * 1024];
    assert!(OwnerActionPayload::new(bytes.clone()).is_err());
    let mut input = generic_input(frame());
    let PendingUserAction::Confirm { payload, .. } = input.pending_action.as_mut().unwrap() else {
        unreachable!()
    };
    *payload = OwnerActionPayload::from_peer_resume(PeerResumePayload::new(bytes.clone()).unwrap());
    assert_eq!(
        payload.digest(),
        OwnerCallDigest::from_bytes(&bytes).as_str()
    );
    let origin = input.origin.clone();
    let reference = store.mint_from_execute(input).unwrap();
    let current = store.present(&reference, &origin).unwrap();
    assert!(store.cleanup().unwrap().payload_bytes > bytes.len());
    assert_eq!(
        store
            .submit_owner_review(&current.binding, OwnerReviewSelection::Confirm)
            .unwrap_err(),
        ResultStoreError::InvalidState("owner_review_required")
    );
    let ResultConfirmationSubmission::Generic(submitted) = store
        .submit_result_review(&current.binding, OwnerReviewSelection::Confirm)
        .unwrap()
    else {
        panic!("generic peer creates no business grant")
    };
    assert_eq!(submitted.into_generic_parts().unwrap().2, bytes);
}

#[test]
fn peer_resume_constructor_does_not_expand_choice_payload_limits() {
    let mut input = generic_input(frame());
    input.view_kind = ResultViewKind::Choice;
    input.pending_action = Some(PendingUserAction::Choice {
        questions: vec![super::super::ResultChoiceQuestion {
            selection_mode: Default::default(),
            id: "choice".into(),
            header: "Choice".into(),
            prompt: "Choose one".into(),
            options: ["one", "two"]
                .map(|id| super::super::ResultChoiceOption {
                    id: id.into(),
                    label: id.into(),
                    description: None,
                })
                .into(),
        }],
        payload: OwnerActionPayload::from_peer_resume(
            PeerResumePayload::new(vec![b'x'; super::super::MAX_OWNER_ACTION_BYTES + 1]).unwrap(),
        ),
    });
    assert_eq!(
        ResultMemoryStore::new()
            .mint_from_execute(input)
            .unwrap_err(),
        ResultStoreError::InvalidInput("owner_action_payload")
    );
}

#[tokio::test]
async fn next_review_expiry_retains_real_saved_rows_and_emits_one_localized_continuation() {
    let (_directory, platform) = setup();
    let store = platform.result_store.clone();
    let first_review = prepared_count(
        &platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        Some(b"first"),
        2,
    );
    let origin = first_review.context().origin().clone();
    let reference = store
        .mint_owner_review(first_review, frame(), peer(b"full batch"), Vec::new())
        .unwrap();
    let current = store
        .present_with_locale(
            &reference,
            &origin,
            crate::PresentationLocale::SimplifiedChinese,
        )
        .unwrap();
    let OwnerReviewSelection::CreatorSave { mut selections } = selection(&current) else {
        unreachable!()
    };
    selections.truncate(1);
    let grant = store
        .submit_owner_review(
            &current.binding,
            OwnerReviewSelection::CreatorSave { selections },
        )
        .unwrap();
    assert_eq!(grant.locale(), crate::PresentationLocale::SimplifiedChinese);
    let binding = grant.binding().clone();
    let first = platform
        .execute_creator_save_submitted(grant)
        .await
        .unwrap();
    let first_outcome = first.outcome.clone();
    let mut progress = completed_prefix();
    progress.outcomes[1] = first.outcome;
    let prefix = first.presentation.sections;
    let deadline = Instant::now() + Duration::from_secs(900);
    let next = store
        .advance_owner_review(
            &binding,
            prepared_count(&platform, "save-2", deadline, Some(b"second"), 2),
            progress,
            peer(b"second"),
            prefix.clone(),
        )
        .unwrap();
    let expired = store.current_at(&next.binding, deadline).unwrap();
    let ResultState::Failed(failure) = &expired.state else {
        panic!("expired current review is terminal")
    };
    assert_eq!(failure.code, "review_expired");
    assert!(failure.summary.contains("已保留"));
    assert_eq!(expired.presentation, next.presentation);
    assert_eq!(expired.completed_sections_count, prefix.len());
    assert_eq!(expired.batch.outcomes[1], first_outcome);
    assert!(
        matches!(&expired.batch.outcomes[3].disposition, CallDisposition::NotRun { reason } if reason == "review_expired")
    );
    assert!(expired.pending_action.is_none());
    assert_eq!(expired.binding.revision, next.binding.revision + 1);
    assert!(
        store
            .submit_owner_review(&next.binding, selection(&next))
            .is_err()
    );
    let repeated = store
        .current_at(&expired.binding, deadline + Duration::from_secs(1))
        .unwrap();
    assert_eq!(repeated.binding, expired.binding);
    assert_eq!(repeated.continuation, expired.continuation);
    let claim_input = super::super::ClaimResultContinuationInput {
        binding: expired.binding.clone(),
        expected_continuation_revision: expired.continuation.continuation_revision,
        mode: super::super::ResultContinuationClaimMode::Pending,
    };
    let claimed = store.claim_continuation(claim_input.clone()).unwrap();
    assert!(claimed.request.message.contains("完整结果"));
    let super::super::ResultContinuationContext::Execution { outcomes } = claimed.request.context
    else {
        panic!("bounded execution context")
    };
    assert_eq!(outcomes.len(), 4);
    assert!(outcomes.iter().all(|call| call.resource_refs.len() <= 64));
    assert_eq!(outcomes[1].disposition, CallDisposition::Succeeded);
    assert!(store.claim_continuation(claim_input).is_err());
    assert_eq!(
        platform
            .creators()
            .unwrap()
            .list_creators_async(Default::default())
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    // Authority expiry keeps history; ordinary terminal retention still applies.
    assert_eq!(
        store
            .cleanup_at(deadline + super::super::TERMINAL_TTL + Duration::from_secs(1))
            .unwrap()
            .active,
        0
    );
}

#[test]
fn confirmation_racing_expiry_has_one_authority_winner() {
    let (_directory, platform) = setup();
    let store = ResultMemoryStore::new();
    let deadline = Instant::now() + Duration::from_secs(900);
    let review = prepared(&platform, "save-1", deadline, Some(b"exact call"));
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, frame(), peer(b"resume"), Vec::new())
        .unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let submitter = {
        let store = store.clone();
        let current = current.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            store.submit_owner_review(&current.binding, selection(&current))
        })
    };
    let expirer = {
        let store = store.clone();
        let binding = current.binding.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            store.current_at(&binding, deadline)
        })
    };
    barrier.wait();
    let submitted = submitter.join().unwrap();
    let expired = expirer.join().unwrap().unwrap();
    match submitted {
        Ok(grant) => {
            assert!(matches!(expired.state, ResultState::Submitted { .. }));
            assert_eq!(expired.binding, *grant.binding());
        }
        Err(_) => assert!(matches!(expired.state, ResultState::Failed(_))),
    }
    assert!(
        store
            .submit_owner_review(&expired.binding, selection(&current))
            .is_err()
    );
}

#[test]
fn expiry_at_capacity_bounds_notification_without_dropping_result_references() {
    let (_directory, platform) = setup();
    let deadline = Instant::now() + Duration::from_secs(900);
    let mut initial_frame = frame();
    initial_frame.outcomes[0].resource_refs = (0..64)
        .map(|index| super::super::ResultResourceRef {
            kind: super::super::ResultResourceKind::ProviderObject,
            id: format!("{index:03}{}", "x".repeat(253)),
            label: None,
        })
        .collect();
    let input = prepared(&platform, "save-1", deadline, Some(b"call"))
        .into_input(initial_frame.clone(), peer(b"resume"))
        .unwrap();
    let max_bytes = super::super::validation::result_payload_bytes(&input).unwrap();
    let store = ResultMemoryStore::with_limits(StoreLimits {
        max_total_payload_bytes: max_bytes,
        ..StoreLimits::default()
    });
    let origin = input.origin.clone();
    let reference = store.mint_from_execute(input).unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let expired = store.current_at(&current.binding, deadline).unwrap();
    assert_eq!(expired.batch.outcomes[0], initial_frame.outcomes[0]);
    assert!(store.cleanup().unwrap().payload_bytes <= max_bytes);
    let claimed = store
        .claim_continuation(super::super::ClaimResultContinuationInput {
            binding: expired.binding,
            expected_continuation_revision: expired.continuation.continuation_revision,
            mode: super::super::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let super::super::ResultContinuationContext::Execution { outcomes } = claimed.request.context
    else {
        unreachable!()
    };
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].call_id, "save-1");
    assert!(outcomes[0].resource_refs.is_empty());
    assert!(
        claimed
            .request
            .message
            .contains("Full result details remain")
    );
}

#[tokio::test]
async fn mixed_cancel_preserves_real_saved_rows_and_derives_bounded_localized_continuation() {
    let (_directory, platform) = setup();
    let store = platform.result_store.clone();
    let review = prepared_count(
        &platform,
        "save-1",
        Instant::now() + Duration::from_secs(900),
        Some(b"first"),
        onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS,
    );
    // One row stays unselected, so the batch is genuinely mixed.
    let selected = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS - 1;
    let origin = review.context().origin().clone();
    let reference = store
        .mint_owner_review(review, frame(), peer(b"original batch"), Vec::new())
        .unwrap();
    let current = store
        .present_with_locale(
            &reference,
            &origin,
            crate::PresentationLocale::SimplifiedChinese,
        )
        .unwrap();
    let OwnerReviewSelection::CreatorSave { mut selections } = selection(&current) else {
        unreachable!()
    };
    selections.truncate(selected);
    let grant = store
        .submit_owner_review(
            &current.binding,
            OwnerReviewSelection::CreatorSave { selections },
        )
        .unwrap();
    let binding = grant.binding().clone();
    let first = platform
        .execute_creator_save_submitted(grant)
        .await
        .unwrap();
    assert!(first.continue_batch);
    assert_eq!(first.outcome.resource_refs.len(), selected);
    let OwnerCallOutput::CreatorSave(first_result) = &first.output else {
        unreachable!()
    };
    assert_eq!(
        first_result.rows[selected].status,
        super::super::CreatorSaveBatchRowStatus::NotSelected
    );
    assert!(!first_result.rows[0].confirmed_route_ids.is_empty());
    let first_outcome = first.outcome.clone();
    let prefix = first.presentation.sections;
    let mut progress = completed_prefix();
    progress.outcomes[1] = first.outcome;
    let next = store
        .advance_owner_review(
            &binding,
            prepared(
                &platform,
                "save-2",
                Instant::now() + Duration::from_secs(900),
                Some(b"second"),
            ),
            progress,
            peer(b"remaining call"),
            prefix.clone(),
        )
        .unwrap();
    let before_metrics = store.cleanup().unwrap();
    let forged = super::super::ResultContinuationRequest {
        message: "Caller-provided execution facts".into(),
        requires_model_context: true,
        context: super::super::ResultContinuationContext::Execution {
            outcomes: vec![super::super::ResultContinuationOutcomeSummary {
                call_id: "forged".into(),
                operation_id: SAVE_OPERATION.into(),
                disposition: CallDisposition::Succeeded,
                summary: "Forged completed action".into(),
                resource_refs: Vec::new(),
            }],
        },
    };
    assert_eq!(
        store
            .cancel(super::super::CancelResultInput {
                binding: next.binding.clone(),
                continuation: Some(forged),
            })
            .unwrap_err(),
        ResultStoreError::InvalidInput("cancel_continuation_context")
    );
    assert_eq!(store.current(&next.binding).unwrap(), next);
    assert_eq!(store.cleanup().unwrap(), before_metrics);
    let cancelled = store
        .cancel(super::super::CancelResultInput {
            binding: next.binding.clone(),
            continuation: Some(super::super::ResultContinuationRequest {
                message: "Caller-provided cancellation text".into(),
                requires_model_context: false,
                context: super::super::ResultContinuationContext::Cancelled,
            }),
        })
        .unwrap();
    assert_eq!(cancelled.binding.result_ref, next.binding.result_ref);
    assert_eq!(cancelled.binding.fingerprint, next.binding.fingerprint);
    assert_eq!(cancelled.binding.revision, next.binding.revision + 1);
    assert_eq!(cancelled.presentation, next.presentation);
    assert_eq!(cancelled.completed_sections_count, prefix.len());
    assert_eq!(cancelled.batch.outcomes[1], first_outcome);
    assert!(
        matches!(&cancelled.batch.outcomes[3].disposition, CallDisposition::NotRun { reason } if reason == "user_cancelled")
    );
    let ResultState::Resolved(resolution) = &cancelled.state else {
        panic!("cancel terminal")
    };
    assert_eq!(resolution.code, "user_cancelled");
    assert!(resolution.summary.contains("已保留"));
    assert!(cancelled.pending_action.is_none());
    assert!(
        store
            .submit_owner_review(&cancelled.binding, selection(&next))
            .is_err()
    );
    assert!(
        store
            .cancel(super::super::CancelResultInput {
                binding: cancelled.binding.clone(),
                continuation: None
            })
            .is_err()
    );
    let claim_input = super::super::ClaimResultContinuationInput {
        binding: cancelled.binding.clone(),
        expected_continuation_revision: cancelled.continuation.continuation_revision,
        mode: super::super::ResultContinuationClaimMode::Pending,
    };
    let claimed = store.claim_continuation(claim_input.clone()).unwrap();
    assert!(claimed.request.requires_model_context);
    assert!(claimed.request.message.contains("完整结果详情"));
    let super::super::ResultContinuationContext::Execution { outcomes } = claimed.request.context
    else {
        panic!("owner-derived execution continuation")
    };
    assert_eq!(outcomes.len(), 4);
    assert_eq!(outcomes[1].disposition, CallDisposition::Succeeded);
    // A full business batch saves fewer Creators than the generic ref budget,
    // so the continuation carries every one of them and truncates nothing.
    assert_eq!(outcomes[1].resource_refs.len(), selected);
    assert!(outcomes[1].resource_refs.len() <= super::super::MAX_OWNER_FRAME_RESOURCE_REFS);
    assert_eq!(
        outcomes[1].resource_refs[0].id,
        first_outcome.resource_refs[0].id
    );
    assert!(
        outcomes[1]
            .resource_refs
            .iter()
            .all(|resource| resource.label.is_none())
    );
    assert!(
        matches!(&outcomes[3].disposition, CallDisposition::NotRun { reason } if reason == "user_cancelled")
    );
    assert!(store.claim_continuation(claim_input).is_err());
    assert_eq!(
        platform
            .creators()
            .unwrap()
            .list_creators_async(onecrew_creator_runtime::ListCreatorsInput {
                limit: Some(100),
                ..Default::default()
            })
            .await
            .unwrap()
            .items
            .len(),
        selected
    );
}

#[test]
fn mixed_cancel_without_requested_continuation_keeps_notifications_opt_in() {
    let store = ResultMemoryStore::new();
    let input = generic_input(frame());
    let origin = input.origin.clone();
    let reference = store.mint_from_execute(input).unwrap();
    let current = store.present(&reference, &origin).unwrap();
    let cancelled = store
        .cancel(super::super::CancelResultInput {
            binding: current.binding,
            continuation: None,
        })
        .unwrap();
    assert_eq!(cancelled.batch.outcomes[0], frame().outcomes[0]);
    assert!(cancelled.batch.outcomes[1..].iter().all(|call| matches!(&call.disposition, CallDisposition::NotRun { reason } if reason == "user_cancelled")));
    assert_eq!(
        cancelled.continuation.state,
        super::super::ResultContinuationState::NotRequested
    );
}
