use super::*;
use crate::{GmailBulkControlInput, GmailBulkRecipient, PrepareGmailBulkInput};

fn input(runtime: &crate::CreatorRuntime) -> PrepareGmailBulkInput {
    let mailbox = runtime
        .sync_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "gmail".into(),
            label: "Gmail".into(),
            email_address: "sender@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send, EmailCapability::SentReadback],
            available: true,
            last_error: None,
        })
        .unwrap();
    PrepareGmailBulkInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        subject: "Bulk test".into(),
        body: "Test body".into(),
        recipients: vec![GmailBulkRecipient {
            email: "recipient@example.com".into(),
            source: "https://example.com/author".into(),
        }],
    }
}
async fn confirm(runtime: &crate::CreatorRuntime, id: &str) {
    runtime
        .control_gmail_bulk(GmailBulkControlInput {
            id: id.into(),
            action: "confirm".into(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn gmail_bulk_draft_never_sends_and_deduplicates() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let mut request = input(&runtime);
    request.recipients.push(request.recipients[0].clone());
    let batch = runtime.prepare_gmail_bulk(request).await.unwrap();
    assert_eq!(batch.items.len(), 1);
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    assert!(runtime.gmail_bulk_recipients().await.unwrap().is_empty());
}

#[tokio::test]
async fn gmail_bulk_ambiguous_send_is_not_retried_or_duplicated_across_batches() {
    let gateway = TestGateway::default();
    *gateway.next_ambiguous_after_delivery.lock().unwrap() = Some("connection lost".into());
    let (directory, runtime, gateway) = runtime(gateway);
    let request = input(&runtime);
    let batch = runtime.prepare_gmail_bulk(request.clone()).await.unwrap();
    confirm(&runtime, &batch.id).await;
    confirm(&runtime, &batch.id).await;
    assert!(runtime.run_gmail_bulk_item().await.unwrap());
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    let read = runtime.list_gmail_bulk().await.unwrap();
    assert_eq!(
        read[0].items[0].state, "unknown",
        "{:?}",
        read[0].items[0].detail
    );
    let runtime = reopen_runtime(&directory, &gateway);
    runtime.interrupt_gmail_bulk().await.unwrap();
    let duplicate = runtime.prepare_gmail_bulk(request).await.unwrap();
    confirm(&runtime, &duplicate.id).await;
    runtime.run_gmail_bulk_item().await.unwrap();
    assert_eq!(
        runtime.list_gmail_bulk().await.unwrap()[0].items[0].state,
        "failed"
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn gmail_bulk_crash_after_claim_locks_attempt_and_pauses_remaining() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mut request = input(&runtime);
    request.recipients.push(GmailBulkRecipient {
        email: "second@example.com".into(),
        source: "test".into(),
    });
    let batch = runtime.prepare_gmail_bulk(request).await.unwrap();
    confirm(&runtime, &batch.id).await;
    assert!(runtime.repository.claim_gmail_bulk().unwrap().is_some());
    let runtime = reopen_runtime(&directory, &gateway);
    runtime.interrupt_gmail_bulk().await.unwrap();
    let restored = runtime.list_gmail_bulk().await.unwrap().remove(0);
    assert_eq!(restored.state, "paused");
    assert_eq!(restored.items[0].state, "unknown");
    assert_eq!(restored.items[1].state, "pending");
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    runtime
        .control_gmail_bulk(GmailBulkControlInput {
            id: batch.id,
            action: "cancel".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        runtime.list_gmail_bulk().await.unwrap()[0].items[1].state,
        "cancelled"
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn gmail_bulk_pause_and_cancel_do_not_send() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let batch = runtime.prepare_gmail_bulk(input(&runtime)).await.unwrap();
    confirm(&runtime, &batch.id).await;
    runtime
        .control_gmail_bulk(GmailBulkControlInput {
            id: batch.id.clone(),
            action: "pause".into(),
        })
        .await
        .unwrap();
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    runtime
        .control_gmail_bulk(GmailBulkControlInput {
            id: batch.id.clone(),
            action: "resume".into(),
        })
        .await
        .unwrap();
    runtime
        .control_gmail_bulk(GmailBulkControlInput {
            id: batch.id,
            action: "cancel".into(),
        })
        .await
        .unwrap();
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn gmail_bulk_sends_individually_and_never_uses_provider_batch() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let batch = runtime.prepare_gmail_bulk(input(&runtime)).await.unwrap();
    confirm(&runtime, &batch.id).await;
    runtime.run_gmail_bulk_item().await.unwrap();
    let batch = runtime.list_gmail_bulk().await.unwrap().remove(0);
    assert_eq!(batch.items[0].state, "sent", "{:?}", batch.items[0].detail);
    assert!(batch.items[0].provider_message_id.is_some());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .to
            .len(),
        1
    );
}

#[tokio::test]
async fn gmail_bulk_claim_is_exclusive_and_attempt_marker_survives_reopen() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let batch = runtime.prepare_gmail_bulk(input(&runtime)).await.unwrap();
    confirm(&runtime, &batch.id).await;
    let other = runtime.clone();
    let (one, two) = tokio::join!(runtime.run_gmail_bulk_item(), other.run_gmail_bulk_item());
    assert_ne!(one.unwrap(), two.unwrap());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    let key = format!("gmail-bulk-{}", batch.items[0].id);
    let runtime = reopen_runtime(&directory, &gateway);
    assert!(!runtime.repository.reserve_gmail_send_attempt(&key).unwrap());
}

#[tokio::test]
async fn gmail_bulk_partial_failure_preserves_each_result() {
    let gateway=TestGateway::default();
    *gateway.next_ambiguous_after_delivery.lock().unwrap()=Some("connection lost".into());
    let (_directory,runtime,gateway)=runtime(gateway);
    let mut request=input(&runtime);
    request.recipients.push(GmailBulkRecipient {email:"second@example.com".into(),source:"test".into()});
    let batch=runtime.prepare_gmail_bulk(request).await.unwrap();
    confirm(&runtime,&batch.id).await;
    runtime.run_gmail_bulk_item().await.unwrap();
    runtime.run_gmail_bulk_item().await.unwrap();
    let batch=runtime.list_gmail_bulk().await.unwrap().remove(0);
    assert_eq!(batch.state,"completed");
    assert_eq!(batch.items[0].state,"unknown");
    assert_eq!(batch.items[1].state,"sent");
    assert!(!runtime.run_gmail_bulk_item().await.unwrap());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst),2);
}
