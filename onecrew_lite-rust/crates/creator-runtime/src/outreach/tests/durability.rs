use super::*;

fn database(directory: &tempfile::TempDir) -> Connection {
    Connection::open(directory.path().join("crm.db")).expect("fixture database")
}

fn audit_count(directory: &tempfile::TempDir, kind: &str) -> u64 {
    database(directory)
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE kind = ?1",
            [kind],
            |row| row.get(0),
        )
        .unwrap()
}

async fn verified_reply(
    runtime: &CreatorRuntime,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
    receipt: &str,
) -> CreatorOutreachLease {
    let lease = runtime.acquire_creator_outreach(key).unwrap();
    runtime
        .creator_outreach_claim(&lease, intent)
        .await
        .unwrap();
    runtime
        .creator_outreach_stage_accepted(
            &lease,
            intent,
            CreatorOutreachReceipt::ContentReply {
                external_reply_id: receipt.into(),
            },
        )
        .await
        .unwrap();
    let state = runtime
        .creator_outreach_stage_verified(
            &lease,
            intent,
            CreatorOutreachVerification::ContentReply {
                external_reply_id: receipt.into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(state.phase, CreatorOutreachPhase::RecordPending);
    assert!(!state.creator_recorded());
    lease
}

#[tokio::test]
async fn preparation_is_read_only_and_body_normalization_matches_the_frozen_context() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let route = route_context(&runtime, &ids).await;
    let mut context = runtime
        .creator_content_reply_context(PrepareContentReplyInput {
            creator_id: ids.0,
            target_platform_account_id: route.target.platform_account_id,
            operator_account_ref_id: ids.3,
            content_id: "  post-42  ".into(),
            reply_to_id: Some("  reply-1  ".into()),
        })
        .await
        .unwrap();
    let first =
        PreparedCreatorOutreachIntent::content_reply(context.clone(), "  hello 世界\n".into())
            .unwrap();
    context.content_id = "post-42".into();
    context.reply_to_id = Some("reply-1".into());
    let second =
        PreparedCreatorOutreachIntent::content_reply(context, "hello 世界".into()).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert_eq!(first.body(), Some("hello 世界"));
    let CreatorOutreachContext::Content(context) = first.context() else {
        panic!("content context")
    };
    assert_eq!(context.content_id, "post-42");
    assert_eq!(context.reply_to_id.as_deref(), Some("reply-1"));
    assert!(!format!("{first:?}").contains("hello"));
    assert_eq!(fact_count(&directory, "idempotency_records"), 0);
}

#[test]
fn version_one_fingerprint_has_a_fixed_encoding_and_covers_every_stable_fact() {
    let identity = CreatorOutreachIdentity {
        operation: CreatorOutreachOperation::DirectMessageSend,
        creator_id: CreatorId::new("creator-1"),
        operator_account_ref_id: AccountRefId::new("operator-1"),
        platform: Platform::X,
        target_platform_account_id: PlatformAccountId::new("account-1"),
        target_kind: TargetKind::User,
        external_target_id: "native-42".into(),
        contact_route_id: Some(CreatorContactRouteId::new("route-1")),
        resolved_target_id: Some(ResolvedTargetId::new("target-1")),
        conversation: Some(CreatorOutreachConversation {
            conversation_id: ConversationId::new("conversation-1"),
            provider_thread_id: "thread-1".into(),
        }),
        content_id: None,
        reply_to_id: None,
        relationship_action: None,
        allow_unfollowed_recipient: true,
    };
    let hash = crate::outreach::intents::fingerprint_v1(&identity, Some("hello 世界")).unwrap();
    assert_eq!(
        hash,
        "sha256:v1:a23016b2e860ac7f5d09235a02166be112e8e74ff1885fcc578e734188b6df65"
    );
    let mut changes = Vec::new();
    macro_rules! change {
        ($field:ident, $value:expr) => {{
            let mut next = identity.clone();
            next.$field = $value;
            changes.push(next);
        }};
    }
    change!(operation, CreatorOutreachOperation::RelationshipUpdate);
    change!(creator_id, CreatorId::new("creator-2"));
    change!(operator_account_ref_id, AccountRefId::new("operator-2"));
    change!(platform, Platform::Instagram);
    change!(
        target_platform_account_id,
        PlatformAccountId::new("account-2")
    );
    change!(target_kind, TargetKind::Business);
    change!(external_target_id, "native-43".into());
    change!(
        contact_route_id,
        Some(CreatorContactRouteId::new("route-2"))
    );
    change!(resolved_target_id, Some(ResolvedTargetId::new("target-2")));
    change!(
        conversation,
        Some(CreatorOutreachConversation {
            conversation_id: ConversationId::new("conversation-2"),
            provider_thread_id: "thread-1".into()
        })
    );
    change!(
        conversation,
        Some(CreatorOutreachConversation {
            conversation_id: ConversationId::new("conversation-1"),
            provider_thread_id: "thread-2".into()
        })
    );
    change!(content_id, Some("post-1".into()));
    change!(reply_to_id, Some("reply-1".into()));
    change!(relationship_action, Some(RelationshipMutation::Follow));
    change!(allow_unfollowed_recipient, false);
    for changed in changes {
        assert_ne!(
            crate::outreach::intents::fingerprint_v1(&changed, Some("hello 世界")).unwrap(),
            hash
        );
    }
    assert_ne!(
        crate::outreach::intents::fingerprint_v1(&identity, Some("hello world")).unwrap(),
        hash
    );
    assert_ne!(
        crate::outreach::intents::fingerprint_v1(&identity, None).unwrap(),
        hash
    );
}

#[tokio::test]
async fn claim_has_stable_attribution_and_hash_but_never_persists_the_body() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "PRIVATE MESSAGE BODY").await;
    let lease = runtime.acquire_creator_outreach("new-content").unwrap();
    let first = runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    assert!(first.is_new);
    assert_eq!(first.state.identity.creator_id, ids.0);
    assert_eq!(first.state.identity.operator_account_ref_id, ids.3);
    assert!(first.state.identity.contact_route_id.is_none());
    let second = runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    assert!(!second.is_new);
    assert_eq!(first.state.claimed_at, second.state.claimed_at);
    let (hash, bytes): (String, String) = database(&directory).query_row("SELECT request_fingerprint, result_json FROM idempotency_records WHERE key = 'new-content'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(hash, intent.fingerprint());
    assert!(!bytes.contains("PRIVATE MESSAGE BODY"));
    assert!(
        !serde_json::to_string(
            &runtime
                .creator_outreach_status("new-content")
                .await
                .unwrap()
        )
        .unwrap()
        .contains("PRIVATE MESSAGE BODY")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&bytes).unwrap()["version"],
        1
    );
    let changed = content_intent(&runtime, &ids, "different").await;
    assert_eq!(
        runtime
            .creator_outreach_claim(&lease, &changed)
            .await
            .unwrap_err(),
        CreatorOutreachError::IntentConflict
    );
    assert_eq!(fact_count(&directory, "idempotency_records"), 1);
}

