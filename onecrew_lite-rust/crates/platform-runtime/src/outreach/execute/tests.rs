use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use async_trait::async_trait;
use onecrew_creator_runtime::{
    AccountRefId, CanonicalCreatorSaveInput, CreatorContentOutreachContext, CreatorOutreachError,
    CreatorQualityInput, CreatorSaveProfile, CreatorSaveProvenance, CreatorSaveRouteSelection,
    CreatorSaveRowOutcome, Platform, PlatformAccountCandidate, PrepareContentReplyInput,
    PreparedCreatorOutreachIntent, RelationshipMutation, TargetKind,
};
use onecrew_dm_contracts::{
    DmCapability, DmCapabilityScope, DmMessage, DmReadbackCoverage, DmReceipt, DmThread,
    DmThreadReadback,
};

use super::*;
use crate::outreach::effects::{RelationshipReadback, ReplyReadback};

struct FakeEffects {
    sends: AtomicUsize,
    reads: AtomicUsize,
    failure: Mutex<Option<EffectFailure>>,
    receipt: Mutex<String>,
    body: Mutex<String>,
    following: AtomicBool,
    following_unknown: AtomicBool,
    follow_relevant: AtomicBool,
    thread: Mutex<Option<DmThread>>,
    message_read_failure: Mutex<Option<onecrew_dm_contracts::DmFailure>>,
    message_send_failure: Mutex<Option<onecrew_dm_contracts::DmSendError>>,
    capability_failure: Mutex<Option<onecrew_dm_contracts::DmFailure>>,
}

impl Default for FakeEffects {
    fn default() -> Self {
        Self {
            sends: AtomicUsize::new(0),
            reads: AtomicUsize::new(0),
            failure: Mutex::new(None),
            receipt: Mutex::new("reply-1".into()),
            body: Mutex::new("reviewed reply".into()),
            following: AtomicBool::new(false),
            following_unknown: AtomicBool::new(false),
            follow_relevant: AtomicBool::new(false),
            thread: Mutex::new(None),
            message_read_failure: Mutex::new(None),
            message_send_failure: Mutex::new(None),
            capability_failure: Mutex::new(None),
        }
    }
}

