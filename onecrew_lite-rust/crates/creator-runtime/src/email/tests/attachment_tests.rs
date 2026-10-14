//! On-demand attachment access: typed outcomes, durable availability
//! evidence, sent-metadata backfill through the listing endpoint, and the
//! guarantee that no signed URL ever reaches durable state.

use super::*;
use crate::{
    EmailAttachmentReadError, EmailAttachmentReadInput, EmailDirection, EmailMessageInput,
    EmailSnapshotInput,
};
use onecrew_email_contracts::{
    PlatformAttachmentAccess, PlatformEmailAttachment, PlatformEmailAttachmentError,
};

fn seeded_email(
    runtime: &crate::CreatorRuntime,
    mailbox_id: &crate::MailboxRefId,
    direction: EmailDirection,
    attachments: Vec<crate::EmailAttachmentInput>,
) -> crate::EmailId {
    let (_, messages) = runtime
        .repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: mailbox_id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: format!("resend:test:{}", direction.as_str()),
            subject: "Attachment".into(),
            messages: vec![EmailMessageInput {
                provider_message_id: format!("provider-{}", direction.as_str()),
                internet_message_id: Some(format!("<{}@example.net>", direction.as_str())),
                direction,
                from: "Creator <creator@example.net>".into(),
                to: vec!["outreach@example.com".into()],
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: None,
                references: Vec::new(),
                subject: "Attachment".into(),
                text: Some("body".into()),
                html: None,
                delivery_status: if direction == EmailDirection::Inbound {
                    "unread".into()
                } else {
                    "sent".into()
                },
                headers: serde_json::json!({}),
                occurred_at: 100,
                origin: crate::models::EmailOrigin::ProviderImport,
                attachments,
            }],
        })
        .unwrap();
    messages[0].id.clone()
}

fn metadata(id: &str) -> PlatformEmailAttachment {
    PlatformEmailAttachment {
        provider_attachment_id: id.into(),
        filename: "media-kit.pdf".into(),
        content_type: "application/pdf".into(),
        content_id: None,
        content_disposition: Some("attachment".into()),
        size_bytes: Some(4096),
    }
}

fn access(id: &str) -> PlatformAttachmentAccess {
    PlatformAttachmentAccess {
        metadata: metadata(id),
        download_url: format!("https://inbound-cdn.resend.com/{id}?signature=sig-fixture"),
        expires_at: "2026-08-24T12:00:00Z".into(),
    }
}

fn attachment_input(id: &str) -> crate::EmailAttachmentInput {
    crate::EmailAttachmentInput {
        provider_attachment_id: id.into(),
        filename: "media-kit.pdf".into(),
        content_type: "application/pdf".into(),
        content_id: None,
        content_disposition: Some("attachment".into()),
        size_bytes: Some(4096),
        available: true,
    }
}