#[tokio::test]
async fn receipt_transitions_reject_unproven_or_mismatched_effects() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = runtime.acquire_creator_outreach("receipt").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    assert_eq!(
        runtime
            .creator_outreach_stage_verified(
                &lease,
                &intent,
                CreatorOutreachVerification::ContentReply {
                    external_reply_id: "unproven".into()
                }
            )
            .await
            .unwrap_err(),
        CreatorOutreachError::ActionUnavailable
    );
    assert!(
        runtime
            .creator_outreach_stage_accepted(
                &lease,
                &intent,
                CreatorOutreachReceipt::ContentReply {
                    external_reply_id: " ".into()
                }
            )
            .await
            .is_err()
    );
    runtime
        .creator_outreach_stage_accepted(
            &lease,
            &intent,
            CreatorOutreachReceipt::ContentReply {
                external_reply_id: "reply-1".into(),
            },
        )
        .await
        .unwrap();
    assert!(
        runtime
            .creator_outreach_stage_verified(
                &lease,
                &intent,
                CreatorOutreachVerification::ContentReply {
                    external_reply_id: "reply-2".into()
                }
            )
            .await
            .is_err()
    );
    assert!(
        runtime
            .creator_outreach_refused(&lease, &intent)
            .await
            .is_err()
    );
    assert_eq!(
        action_state(&directory, "receipt").as_deref(),
        Some("remote_accepted")
    );
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
}

