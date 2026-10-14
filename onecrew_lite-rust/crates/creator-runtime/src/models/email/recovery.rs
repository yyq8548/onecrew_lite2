use serde::{Deserialize, Serialize};

use super::{
    drafts::EmailDraftSendResult,
    mailboxes::{MailboxDetail, MailboxIssue, MailboxProvider},
    sending::{
        EmailActionResult, EmailPreparation, ManualEmailActionResult, ManualEmailPreparation,
    },
    threads::{EmailMessage, EmailMessageView, EmailThread},
};
use crate::models::{
    AuditEvent, ContactLocatorId, CreatorContactRouteId, CreatorContactRouteRecord, CreatorId,
    CreatorRecord, EmailCapability, EmailDirection, EmailDraftId, MailboxRefId, Touchpoint,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct PersistedMailboxSnapshot {
    pub(crate) id: MailboxRefId,
    pub(crate) provider: String,
    pub(crate) label: String,
    pub(crate) email_address: String,
    pub(crate) display_name: Option<String>,
    pub(crate) capabilities: Vec<EmailCapability>,
    pub(crate) available: bool,
    pub(crate) last_error: Option<String>,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
}

impl std::fmt::Debug for PersistedMailboxSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PersistedMailboxSnapshot")
            .field("id", &self.id)
            .field("provider", &self.provider)
            .field("label", &self.label)
            .field("email_address", &self.email_address)
            .field("display_name", &self.display_name)
            .field("capabilities", &self.capabilities)
            .field("available", &self.available)
            .field("has_recorded_error", &self.last_error.is_some())
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

pub(crate) fn persisted_mailbox_snapshot(mailbox: MailboxDetail) -> PersistedMailboxSnapshot {
    PersistedMailboxSnapshot {
        id: mailbox.mailbox_ref_id,
        provider: mailbox.provider.as_str().into(),
        label: mailbox.label,
        email_address: mailbox.email_address,
        display_name: mailbox.display_name,
        capabilities: mailbox.capabilities,
        available: mailbox.available,
        last_error: None,
        created_at: mailbox.created_at,
        updated_at: mailbox.updated_at,
    }
}

fn mailbox_detail(snapshot: PersistedMailboxSnapshot) -> Result<MailboxDetail, String> {
    let provider = match snapshot.provider.as_str() {
        "resend" => MailboxProvider::Resend,
        "gmail" => MailboxProvider::Gmail,
        provider => return Err(format!("unsupported persisted mailbox provider {provider}")),
    };
    let mut issues = Vec::new();
    if !snapshot.available {
        issues.push(MailboxIssue::Unavailable);
    }
    if snapshot.last_error.is_some() {
        issues.push(MailboxIssue::RecordedFailure);
    }
    Ok(MailboxDetail {
        mailbox_ref_id: snapshot.id,
        provider,
        label: snapshot.label,
        email_address: snapshot.email_address,
        display_name: snapshot.display_name,
        capabilities: snapshot.capabilities,
        available: snapshot.available,
        issues,
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    })
}

#[derive(Serialize, Deserialize)]
struct PersistedEmailPreparation {
    creator: CreatorRecord,
    route: CreatorContactRouteRecord,
    mailbox: PersistedMailboxSnapshot,
    thread: Option<EmailThread>,
    reply_to: Option<EmailMessageView>,
    confirmation_required: bool,
    confirmation_reason: String,
}

impl From<&EmailPreparation> for PersistedEmailPreparation {
    fn from(value: &EmailPreparation) -> Self {
        Self {
            creator: value.creator.clone(),
            route: value.route.clone(),
            mailbox: persisted_mailbox_snapshot(value.mailbox.clone()),
            thread: value.thread.clone(),
            reply_to: value.reply_to.clone(),
            confirmation_required: value.confirmation_required,
            confirmation_reason: value.confirmation_reason.clone(),
        }
    }
}

impl TryFrom<PersistedEmailPreparation> for EmailPreparation {
    type Error = String;

    fn try_from(value: PersistedEmailPreparation) -> Result<Self, Self::Error> {
        Ok(Self {
            creator: value.creator,
            route: value.route,
            mailbox: mailbox_detail(value.mailbox)?,
            thread: value.thread,
            reply_to: value.reply_to,
            confirmation_required: value.confirmation_required,
            confirmation_reason: value.confirmation_reason,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedManualEmailPreparation {
    mailbox: PersistedMailboxSnapshot,
    to: String,
    confirmation_required: bool,
    confirmation_reason: String,
}

impl From<&ManualEmailPreparation> for PersistedManualEmailPreparation {
    fn from(value: &ManualEmailPreparation) -> Self {
        Self {
            mailbox: persisted_mailbox_snapshot(value.mailbox.clone()),
            to: value.to.clone(),
            confirmation_required: value.confirmation_required,
            confirmation_reason: value.confirmation_reason.clone(),
        }
    }
}

impl TryFrom<PersistedManualEmailPreparation> for ManualEmailPreparation {
    type Error = String;

    fn try_from(value: PersistedManualEmailPreparation) -> Result<Self, Self::Error> {
        Ok(Self {
            mailbox: mailbox_detail(value.mailbox)?,
            to: value.to,
            confirmation_required: value.confirmation_required,
            confirmation_reason: value.confirmation_reason,
        })
    }
}

fn serialize_email_preparation<S>(
    value: &EmailPreparation,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    PersistedEmailPreparation::from(value).serialize(serializer)
}

fn deserialize_email_preparation<'de, D>(deserializer: D) -> Result<EmailPreparation, D::Error>
where
    D: serde::Deserializer<'de>,
{
    PersistedEmailPreparation::deserialize(deserializer)?
        .try_into()
        .map_err(serde::de::Error::custom)
}

fn serialize_boxed_email_preparation<S>(
    value: &EmailPreparation,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serialize_email_preparation(value, serializer)
}

fn deserialize_boxed_email_preparation<'de, D>(
    deserializer: D,
) -> Result<Box<EmailPreparation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    deserialize_email_preparation(deserializer).map(Box::new)
}

fn serialize_manual_email_preparation<S>(
    value: &ManualEmailPreparation,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    PersistedManualEmailPreparation::from(value).serialize(serializer)
}

fn deserialize_manual_email_preparation<'de, D>(
    deserializer: D,
) -> Result<ManualEmailPreparation, D::Error>
where
    D: serde::Deserializer<'de>,
{
    PersistedManualEmailPreparation::deserialize(deserializer)?
        .try_into()
        .map_err(serde::de::Error::custom)
}

#[derive(Serialize, Deserialize)]
struct PersistedEmailActionResult {
    preparation: PersistedEmailPreparation,
    thread: EmailThread,
    email: EmailMessageView,
    touchpoint: Touchpoint,
    audit_event: AuditEvent,
    replayed: bool,
}

#[derive(Serialize, Deserialize)]
struct PersistedManualEmailActionResult {
    preparation: PersistedManualEmailPreparation,
    thread: EmailThread,
    email: EmailMessageView,
    audit_event: AuditEvent,
    replayed: bool,
}

pub(crate) fn encode_persisted_email_action_result(
    value: &EmailActionResult,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(PersistedEmailActionResult {
        preparation: PersistedEmailPreparation::from(&value.preparation),
        thread: value.thread.clone(),
        email: value.email.clone(),
        touchpoint: value.touchpoint.clone(),
        audit_event: value.audit_event.clone(),
        replayed: value.replayed,
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn decode_persisted_email_action_result(
    value: serde_json::Value,
) -> Result<EmailActionResult, String> {
    let value: PersistedEmailActionResult =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(EmailActionResult {
        preparation: value.preparation.try_into()?,
        thread: value.thread,
        email: value.email,
        touchpoint: value.touchpoint,
        audit_event: value.audit_event,
        replayed: value.replayed,
    })
}

pub(crate) fn encode_persisted_manual_email_action_result(
    value: &ManualEmailActionResult,
) -> Result<serde_json::Value, String> {
    serde_json::to_value(PersistedManualEmailActionResult {
        preparation: PersistedManualEmailPreparation::from(&value.preparation),
        thread: value.thread.clone(),
        email: value.email.clone(),
        audit_event: value.audit_event.clone(),
        replayed: value.replayed,
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn decode_persisted_manual_email_action_result(
    value: serde_json::Value,
) -> Result<ManualEmailActionResult, String> {
    let value: PersistedManualEmailActionResult =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(ManualEmailActionResult {
        preparation: value.preparation.try_into()?,
        thread: value.thread,
        email: value.email,
        audit_event: value.audit_event,
        replayed: value.replayed,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSnapshotInput {
    pub mailbox_ref_id: MailboxRefId,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub provider_thread_key: String,
    pub subject: String,
    pub messages: Vec<EmailMessageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessageInput {
    pub provider_message_id: String,
    pub internet_message_id: Option<String>,
    pub direction: EmailDirection,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub received_for: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub delivery_status: String,
    pub headers: serde_json::Value,
    pub occurred_at: u64,
    pub origin: crate::models::EmailOrigin,
    pub attachments: Vec<crate::models::EmailAttachmentInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEmailActionInput {
    #[serde(
        serialize_with = "serialize_email_preparation",
        deserialize_with = "deserialize_email_preparation"
    )]
    pub preparation: EmailPreparation,
    pub creator_id: CreatorId,
    pub contact_locator_id: ContactLocatorId,
    pub snapshot: EmailSnapshotInput,
    pub verified_provider_message_id: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEmailActionResult {
    pub thread: EmailThread,
    pub email: EmailMessage,
    pub touchpoint: Touchpoint,
    pub audit_event: AuditEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitManualEmailActionInput {
    #[serde(
        serialize_with = "serialize_manual_email_preparation",
        deserialize_with = "deserialize_manual_email_preparation"
    )]
    pub preparation: ManualEmailPreparation,
    pub snapshot: EmailSnapshotInput,
    pub verified_provider_message_id: String,
    pub recipient: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitManualEmailActionResult {
    pub thread: EmailThread,
    pub email: EmailMessage,
    pub audit_event: AuditEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEmailDraftActionInput {
    pub draft_id: EmailDraftId,
    pub expected_revision: u64,
    pub creator_id: Option<CreatorId>,
    pub contact_locator_id: Option<ContactLocatorId>,
    pub recipient: String,
    pub snapshot: EmailSnapshotInput,
    pub verified_provider_message_id: String,
    pub idempotency_key: String,
    pub request_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmailActionKind {
    #[serde(rename = "email_send")]
    Creator,
    #[serde(rename = "manual_email_send")]
    Manual,
    #[serde(rename = "email_draft_send")]
    Draft,
}

impl EmailActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Creator => "email_send",
            Self::Manual => "manual_email_send",
            Self::Draft => "email_draft_send",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct EmailAddress(String);

impl EmailAddress {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        let address = match (value.rfind('<'), value.rfind('>')) {
            (Some(start), Some(end)) if start < end => &value[start + 1..end],
            _ => value,
        }
        .trim()
        .to_ascii_lowercase();
        let mut parts = address.split('@');
        let local = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        if local.is_empty() || domain.is_empty() || parts.next().is_some() {
            return Err("email address is invalid".into());
        }
        Ok(Self(address))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl<'de> Deserialize<'de> for EmailAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEmailRecovery {
    pub action: EmailActionKind,
    pub request_fingerprint: String,
    pub canonical_request_fingerprint: String,
    pub(crate) mailbox: PersistedMailboxSnapshot,
    pub expected_recipient: EmailAddress,
    pub request: onecrew_email_contracts::PlatformEmailRequest,
    pub provider_thread_key: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub context: RemoteAcceptedEmailContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStartedEmailAction {
    #[serde(flatten)]
    pub recovery: RemoteEmailRecovery,
}

impl std::ops::Deref for RemoteStartedEmailAction {
    type Target = RemoteEmailRecovery;

    fn deref(&self) -> &Self::Target {
        &self.recovery
    }
}

impl std::ops::DerefMut for RemoteStartedEmailAction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recovery
    }
}

impl RemoteStartedEmailAction {
    pub fn accept(self, provider_message_id: String) -> RemoteAcceptedEmailAction {
        RemoteAcceptedEmailAction {
            recovery: self.recovery,
            provider_message_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAcceptedEmailAction {
    #[serde(flatten)]
    pub recovery: RemoteEmailRecovery,
    pub provider_message_id: String,
}

impl std::ops::Deref for RemoteAcceptedEmailAction {
    type Target = RemoteEmailRecovery;

    fn deref(&self) -> &Self::Target {
        &self.recovery
    }
}

impl std::ops::DerefMut for RemoteAcceptedEmailAction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.recovery
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RemoteAcceptedEmailContext {
    Creator {
        #[serde(
            serialize_with = "serialize_boxed_email_preparation",
            deserialize_with = "deserialize_boxed_email_preparation"
        )]
        preparation: Box<EmailPreparation>,
    },
    Manual {
        #[serde(
            serialize_with = "serialize_manual_email_preparation",
            deserialize_with = "deserialize_manual_email_preparation"
        )]
        preparation: ManualEmailPreparation,
    },
    Draft {
        draft_id: EmailDraftId,
        expected_revision: u64,
        creator_id: Option<CreatorId>,
        contact_route_id: Option<CreatorContactRouteId>,
        contact_locator_id: Option<ContactLocatorId>,
    },
}

#[derive(Debug, Clone)]
pub struct ReserveEmailDraftSendInput {
    pub draft_id: EmailDraftId,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub enum EmailDraftSendClaim {
    RemoteStarted(RemoteStartedEmailAction),
    RemoteAccepted(RemoteAcceptedEmailAction),
    RemoteVerified(CommitEmailDraftActionInput),
    Completed(Box<EmailDraftSendResult>),
    Failed(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailDraftSendFingerprint {
    pub mailbox_ref_id: String,
    pub draft_id: String,
    pub expected_revision: u64,
}

impl EmailDraftSendFingerprint {
    pub fn new(
        mailbox_ref_id: &MailboxRefId,
        draft_id: &EmailDraftId,
        expected_revision: u64,
    ) -> Self {
        Self {
            mailbox_ref_id: mailbox_ref_id.as_str().to_owned(),
            draft_id: draft_id.as_str().to_owned(),
            expected_revision,
        }
    }

    pub fn ensure_matches(
        &self,
        draft_id: &EmailDraftId,
        expected_revision: u64,
    ) -> Result<(), String> {
        if self.draft_id == draft_id.as_str() && self.expected_revision == expected_revision {
            Ok(())
        } else {
            Err("idempotency key was already used for another draft or draft revision".to_string())
        }
    }
}

#[cfg(test)]
mod remote_email_recovery_tests {
    use super::{RemoteAcceptedEmailAction, RemoteStartedEmailAction};

    fn legacy_recovery_payload() -> serde_json::Value {
        serde_json::json!({
            "action": "email_draft_send",
            "request_fingerprint": "{\"mailbox_ref_id\":\"mailbox-1\",\"draft_id\":\"draft-1\",\"expected_revision\":2}",
            "canonical_request_fingerprint": "canonical-request",
            "mailbox": {
                "id": "mailbox-1",
                "provider": "resend",
                "label": "Partnerships",
                "email_address": "partnerships@example.com",
                "display_name": null,
                "capabilities": ["send", "sent_readback"],
                "available": true,
                "last_error": null,
                "created_at": 1,
                "updated_at": 2
            },
            "expected_recipient": "creator@example.com",
            "request": {
                "to": ["creator@example.com"],
                "cc": [],
                "bcc": [],
                "reply_to": [],
                "in_reply_to": null,
                "references": [],
                "subject": "Partnership",
                "text": "Hello",
                "html": null,
                "idempotency_key": "draft-send-1"
            },
            "provider_thread_key": "creator:1:route:1",
            "cursor": null,
            "in_reply_to": null,
            "references": [],
            "context": {
                "kind": "draft",
                "draft_id": "draft-1",
                "expected_revision": 2,
                "creator_id": "creator-1",
                "contact_route_id": "route-1",
                "contact_locator_id": "locator-1"
            }
        })
    }

    #[test]
    fn remote_started_recovery_preserves_the_legacy_flat_json_shape() {
        let legacy = legacy_recovery_payload();
        let recovery: RemoteStartedEmailAction =
            serde_json::from_value(legacy.clone()).expect("legacy recovery payload must load");

        let serialized = serde_json::to_value(recovery).expect("recovery payload must serialize");
        // The obsolete provider-paging cursor is tolerated on read (v23 rows
        // carry it) and dropped from every v24 write.
        let mut expected = legacy;
        expected.as_object_mut().unwrap().remove("cursor");
        assert_eq!(serialized, expected);
        assert!(serialized.get("recovery").is_none());
        assert!(serialized.get("stage").is_none());
    }

    #[test]
    fn remote_accepted_recovery_preserves_the_legacy_flat_json_shape() {
        let mut legacy = legacy_recovery_payload();
        legacy
            .as_object_mut()
            .expect("payload must be an object")
            .insert(
                "provider_message_id".into(),
                serde_json::json!("resend-message-1"),
            );
        let recovery: RemoteAcceptedEmailAction =
            serde_json::from_value(legacy.clone()).expect("legacy recovery payload must load");

        let serialized = serde_json::to_value(recovery).expect("recovery payload must serialize");
        let mut expected = legacy;
        expected.as_object_mut().unwrap().remove("cursor");
        assert_eq!(serialized, expected);
        assert!(serialized.get("recovery").is_none());
        assert!(serialized.get("stage").is_none());
    }
}
