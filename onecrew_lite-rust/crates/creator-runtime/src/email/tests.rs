use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::*;
use crate::{
    Confidence, CreateCreatorInput, CreatorGateways, EmailDraftGetInput, EmailDraftMutationInput,
    EmailDraftState, EmailGateway, EmailTemplateId, LinkState, ListEmailDraftsInput, MailboxRefId,
    ObservationSourceInput, Platform, PlatformAccountObservationInput, PrepareEmailInput,
    RenderEmailTemplateInput, SaveEmailDraftInput, SaveEmailTemplateInput, SendEmailDraftInput,
    SendEmailInput, SendManualEmailInput, SourceKind, TargetKind, UpsertPlatformAccountInput,
};
use onecrew_email_contracts::{
    EmailBatchChunkResult as ProviderEmailBatchChunkResult, EmailBatchItemOutcome,
    EmailBatchItemResult as ProviderEmailBatchItemResult, EmailBatchRequest, EmailBatchResult,
    PlatformEmailMessage, PlatformEmailPage, PlatformEmailReceipt, PlatformEmailRequest,
    PlatformEmailSendError, PlatformEmailSummary, PlatformSentEmailPage, PlatformSentEmailSummary,
};

type ScriptedOutcomes<T> =
    Arc<Mutex<VecDeque<Result<T, onecrew_email_contracts::PlatformEmailAttachmentError>>>>;

#[derive(Clone, Default)]
pub(crate) struct TestGateway {
    /// Scripted Receiving pages consumed in order; when empty, the flat
    /// `received` list serves one final page.
    received_pages: Arc<Mutex<VecDeque<PlatformEmailPage>>>,
    list_cursors: Arc<Mutex<Vec<Option<String>>>>,
    received_mailbox_calls: Arc<Mutex<Vec<String>>>,
    received_list_errors: Arc<Mutex<VecDeque<String>>>,
    received_list_panics: Arc<AtomicUsize>,
    received_list_gate: Option<Arc<tokio::sync::Semaphore>>,
    received_by_mailbox: Arc<HashMap<String, Vec<PlatformEmailMessage>>>,
    detail_calls: Arc<Mutex<Vec<String>>>,
    failing_details: Arc<Mutex<HashSet<String>>>,
    /// Scripted sent pages consumed in order; when empty, `sent_history`
    /// serves one final page.
    sent_pages: Arc<Mutex<VecDeque<PlatformSentEmailPage>>>,
    sent_history: Arc<Vec<PlatformEmailMessage>>,
    sent_list_cursors: Arc<Mutex<Vec<Option<String>>>>,
    sent_list_errors: Arc<Mutex<VecDeque<String>>>,
    sent_detail_calls: Arc<Mutex<Vec<String>>>,
    failing_sent_details: Arc<Mutex<HashSet<String>>>,
    sent_detail_batch_error: Arc<Mutex<Option<String>>>,
    received_attachment_outcomes:
        ScriptedOutcomes<onecrew_email_contracts::PlatformAttachmentAccess>,
    sent_attachment_lists: ScriptedOutcomes<Vec<onecrew_email_contracts::PlatformAttachmentAccess>>,
    attachment_downloads: ScriptedOutcomes<Vec<u8>>,
    send_calls: Arc<AtomicUsize>,
    provider_deliveries: Arc<AtomicUsize>,
    delivered_idempotency_keys: Arc<Mutex<HashSet<String>>>,
    send_provider_message_ids: Arc<Mutex<VecDeque<String>>>,
    sent_readback_calls: Arc<AtomicUsize>,
    received: Arc<Vec<PlatformEmailMessage>>,
    send_error: Arc<Option<PlatformEmailSendError>>,
    next_ambiguous_after_delivery: Arc<Mutex<Option<String>>>,
    sent_readback: Arc<Option<PlatformEmailMessage>>,
    next_sent_readback: Arc<Mutex<Option<Result<PlatformEmailMessage, String>>>>,
    last_email_request: Arc<Mutex<Option<PlatformEmailRequest>>>,
    batch_calls: Arc<AtomicUsize>,
    next_batch_result: Arc<Mutex<Option<Result<EmailBatchResult, PlatformEmailSendError>>>>,
}