#[tokio::test]
async fn content_record_rollback_keeps_verification_and_retry_ignores_later_disable() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let initial_audits = fact_count(&directory, "audit_events");
    let intent = content_intent(&runtime, &ids, "private-public-reply").await;
    let lease = verified_reply(&runtime, "record-failure", &intent, "reply-1").await;
    database(&directory).execute_batch("CREATE TRIGGER fail_record BEFORE INSERT ON audit_events WHEN NEW.kind = 'content_reply_sent' BEGIN SELECT RAISE(ABORT, 'fixture record failure'); END;").unwrap();
    assert!(
        runtime
            .creator_outreach_record_content_reply(&lease)
            .await
            .is_err()
    );
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits);
    assert_eq!(audit_count(&directory, "content_reply_sent"), 0);
    assert_eq!(
        action_state(&directory, "record-failure").as_deref(),
        Some("remote_verified")
    );
    database(&directory).execute_batch("DROP TRIGGER fail_record; UPDATE creators SET archived_at = 123; UPDATE account_refs SET active = 0;").unwrap();
    assert!(runtime.validate_new_outreach_intent(&intent).await.is_err());
    assert!(
        !runtime
            .creator_outreach_claim(&lease, &intent)
            .await
            .unwrap()
            .is_new
    );
    let result = runtime
        .creator_outreach_record_content_reply(&lease)
        .await
        .unwrap();
    assert_eq!(result.external_reply_id, "reply-1");
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits + 1);
    assert_eq!(audit_count(&directory, "content_reply_sent"), 1);
    assert!(
        !serde_json::to_string(&result.audit_event)
            .unwrap()
            .contains("private-public-reply")
    );
    let replay = runtime.creator_outreach_recorded(&lease).await.unwrap();
    let CreatorOutreachRecord::ContentReply(replay) = replay else {
        panic!("reply record")
    };
    assert_eq!(replay.touchpoint.id, result.touchpoint.id);
    assert_eq!(replay.audit_event.id, result.audit_event.id);
    assert_eq!(
        action_state(&directory, "record-failure").as_deref(),
        Some("completed")
    );
}

#[tokio::test]
async fn a_second_content_key_reuses_real_provider_receipt_facts() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let initial_audits = fact_count(&directory, "audit_events");
    let intent = content_intent(&runtime, &ids, "body").await;
    let first = verified_reply(&runtime, "first", &intent, "same-reply").await;
    let first = runtime
        .creator_outreach_record_content_reply(&first)
        .await
        .unwrap();
    let second = verified_reply(&runtime, "second", &intent, "same-reply").await;
    let second = runtime
        .creator_outreach_record_content_reply(&second)
        .await
        .unwrap();
    assert_eq!(first.touchpoint.id, second.touchpoint.id);
    assert_eq!(first.audit_event.id, second.audit_event.id);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits + 1);
    assert_eq!(audit_count(&directory, "content_reply_sent"), 1);
    assert_eq!(fact_count(&directory, "idempotency_records"), 2);
}

