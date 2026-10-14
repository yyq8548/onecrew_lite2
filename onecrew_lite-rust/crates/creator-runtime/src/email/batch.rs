//! Durable provider-neutral native batch email owner.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CreatorContactRouteId, CreatorId, CreatorStage, EmailCapability, EmailTemplate,
    EmailTemplateId, MailboxDetail, MailboxRefId,
};
use onecrew_email_contracts::{
    EmailBatchItem, EmailBatchItemOutcome, EmailBatchRequest, EmailBatchResult, EmailBatchTemplate,
    PlatformEmailMessage, SentDelivery, sent_delivery,
};

use super::{
    EMAIL_BATCH_ACTION, ensure_mailbox, failed_replay, normalize_email_address,
    validate_idempotency_key,
};

pub(crate) const PREFLIGHT_CREATOR_CHUNK_SIZE: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmailBatchRecipient {
    ExplicitAddress {
        to: Vec<String>,
    },
    CreatorRoute {
        creator_id: CreatorId,
        contact_route_id: CreatorContactRouteId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmailBatchContentInput {
    InlineRendered {
        subject: String,
        text: Option<String>,
        html: Option<String>,
    },
    LocalTemplate {
        template_id: EmailTemplateId,
        revision: u64,
        variables: BTreeMap<String, String>,
    },
    PublishedTemplate {
        template_id_or_alias: String,
        variables: BTreeMap<String, serde_json::Value>,
        from_override: Option<String>,
        subject_override: Option<String>,
        reply_to_override: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailBatchItemInput {
    pub recipient: EmailBatchRecipient,
    pub from_name: Option<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub content: EmailBatchContentInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendManyEmailInput {
    pub mailbox_ref_id: MailboxRefId,
    pub idempotency_key: String,
    pub validation: EmailBatchValidation,
    pub items: Vec<EmailBatchItemInput>,
}

pub type SendEmailManyInput = SendManyEmailInput;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReconcileEmailBatchInput {
    pub mailbox_ref_id: MailboxRefId,
    pub idempotency_key: String,
    /// The Creator action fingerprint returned by preflight. It prevents a
    /// reconciliation from attaching to a different frozen request.
    pub logical_fingerprint: String,
    pub items: Vec<EmailBatchReconciliationItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailBatchReconciliationItem {
    pub input_index: usize,
    pub outcome: EmailBatchReconciliationOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmailBatchReconciliationOutcome {
    Accepted { provider_message_id: String },
    Rejected { message: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmailBatchValidation {
    Strict,
    Permissive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmailBatchOutcome {
    Accepted { provider_message_id: String },
    Rejected { message: String },
    Unknown { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EmailBatchItemResult {
    pub input_index: usize,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    /// Fingerprint of the frozen, rendered provider payload for this item.
    pub payload_fingerprint: String,
    pub outcome: EmailBatchOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EmailBatchChunkResult {
    pub chunk_index: usize,
    pub idempotency_key: String,
    pub outcomes: Vec<EmailBatchItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct EmailBatchActionResult {
    pub logical_fingerprint: String,
    pub provider_payload_fingerprint: String,
    pub chunks: Vec<EmailBatchChunkResult>,
    pub outcomes: Vec<EmailBatchItemResult>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailBatchPreparation {
    pub mailbox: MailboxDetail,
    pub logical_fingerprint: String,
    pub provider_payload_fingerprint: String,
    pub item_count: usize,
    /// See [`crate::EmailPreparation::confirmation_required`]: advance notice of
    /// the owner review, not an input that authorizes the batch.
    pub confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FrozenEmailBatchItem {
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub preflight_rejection: Option<String>,
    pub request: EmailBatchItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteStartedEmailBatchAction {
    pub mailbox: MailboxDetail,
    pub request: EmailBatchRequest,
    pub logical_fingerprint: String,
    pub action_fingerprint: String,
    pub items: Vec<FrozenEmailBatchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteAcceptedEmailBatchAction {
    #[serde(flatten)]
    pub started: RemoteStartedEmailBatchAction,
    pub result: EmailBatchResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBatchCommitInput {
    pub mailbox: MailboxDetail,
    pub request: EmailBatchRequest,
    pub logical_fingerprint: String,
    pub result: EmailBatchActionResult,
    pub verified: Vec<EmailBatchVerifiedItem>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailBatchVerifiedItem {
    pub input_index: usize,
    pub provider_message_id: String,
    pub message: PlatformEmailMessage,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

struct PreparedEmailBatch {
    mailbox: MailboxDetail,
    request: EmailBatchRequest,
    logical_fingerprint: String,
    action_fingerprint: String,
    items: Vec<FrozenEmailBatchItem>,
}

pub struct EmailBatchPreflightSnapshot {
    pub mailbox: MailboxDetail,
    pub creators: HashMap<CreatorId, EmailBatchCreatorSnapshot>,
    pub templates: HashMap<EmailTemplateId, EmailTemplate>,
}

pub struct EmailBatchCreatorSnapshot {
    pub display_name: String,
    pub archived_at: Option<u64>,
    pub stage: CreatorStage,
    pub confirmed_handles: HashSet<String>,
    pub email_routes: HashMap<CreatorContactRouteId, String>,
}

enum BatchPreflightError {
    Item(String),
    Infrastructure(String),
}

impl std::fmt::Display for BatchPreflightError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Item(message) | Self::Infrastructure(message) => formatter.write_str(message),
        }
    }
}

fn item_error(message: String) -> BatchPreflightError {
    BatchPreflightError::Item(message)
}

fn classify_lookup_error(message: String) -> BatchPreflightError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("not found")
        || lower.contains("does not belong")
        || lower.contains("archived")
        || lower.contains("not an email")
        || lower.contains("unavailable")
        || lower.contains("required")
        || lower.contains("unresolved")
        || lower.contains("revision mismatch")
    {
        BatchPreflightError::Item(message)
    } else {
        BatchPreflightError::Infrastructure(message)
    }
}

fn creator_builtins(record: Option<&EmailBatchCreatorSnapshot>) -> BTreeMap<String, String> {
    let Some(record) = record else {
        return BTreeMap::new();
    };
    let emails = record
        .email_routes
        .values()
        .cloned()
        .collect::<BTreeSet<_>>();
    let handles = record
        .confirmed_handles
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    crate::mail::templates::email_template_builtins_from_values(
        &record.display_name,
        &handles,
        &emails,
    )
}

impl EmailBatchItemInput {
    fn normalized_addresses(values: &[String], field: &str) -> Result<Vec<String>, String> {
        values
            .iter()
            .map(|value| normalize_email_address(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("{field}: {error}"))
    }

    fn render_inline(
        subject: &str,
        text: Option<&str>,
        html: Option<&str>,
    ) -> Result<(String, Option<String>, Option<String>), String> {
        let subject = subject.trim();
        let text = text.map(str::trim).filter(|value| !value.is_empty());
        let html = html.map(str::trim).filter(|value| !value.is_empty());
        if subject.is_empty() {
            return Err("inline rendered subject is required".into());
        }
        if text.is_none() && html.is_none() {
            return Err("inline rendered text or html body is required".into());
        }
        Ok((
            subject.to_owned(),
            text.map(str::to_owned),
            html.map(str::to_owned),
        ))
    }
}

impl crate::CreatorRuntime {
    pub fn prepare_email_send_many(
        &self,
        input: SendManyEmailInput,
    ) -> Result<EmailBatchPreparation, String> {
        let prepared = self.prepare_batch_request(&input)?;
        Ok(EmailBatchPreparation {
            mailbox: prepared.mailbox,
            logical_fingerprint: prepared.action_fingerprint,
            provider_payload_fingerprint: prepared.logical_fingerprint,
            item_count: prepared.request.items.len(),
            confirmation_required: true,
            confirmation_reason: "sending a batch of emails is an external write".into(),
        })
    }

    fn prepare_batch_request(
        &self,
        input: &SendManyEmailInput,
    ) -> Result<PreparedEmailBatch, String> {
        validate_idempotency_key(&input.idempotency_key)?;
        if input.items.is_empty() {
            return Err("batch must contain at least one item".into());
        }
        let mut seen_creators = HashSet::new();
        let creator_ids = input
            .items
            .iter()
            .filter_map(|item| match &item.recipient {
                EmailBatchRecipient::CreatorRoute { creator_id, .. }
                    if seen_creators.insert(creator_id.as_str().to_owned()) =>
                {
                    Some(creator_id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut seen_templates = HashSet::new();
        let template_ids = input
            .items
            .iter()
            .filter_map(|item| match &item.content {
                EmailBatchContentInput::LocalTemplate { template_id, .. }
                    if seen_templates.insert(template_id.as_str().to_owned()) =>
                {
                    Some(template_id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let snapshot = self
            .repository
            .email_batch_preflight_snapshot(&input.mailbox_ref_id, &creator_ids, &template_ids)
            .map_err(classify_lookup_error)
            .map_err(|error| error.to_string())?;
        let mailbox = snapshot.mailbox.clone();
        ensure_mailbox(&mailbox, EmailCapability::Send)?;
        ensure_mailbox(&mailbox, EmailCapability::SentReadback)?;
        let mut items = Vec::with_capacity(input.items.len());
        for item in &input.items {
            match self.prepare_batch_item(&mailbox, item, &snapshot) {
                Ok(item) => items.push(item),
                Err(BatchPreflightError::Item(_error))
                    if input.validation == EmailBatchValidation::Permissive =>
                {
                    let (creator_id, contact_route_id) = match &item.recipient {
                        EmailBatchRecipient::CreatorRoute {
                            creator_id,
                            contact_route_id,
                        } => (Some(creator_id.clone()), Some(contact_route_id.clone())),
                        EmailBatchRecipient::ExplicitAddress { .. } => (None, None),
                    };
                    items.push(FrozenEmailBatchItem {
                        creator_id,
                        contact_route_id,
                        preflight_rejection: Some(_error),
                        request: EmailBatchItem {
                            from_name: None,
                            from_email: mailbox.email_address.clone(),
                            to: Vec::new(),
                            cc: Vec::new(),
                            bcc: Vec::new(),
                            reply_to: Vec::new(),
                            in_reply_to: None,
                            references: Vec::new(),
                            subject: Some(String::new()),
                            text: None,
                            html: None,
                            template: None,
                        },
                    });
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        let request = EmailBatchRequest {
            idempotency_key: input.idempotency_key.trim().to_owned(),
            validation: match input.validation {
                EmailBatchValidation::Strict => {
                    onecrew_email_contracts::EmailBatchValidation::Strict
                }
                EmailBatchValidation::Permissive => {
                    onecrew_email_contracts::EmailBatchValidation::Permissive
                }
            },
            items: items.iter().map(|item| item.request.clone()).collect(),
        };
        let logical_fingerprint = logical_fingerprint(&request)?;
        let action_fingerprint = action_fingerprint(input, &logical_fingerprint)?;
        Ok(PreparedEmailBatch {
            mailbox,
            request,
            logical_fingerprint,
            action_fingerprint,
            items,
        })
    }

    fn prepare_batch_item(
        &self,
        mailbox: &MailboxDetail,
        input: &EmailBatchItemInput,
        snapshot: &EmailBatchPreflightSnapshot,
    ) -> Result<FrozenEmailBatchItem, BatchPreflightError> {
        let (to, creator_id, contact_route_id) = match &input.recipient {
            EmailBatchRecipient::ExplicitAddress { to } => {
                let to = EmailBatchItemInput::normalized_addresses(to, "to").map_err(item_error)?;
                if to.is_empty() {
                    return Err(item_error(
                        "explicit address recipient requires at least one address".into(),
                    ));
                }
                (to, None, None)
            }
            EmailBatchRecipient::CreatorRoute {
                creator_id,
                contact_route_id,
            } => {
                let creator = snapshot
                    .creators
                    .get(creator_id)
                    .ok_or_else(|| item_error("Creator was not found".into()))?;
                if creator.archived_at.is_some() || creator.stage == CreatorStage::Archived {
                    return Err(item_error(
                        "archived Creator cannot be used for outreach".into(),
                    ));
                }
                let route = creator.email_routes.get(contact_route_id).ok_or_else(|| {
                    item_error("email route does not belong to this Creator".into())
                })?;
                (
                    vec![route.clone()],
                    Some(creator_id.clone()),
                    Some(contact_route_id.clone()),
                )
            }
        };
        let cc = EmailBatchItemInput::normalized_addresses(&input.cc, "cc").map_err(item_error)?;
        let bcc =
            EmailBatchItemInput::normalized_addresses(&input.bcc, "bcc").map_err(item_error)?;
        let reply_to = EmailBatchItemInput::normalized_addresses(&input.reply_to, "reply_to")
            .map_err(item_error)?;
        let from_name = input
            .from_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let (subject, text, html, template): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<EmailBatchTemplate>,
        ) = match &input.content {
            EmailBatchContentInput::InlineRendered {
                subject,
                text,
                html,
            } => {
                let (subject, text, html) =
                    EmailBatchItemInput::render_inline(subject, text.as_deref(), html.as_deref())
                        .map_err(item_error)?;
                (Some(subject), text, html, None)
            }
            EmailBatchContentInput::LocalTemplate {
                template_id,
                revision,
                variables,
            } => {
                if *revision == 0 {
                    return Err(item_error(
                        "local template revision must be positive".into(),
                    ));
                }
                for key in variables.keys() {
                    crate::mail::templates::validate_template_key(key).map_err(item_error)?;
                }
                let template = snapshot
                    .templates
                    .get(template_id)
                    .ok_or_else(|| item_error("email template not found".into()))?;
                let mut values =
                    creator_builtins(creator_id.as_ref().and_then(|id| snapshot.creators.get(id)));
                values.extend(
                    variables
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
                let mut unresolved = std::collections::BTreeSet::new();
                let subject = crate::mail::templates::render_template_value(
                    &template.subject,
                    &values,
                    &mut unresolved,
                )
                .map_err(item_error)?;
                let text = crate::mail::templates::render_template_value(
                    &template.text,
                    &values,
                    &mut unresolved,
                )
                .map_err(item_error)?;
                let rendered = crate::RenderedEmailTemplate {
                    template_id: template.id.clone(),
                    revision: template.revision,
                    subject,
                    text,
                    html: template.html.clone(),
                    unresolved: unresolved.into_iter().collect(),
                };
                if rendered.revision != *revision {
                    return Err(item_error(format!(
                        "local template revision mismatch: expected {}, found {}",
                        revision, rendered.revision
                    )));
                }
                if !rendered.unresolved.is_empty() {
                    return Err(item_error(format!(
                        "local template has unresolved variables: {}",
                        rendered.unresolved.join(", ")
                    )));
                }
                let (subject, text, html) = EmailBatchItemInput::render_inline(
                    &rendered.subject,
                    Some(&rendered.text),
                    rendered.html.as_deref(),
                )
                .map_err(item_error)?;
                (
                    Some(subject),
                    text,
                    html,
                    Some(EmailBatchTemplate::Local {
                        template_id: template_id.as_str().to_owned(),
                        revision: *revision,
                        variables: variables.clone(),
                    }),
                )
            }
            EmailBatchContentInput::PublishedTemplate {
                template_id_or_alias,
                variables,
                from_override,
                subject_override,
                reply_to_override,
            } => {
                if template_id_or_alias.trim().is_empty() {
                    return Err(item_error(
                        "published template id or alias is required".into(),
                    ));
                }
                if from_override
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
                    || reply_to_override
                        .iter()
                        .any(|value| value.trim().is_empty())
                {
                    return Err(item_error("published template override is empty".into()));
                }
                (
                    subject_override
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                    None,
                    None,
                    Some(EmailBatchTemplate::Published {
                        template_id_or_alias: template_id_or_alias.clone(),
                        variables: variables.clone(),
                        from_override: from_override.clone(),
                        subject_override: subject_override.clone(),
                        reply_to_override: reply_to_override.clone(),
                    }),
                )
            }
        };
        Ok(FrozenEmailBatchItem {
            creator_id,
            contact_route_id,
            preflight_rejection: None,
            request: EmailBatchItem {
                from_name,
                from_email: mailbox.email_address.clone(),
                to,
                cc,
                bcc,
                reply_to,
                in_reply_to: input.in_reply_to.clone(),
                references: input.references.clone(),
                subject,
                text,
                html,
                template,
            },
        })
    }

    pub async fn send_email_many(
        &self,
        input: SendManyEmailInput,
    ) -> Result<EmailBatchActionResult, crate::EmailSendFailure> {
        let _guard =
            crate::action_guard::InFlightActionGuard::acquire(self, &input.idempotency_key)
                .map_err(crate::EmailSendFailure::not_sent)?;
        let prepare_runtime = self.clone();
        let preflight_input = input.clone();
        let prepared = self
            .repository_read_segment(move || {
                prepare_runtime.prepare_batch_request(&preflight_input)
            })
            .await
            .map_err(|error| {
                crate::EmailSendFailure::not_sent(format!("email batch preflight failed: {error}"))
            })?;
        let request = prepared.request;
        let fingerprint = prepared.action_fingerprint;
        let provider_payload_fingerprint = prepared.logical_fingerprint;
        let mailbox_id = prepared.mailbox.mailbox_ref_id.clone();
        let key = input.idempotency_key.clone();
        let items = prepared.items;
        let claim_runtime = self.clone();
        let (claim, replayed) = self
            .repository_write_segment(move || {
                if let Some(claim) = claim_runtime.repository.find_recoverable_action(
                    &key,
                    EMAIL_BATCH_ACTION,
                    Some(&mailbox_id),
                    None,
                    &fingerprint,
                )? {
                    return Ok((claim, true));
                }
                let mailbox = claim_runtime.require_mailbox_ref(&mailbox_id)?;
                ensure_mailbox(&mailbox, EmailCapability::Send)?;
                ensure_mailbox(&mailbox, EmailCapability::SentReadback)?;
                let recovery = RemoteStartedEmailBatchAction {
                    mailbox,
                    request,
                    logical_fingerprint: provider_payload_fingerprint,
                    action_fingerprint: fingerprint,
                    items,
                };
                let recovery_json =
                    serde_json::to_value(&recovery).map_err(|error| error.to_string())?;
                let recovery_fingerprint = recovery.action_fingerprint.clone();
                let claim = claim_runtime.repository.claim_remote_started_action(
                    &key,
                    EMAIL_BATCH_ACTION,
                    Some(&mailbox_id),
                    None,
                    &recovery_fingerprint,
                    recovery_json,
                )?;
                Ok((claim, false))
            })
            .await
            .map_err(batch_claim_error)?;
        self.resume_email_batch_claim(claim, replayed).await
    }

    /// Reconcile an email batch that is intentionally held in
    /// `remote_accepted`. This operation never calls the batch-send endpoint;
    /// accepted entries are verified through sent readback before completion.
    pub async fn reconcile_email_send_many(
        &self,
        input: ReconcileEmailBatchInput,
    ) -> Result<EmailBatchActionResult, crate::EmailSendFailure> {
        // Reconciliation never calls the batch-send endpoint, so until a
        // remote_accepted claim proves the provider already holds the batch,
        // every refusal here leaves the mail exactly where it was.
        validate_idempotency_key(&input.idempotency_key)
            .map_err(crate::EmailSendFailure::not_sent)?;
        let _guard =
            crate::action_guard::InFlightActionGuard::acquire(self, &input.idempotency_key)
                .map_err(crate::EmailSendFailure::not_sent)?;
        if input.logical_fingerprint.trim().is_empty() {
            return Err(crate::EmailSendFailure::not_sent(
                "email batch logical_fingerprint is required",
            ));
        }
        let key = input.idempotency_key.clone();
        let mailbox_id = input.mailbox_ref_id.clone();
        let fingerprint = input.logical_fingerprint.clone();
        let runtime = self.clone();
        let claim = self
            .repository_read_segment(move || {
                runtime.repository.find_recoverable_action(
                    &key,
                    EMAIL_BATCH_ACTION,
                    Some(&mailbox_id),
                    None,
                    &fingerprint,
                )
            })
            .await
            .map_err(|error| {
                crate::EmailSendFailure::not_sent(format!(
                    "email batch reconciliation lookup failed: {error}"
                ))
            })?
            .ok_or_else(|| {
                crate::EmailSendFailure::not_sent("email batch reconciliation claim was not found")
            })?;
        let value = match claim {
            crate::RecoverableActionClaim::Completed(value) => {
                let mut result: EmailBatchActionResult = serde_json::from_value(value)
                    .map_err(|error| crate::EmailSendFailure::sent(error.to_string()))?;
                result.replayed = true;
                return Ok(result);
            }
            crate::RecoverableActionClaim::RemoteAccepted(value) => value,
            // No acceptance evidence exists for this key, and this call starts
            // no send of its own.
            _ => {
                return Err(crate::EmailSendFailure::not_sent(
                    "email batch reconciliation requires a remote_accepted durable action",
                ));
            }
        };
        // From here the durable claim proves the provider accepted this batch.
        let mut recovery: RemoteAcceptedEmailBatchAction = serde_json::from_value(value)
            .map_err(|error| crate::EmailSendFailure::sent(error.to_string()))?;
        if recovery.started.action_fingerprint != input.logical_fingerprint {
            return Err(crate::EmailSendFailure::sent(
                "email batch reconciliation fingerprint does not match the frozen action",
            ));
        }
        let normalized = normalize_provider_result(
            &recovery.result,
            &recovery.started.request,
            &recovery.started.logical_fingerprint,
            &recovery.started.action_fingerprint,
            &recovery.started.items,
        );
        let mut overrides = BTreeMap::new();
        for item in input.items {
            if overrides.insert(item.input_index, item.outcome).is_some() {
                return Err(crate::EmailSendFailure::sent(
                    "email batch reconciliation contains duplicate input_index",
                ));
            }
        }
        for item in &normalized.outcomes {
            if !matches!(item.outcome, EmailBatchOutcome::Unknown { .. }) {
                continue;
            }
            let Some(outcome) = overrides.remove(&item.input_index) else {
                return Err(crate::EmailSendFailure::sent(format!(
                    "email batch item {} still requires reconciliation",
                    item.input_index
                )));
            };
            match outcome {
                EmailBatchReconciliationOutcome::Accepted {
                    provider_message_id,
                } => {
                    if provider_message_id.trim().is_empty() {
                        return Err(crate::EmailSendFailure::sent(format!(
                            "email batch item {} provider_message_id is required",
                            item.input_index
                        )));
                    }
                    replace_provider_outcome(
                        &mut recovery.result,
                        item.input_index,
                        EmailBatchItemOutcome::Accepted {
                            provider_message_id,
                        },
                    );
                }
                EmailBatchReconciliationOutcome::Rejected { message } => {
                    if message.trim().is_empty() {
                        return Err(crate::EmailSendFailure::sent(format!(
                            "email batch item {} rejection message is required",
                            item.input_index
                        )));
                    }
                    replace_provider_outcome(
                        &mut recovery.result,
                        item.input_index,
                        EmailBatchItemOutcome::Rejected { message },
                    );
                }
            }
        }
        if let Some((index, _)) = overrides.into_iter().next() {
            return Err(crate::EmailSendFailure::sent(format!(
                "email batch reconciliation input_index {index} is not unknown"
            )));
        }
        // The original provider response was explicitly uncorrelatable. The
        // operator-supplied mapping is the reconciliation authority; retain
        // the frozen provider payload fingerprint for the final commit.
        recovery.result.logical_fingerprint = recovery.started.logical_fingerprint.clone();
        self.finish_email_batch(recovery, true).await
    }

    async fn resume_email_batch_claim(
        &self,
        claim: crate::RecoverableActionClaim,
        replayed: bool,
    ) -> Result<EmailBatchActionResult, crate::EmailSendFailure> {
        match claim {
            crate::RecoverableActionClaim::Completed(value) => {
                let mut result: EmailBatchActionResult = serde_json::from_value(value)
                    .map_err(|error| crate::EmailSendFailure::sent(error.to_string()))?;
                result.replayed = true;
                Ok(result)
            }
            // A durable `failed` batch claim records a terminal provider rejection.
            crate::RecoverableActionClaim::Failed(value) => {
                Err(crate::EmailSendFailure::not_sent(failed_replay(value)))
            }
            crate::RecoverableActionClaim::RemoteStarted(value) => {
                // The batch was claimed but never handed to the provider.
                let recovery: RemoteStartedEmailBatchAction = serde_json::from_value(value)
                    .map_err(|error| crate::EmailSendFailure::not_sent(error.to_string()))?;
                let gateway = self.gateways.email.as_ref().ok_or_else(|| {
                    crate::EmailSendFailure::not_sent("email batch gateway is not configured; retry the same action and idempotency key")
                })?;
                let result = gateway
                    .send_many_email(
                        &recovery.mailbox.mailbox_ref_id,
                        recovery.request.clone(),
                    )
                    .await
                    .map_err(|error| {
                        if error.is_terminal_rejection() {
                            // The provider rejected the batch outright.
                            crate::EmailSendFailure::not_sent(self.fail_email_action_for(
                                &recovery.request.idempotency_key,
                                EMAIL_BATCH_ACTION,
                                crate::ActionFailureKind::RemoteWriteRejected,
                                error.to_string(),
                                serde_json::to_value(&error).ok(),
                            ))
                        } else {
                            // The provider may still have taken the batch.
                            crate::EmailSendFailure::unknown(format!(
                                "{error}; durable batch claim remains remote_started; retry the same action and idempotency key"
                            ))
                        }
                    })?;
                let accepted = RemoteAcceptedEmailBatchAction {
                    started: recovery,
                    result,
                };
                let stage = accepted.clone();
                let key = stage.started.request.idempotency_key.clone();
                let fingerprint = stage.started.action_fingerprint.clone();
                let runtime = self.clone();
                self.repository_write_segment(move || {
                    runtime.repository.stage_remote_accepted_action(
                        &key,
                        EMAIL_BATCH_ACTION,
                        &fingerprint,
                        serde_json::to_value(stage).map_err(|error| error.to_string())?,
                    )
                })
                .await
                .map_err(|error| {
                    crate::EmailSendFailure::sent(format!(
                        "batch provider acceptance was received but durable staging failed: {error}"
                    ))
                })?;
                self.finish_email_batch(accepted, replayed).await
            }
            crate::RecoverableActionClaim::RemoteAccepted(value) => {
                let recovery: RemoteAcceptedEmailBatchAction = serde_json::from_value(value)
                    .map_err(|error| crate::EmailSendFailure::sent(error.to_string()))?;
                self.finish_email_batch(recovery, replayed).await
            }
            // An unsupported durable state carries no evidence either way.
            crate::RecoverableActionClaim::RemoteVerified(_) => {
                Err(crate::EmailSendFailure::unknown(
                    "email batch recovery has an unsupported remote_verified state; retry the same action and idempotency key",
                ))
            }
        }
    }

    async fn finish_email_batch(
        &self,
        recovery: RemoteAcceptedEmailBatchAction,
        replayed: bool,
    ) -> Result<EmailBatchActionResult, crate::EmailSendFailure> {
        let mut result = normalize_provider_result(
            &recovery.result,
            &recovery.started.request,
            &recovery.started.logical_fingerprint,
            &recovery.started.action_fingerprint,
            &recovery.started.items,
        );
        if result
            .outcomes
            .iter()
            .any(|item| matches!(item.outcome, EmailBatchOutcome::Unknown { .. }))
        {
            // The provider itself could not say what happened to these items.
            return Err(crate::EmailSendFailure::unknown(
                "email batch contains provider-unknown items; durable action remains remote_accepted for manual reconciliation; retry the same action and idempotency key after reconciliation",
            ));
        }
        let gateway = self.gateways.email.as_ref().ok_or_else(|| {
            crate::EmailSendFailure::sent("email batch readback requires the email gateway; retry the same action and idempotency key")
        })?;
        let mut verified = Vec::new();
        let mut readback_pending = false;
        let mut undelivered = false;
        for item in &mut result.outcomes {
            let EmailBatchOutcome::Accepted {
                provider_message_id,
            } = &item.outcome
            else {
                continue;
            };
            let Some(frozen) = recovery.started.items.get(item.input_index) else {
                item.outcome = EmailBatchOutcome::Unknown {
                    message: "provider returned an out-of-range accepted item".into(),
                };
                readback_pending = true;
                continue;
            };
            match gateway
                .sent_email(
                    &recovery.started.mailbox.mailbox_ref_id,
                    provider_message_id,
                )
                .await
            {
                Ok(message) => {
                    // The borrow ends here so the verified arm can take the
                    // message while the other arms still read its status.
                    let delivery = verify_batch_item_readback(provider_message_id, &message);
                    match delivery {
                        Ok(SentDelivery::Sent) => {
                            verified.push(EmailBatchVerifiedItem {
                                input_index: item.input_index,
                                provider_message_id: provider_message_id.clone(),
                                message,
                                creator_id: frozen.creator_id.clone(),
                                contact_route_id: frozen.contact_route_id.clone(),
                                in_reply_to: frozen.request.in_reply_to.clone(),
                                references: frozen.request.references.clone(),
                            });
                        }
                        // The provider is still moving this item, so the same
                        // key picks it up once the status advances.
                        Ok(SentDelivery::Advancing) => {
                            item.outcome = EmailBatchOutcome::Unknown {
                                message: format!(
                                    "provider has not finished sending this item; its delivery status is still {}",
                                    message.last_event
                                ),
                            };
                            readback_pending = true;
                        }
                        // Known and terminal. `Rejected` would record that this
                        // item was never sent, which is false, so it stays an
                        // item only reconciliation can settle.
                        Ok(SentDelivery::Failed) => {
                            item.outcome = EmailBatchOutcome::Unknown {
                                message: format!(
                                    "provider accepted this item but reports delivery status {}; it was not delivered",
                                    message.last_event
                                ),
                            };
                            undelivered = true;
                        }
                        Err(error) => {
                            item.outcome = EmailBatchOutcome::Unknown { message: error };
                            readback_pending = true;
                        }
                    }
                }
                Err(error) => {
                    item.outcome = EmailBatchOutcome::Unknown {
                        message: format!("batch item readback failed: {error}"),
                    };
                    readback_pending = true;
                }
            }
        }
        if undelivered {
            // The provider accepted these items and then could not deliver
            // them. Reading the status again returns the same answer, so
            // reconciliation is what settles them. The durable claim stays
            // remote_accepted on purpose: the mail did leave OneCrew, and an
            // accepted send is never recorded as one that never happened.
            return Err(crate::EmailSendFailure::sent(
                "email batch contains items the provider accepted but did not deliver; retrying the same action and idempotency key cannot change that; reconcile those items to settle this batch; no provider resend will occur",
            ));
        }
        if readback_pending {
            // The provider accepted these items with stable message ids; only
            // reading them back afterwards is incomplete.
            return Err(crate::EmailSendFailure::sent(
                "email batch readback is incomplete; retry the same action and idempotency key; no provider resend will occur",
            ));
        }
        result.replayed = replayed;
        let idempotency_key = recovery.started.request.idempotency_key.clone();
        let commit = EmailBatchCommitInput {
            mailbox: recovery.started.mailbox,
            request: recovery.started.request,
            logical_fingerprint: recovery.started.action_fingerprint,
            result,
            verified,
            idempotency_key,
        };
        let commit_runtime = self.clone();
        let mut committed = self
            .repository_write_segment(move || {
            commit_runtime.repository.commit_email_batch_action(commit)
            })
            .await
            .map_err(|error| {
                crate::EmailSendFailure::sent(format!(
                    "batch readback was resolved but local durable commit failed: {error}; retry the same action and idempotency key"
                ))
            })?;
        committed.replayed = replayed;
        Ok(committed)
    }
}

fn logical_fingerprint(request: &EmailBatchRequest) -> Result<String, String> {
    let bytes = serde_json::to_vec(&request.items).map_err(|error| error.to_string())?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod byte_goldens;

#[derive(Serialize)]
struct EmailBatchActionIdentity<'a> {
    mailbox_ref_id: &'a MailboxRefId,
    validation: EmailBatchValidation,
    items: &'a [EmailBatchItemInput],
}

fn action_fingerprint(
    input: &SendManyEmailInput,
    provider_payload_fingerprint: &str,
) -> Result<String, String> {
    let identity = EmailBatchActionIdentity {
        mailbox_ref_id: &input.mailbox_ref_id,
        validation: input.validation,
        items: &input.items,
    };
    let mut bytes = serde_json::to_vec(&identity).map_err(|error| error.to_string())?;
    bytes.extend_from_slice(provider_payload_fingerprint.as_bytes());
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn normalize_provider_result(
    result: &EmailBatchResult,
    request: &EmailBatchRequest,
    expected_provider_fingerprint: &str,
    action_fingerprint: &str,
    frozen_items: &[FrozenEmailBatchItem],
) -> EmailBatchActionResult {
    let mut by_index = BTreeMap::new();
    for item in &result.outcomes {
        let outcome = match &item.outcome {
            EmailBatchItemOutcome::Accepted {
                provider_message_id,
            } if !provider_message_id.trim().is_empty() => EmailBatchOutcome::Accepted {
                provider_message_id: provider_message_id.clone(),
            },
            EmailBatchItemOutcome::Accepted { .. } => EmailBatchOutcome::Unknown {
                message: "provider returned an accepted item without a stable message id".into(),
            },
            EmailBatchItemOutcome::Rejected { message } => EmailBatchOutcome::Rejected {
                message: message.clone(),
            },
            EmailBatchItemOutcome::Unknown { message } => EmailBatchOutcome::Unknown {
                message: message.clone(),
            },
        };
        by_index.entry(item.input_index).or_insert(outcome);
    }
    let mut outcomes = Vec::with_capacity(request.items.len());
    for index in 0..request.items.len() {
        let outcome = by_index
            .remove(&index)
            .unwrap_or(EmailBatchOutcome::Unknown {
                message: "provider omitted an item outcome".into(),
            });
        let outcome = frozen_items
            .get(index)
            .and_then(|item| item.preflight_rejection.as_ref())
            .map(|message| EmailBatchOutcome::Rejected {
                message: message.clone(),
            })
            .unwrap_or(outcome);
        outcomes.push(EmailBatchItemResult {
            input_index: index,
            creator_id: frozen_items
                .get(index)
                .and_then(|item| item.creator_id.clone()),
            contact_route_id: frozen_items
                .get(index)
                .and_then(|item| item.contact_route_id.clone()),
            payload_fingerprint: request
                .items
                .get(index)
                .and_then(|item| serde_json::to_vec(item).ok())
                .map(|bytes| {
                    Sha256::digest(bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect()
                })
                .unwrap_or_default(),
            outcome,
        });
    }
    if result.logical_fingerprint != expected_provider_fingerprint {
        for item in &mut outcomes {
            if matches!(item.outcome, EmailBatchOutcome::Accepted { .. }) {
                item.outcome = EmailBatchOutcome::Unknown {
                    message: "provider logical fingerprint did not match the frozen request".into(),
                };
            }
        }
    }
    let chunks = result
        .chunks
        .iter()
        .map(|chunk| EmailBatchChunkResult {
            chunk_index: chunk.chunk_index,
            idempotency_key: chunk.idempotency_key.clone(),
            outcomes: chunk
                .outcomes
                .iter()
                .filter_map(|item| outcomes.get(item.input_index).cloned())
                .collect(),
        })
        .collect();
    EmailBatchActionResult {
        logical_fingerprint: action_fingerprint.to_owned(),
        provider_payload_fingerprint: expected_provider_fingerprint.to_owned(),
        chunks,
        outcomes,
        replayed: false,
    }
}

fn replace_provider_outcome(
    result: &mut EmailBatchResult,
    input_index: usize,
    outcome: EmailBatchItemOutcome,
) {
    let mut replaced = false;
    for item in &mut result.outcomes {
        if item.input_index == input_index {
            item.outcome = outcome.clone();
            replaced = true;
        }
    }
    for chunk in &mut result.chunks {
        for item in &mut chunk.outcomes {
            if item.input_index == input_index {
                item.outcome = outcome.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        result
            .outcomes
            .push(onecrew_email_contracts::EmailBatchItemResult {
                input_index,
                outcome,
            });
    }
}

/// Identity is the one thing a batch readback must still prove; delivery comes
/// from the provider's own status. The request fields are no longer compared:
/// Resend echoes back what it normalized, so one reformatted address or subject
/// wedged an accepted batch forever, with no resend available to clear it.
fn verify_batch_item_readback(
    provider_message_id: &str,
    readback: &PlatformEmailMessage,
) -> Result<SentDelivery, String> {
    if readback.inbound || readback.provider_message_id != provider_message_id {
        return Err("sent batch item readback did not match the accepted message".into());
    }
    Ok(sent_delivery(&readback.last_event))
}

/// The batch claim segment runs before the provider is asked, so every outcome
/// here leaves the batch unsent.
fn batch_claim_error(error: crate::CreatorRepositoryCallError<String>) -> crate::EmailSendFailure {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => crate::EmailSendFailure::not_sent(error),
        crate::CreatorRepositoryCallError::Dispatch(error) => {
            crate::EmailSendFailure::not_sent(format!(
                "email batch durable claim failed: {error}; retry only with the same action and idempotency key"
            ))
        }
    }
}