#[tokio::test]
async fn a_received_attachment_downloads_through_a_fresh_capability() {
    let gateway = TestGateway {
        received_attachment_outcomes: Arc::new(Mutex::new(VecDeque::from(vec![Ok(access(
            "attachment-1",
        ))]))),
        attachment_downloads: Arc::new(Mutex::new(VecDeque::from(vec![Ok(vec![1_u8, 2, 3])]))),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let email_id = seeded_email(
        &runtime,
        &mailbox.mailbox_ref_id,
        EmailDirection::Inbound,
        vec![attachment_input("attachment-1")],
    );

    let content = runtime
        .read_email_attachment(EmailAttachmentReadInput {
            email_id: email_id.clone(),
            provider_attachment_id: "attachment-1".into(),
        })
        .await
        .unwrap();
    assert_eq!(content.bytes, vec![1, 2, 3]);
    assert_eq!(content.metadata.filename, "media-kit.pdf");
    assert!(content.metadata.available);

    // No signed URL or capability material exists anywhere durable: scan
    // every text column of every table for the capability fingerprints.
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for table in tables {
        let columns: Vec<String> = connection
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for column in columns {
            let leaked: i64 = connection
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM \"{table}\"
                         WHERE CAST(\"{column}\" AS TEXT) LIKE '%signature=%'
                            OR CAST(\"{column}\" AS TEXT) LIKE '%cdn.resend.com%'"
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(leaked, 0, "signed capability leaked into {table}.{column}");
        }
    }
}

#[tokio::test]
async fn retention_expiry_records_durable_unavailability_and_keeps_the_metadata() {
    let gateway = TestGateway {
        received_attachment_outcomes: Arc::new(Mutex::new(VecDeque::from(vec![Err(
            PlatformEmailAttachmentError::Unavailable {
                message: "provider no longer holds this object".into(),
            },
        )]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let email_id = seeded_email(
        &runtime,
        &mailbox.mailbox_ref_id,
        EmailDirection::Inbound,
        vec![attachment_input("attachment-gone")],
    );

    let error = runtime
        .read_email_attachment(EmailAttachmentReadInput {
            email_id: email_id.clone(),
            provider_attachment_id: "attachment-gone".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        EmailAttachmentReadError::Unavailable { .. }
    ));

    let views = runtime.list_email_attachments(&email_id).unwrap();
    assert_eq!(views.len(), 1, "the email and its metadata remain local");
    assert!(!views[0].available, "unavailability is durable evidence");
    assert_eq!(views[0].filename, "media-kit.pdf");
}

#[tokio::test]
async fn typed_outcomes_distinguish_rate_limit_quota_and_size() {
    let gateway = TestGateway {
        received_attachment_outcomes: Arc::new(Mutex::new(VecDeque::from(vec![
            Err(PlatformEmailAttachmentError::RateLimited {
                message: "429".into(),
            }),
            Err(PlatformEmailAttachmentError::QuotaExhausted {
                message: "monthly quota".into(),
            }),
            Ok(access("attachment-1")),
        ]))),
        attachment_downloads: Arc::new(Mutex::new(VecDeque::from(vec![Ok(vec![
            0_u8;
            crate::MAX_EMAIL_ATTACHMENT_BYTES
                + 1
        ])]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let email_id = seeded_email(
        &runtime,
        &mailbox.mailbox_ref_id,
        EmailDirection::Inbound,
        vec![attachment_input("attachment-1")],
    );
    let input = || EmailAttachmentReadInput {
        email_id: email_id.clone(),
        provider_attachment_id: "attachment-1".into(),
    };

    assert!(matches!(
        runtime.read_email_attachment(input()).await.unwrap_err(),
        EmailAttachmentReadError::RateLimited { .. }
    ));
    assert!(matches!(
        runtime.read_email_attachment(input()).await.unwrap_err(),
        EmailAttachmentReadError::QuotaExhausted { .. }
    ));
    assert!(matches!(
        runtime.read_email_attachment(input()).await.unwrap_err(),
        EmailAttachmentReadError::TooLarge { .. }
    ));
    // Transient failures never flip durable availability.
    let views = runtime.list_email_attachments(&email_id).unwrap();
    assert!(views[0].available);
}

#[tokio::test]
async fn a_sent_attachment_backfills_metadata_through_the_listing() {
    let gateway = TestGateway {
        sent_attachment_lists: Arc::new(Mutex::new(VecDeque::from(vec![Ok(vec![
            access("sent-attachment-1"),
            access("sent-attachment-2"),
        ])]))),
        attachment_downloads: Arc::new(Mutex::new(VecDeque::from(vec![Ok(b"pdf".to_vec())]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    // Sent detail embeds no attachment metadata at import time.
    let email_id = seeded_email(
        &runtime,
        &mailbox.mailbox_ref_id,
        EmailDirection::Outbound,
        Vec::new(),
    );
    assert!(
        runtime
            .list_email_attachments(&email_id)
            .unwrap()
            .is_empty()
    );

    let content = runtime
        .read_email_attachment(EmailAttachmentReadInput {
            email_id: email_id.clone(),
            provider_attachment_id: "sent-attachment-1".into(),
        })
        .await
        .unwrap();
    assert_eq!(content.bytes, b"pdf".to_vec());

    let views = runtime.list_email_attachments(&email_id).unwrap();
    assert_eq!(views.len(), 2, "the listing backfilled durable metadata");
    assert!(views.iter().all(|view| view.available));
    assert!(
        views
            .iter()
            .all(|view| view.direction == EmailDirection::Outbound)
    );
}

#[tokio::test]
async fn an_unknown_email_or_attachment_is_a_typed_miss() {
    let (_directory, runtime, _) = runtime(TestGateway::default());
    let error = runtime
        .read_email_attachment(EmailAttachmentReadInput {
            email_id: crate::EmailId::new("missing"),
            provider_attachment_id: "whatever".into(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        EmailAttachmentReadError::UnknownEmail { .. }
    ));
}