#[async_trait]
impl EmailGateway for TestGateway {
    async fn send_email(
        &self,
        _: &MailboxRefId,
        request: PlatformEmailRequest,
    ) -> Result<PlatformEmailReceipt, PlatformEmailSendError> {
        self.send_calls.fetch_add(1, Ordering::SeqCst);
        let idempotency_key = request.idempotency_key.clone();
        *self.last_email_request.lock().unwrap() = Some(request);
        if let Some(error) = self.send_error.as_ref() {
            return Err(error.clone());
        }
        if self
            .delivered_idempotency_keys
            .lock()
            .unwrap()
            .insert(idempotency_key)
        {
            self.provider_deliveries.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(message) = self.next_ambiguous_after_delivery.lock().unwrap().take() {
            return Err(PlatformEmailSendError::Ambiguous { message });
        }
        let external_message_id = self
            .send_provider_message_ids
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| "sent-1".into());
        Ok(PlatformEmailReceipt {
            provider: "resend".into(),
            external_message_id: Some(external_message_id),
            accepted: true,
        })
    }
    async fn send_many_email(
        &self,
        _: &MailboxRefId,
        request: EmailBatchRequest,
    ) -> Result<EmailBatchResult, PlatformEmailSendError> {
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(result) = self.next_batch_result.lock().unwrap().take() {
            return result;
        }
        let payload = serde_json::to_vec(&request.items).map_err(|error| {
            PlatformEmailSendError::Validation {
                message: error.to_string(),
            }
        })?;
        let logical_fingerprint = Sha256::digest(payload)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let outcomes = request
            .items
            .iter()
            .enumerate()
            .map(|(input_index, _)| ProviderEmailBatchItemResult {
                input_index,
                outcome: EmailBatchItemOutcome::Accepted {
                    provider_message_id: format!("batch-sent-{input_index}"),
                },
            })
            .collect::<Vec<_>>();
        Ok(EmailBatchResult {
            logical_fingerprint,
            chunks: vec![ProviderEmailBatchChunkResult {
                chunk_index: 0,
                idempotency_key: format!("{}:chunk-0", request.idempotency_key),
                outcomes: outcomes.clone(),
            }],
            outcomes,
        })
    }
    async fn sent_email(
        &self,
        _: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        self.sent_readback_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(result) = self.next_sent_readback.lock().unwrap().take() {
            return result;
        }
        if let Some(message) = self.sent_readback.as_ref() {
            return Ok(message.clone());
        }
        Ok(PlatformEmailMessage {
            provider_message_id: provider_message_id.into(),
            internet_message_id: Some("<sent-1@example.com>".into()),
            from: "Reach <outreach@example.com>".into(),
            to: vec!["creator@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            subject: "Partnership".into(),
            text: Some("Hello".into()),
            html: None,
            headers: serde_json::json!({ "Message-Id": "<sent-1@example.com>" }),
            last_event: "delivered".into(),
            created_at: "2026-08-05T10:00:00Z".into(),
            inbound: false,
            attachments: Vec::new(),
        })
    }
    async fn outreach_reply_ids(&self, _: &MailboxRefId, sent_message_id: &str) -> Result<Vec<String>, String> {
        assert_eq!(sent_message_id, "sent-1");
        Ok(self.received.iter().map(|m| m.provider_message_id.clone()).collect())
    }

    async fn received_emails(
        &self,
        credential_id: &MailboxRefId,
        after: Option<&str>,
        _: u16,
    ) -> Result<PlatformEmailPage, String> {
        self.received_mailbox_calls
            .lock()
            .unwrap()
            .push(credential_id.as_str().to_owned());
        self.list_cursors
            .lock()
            .unwrap()
            .push(after.map(str::to_owned));
        if let Some(gate) = &self.received_list_gate {
            gate.acquire().await.unwrap().forget();
        }
        if self
            .received_list_panics
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            panic!("test-only secret provider panic payload");
        }
        if let Some(error) = self.received_list_errors.lock().unwrap().pop_front() {
            return Err(error);
        }
        if let Some(page) = self.received_pages.lock().unwrap().pop_front() {
            return Ok(page);
        }
        let received = self
            .received_by_mailbox
            .get(credential_id.as_str())
            .unwrap_or(self.received.as_ref());
        let messages: Vec<PlatformEmailSummary> = received
            .iter()
            .map(|message| PlatformEmailSummary {
                provider_message_id: message.provider_message_id.clone(),
                created_at: message.created_at.clone(),
            })
            .collect();
        let bounds = messages
            .first()
            .zip(messages.last())
            .map(
                |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                    newest_provider_message_id: newest.provider_message_id.clone(),
                    newest_created_at: newest.created_at.clone(),
                    oldest_created_at: oldest.created_at.clone(),
                },
            );
        Ok(PlatformEmailPage {
            messages,
            has_more: false,
            next_cursor: Some("next".into()),
            bounds,
        })
    }
    async fn received_email(
        &self,
        credential_id: &MailboxRefId,
        provider_message_id: &str,
    ) -> Result<PlatformEmailMessage, String> {
        self.received_by_mailbox
            .get(credential_id.as_str())
            .unwrap_or(self.received.as_ref())
            .iter()
            .find(|message| message.provider_message_id == provider_message_id)
            .cloned()
            .ok_or_else(|| "missing received email".into())
    }
    async fn received_email_details(
        &self,
        credential_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        let mut outcomes = Vec::new();
        for id in provider_message_ids {
            self.detail_calls.lock().unwrap().push(id.clone());
            if self.failing_details.lock().unwrap().contains(id) {
                outcomes.push((id.clone(), Err("scripted detail failure".to_owned())));
                continue;
            }
            outcomes.push((id.clone(), self.received_email(credential_id, id).await));
        }
        Ok(outcomes)
    }
    async fn sent_emails(
        &self,
        _: &MailboxRefId,
        after: Option<&str>,
        _: u16,
    ) -> Result<PlatformSentEmailPage, String> {
        self.sent_list_cursors
            .lock()
            .unwrap()
            .push(after.map(str::to_owned));
        if let Some(error) = self.sent_list_errors.lock().unwrap().pop_front() {
            return Err(error);
        }
        if let Some(page) = self.sent_pages.lock().unwrap().pop_front() {
            return Ok(page);
        }
        let messages: Vec<PlatformSentEmailSummary> = self
            .sent_history
            .iter()
            .map(|message| PlatformSentEmailSummary {
                provider_message_id: message.provider_message_id.clone(),
                internet_message_id: message.internet_message_id.clone(),
                from: message.from.clone(),
                to: message.to.clone(),
                cc: message.cc.clone(),
                bcc: message.bcc.clone(),
                reply_to: message.reply_to.clone(),
                subject: message.subject.clone(),
                last_event: message.last_event.clone(),
                created_at: message.created_at.clone(),
            })
            .collect();
        let bounds = messages
            .first()
            .zip(messages.last())
            .map(
                |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                    newest_provider_message_id: newest.provider_message_id.clone(),
                    newest_created_at: newest.created_at.clone(),
                    oldest_created_at: oldest.created_at.clone(),
                },
            );
        Ok(PlatformSentEmailPage {
            messages,
            has_more: false,
            next_cursor: None,
            bounds,
        })
    }
    async fn received_attachment_access(
        &self,
        _: &MailboxRefId,
        _: &str,
        _: &str,
    ) -> Result<
        onecrew_email_contracts::PlatformAttachmentAccess,
        onecrew_email_contracts::PlatformEmailAttachmentError,
    > {
        self.received_attachment_outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(
                    onecrew_email_contracts::PlatformEmailAttachmentError::Failed {
                        message: "unscripted received attachment access".into(),
                    },
                )
            })
    }
    async fn sent_attachments_access(
        &self,
        _: &MailboxRefId,
        _: &str,
    ) -> Result<
        Vec<onecrew_email_contracts::PlatformAttachmentAccess>,
        onecrew_email_contracts::PlatformEmailAttachmentError,
    > {
        self.sent_attachment_lists
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(
                    onecrew_email_contracts::PlatformEmailAttachmentError::Failed {
                        message: "unscripted sent attachment access".into(),
                    },
                )
            })
    }
    async fn download_email_attachment(
        &self,
        _: &MailboxRefId,
        _: &onecrew_email_contracts::PlatformAttachmentAccess,
        limit: usize,
    ) -> Result<Vec<u8>, onecrew_email_contracts::PlatformEmailAttachmentError> {
        let outcome = self
            .attachment_downloads
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(
                    onecrew_email_contracts::PlatformEmailAttachmentError::Failed {
                        message: "unscripted attachment download".into(),
                    },
                )
            });
        match outcome {
            Ok(bytes) if bytes.len() > limit => Err(
                onecrew_email_contracts::PlatformEmailAttachmentError::TooLarge {
                    message: format!("{} bytes exceeds {limit}", bytes.len()),
                },
            ),
            other => other,
        }
    }
    async fn sent_email_details(
        &self,
        _: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<(String, Result<PlatformEmailMessage, String>)>, String> {
        if let Some(error) = self.sent_detail_batch_error.lock().unwrap().take() {
            return Err(error);
        }
        let mut outcomes = Vec::new();
        for id in provider_message_ids {
            self.sent_detail_calls.lock().unwrap().push(id.clone());
            if self.failing_sent_details.lock().unwrap().contains(id) {
                outcomes.push((id.clone(), Err("scripted sent detail failure".to_owned())));
                continue;
            }
            let detail = self
                .sent_history
                .iter()
                .find(|message| &message.provider_message_id == id)
                .cloned()
                .ok_or_else(|| "missing sent email".to_owned());
            outcomes.push((id.clone(), detail));
        }
        Ok(outcomes)
    }
}