#[tokio::test]
async fn relationship_record_rolls_back_all_facts_and_replays_the_verified_observation() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let initial_audits = fact_count(&directory, "audit_events");
    let intent = PreparedCreatorOutreachIntent::relationship(
        route_context(&runtime, &ids).await,
        RelationshipMutation::Follow,
    )
    .unwrap();
    let lease = runtime.acquire_creator_outreach("follow").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    assert!(
        runtime
            .creator_outreach_stage_verified(
                &lease,
                &intent,
                CreatorOutreachVerification::Relationship {
                    following: RelationshipValue::No,
                    can_message: RelationshipValue::Unknown,
                    checked_at: 10
                }
            )
            .await
            .is_err()
    );
    runtime
        .creator_outreach_stage_verified(
            &lease,
            &intent,
            CreatorOutreachVerification::Relationship {
                following: RelationshipValue::Yes,
                can_message: RelationshipValue::Unknown,
                checked_at: 11,
            },
        )
        .await
        .unwrap();
    database(&directory).execute_batch("CREATE TRIGGER fail_record BEFORE INSERT ON audit_events WHEN NEW.kind = 'relationship_updated' BEGIN SELECT RAISE(ABORT, 'fixture record failure'); END;").unwrap();
    assert!(
        runtime
            .creator_outreach_record_relationship(&lease)
            .await
            .is_err()
    );
    for table in ["account_relationships", "touchpoints"] {
        assert_eq!(fact_count(&directory, table), 0);
    }
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits);
    assert_eq!(audit_count(&directory, "relationship_updated"), 0);
    database(&directory)
        .execute_batch("DROP TRIGGER fail_record")
        .unwrap();
    let first = runtime
        .creator_outreach_record_relationship(&lease)
        .await
        .unwrap();
    runtime
        .creator_record_relationship_observation(
            ids.3,
            intent.identity().target_platform_account_id.clone(),
            RelationshipValue::No,
            RelationshipValue::No,
            99,
        )
        .await
        .unwrap();
    let CreatorOutreachRecord::Relationship(replay) =
        runtime.creator_outreach_recorded(&lease).await.unwrap()
    else {
        panic!("relationship record")
    };
    assert_eq!(replay.relationship.value, RelationshipValue::Yes);
    assert_eq!(replay.relationship.checked_at, 11);
    assert_eq!(replay.can_message.value, RelationshipValue::Unknown);
    assert_eq!(replay.touchpoint.id, first.touchpoint.id);
    assert_eq!(replay.audit_event.id, first.audit_event.id);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits + 1);
    assert_eq!(audit_count(&directory, "relationship_updated"), 1);
}

#[tokio::test]
async fn verified_message_records_only_matching_body_and_deduplicates_across_keys() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let initial_audits = fact_count(&directory, "audit_events");
    let intent = PreparedCreatorOutreachIntent::direct_message(
        route_context(&runtime, &ids).await,
        " secret body ".into(),
        true,
    )
    .unwrap();
    let mut first_ids = None;
    for key in ["message-first", "message-second"] {
        if key == "message-second" {
            database(&directory)
                .execute(
                    "UPDATE messages SET public_json = '{\"provider\":{\"readback_fact\":true}}'",
                    [],
                )
                .unwrap();
        }
        let lease = runtime.acquire_creator_outreach(key).unwrap();
        let claim = runtime
            .creator_outreach_claim(&lease, &intent)
            .await
            .unwrap();
        if key == "message-first" {
            assert!(
                runtime
                    .creator_outreach_stage_verified(
                        &lease,
                        &intent,
                        CreatorOutreachVerification::DirectMessage {
                            external_message_id: "message-1".into(),
                            provider_thread_id: "thread-1".into(),
                            sent_at: claim.state.claimed_at - 1
                        }
                    )
                    .await
                    .is_err()
            );
        }
        runtime
            .creator_outreach_stage_accepted(
                &lease,
                &intent,
                CreatorOutreachReceipt::DirectMessage {
                    external_message_id: "message-1".into(),
                    provider_thread_id: None,
                },
            )
            .await
            .unwrap();
        runtime
            .creator_outreach_stage_verified(
                &lease,
                &intent,
                CreatorOutreachVerification::DirectMessage {
                    external_message_id: "message-1".into(),
                    provider_thread_id: "thread-1".into(),
                    sent_at: 100,
                },
            )
            .await
            .unwrap();
        let mut message = CreatorOutreachMessageRecord {
            external_message_id: "message-1".into(),
            provider_thread_id: "thread-1".into(),
            body: "wrong body".into(),
            sent_at: 100,
        };
        assert_eq!(
            runtime
                .creator_outreach_record_message(&lease, message.clone())
                .await
                .unwrap_err(),
            CreatorOutreachError::IntentConflict
        );
        message.body = "secret body".into();
        if key == "message-first" {
            database(&directory).execute_batch("CREATE TRIGGER fail_record BEFORE INSERT ON audit_events WHEN NEW.kind = 'direct_message_sent' BEGIN SELECT RAISE(ABORT, 'fixture record failure'); END;").unwrap();
            assert!(
                runtime
                    .creator_outreach_record_message(&lease, message.clone())
                    .await
                    .is_err()
            );
            for table in ["conversations", "messages", "touchpoints"] {
                assert_eq!(fact_count(&directory, table), 0);
            }
            assert_eq!(fact_count(&directory, "audit_events"), initial_audits);
            assert_eq!(audit_count(&directory, "direct_message_sent"), 0);
            database(&directory)
                .execute_batch("DROP TRIGGER fail_record")
                .unwrap();
        }
        let result = runtime
            .creator_outreach_record_message(&lease, message)
            .await
            .unwrap();
        let ids = (
            result.message.id.clone(),
            result.touchpoint.id.clone(),
            result.audit_event.id.clone(),
        );
        if let Some(first) = &first_ids {
            assert_eq!(first, &ids);
        } else {
            first_ids = Some(ids);
        }
        assert_eq!(result.message.body, "secret body");
        if key == "message-second" {
            assert_eq!(
                result.message.public_data["provider"]["readback_fact"],
                true
            );
        }
        assert!(
            !serde_json::to_string(&result.audit_event)
                .unwrap()
                .contains("secret body")
        );
        let CreatorOutreachRecord::DirectMessage(replay) =
            runtime.creator_outreach_recorded(&lease).await.unwrap()
        else {
            panic!("message record")
        };
        assert_eq!(replay.message.id, result.message.id);
    }
    for table in ["conversations", "messages", "touchpoints"] {
        assert_eq!(fact_count(&directory, table), 1);
    }
    assert_eq!(fact_count(&directory, "audit_events"), initial_audits + 1);
    assert_eq!(audit_count(&directory, "direct_message_sent"), 1);
    let bodies: u64 = database(&directory).query_row("SELECT COUNT(*) FROM idempotency_records WHERE result_json LIKE '%secret body%' OR request_fingerprint LIKE '%secret body%'", [], |row| row.get(0)).unwrap();
    assert_eq!(bodies, 0);
}