#[async_trait]
impl OutreachEffects for FakeEffects {
    async fn relationship_read(
        &self,
        _: &str,
        _: &onecrew_creator_runtime::CreatorOutreachTarget,
    ) -> Result<RelationshipReadback, OutreachError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(RelationshipReadback {
            following: (!self.following_unknown.load(Ordering::SeqCst))
                .then(|| self.following.load(Ordering::SeqCst)),
            can_message: Some(true),
            pending_follow_request: None,
        })
    }
    async fn relationship_write(
        &self,
        _: &onecrew_creator_runtime::CreatorRouteOutreachContext,
        action: RelationshipMutation,
    ) -> Result<(), EffectFailure> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        self.following
            .store(action == RelationshipMutation::Follow, Ordering::SeqCst);
        Ok(())
    }
    async fn reply_read(&self, _: &str, id: &str) -> Result<Option<ReplyReadback>, OutreachError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(Some(ReplyReadback {
            id: id.to_owned(),
            body: self.body.lock().unwrap().clone(),
            author_external_id: "123".into(),
            reply_to_id: (id != "post-1").then(|| "post-1".into()),
            conversation_id: Some("post-1".into()),
        }))
    }
    async fn reply_write(&self, _: &str, _: &str, _: &str) -> Result<String, EffectFailure> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = *self.failure.lock().unwrap() {
            return Err(error);
        }
        Ok(self.receipt.lock().unwrap().clone())
    }
    async fn message_capability(
        &self,
        _: &onecrew_creator_runtime::CreatorRouteOutreachContext,
    ) -> Result<DmCapability, OutreachError> {
        if let Some(failure) = self.capability_failure.lock().unwrap().clone() {
            return Err(OutreachError::DirectMessage(failure));
        }
        Ok(DmCapability {
            state: DmCapabilityState::Allowed,
            scope: DmCapabilityScope::NewMessageRequest,
            reason: None,
            detail: None,
            follow_relevant: self.follow_relevant.load(Ordering::SeqCst),
        })
    }
    async fn message_write(
        &self,
        context: &onecrew_creator_runtime::CreatorRouteOutreachContext,
        body: &str,
    ) -> Result<DmReceipt, onecrew_dm_contracts::DmSendError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.message_send_failure.lock().unwrap().clone() {
            return Err(error);
        }
        if let Some(error) = *self.failure.lock().unwrap() {
            let diagnostic = onecrew_dm_contracts::DmDiagnostic::provider(
                "test",
                "test_dm",
                "test send failure",
            );
            return Err(match error {
                EffectFailure::Refused => onecrew_dm_contracts::DmSendError::refused(diagnostic),
                EffectFailure::Unknown => onecrew_dm_contracts::DmSendError::unknown(diagnostic),
            });
        }
        let id = self.receipt.lock().unwrap().clone();
        *self.thread.lock().unwrap() = Some(DmThread {
            external_thread_id: "thread-1".into(),
            peer_external_id: context.target.external_target_id.clone(),
            messages: vec![DmMessage {
                external_message_id: id.clone(),
                body: body.into(),
                sent_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                outgoing: true,
                provider: serde_json::json!({}),
            }],
        });
        Ok(DmReceipt {
            external_message_id: id,
            external_thread_id: Some("thread-1".into()),
        })
    }
    async fn message_read(
        &self,
        _: &onecrew_creator_runtime::CreatorRouteOutreachContext,
        _: Option<&str>,
    ) -> Result<DmThreadReadback, OutreachError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if let Some(failure) = self.message_read_failure.lock().unwrap().clone() {
            return Err(OutreachError::DirectMessage(failure));
        }
        Ok(DmThreadReadback {
            thread: self.thread.lock().unwrap().clone(),
            coverage: DmReadbackCoverage::Incomplete,
        })
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    platform: Arc<LocalPlatform>,
    context: CreatorContentOutreachContext,
    route_id: onecrew_creator_runtime::CreatorContactRouteId,
    db: rusqlite::Connection,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let platform = Arc::new(LocalPlatform::open(directory.path()).unwrap());
        let creators = &platform.creators;
        let operator = AccountRefId::new("00000000-0000-0000-0000-000000000001");
        creators
            .sync_account_ref(operator.clone(), Platform::X, "profile", "operator")
            .unwrap();
        let prepared = creators
            .prepare_creator_save_batch_async(
                vec![CanonicalCreatorSaveInput {
                    row_id: "creator-row".into(),
                    account: PlatformAccountCandidate {
                        platform: Platform::X,
                        external_id: Some("123".into()),
                        target_kind: Some(TargetKind::User),
                        handle: Some("person".into()),
                        display_name: "Person".into(),
                        profile_url: Some("https://x.com/person".into()),
                    },
                    profile: CreatorSaveProfile::default(),
                    profile_observed_at: 1_800_000_000,
                    provenance: CreatorSaveProvenance::DirectSelection {
                        source_ref: "fixture".into(),
                        source_url: Some("https://x.com/person".into()),
                        description: None,
                        operator_account_ref_id: Some(operator.clone()),
                    },
                    contact_evidence: Vec::new(),
                    notes: None,
                    tags: None,
                    quality: CreatorQualityInput::default(),
                }],
                onecrew_creator_runtime::CreatorSaveIntent::SaveOrUpdate,
            )
            .await
            .unwrap();
        let selections = prepared
            .rows()
            .iter()
            .map(|row| CreatorSaveRouteSelection {
                row_id: row.row_id.clone(),
                route_ids: row
                    .route_options
                    .iter()
                    .map(|route| route.route_id.clone())
                    .collect(),
            })
            .collect();
        let selected = prepared.select_routes(selections).unwrap();
        let saved = creators
            .execute_creator_save_batch_async(selected, "fixture-save".into())
            .await
            .unwrap();
        let CreatorSaveRowOutcome::Saved { creator, .. } = &saved.outcomes[0] else {
            panic!("fixture Creator save failed");
        };
        let context = creators
            .creator_content_reply_context(PrepareContentReplyInput {
                creator_id: creator.creator.id.clone(),
                target_platform_account_id: creator.platform_accounts[0].id.clone(),
                operator_account_ref_id: operator,
                content_id: "post-1".into(),
                reply_to_id: None,
            })
            .await
            .unwrap();
        let route_id = creator.contact_routes[0].route.id.clone();
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        Self {
            _directory: directory,
            platform,
            context,
            route_id,
            db,
        }
    }

    fn action(&self, key: &str, body: &str) -> PreparedOutreachAction {
        let intent =
            PreparedCreatorOutreachIntent::content_reply(self.context.clone(), body.to_owned())
                .unwrap();
        PreparedOutreachAction {
            fingerprint: intent.fingerprint().to_owned(),
            intent,
            idempotency_key: key.into(),
            expires: Instant::now() + Duration::from_secs(900),
        }
    }

    async fn execute(
        &self,
        provider: &FakeEffects,
        key: &str,
    ) -> Result<OutreachCompletion, OutreachError> {
        execute_action(
            provider,
            Arc::clone(&self.platform.creators),
            self.action(key, "reviewed reply"),
        )
        .await
    }

    async fn state(&self, key: &str) -> CreatorOutreachActionState {
        self.platform
            .creators
            .creator_outreach_read(key, &self.action(key, "reviewed reply").intent)
            .await
            .unwrap()
            .unwrap()
    }

    async fn route_context(&self) -> onecrew_creator_runtime::CreatorRouteOutreachContext {
        use onecrew_creator_runtime::{
            CreatorContactTargetPreparation, PlatformResolvedTarget, PlatformTargetResolution,
            PrepareDirectMessageInput, ResolveContactTargetInput,
        };
        let input = ResolveContactTargetInput {
            creator_id: self.context.creator_id.clone(),
            contact_route_id: self.route_id.clone(),
            operator_account_ref_id: self.context.operator_account.id.clone(),
            refresh: true,
        };
        let resolution = match self
            .platform
            .creators
            .prepare_contact_target_resolution(input)
            .await
            .unwrap()
        {
            CreatorContactTargetPreparation::Ready(result) => *result,
            CreatorContactTargetPreparation::Read(context) => self
                .platform
                .creators
                .finish_contact_target_resolution(
                    context,
                    PlatformTargetResolution::Resolved {
                        target: PlatformResolvedTarget {
                            platform: Platform::X,
                            target_kind: TargetKind::User,
                            external_target_id: "123".into(),
                            handle: Some("person".into()),
                            display_name: Some("Person".into()),
                            canonical_url: Some("https://x.com/person".into()),
                        },
                    },
                )
                .await
                .unwrap(),
            CreatorContactTargetPreparation::Commit { context, evidence } => self
                .platform
                .creators
                .finish_contact_target_resolution(context, evidence)
                .await
                .unwrap(),
        };
        self.platform
            .creators
            .creator_direct_message_context(PrepareDirectMessageInput {
                creator_id: self.context.creator_id.clone(),
                contact_route_id: self.route_id.clone(),
                resolved_target_id: resolution.resolved.unwrap().target.id,
                operator_account_ref_id: self.context.operator_account.id.clone(),
            })
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn successful_write_records_once_and_completed_replay_ignores_later_disable() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let first = fixture.execute(&provider, "reply-complete").await.unwrap();
    assert_eq!(first.phase, OutreachPhase::Completed);
    assert!(first.creator_recorded);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    fixture
        .db
        .execute("UPDATE account_refs SET active = 0", [])
        .unwrap();
    let read_count = provider.reads.load(Ordering::SeqCst);
    let replay = fixture.execute(&provider, "reply-complete").await.unwrap();
    assert_eq!(replay.phase, OutreachPhase::Completed);
    assert!(replay.replayed && replay.creator_recorded);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(provider.reads.load(Ordering::SeqCst), read_count);
}