pub(crate) fn runtime(
    gateway: TestGateway,
) -> (tempfile::TempDir, crate::CreatorRuntime, TestGateway) {
    let directory = tempfile::tempdir().unwrap();
    let gateway = Arc::new(gateway);
    let gateways = CreatorGateways::default().with_email(gateway.clone());
    let runtime = crate::CreatorRuntime::open(directory.path(), gateways).unwrap();
    (directory, runtime, gateway.as_ref().clone())
}

fn reopen_runtime(directory: &tempfile::TempDir, gateway: &TestGateway) -> crate::CreatorRuntime {
    let gateway = Arc::new(gateway.clone());
    let gateways = CreatorGateways::default().with_email(gateway.clone());
    crate::CreatorRuntime::open(directory.path(), gateways).unwrap()
}

fn creator_email_route(
    runtime: &crate::CreatorRuntime,
) -> (crate::CreatorId, crate::CreatorContactRouteId) {
    let creator = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator".into(),
            notes: None,
        })
        .unwrap();
    let locator = runtime
        .repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            normalized_value: "creator@example.com".into(),
            display_value: "creator@example.com".into(),
        })
        .unwrap();
    let route = runtime
        .repository
        .link_creator_contact_route(&creator.creator.id, &locator.id, Confidence::Exact)
        .unwrap();
    (creator.creator.id, route.id)
}