#[tokio::test]
async fn null_route_claim_blocks_creator_and_operator_deletion_until_refused() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = runtime.acquire_creator_outreach("null-route").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    assert!(
        runtime
            .repository
            .delete_creator(DeleteCreatorInput {
                creator_id: ids.0.clone()
            })
            .is_err()
    );
    assert!(matches!(
        runtime
            .repository
            .begin_operator_account_disconnect("disconnect", &ids.3),
        Err(AccountLifecycleClaimError::OutreachInProgress)
    ));
    assert!(runtime.get_creator(&ids.0).is_ok());
    assert!(runtime.repository.get_account_ref(&ids.3).unwrap().active);
    assert_eq!(fact_count(&directory, "idempotency_records"), 1);
    runtime
        .creator_outreach_refused(&lease, &intent)
        .await
        .unwrap();
    runtime
        .repository
        .delete_creator(DeleteCreatorInput {
            creator_id: ids.0.clone(),
        })
        .expect("definite refusal no longer blocks Creator deletion");
}

#[tokio::test]
async fn a_new_claim_rechecks_creator_and_operator_after_preparation() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    runtime.validate_new_outreach_intent(&intent).await.unwrap();
    database(&directory)
        .execute("UPDATE account_refs SET active = 0", [])
        .unwrap();
    let lease = runtime
        .acquire_creator_outreach("disabled-after-prepare")
        .unwrap();
    assert_eq!(
        runtime
            .creator_outreach_claim(&lease, &intent)
            .await
            .unwrap_err(),
        CreatorOutreachError::ScopeMismatch
    );
    assert_eq!(fact_count(&directory, "idempotency_records"), 0);
    database(&directory)
        .execute("UPDATE account_refs SET active = 1", [])
        .unwrap();
    runtime
        .repository
        .delete_creator(DeleteCreatorInput {
            creator_id: ids.0.clone(),
        })
        .unwrap();
    assert!(
        runtime
            .creator_outreach_claim(&lease, &intent)
            .await
            .is_err()
    );
    assert_eq!(fact_count(&directory, "idempotency_records"), 0);
}

