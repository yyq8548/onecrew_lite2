//! Gmail-only, confirmed, durable outbox. An attempted item is never sent twice.
use crate::MailboxRefId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GmailBulkRecipient {
    pub email: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrepareGmailBulkInput {
    pub mailbox_ref_id: MailboxRefId,
    pub subject: String,
    pub body: String,
    pub recipients: Vec<GmailBulkRecipient>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GmailBulkItem {
    pub id: String,
    pub recipient: String,
    pub source: String,
    pub fingerprint: String,
    pub state: String,
    pub provider_message_id: Option<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GmailBulkBatch {
    pub id: String,
    pub mailbox_ref_id: MailboxRefId,
    pub subject: String,
    pub body: String,
    pub state: String,
    pub created_at: i64,
    pub items: Vec<GmailBulkItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GmailBulkControlInput {
    pub id: String,
    /// confirm, pause, resume, or cancel. Confirm acts on the frozen draft only.
    pub action: String,
}

impl crate::CreatorRuntime {
    pub async fn gmail_bulk_recipients(&self) -> Result<Vec<GmailBulkRecipient>, String> {
        let runtime = self.clone();
        self.repository_read_segment(move || runtime.repository.gmail_bulk_recipients())
            .await
            .map_err(|e| e.to_string())
    }
    pub async fn prepare_gmail_bulk(
        &self,
        mut input: PrepareGmailBulkInput,
    ) -> Result<GmailBulkBatch, String> {
        use sha2::{Digest, Sha256};
        input.subject = input.subject.trim().to_owned();
        input.body = input.body.trim().to_owned();
        if input.recipients.is_empty() || input.recipients.len() > 100 {
            return Err("Select 1–100 email recipients".into());
        }
        if input.subject.trim().is_empty()
            || input.subject.len() > 998
            || input.subject.contains(['\r', '\n'])
            || input.body.trim().is_empty()
            || input.body.len() > 100_000
        {
            return Err("Invalid email subject or body".into());
        }
        let runtime = self.clone();
        self.repository_write_segment(move || {
            let mailbox = runtime.require_mailbox_ref(&input.mailbox_ref_id)?;
            if mailbox.provider != crate::MailboxProvider::Gmail {
                return Err("Bulk mail requires a Gmail mailbox".into());
            }
            crate::email::ensure_mailbox(&mailbox, crate::EmailCapability::Send)?;
            crate::email::ensure_mailbox(&mailbox, crate::EmailCapability::SentReadback)?;
            let mut seen = std::collections::BTreeSet::new();
            let mut items = Vec::new();
            for recipient in input.recipients {
                let email = crate::email::normalize_email_address(&recipient.email)?;
                if !seen.insert(email.to_lowercase()) {
                    continue;
                }
                if recipient.source.len() > 2048 {
                    return Err("Recipient source is too long".into());
                }
                let canonical = serde_json::to_vec(&(
                    mailbox.email_address.to_lowercase(),
                    email.to_lowercase(),
                    &input.subject,
                    &input.body,
                ))
                .map_err(|e| e.to_string())?;
                items.push(GmailBulkItem {
                    id: uuid::Uuid::new_v4().to_string(),
                    recipient: email,
                    source: recipient.source,
                    fingerprint: format!("{:x}", Sha256::digest(canonical)),
                    state: "pending".into(),
                    provider_message_id: None,
                    detail: None,
                });
            }
            runtime.repository.create_gmail_bulk(GmailBulkBatch {
                id: uuid::Uuid::new_v4().to_string(),
                mailbox_ref_id: input.mailbox_ref_id,
                subject: input.subject,
                body: input.body,
                state: "draft".into(),
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_secs() as i64,
                items,
            })
        })
        .await
        .map_err(|e| e.to_string())
    }

    pub async fn list_gmail_bulk(&self) -> Result<Vec<GmailBulkBatch>, String> {
        let runtime = self.clone();
        self.repository_read_segment(move || runtime.repository.list_gmail_bulk())
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn control_gmail_bulk(&self, input: GmailBulkControlInput) -> Result<(), String> {
        let runtime = self.clone();
        self.repository_write_segment(move || {
            runtime
                .repository
                .control_gmail_bulk(&input.id, &input.action)
        })
        .await
        .map_err(|e| e.to_string())
    }

    pub async fn interrupt_gmail_bulk(&self) -> Result<(), String> {
        let runtime = self.clone();
        self.repository_write_segment(move || runtime.repository.interrupt_gmail_bulk())
            .await
            .map_err(|e| e.to_string())
    }

    /// One durable reservation, one provider attempt. Unknown effects are never retried.
    pub async fn run_gmail_bulk_item(&self) -> Result<bool, String> {
        let runtime = self.clone();
        let claim = self
            .repository_write_segment(move || runtime.repository.claim_gmail_bulk())
            .await
            .map_err(|e| e.to_string())?;
        let Some((batch, mut item)) = claim else {
            return Ok(false);
        };
        if item.state == "attempting" {
            let runtime = self.clone();
            let mailbox_id = batch.mailbox_ref_id.clone();
            let gmail = self
                .repository_read_segment(move || {
                    runtime
                        .require_mailbox_ref(&mailbox_id)
                        .and_then(|mailbox| {
                            if mailbox.provider == crate::MailboxProvider::Gmail {
                                Ok(())
                            } else {
                                Err("Bulk mail requires Gmail".into())
                            }
                        })
                })
                .await;
            let result = match gmail {
                Ok(()) => {
                    self.send_manual_email(crate::SendManualEmailInput {
                        mailbox_ref_id: batch.mailbox_ref_id,
                        to: item.recipient.clone(),
                        subject: batch.subject,
                        text: batch.body,
                        html: None,
                        idempotency_key: format!("gmail-bulk-{}", item.id),
                    })
                    .await
                }
                Err(error) => Err(crate::EmailSendFailure::not_sent(error.to_string())),
            };
            match result {
                Ok(sent) => {
                    item.state = "sent".into();
                    item.provider_message_id = Some(sent.email.provider_message_id);
                }
                Err(error) => {
                    item.state = match error.effect() {
                        crate::EmailSendEffect::NotSent => "failed",
                        crate::EmailSendEffect::Sent => "accepted",
                        crate::EmailSendEffect::Unknown => "unknown",
                    }
                    .into();
                    item.detail = Some(error.into_message());
                }
            }
        }
        let runtime = self.clone();
        self.repository_write_segment(move || runtime.repository.finish_gmail_bulk(item))
            .await
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
}