fn mailbox(runtime: &crate::CreatorRuntime, capabilities: Vec<EmailCapability>) -> MailboxDetail {
    mailbox_with_id(
        runtime,
        "11111111-1111-4111-8111-111111111111",
        "outreach@example.com",
        capabilities,
    )
}

fn mailbox_with_id(
    runtime: &crate::CreatorRuntime,
    id: &str,
    email_address: &str,
    capabilities: Vec<EmailCapability>,
) -> MailboxDetail {
    runtime
        .sync_mailbox_ref(MailboxRefInput {
            id: crate::MailboxRefId::new(id),
            provider: "resend".into(),
            label: "Outreach".into(),
            email_address: email_address.into(),
            display_name: Some("Reach".into()),
            capabilities,
            available: true,
            last_error: None,
        })
        .unwrap()
}

fn send_input(
    creator_id: crate::CreatorId,
    contact_route_id: crate::CreatorContactRouteId,
    mailbox_ref_id: crate::MailboxRefId,
    idempotency_key: &str,
) -> SendEmailInput {
    SendEmailInput {
        creator_id,
        contact_route_id,
        mailbox_ref_id,
        reply_to_email_id: None,
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: idempotency_key.into(),
    }
}

struct CreatorReplyFixture {
    directory: tempfile::TempDir,
    runtime: crate::CreatorRuntime,
    gateway: TestGateway,
    input: SendEmailInput,
    thread_id: crate::EmailThreadId,
    provider_thread_key: String,
    seed_email_id: crate::EmailId,
    inbound_email_id: crate::EmailId,
}