#[tokio::test]
async fn route_claim_blocks_a_prepared_route_deletion_and_keeps_the_scope_column() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = PreparedCreatorOutreachIntent::direct_message(
        route_context(&runtime, &ids).await,
        "hello".into(),
        true,
    )
    .unwrap();
    let lease = runtime.acquire_creator_outreach("route-owned").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    let route: String = database(&directory)
        .query_row(
            "SELECT contact_route_id FROM idempotency_records WHERE key = 'route-owned'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(route, ids.1.as_str());
    let blocked = runtime
        .repository
        .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
            creator_id: ids.0.clone(),
            contact_route_id: ids.1.clone(),
        })
        .unwrap();
    assert!(matches!(
        blocked,
        PrepareCreatorContactRouteDeleteResult::Blocked {
            reason: CreatorContactRouteDeleteBlockReason::InFlightExternalAction,
            ..
        }
    ));
    runtime
        .creator_outreach_refused(&lease, &intent)
        .await
        .unwrap();
    let ready = runtime
        .repository
        .prepare_creator_contact_route_delete(PrepareCreatorContactRouteDeleteInput {
            creator_id: ids.0,
            contact_route_id: ids.1,
        })
        .unwrap();
    assert!(matches!(
        ready,
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired { .. }
    ));
}

#[tokio::test]
async fn shared_email_completion_cannot_complete_a_new_outreach_verified_row() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = verified_reply(&runtime, "versioned", &intent, "reply-1").await;
    assert!(
        runtime
            .repository
            .complete_action(
                "versioned",
                "content_reply_send_v1",
                serde_json::json!({"unsafe":true})
            )
            .is_err()
    );
    assert_eq!(
        action_state(&directory, "versioned").as_deref(),
        Some("remote_verified")
    );
    runtime
        .creator_outreach_record_content_reply(&lease)
        .await
        .unwrap();
    runtime
        .repository
        .begin_action("email-shared", "email_send")
        .unwrap();
    let receipt = serde_json::json!({"provider_message_id":"existing-email-receipt"});
    runtime
        .repository
        .complete_action("email-shared", "email_send", receipt.clone())
        .unwrap();
    assert!(
        matches!(runtime.repository.find_action("email-shared", "email_send").unwrap(), Some(IdempotencyClaim::Completed(value)) if value == receipt)
    );
}

#[tokio::test]
async fn incomplete_completed_envelopes_are_rejected_without_erasing_the_row() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = runtime.acquire_creator_outreach("malformed-phase").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    database(&directory).execute("UPDATE idempotency_records SET state='completed', result_json=json_set(result_json,'$.state.phase','completed') WHERE key='malformed-phase'", []).unwrap();
    assert_eq!(
        runtime
            .creator_outreach_status("malformed-phase")
            .await
            .unwrap_err(),
        CreatorOutreachError::Storage
    );
    assert_eq!(fact_count(&directory, "idempotency_records"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
}

#[tokio::test]
async fn malformed_new_envelope_fails_closed_for_status_and_deletion() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = runtime.acquire_creator_outreach("corrupt").unwrap();
    runtime
        .creator_outreach_claim(&lease, &intent)
        .await
        .unwrap();
    database(&directory).execute("UPDATE idempotency_records SET result_json = '{\"version\":1,\"state\":{}}' WHERE key = 'corrupt'", []).unwrap();
    assert_eq!(
        runtime
            .creator_outreach_status("corrupt")
            .await
            .unwrap_err(),
        CreatorOutreachError::Storage
    );
    assert!(
        runtime
            .repository
            .delete_creator(DeleteCreatorInput {
                creator_id: ids.0.clone()
            })
            .is_err()
    );
    assert!(matches!(
        runtime
            .repository
            .begin_operator_account_disconnect("disconnect", &ids.3),
        Err(AccountLifecycleClaimError::OutreachInProgress)
    ));
}