#[tokio::test]
async fn record_failure_preserves_verified_receipt_and_retries_only_local_record() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    fixture.db.execute_batch("CREATE TRIGGER fixture_record_failure BEFORE INSERT ON touchpoints BEGIN SELECT RAISE(FAIL, 'fixture record failure'); END;").unwrap();
    let first = fixture
        .execute(&provider, "reply-record-failure")
        .await
        .unwrap();
    assert_eq!(first.phase, OutreachPhase::RecordPending);
    assert!(!first.creator_recorded);
    assert_eq!(
        fixture.state("reply-record-failure").await.phase,
        CreatorOutreachPhase::RecordPending
    );
    fixture
        .db
        .execute_batch("DROP TRIGGER fixture_record_failure;")
        .unwrap();
    let read_count = provider.reads.load(Ordering::SeqCst);
    let replay = fixture
        .execute(&provider, "reply-record-failure")
        .await
        .unwrap();
    assert_eq!(replay.phase, OutreachPhase::Completed);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(provider.reads.load(Ordering::SeqCst), read_count);
}

#[tokio::test]
async fn changed_body_with_same_key_is_rejected_before_any_provider_call() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    fixture.execute(&provider, "reply-tamper").await.unwrap();
    let error = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        fixture.action("reply-tamper", "different reply"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        OutreachError::Creator(CreatorOutreachError::IntentConflict)
    ));
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn started_public_reply_without_receipt_never_republishes() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let action = fixture.action("reply-started", "reviewed reply");
    let lease = fixture
        .platform
        .creators
        .acquire_creator_outreach(&action.idempotency_key)
        .unwrap();
    fixture
        .platform
        .creators
        .creator_outreach_claim(&lease, &action.intent)
        .await
        .unwrap();
    drop(lease);
    let replay = fixture.execute(&provider, "reply-started").await.unwrap();
    assert_eq!(replay.phase, OutreachPhase::RemoteEffectUnknown);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert_eq!(provider.reads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn accepted_reply_mismatch_resumes_readback_without_a_second_write() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    *provider.body.lock().unwrap() = "mismatched body".into();
    let first = fixture.execute(&provider, "reply-mismatch").await.unwrap();
    assert_eq!(first.phase, OutreachPhase::RemoteAccepted);
    *provider.body.lock().unwrap() = "reviewed reply".into();
    let replay = fixture.execute(&provider, "reply-mismatch").await.unwrap();
    assert_eq!(replay.phase, OutreachPhase::Completed);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn explicit_refusal_and_empty_receipt_remain_distinct_on_replay() {
    let fixture = Fixture::new().await;
    let refused = FakeEffects::default();
    *refused.failure.lock().unwrap() = Some(EffectFailure::Refused);
    assert_eq!(
        fixture
            .execute(&refused, "reply-refused")
            .await
            .unwrap()
            .phase,
        OutreachPhase::Rejected
    );
    assert_eq!(
        fixture
            .execute(&refused, "reply-refused")
            .await
            .unwrap()
            .phase,
        OutreachPhase::Rejected
    );
    assert_eq!(refused.sends.load(Ordering::SeqCst), 1);
    let empty = FakeEffects::default();
    empty.receipt.lock().unwrap().clear();
    assert_eq!(
        fixture.execute(&empty, "reply-empty").await.unwrap().phase,
        OutreachPhase::RemoteEffectUnknown
    );
    assert_eq!(
        fixture.execute(&empty, "reply-empty").await.unwrap().phase,
        OutreachPhase::RemoteEffectUnknown
    );
    assert_eq!(empty.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn accepted_staging_failure_keeps_the_received_id_and_never_blindly_republishes() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    fixture.db.execute_batch("CREATE TRIGGER fixture_stage_failure BEFORE UPDATE ON idempotency_records WHEN NEW.state = 'remote_accepted' BEGIN SELECT RAISE(FAIL, 'fixture staging failure'); END;").unwrap();
    let first = fixture
        .execute(&provider, "reply-stage-failure")
        .await
        .unwrap();
    assert_eq!(first.phase, OutreachPhase::RemoteAccepted);
    assert!(matches!(
        first.receipt,
        Some(OutreachReceiptReference::ContentReply { .. })
    ));
    fixture
        .db
        .execute_batch("DROP TRIGGER fixture_stage_failure;")
        .unwrap();
    assert_eq!(
        fixture
            .execute(&provider, "reply-stage-failure")
            .await
            .unwrap()
            .phase,
        OutreachPhase::RemoteEffectUnknown
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn the_result_owner_freezes_a_single_use_grant_before_any_claim_or_send() {
    use crate::{
        CallDisposition, ExecutedBatch, ExecutedCallOutcome, OwnerReviewContext,
        OwnerReviewSelection, PreparedOwnerReview, PresentationProjection, ResultOrigin,
        ResultPresentationKind,
    };
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let origin = ResultOrigin::new("outreach-fixture").unwrap();
    let context = OwnerReviewContext::new(
        origin.clone(),
        "send-1",
        "onecrew.creator.content_reply.send",
        None,
    )
    .unwrap();
    let review = PreparedOwnerReview::outreach(
        fixture.action("grant-1", "reviewed reply"),
        context,
        PresentationProjection {
            kind: ResultPresentationKind::new("outreach_confirmation").unwrap(),
            title: "Review public reply".into(),
            summary: Some(
                "Confirm the exact operator, stable target and content shown below.".into(),
            ),
            sections: vec![crate::PresentationSection::Fields {
                heading: None,
                fields: vec![crate::PresentationField {
                    label: "Reply".into(),
                    value: "reviewed reply".into(),
                }],
            }],
        },
        "Confirm reviewed reply?".into(),
        "Confirm".into(),
    )
    .unwrap();
    let result_ref = fixture
        .platform
        .mint_owner_review(
            review,
            ExecutedBatch {
                outcomes: vec![ExecutedCallOutcome {
                    call_id: "send-1".into(),
                    operation_id: "onecrew.creator.content_reply.send".into(),
                    disposition: CallDisposition::NotRun {
                        reason: "confirmation required".into(),
                    },
                    summary: "reviewed reply".into(),
                    resource_refs: Vec::new(),
                }],
            },
            None,
            Vec::new(),
        )
        .unwrap();
    assert!(
        fixture
            .platform
            .creators
            .creator_outreach_read(
                "grant-1",
                &fixture.action("grant-1", "reviewed reply").intent
            )
            .await
            .unwrap()
            .is_none()
    );
    let presented = fixture
        .platform
        .present_result(&result_ref, &origin)
        .unwrap();
    assert!(
        fixture
            .platform
            .submit_owner_review(
                &presented.binding,
                OwnerReviewSelection::CreatorSave {
                    selections: Vec::new()
                }
            )
            .is_err()
    );
    let grant = fixture
        .platform
        .submit_owner_review(&presented.binding, OwnerReviewSelection::Confirm)
        .unwrap();
    assert!(
        fixture
            .platform
            .submit_owner_review(&presented.binding, OwnerReviewSelection::Confirm)
            .is_err()
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    let completed = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        grant.into_outreach().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(completed.phase, OutreachPhase::Completed);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_second_key_with_the_same_provider_receipt_cannot_duplicate_creator_touchpoints() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    fixture.execute(&provider, "receipt-first").await.unwrap();
    let second = fixture.execute(&provider, "receipt-second").await.unwrap();
    assert_eq!(second.phase, OutreachPhase::Completed);
    let touchpoints: u64 = fixture
        .db
        .query_row("SELECT COUNT(*) FROM touchpoints", [], |row| row.get(0))
        .unwrap();
    assert_eq!(touchpoints, 1);
}

fn route_action(intent: PreparedCreatorOutreachIntent, key: &str) -> PreparedOutreachAction {
    PreparedOutreachAction {
        fingerprint: intent.fingerprint().into(),
        intent,
        idempotency_key: key.into(),
        expires: Instant::now() + Duration::from_secs(900),
    }
}

#[tokio::test]
async fn dm_read_diagnostics_preserve_acceptance_and_do_not_control_recovery() {
    use onecrew_dm_contracts::{DmDiagnostic, DmFailure};
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let intent = PreparedCreatorOutreachIntent::direct_message(
        fixture.route_context().await,
        "reviewed message".into(),
        false,
    )
    .unwrap();
    for message in ["login", "429", "sent successfully", "timeout", "passcode"] {
        let mut diagnostic = DmDiagnostic::provider("test", "test_dm_protocol", message);
        diagnostic.code = Some("NEW_UNRECOGNIZED_CODE".into());
        let failure = DmFailure {
            stage: "message_readback",
            diagnostic,
        };
        *provider.message_read_failure.lock().unwrap() = Some(failure.clone());
        let completion = execute_action(
            &provider,
            Arc::clone(&fixture.platform.creators),
            route_action(intent.clone(), "accepted-with-diagnostic"),
        )
        .await
        .unwrap();
        assert_eq!(completion.phase, OutreachPhase::RemoteAccepted);
        assert!(!completion.creator_recorded);
        assert_eq!(completion.failure, Some(failure));
        assert_eq!(
            completion.receipt,
            Some(OutreachReceiptReference::Message {
                external_message_id: "reply-1".into(),
                external_thread_id: Some("thread-1".into()),
            })
        );
        assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    }
    *provider.message_read_failure.lock().unwrap() = None;
    let completion = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "accepted-with-diagnostic"),
    )
    .await
    .unwrap();
    assert_eq!(completion.phase, OutreachPhase::Completed);
    assert!(completion.creator_recorded);
    assert!(completion.failure.is_none());
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dm_unknown_send_diagnostics_never_authorize_resending_from_incomplete_reads() {
    use onecrew_dm_contracts::{DmDiagnostic, DmFailure, DmSendError};
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let intent = PreparedCreatorOutreachIntent::direct_message(
        fixture.route_context().await,
        "reviewed message".into(),
        false,
    )
    .unwrap();
    let diagnostic = DmDiagnostic::provider("test", "test_dm_send", "sent successfully");
    *provider.message_send_failure.lock().unwrap() = Some(DmSendError::unknown(diagnostic.clone()));
    let first = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "unknown-diagnostic"),
    )
    .await
    .unwrap();
    assert_eq!(first.phase, OutreachPhase::RemoteEffectUnknown);
    assert!(first.receipt.is_none());
    assert_eq!(
        first.failure,
        Some(DmFailure {
            stage: "message_send",
            diagnostic
        })
    );
    for (protocol, code) in [
        ("test_x_chat", Some("429")),
        ("test_ig_private", Some("429")),
        ("test_mtproto", Some("NEW_ERROR")),
        ("test_mtproto", None),
    ] {
        let mut diagnostic = DmDiagnostic::provider("test", protocol, "login timeout");
        diagnostic.code = code.map(str::to_owned);
        let failure = DmFailure {
            stage: "message_readback",
            diagnostic,
        };
        *provider.message_read_failure.lock().unwrap() = Some(failure.clone());
        let replay = execute_action(
            &provider,
            Arc::clone(&fixture.platform.creators),
            route_action(intent.clone(), "unknown-diagnostic"),
        )
        .await
        .unwrap();
        assert_eq!(replay.phase, OutreachPhase::RemoteEffectUnknown);
        assert!(replay.receipt.is_none());
        assert_eq!(replay.failure, Some(failure));
        assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    }
    *provider.message_read_failure.lock().unwrap() = None;
    let incomplete = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "unknown-diagnostic"),
    )
    .await
    .unwrap();
    assert_eq!(incomplete.phase, OutreachPhase::RemoteEffectUnknown);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dm_capability_failure_keeps_its_stage_and_never_claims_or_sends() {
    use onecrew_dm_contracts::{DmDiagnostic, DmFailure};
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let intent = PreparedCreatorOutreachIntent::direct_message(
        fixture.route_context().await,
        "reviewed message".into(),
        false,
    )
    .unwrap();
    let failure = DmFailure {
        stage: "message_capability",
        diagnostic: DmDiagnostic::provider("test", "test_dm_protocol", "login"),
    };
    *provider.capability_failure.lock().unwrap() = Some(failure.clone());
    let error = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "capability-failed"),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, OutreachError::DirectMessage(actual) if actual == failure));
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert!(
        fixture
            .platform
            .creators
            .creator_outreach_read("capability-failed", &intent)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn dm_unfollowed_confirmation_rejects_unknown_following_before_claim_or_send() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    provider.follow_relevant.store(true, Ordering::SeqCst);
    provider.following_unknown.store(true, Ordering::SeqCst);
    let intent = PreparedCreatorOutreachIntent::direct_message(
        fixture.route_context().await,
        "reviewed message".into(),
        false,
    )
    .unwrap();
    let error = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "unknown-following-confirmation"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        OutreachError::UnfollowedConfirmationRequired
    ));
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    assert!(
        fixture
            .platform
            .creators
            .creator_outreach_read("unknown-following-confirmation", &intent)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn dm_accepted_readback_requires_target_thread_message_identity_and_body() {
    use onecrew_dm_contracts::{DmDiagnostic, DmFailure};
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let intent = PreparedCreatorOutreachIntent::direct_message(
        fixture.route_context().await,
        "reviewed message".into(),
        false,
    )
    .unwrap();
    *provider.message_read_failure.lock().unwrap() = Some(DmFailure {
        stage: "message_readback",
        diagnostic: DmDiagnostic::provider("test", "test_dm", "timeout"),
    });
    let accepted = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "readback-identity"),
    )
    .await
    .unwrap();
    assert_eq!(accepted.phase, OutreachPhase::RemoteAccepted);
    *provider.message_read_failure.lock().unwrap() = None;
    let original = provider.thread.lock().unwrap().clone().unwrap();
    for mismatch in 0..5 {
        let mut thread = original.clone();
        match mismatch {
            0 => thread.peer_external_id = "another-peer".into(),
            1 => thread.external_thread_id = "another-thread".into(),
            2 => thread.messages[0].external_message_id = "another-message".into(),
            3 => thread.messages[0].body = "another body".into(),
            _ => thread.messages[0].outgoing = false,
        }
        *provider.thread.lock().unwrap() = Some(thread);
        let replay = execute_action(
            &provider,
            Arc::clone(&fixture.platform.creators),
            route_action(intent.clone(), "readback-identity"),
        )
        .await
        .unwrap();
        assert_eq!(replay.phase, OutreachPhase::RemoteAccepted);
        assert!(!replay.creator_recorded);
        assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    }
    *provider.thread.lock().unwrap() = Some(original);
    let completed = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "readback-identity"),
    )
    .await
    .unwrap();
    assert_eq!(completed.phase, OutreachPhase::Completed);
    let Some(CreatorOutreachRecord::DirectMessage(record)) = completed.recorded else {
        panic!("verified message must be recorded")
    };
    assert_eq!(record.message.status, "sent");
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_started_relationship_reads_the_stable_state_and_never_assumes_safe_resend() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let context = fixture.route_context().await;
    let intent =
        PreparedCreatorOutreachIntent::relationship(context, RelationshipMutation::Follow).unwrap();
    let lease = fixture
        .platform
        .creators
        .acquire_creator_outreach("relationship-started")
        .unwrap();
    fixture
        .platform
        .creators
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    drop(lease);
    let unknown = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "relationship-started"),
    )
    .await
    .unwrap();
    assert_eq!(unknown.phase, OutreachPhase::RemoteEffectUnknown);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
    provider.following.store(true, Ordering::SeqCst);
    let adopted = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "relationship-started"),
    )
    .await
    .unwrap();
    assert_eq!(adopted.phase, OutreachPhase::Completed);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_started_dm_with_an_incomplete_empty_thread_keeps_unknown_without_sending() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let context = fixture.route_context().await;
    let intent =
        PreparedCreatorOutreachIntent::direct_message(context, "reviewed message".into(), false)
            .unwrap();
    let lease = fixture
        .platform
        .creators
        .acquire_creator_outreach("message-started")
        .unwrap();
    fixture
        .platform
        .creators
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    drop(lease);
    let unknown = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "message-started"),
    )
    .await
    .unwrap();
    assert_eq!(unknown.phase, OutreachPhase::RemoteEffectUnknown);
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn verified_dm_record_failure_resumes_from_durable_ids_and_same_body_after_disable() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    let context = fixture.route_context().await;
    let intent =
        PreparedCreatorOutreachIntent::direct_message(context, "reviewed message".into(), false)
            .unwrap();
    fixture.db.execute_batch("CREATE TRIGGER fixture_dm_record_failure BEFORE INSERT ON touchpoints BEGIN SELECT RAISE(FAIL, 'fixture DM record failure'); END;").unwrap();
    let first = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent.clone(), "message-record-pending"),
    )
    .await
    .unwrap();
    assert_eq!(first.phase, OutreachPhase::RecordPending);
    let accepted_receipt = first.receipt.clone();
    fixture
        .db
        .execute_batch(
            "DROP TRIGGER fixture_dm_record_failure; UPDATE account_refs SET active = 0;",
        )
        .unwrap();
    let read_count = provider.reads.load(Ordering::SeqCst);
    let replay = execute_action(
        &provider,
        Arc::clone(&fixture.platform.creators),
        route_action(intent, "message-record-pending"),
    )
    .await
    .unwrap();
    assert_eq!(replay.phase, OutreachPhase::Completed);
    assert_eq!(replay.receipt, accepted_receipt);
    assert_eq!(replay.idempotency_key, "message-record-pending");
    let Some(CreatorOutreachRecord::DirectMessage(record)) = &replay.recorded else {
        panic!("verified message must be recorded")
    };
    assert_eq!(record.message.body, "reviewed message");
    assert_eq!(
        record.message.external_message_id.as_deref(),
        Some("reply-1")
    );
    assert_eq!(provider.sends.load(Ordering::SeqCst), 1);
    assert_eq!(provider.reads.load(Ordering::SeqCst), read_count);
}