async fn creator_reply_fixture(
    reply_key: &str,
    repeat_reply_receipt: bool,
    mark_read: bool,
) -> CreatorReplyFixture {
    let inbound = PlatformEmailMessage {
        provider_message_id: "creator-inbound-1".into(),
        internet_message_id: Some("<creator-inbound-1@example.net>".into()),
        from: "Creator <creator@example.com>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Re: Partnership".into(),
        text: Some("Reply".into()),
        html: None,
        headers: serde_json::json!({
            "Message-Id": "<creator-inbound-1@example.net>",
            "References": "<creator-seed@example.com>",
            "In-Reply-To": "<creator-seed@example.com>"
        }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let mut provider_message_ids = VecDeque::from([
        "creator-seed-provider".to_owned(),
        "creator-reply-provider".to_owned(),
    ]);
    if repeat_reply_receipt {
        provider_message_ids.push_back("creator-reply-provider".to_owned());
    }
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        send_provider_message_ids: Arc::new(Mutex::new(provider_message_ids)),
        next_sent_readback: Arc::new(Mutex::new(Some(Ok(sent_readback_message(
            "creator-seed-provider",
            "<creator-seed@example.com>",
        ))))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
    );
    let seed = runtime
        .send_email(send_input(
            creator_id.clone(),
            contact_route_id.clone(),
            mailbox.mailbox_ref_id.clone(),
            &format!("{reply_key}-seed"),
        ))
        .await
        .unwrap();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    if mark_read {
        runtime
            .mark_email_thread_read(EmailThreadMutationInput {
                thread_id: seed.thread.id.clone(),
            })
            .unwrap();
    }
    let thread = runtime
        .get_email_thread(EmailThreadGetInput {
            thread_id: seed.thread.id.clone(),
            cursor: None,
            limit: None,
        })
        .unwrap();
    let inbound_email = thread
        .messages
        .iter()
        .find(|message| message.provider_message_id == "creator-inbound-1")
        .unwrap();
    assert_eq!(
        inbound_email.delivery_status,
        if mark_read { "read" } else { "unread" }
    );
    assert_eq!(thread.messages.len(), 2);
    assert_eq!(thread.thread.creator_id.as_ref(), Some(&creator_id));
    assert_eq!(
        thread.thread.contact_route_id.as_ref(),
        Some(&contact_route_id)
    );
    let mut input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        reply_key,
    );
    input.reply_to_email_id = Some(inbound_email.id.clone());
    CreatorReplyFixture {
        directory,
        runtime,
        gateway,
        input,
        thread_id: thread.thread.id,
        provider_thread_key: thread.thread.provider_thread_key,
        seed_email_id: seed.email.id,
        inbound_email_id: inbound_email.id.clone(),
    }
}

fn sent_readback_message(
    provider_message_id: &str,
    internet_message_id: &str,
) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(internet_message_id.into()),
        from: "Reach <outreach@example.com>".into(),
        to: vec!["creator@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Partnership".into(),
        text: Some("Hello".into()),
        html: None,
        headers: serde_json::json!({ "Message-Id": internet_message_id }),
        last_event: "delivered".into(),
        created_at: "2026-08-05T12:00:00Z".into(),
        inbound: false,
        attachments: Vec::new(),
    }
}