#[tokio::test]
async fn retired_action_states_are_read_only_safe_markers_and_keep_original_bytes() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let connection = database(&directory);
    // Frozen released bytes, deliberately not built with retired production DTOs.
    const LEGACY: &str =
        r#"{"body":"legacy private body","remote_receipt":{"external_message_id":"old-message"}}"#;
    const MARKED: &str =
        r#"{"kind":"retired_action_unresolved","original":{"body":"legacy private body"}}"#;
    for action in [
        "relationship_update",
        "content_reply_send",
        "direct_message_send",
    ] {
        for phase in [
            "in_progress",
            "remote_started",
            "remote_accepted",
            "remote_verified",
            "completed",
            "failed",
        ] {
            for (marked, bytes) in [(false, LEGACY), (true, MARKED)] {
                let key = format!("{action}-{phase}-{marked}");
                connection.execute("INSERT INTO idempotency_records(key, action, state, result_json, request_fingerprint, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, 'released-fingerprint', 1, 2)", rusqlite::params![key,action,phase,bytes]).unwrap();
                let result = runtime
                    .creator_outreach_status(&key)
                    .await
                    .unwrap()
                    .unwrap();
                let CreatorOutreachStatus::Retired {
                    phase: status,
                    marked: actual,
                    ..
                } = &result
                else {
                    panic!("retired status")
                };
                assert_eq!(*actual, marked);
                assert_eq!(
                    *status,
                    if marked {
                        CreatorRetiredOutreachPhase::RetiredActionUnresolved
                    } else {
                        match phase {
                            "completed" => CreatorRetiredOutreachPhase::Completed,
                            "failed" => CreatorRetiredOutreachPhase::Failed,
                            _ => CreatorRetiredOutreachPhase::RetiredActionUnresolved,
                        }
                    }
                );
                assert!(
                    !serde_json::to_string(&result)
                        .unwrap()
                        .contains("legacy private body")
                );
                let stored: String = connection
                    .query_row(
                        "SELECT result_json FROM idempotency_records WHERE key = ?1",
                        [&key],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(stored.as_bytes(), bytes.as_bytes());
            }
        }
    }
}

#[tokio::test]
async fn retirement_marker_keeps_failed_database_rows_unresolved_without_exposing_the_old_payload()
{
    let (directory, runtime, _) = runtime(TestGateway::default());
    let connection = database(&directory);
    const MARKED: &str = r#"{"kind":"retired_action_unresolved","original":{"state":"remote_accepted","result_json":{"body":"private legacy message","provider_receipt":"old-receipt"}}}"#;
    connection.execute(
        "INSERT INTO idempotency_records(key, action, state, result_json, created_at, updated_at) VALUES('retired-failed', 'direct_message_send', 'failed', ?1, 1, 2)",
        [MARKED],
    ).unwrap();
    let status = runtime
        .creator_outreach_status("retired-failed")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        &status,
        CreatorOutreachStatus::Retired {
            operation: CreatorOutreachOperation::DirectMessageSend,
            phase: CreatorRetiredOutreachPhase::RetiredActionUnresolved,
            marked: true,
        }
    ));
    let encoded = serde_json::to_string(&status).unwrap();
    assert!(!encoded.contains("private legacy message"));
    assert!(!encoded.contains("old-receipt"));
    let (state, payload): (String, String) = connection
        .query_row(
            "SELECT state, result_json FROM idempotency_records WHERE key='retired-failed'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "failed");
    assert_eq!(payload.as_bytes(), MARKED.as_bytes());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_claim_keeps_its_lease_until_the_admitted_write_finishes() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let intent = content_intent(&runtime, &ids, "body").await;
    let lease = runtime.acquire_creator_outreach("cancelled-claim").unwrap();
    let (started, release) = runtime.install_next_repository_write_barrier();
    let worker = runtime.clone();
    let task = tokio::spawn(async move { worker.creator_outreach_claim(&lease, &intent).await });
    started.await.unwrap();
    assert_runtime_health().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(runtime.acquire_creator_outreach("cancelled-claim").is_err());
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(lease) = runtime.acquire_creator_outreach("cancelled-claim") {
                drop(lease);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("admitted claim released its guard");
    assert_eq!(
        action_state(&directory, "cancelled-claim").as_deref(),
        Some("remote_started")
    );
}