#[tokio::test]
async fn a_claim_failure_cannot_reach_the_provider_write() {
    let fixture = Fixture::new().await;
    let provider = FakeEffects::default();
    fixture.db.execute_batch("CREATE TRIGGER fixture_claim_failure BEFORE INSERT ON idempotency_records WHEN NEW.action = 'content_reply_send_v1' BEGIN SELECT RAISE(FAIL, 'fixture claim failure'); END;").unwrap();
    assert!(fixture.execute(&provider, "claim-failed").await.is_err());
    assert_eq!(provider.sends.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn readonly_public_reply_preview_uses_real_records_and_preserves_live_admission() {
    let fixture = Fixture::new().await;
    let input = PrepareContentReplyInput {
        creator_id: fixture.context.creator_id.clone(),
        target_platform_account_id: fixture.context.target.platform_account_id.clone(),
        operator_account_ref_id: fixture.context.operator_account.id.clone(),
        content_id: "  post-1  ".into(),
        reply_to_id: Some(" parent-1 ".into()),
    };
    let preview = fixture
        .platform
        .prepare_content_reply(input.clone())
        .await
        .unwrap();
    assert_eq!(preview.creator.creator.id, fixture.context.creator_id);
    assert_eq!(preview.target_platform_account.external_id, "123");
    assert_eq!(preview.content_id, "post-1");
    assert_eq!(preview.reply_to_id.as_deref(), Some("parent-1"));
    assert!(preview.confirmation_required);
    let claims: u64 = fixture
        .db
        .query_row(
            "SELECT COUNT(*) FROM idempotency_records WHERE action = 'content_reply_send_v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(claims, 0);
    fixture
        .db
        .execute("UPDATE account_refs SET active = 0", [])
        .unwrap();
    assert!(
        fixture
            .platform
            .prepare_content_reply(input.clone())
            .await
            .is_err()
    );
    fixture
        .db
        .execute("UPDATE account_refs SET active = 1", [])
        .unwrap();
    fixture
        .platform
        .creators
        .archive_creator(&fixture.context.creator_id)
        .unwrap();
    assert!(fixture.platform.prepare_content_reply(input).await.is_err());
}