fn assert_creator_reply_facts(
    runtime: &crate::CreatorRuntime,
    directory: &tempfile::TempDir,
    thread_id: &crate::EmailThreadId,
    provider_thread_key: &str,
    seed_email_id: &crate::EmailId,
    inbound_email_id: &crate::EmailId,
    reply_email_id: &crate::EmailId,
) {
    let thread = runtime
        .get_email_thread(EmailThreadGetInput {
            thread_id: thread_id.clone(),
            cursor: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(&thread.thread.id, thread_id);
    assert_eq!(thread.thread.provider_thread_key, provider_thread_key);
    assert_eq!(thread.messages.len(), 3);
    for (email_id, provider_message_id) in [
        (seed_email_id, "creator-seed-provider"),
        (inbound_email_id, "creator-inbound-1"),
        (reply_email_id, "creator-reply-provider"),
    ] {
        let message = thread
            .messages
            .iter()
            .find(|message| &message.id == email_id)
            .unwrap();
        assert_eq!(message.provider_message_id, provider_message_id);
    }
    assert_eq!(fact_count(directory, "email_threads"), 1);
    assert_eq!(fact_count(directory, "emails"), 3);
    assert_eq!(fact_count(directory, "touchpoints"), 2);
    assert_eq!(fact_count(directory, "audit_events"), 2);
}

fn action_state(directory: &tempfile::TempDir, key: &str) -> Option<String> {
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT state FROM idempotency_records WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
}

fn assert_current_claim_uses_persisted_shape(directory: &tempfile::TempDir, key: &str) {
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        crate::CREATOR_SCHEMA_VERSION
    );
    let (action, state, result_json) = connection
        .query_row(
            "SELECT action, state, result_json FROM idempotency_records WHERE key = ?1",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&result_json).unwrap();
    let mut persisted_mailbox_count = 0;
    for path in [
        "/mailbox",
        "/context/preparation/mailbox",
        "/preparation/mailbox",
    ] {
        let Some(mailbox) = value.pointer(path) else {
            continue;
        };
        persisted_mailbox_count += 1;
        assert!(mailbox.get("id").is_some());
        assert!(mailbox.get("mailbox_ref_id").is_none());
        assert!(mailbox.get("issues").is_none());
    }
    assert!(
        persisted_mailbox_count > 0
            || (action == "email_draft_send"
                && matches!(state.as_str(), "remote_verified" | "completed"))
    );
}

fn block_remote_accepted_stage(directory: &tempfile::TempDir) {
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_remote_accepted_stage
             BEFORE UPDATE OF state ON idempotency_records
             WHEN NEW.state = 'remote_accepted'
             BEGIN SELECT RAISE(ABORT, 'remote accepted stage blocked'); END;",
        )
        .unwrap();
}

fn unblock_remote_accepted_stage(directory: &tempfile::TempDir) {
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_remote_accepted_stage;")
        .unwrap();
}

fn fact_count(directory: &tempfile::TempDir, table: &str) -> u64 {
    let query = format!("SELECT COUNT(*) FROM {table}");
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(&query, [], |row| row.get(0))
        .unwrap()
}

mod attachment_tests;
mod compose_tests;
mod draft_tests;
mod mailbox_tests;
mod message_tests;
mod received_engine_tests;
mod recovery_tests;
mod send_tests;
mod sent_engine_tests;
mod sync_full_tests;
mod sync_tests;
mod thread_tests;

mod gmail_bulk_tests;
