use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use onecrew_creator_runtime::{
    CreatorContactRouteId, CreatorContactRouteRecord, CreatorId, CreatorRecord, CreatorRuntime,
    EmailBatchContentInput, EmailBatchItemInput, EmailBatchRecipient,
    EmailBatchReconciliationOutcome, EmailBatchValidation, EmailDraftGetInput, EmailSendFailure,
    MailboxDetail, MailboxRefId, PrepareEmailInput, PrepareManualEmailInput,
    ReconcileEmailBatchInput, SendEmailDraftInput, SendEmailInput, SendManualEmailInput,
    SendManyEmailInput,
};

use super::models::{EmailSendIntent, PreparedEmailAction};
use crate::{
    LocalPlatform, NoticeTone, OwnerReviewContext, PreparedOwnerReview, PresentationField,
    PresentationItem, PresentationProjection, PresentationSection, ResultPresentationKind,
    ResultStoreError,
};

/// A review is a decision the user is making now. It expires on the same budget
/// as outreach, so a stale card can never send an hour-old draft.
const PREPARE_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// A batch review lists who is being mailed. The list is bounded so one large
/// batch cannot push the review past the result store's projection budget.
const MAX_REVIEW_ITEMS: usize = 100;

/// A batch row whose Creator or contact route can no longer be read. The row
/// still appears — a mass send the reader cannot see in full is worse than one
/// row they cannot identify — and says plainly that it has no name rather than
/// falling back to the identifier that failed to resolve.
const UNNAMED_RECIPIENT: &str = "(recipient could not be named)";

/// A Creator as a batch row names them: the display name a reader recognizes,
/// and the address each of that Creator's saved routes reaches.
///
/// One `CreatorRecord` read fills both, because the record already carries the
/// Creator's contact routes. Routes are keyed by their identifier so a batch
/// addressing several routes of one Creator still costs a single read; the key
/// selects a row's address and is never rendered.
struct NamedCreator {
    display_name: String,
    route_addresses: HashMap<CreatorContactRouteId, String>,
}

impl LocalPlatform {
    /// Run one synchronous Creator prepare use case off the async worker.
    ///
    /// These use cases are blocking repository reads with no async wrapper of
    /// their own, so the blocking pool owns them here exactly as the MCP
    /// adapter's own `mail` reads do.
    ///
    /// # Errors
    /// Reports the owner's own rejection, or a lost blocking task. Both precede
    /// the provider: a card that was never minted claimed nothing.
    async fn creator_prepare<T, F>(&self, work: F) -> Result<T, EmailSendFailure>
    where
        T: Send + 'static,
        F: FnOnce(&CreatorRuntime) -> Result<T, String> + Send + 'static,
    {
        let creators = Arc::clone(&self.creators);
        tokio::task::spawn_blocking(move || work(&creators))
            .await
            .map_err(|_| {
                EmailSendFailure::not_sent(
                    "the review could not read the mailbox; nothing was sent",
                )
            })?
            .map_err(EmailSendFailure::not_sent)
    }

    /// The mailbox a batch card sends from. A batch addresses many recipients,
    /// so its sender is read directly rather than through a per-recipient
    /// preparation.
    ///
    /// # Errors
    /// Reports a mailbox the Creator owner does not hold; nothing was sent.
    async fn review_mailbox(
        &self,
        mailbox_ref_id: MailboxRefId,
    ) -> Result<MailboxDetail, EmailSendFailure> {
        self.creator_prepare(move |creators| {
            creators
                .list_mailboxes()?
                .into_iter()
                .find(|mailbox| mailbox.mailbox_ref_id == mailbox_ref_id)
                .ok_or_else(|| {
                    format!(
                        "mailbox {} is not available; nothing was sent",
                        mailbox_ref_id.as_str()
                    )
                })
        })
        .await
    }

    /// Name the Creator-routed recipients the batch card will actually list.
    ///
    /// The work is bounded by the display cap, not by the batch: the card lists
    /// at most `MAX_REVIEW_ITEMS` rows, so at most that many Creators are read
    /// however long `SendManyEmailInput::items` is, and one read per unique
    /// Creator however many of their routes the batch addresses. Resolving past
    /// the listed prefix would be unbounded work for rows nobody reads.
    ///
    /// A Creator that cannot be read — deleted, or holding no such route any
    /// more — is simply absent from the result, and its row degrades to an
    /// unnamed recipient. One unreadable recipient must not deny the owner the
    /// rest of the confirmation, which is the only place the remaining
    /// recipients can still be checked before the batch leaves.
    ///
    /// # Errors
    /// Reports only a lost blocking task, exactly as the mailbox read before it
    /// does. Nothing was sent.
    async fn review_recipient_names(
        &self,
        items: &[EmailBatchItemInput],
    ) -> Result<HashMap<CreatorId, NamedCreator>, EmailSendFailure> {
        let listed = listed_creator_ids(items);
        if listed.is_empty() {
            return Ok(HashMap::new());
        }
        self.creator_prepare(move |creators| {
            Ok(listed
                .into_iter()
                .filter_map(|creator_id| {
                    let record = creators.get_creator(&creator_id).ok()?;
                    Some((creator_id, named_creator(record)))
                })
                .collect())
        })
        .await
    }

    /// Freeze one Creator-route email send for owner review.
    ///
    /// Nothing is claimed and nothing is sent here. The message leaves only
    /// when the grant this mints is consumed.
    ///
    /// The Creator owner resolves the mailbox, Creator, and route, so the card
    /// names them as a reader knows them. That resolution is the owner's own
    /// send preparation, so the checks it performs now refuse an impossible
    /// send before the card is minted rather than after the owner confirms it.
    ///
    /// # Errors
    /// Rejects whatever the Creator owner's preparation rejects, an invalid
    /// idempotency key, and review copy the result store refuses. All precede
    /// the provider, so no mail can have been sent.
    pub async fn prepare_email_direct_send(
        &self,
        input: SendEmailInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, EmailSendFailure> {
        let preparation = {
            let creator_id = input.creator_id.clone();
            let contact_route_id = input.contact_route_id.clone();
            let mailbox_ref_id = input.mailbox_ref_id.clone();
            let reply_to_email_id = input.reply_to_email_id.clone();
            self.creator_prepare(move |creators| {
                creators.prepare_email(PrepareEmailInput {
                    creator_id,
                    contact_route_id,
                    mailbox_ref_id,
                    reply_to_email_id,
                })
            })
            .await?
        };
        let fields = direct_send_fields(
            &preparation.mailbox,
            preparation.creator.creator.display_name.as_str(),
            route_address(&preparation.route),
            preparation
                .reply_to
                .as_ref()
                .map(|message| message.subject.as_str()),
            input.subject.as_str(),
            input.text.as_str(),
        );
        let idempotency_key = input.idempotency_key.clone();
        seal(
            EmailSendIntent::Direct(Box::new(input)),
            idempotency_key,
            review_context,
            "Review Creator email",
            "Send this email to the selected Creator route?",
            vec![fields_section(fields)],
        )
    }

    /// Freeze one manually addressed email send for owner review.
    ///
    /// The Creator owner resolves the mailbox, so the card names its sender.
    /// That resolution also checks the mailbox can send and read back what it
    /// sent, which now refuses an unusable mailbox before the card is minted
    /// rather than after the owner confirms it.
    ///
    /// # Errors
    /// Rejects an unusable mailbox or recipient address, an invalid idempotency
    /// key, and review copy the result store refuses; all precede the provider.
    pub async fn prepare_email_manual_send(
        &self,
        input: SendManualEmailInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, EmailSendFailure> {
        let preparation = {
            let mailbox_ref_id = input.mailbox_ref_id.clone();
            let to = input.to.clone();
            self.creator_prepare(move |creators| {
                creators.prepare_manual_email(PrepareManualEmailInput { mailbox_ref_id, to })
            })
            .await?
        };
        let fields = message_fields(
            &preparation.mailbox,
            preparation.to.as_str(),
            input.subject.as_str(),
            input.text.as_str(),
        );
        let idempotency_key = input.idempotency_key.clone();
        seal(
            EmailSendIntent::Manual(Box::new(input)),
            idempotency_key,
            review_context,
            "Review manual email",
            "Send this email to the explicitly entered address?",
            vec![fields_section(fields)],
        )
    }

    /// Freeze one saved draft send for owner review.
    ///
    /// The draft is read under bounded admission so the card shows the real
    /// recipient, subject, and body rather than an opaque identifier. The
    /// sealed intent still carries the caller's `expected_revision`, so a draft
    /// edited after this review is rejected at execution instead of sent.
    ///
    /// The sender comes from the same preparation the send itself will use, so
    /// the card names the mailbox the draft actually leaves from, and a draft
    /// that cannot be delivered is refused before the card is minted.
    ///
    /// # Errors
    /// Rejects an unreadable draft, whatever the Creator owner's preparation
    /// rejects, an invalid idempotency key, and review copy the result store
    /// refuses. All precede the provider.
    pub async fn prepare_email_draft_send(
        &self,
        input: SendEmailDraftInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, EmailSendFailure> {
        let draft = self
            .creators
            .get_email_draft_async(EmailDraftGetInput {
                draft_id: input.draft_id.clone(),
            })
            .await
            .map_err(|error| match error {
                onecrew_creator_runtime::CreatorRepositoryCallError::Owner(error) => {
                    EmailSendFailure::not_sent(error)
                }
                onecrew_creator_runtime::CreatorRepositoryCallError::Dispatch(error) => {
                    EmailSendFailure::not_sent(format!(
                        "the email draft could not be read for review: {error}; nothing was sent"
                    ))
                }
            })?;
        // The draft's own delivery decides which preparation names the sender,
        // exactly as the send will: a Creator-routed draft goes through the
        // Creator preparation, an unowned one through the manual preparation.
        let mailbox = {
            let mailbox_ref_id = draft.mailbox_ref_id.clone();
            let reply_to_email_id = draft.reply_to_email_id.clone();
            let recipient = draft.to.clone();
            let route = draft.creator_id.clone().zip(draft.contact_route_id.clone());
            self.creator_prepare(move |creators| match route {
                Some((creator_id, contact_route_id)) => creators
                    .prepare_email(PrepareEmailInput {
                        creator_id,
                        contact_route_id,
                        mailbox_ref_id,
                        reply_to_email_id,
                    })
                    .map(|preparation| preparation.mailbox),
                None => creators
                    .prepare_manual_email(PrepareManualEmailInput {
                        mailbox_ref_id,
                        to: recipient,
                    })
                    .map(|preparation| preparation.mailbox),
            })
            .await?
        };
        let fields = message_fields(
            &mailbox,
            draft.to.as_str(),
            draft.subject.as_str(),
            draft.text.as_str(),
        );
        let mut sections = vec![fields_section(fields)];
        if draft.revision != input.expected_revision {
            sections.push(PresentationSection::Notice {
                heading: Some("Draft changed".to_owned()),
                body: format!(
                    "This draft is now at revision {}, but the send was prepared against revision \
                     {}. Sending will be refused; reopen the draft and prepare the send again.",
                    draft.revision, input.expected_revision
                ),
                tone: NoticeTone::Warning,
            });
        }
        let idempotency_key = input.idempotency_key.clone();
        seal(
            EmailSendIntent::Draft(input),
            idempotency_key,
            review_context,
            "Review saved email draft",
            "Send this saved draft?",
            sections,
        )
    }

    /// Freeze one email batch send for owner review.
    ///
    /// Every listed recipient is named before the card is minted. A mass send
    /// is confirmed by reading who it reaches, and a Creator route picked out
    /// by identifier tells the reader nothing about that. A recipient that
    /// cannot be read is shown as unnamed rather than by its identifier.
    ///
    /// # Errors
    /// Rejects a mailbox the Creator owner does not hold, an invalid
    /// idempotency key, and review copy the result store refuses; all precede
    /// the provider.
    pub async fn prepare_email_batch_send(
        &self,
        input: SendManyEmailInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, EmailSendFailure> {
        let mailbox = self.review_mailbox(input.mailbox_ref_id.clone()).await?;
        let names = self.review_recipient_names(&input.items).await?;
        let sections = batch_review_sections(&mailbox, &input, &names);
        let idempotency_key = input.idempotency_key.clone();
        seal(
            EmailSendIntent::Batch(Box::new(input)),
            idempotency_key,
            review_context,
            "Review email batch",
            "Send this email batch to every recipient listed?",
            sections,
        )
    }

    /// Freeze one email batch reconciliation for owner review. Reconciliation
    /// never calls the batch-send endpoint; it records outcomes the provider
    /// already holds, which is still an owner decision.
    ///
    /// # Errors
    /// Rejects a mailbox the Creator owner does not hold, an invalid
    /// idempotency key, and review copy the result store refuses.
    pub async fn prepare_email_batch_reconcile(
        &self,
        input: ReconcileEmailBatchInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, EmailSendFailure> {
        let mailbox = self.review_mailbox(input.mailbox_ref_id.clone()).await?;
        let sections = reconcile_review_sections(&mailbox, &input);
        let idempotency_key = input.idempotency_key.clone();
        seal(
            EmailSendIntent::Reconcile(Box::new(input)),
            idempotency_key,
            review_context,
            "Review email batch reconciliation",
            "Record these provider outcomes without resending?",
            sections,
        )
    }
}

/// The mailbox as a person recognizes it: `Name <address>`, or the bare address
/// when the mailbox carries no display name. A reader recognizes a name, and
/// the card's own sender parser reads this exact shape, so neither is left with
/// the mailbox's identifier.
fn sender_value(mailbox: &MailboxDetail) -> String {
    match mailbox.display_name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => format!("{name} <{}>", mailbox.email_address),
        _ => mailbox.email_address.clone(),
    }
}

/// `From` is the label the card recognizes as the sender of a reviewed message.
fn sender_field(mailbox: &MailboxDetail) -> PresentationField {
    field("From", sender_value(mailbox))
}

/// The address a saved contact route reaches, preferring how it was written
/// over its normalized form.
fn route_address(route: &CreatorContactRouteRecord) -> &str {
    let display = route.locator.display_value.trim();
    if display.is_empty() {
        route.locator.normalized_value.as_str()
    } else {
        display
    }
}

/// One reviewed message: who it comes from, who it reaches, and what it says.
fn message_fields(
    mailbox: &MailboxDetail,
    recipient: &str,
    subject: &str,
    text: &str,
) -> Vec<PresentationField> {
    vec![
        sender_field(mailbox),
        field("To", recipient),
        field("Subject", subject),
        field("Body", text),
    ]
}

/// A Creator-route send names the Creator and the route it reaches; a reply
/// names the message it answers by subject.
fn direct_send_fields(
    mailbox: &MailboxDetail,
    creator_name: &str,
    recipient: &str,
    reply_subject: Option<&str>,
    subject: &str,
    text: &str,
) -> Vec<PresentationField> {
    let mut fields = vec![sender_field(mailbox)];
    push_named(&mut fields, "Creator", creator_name);
    push_named(&mut fields, "To", recipient);
    if let Some(reply_subject) = reply_subject {
        fields.push(field("Reply to", reply_subject));
    }
    fields.push(field("Subject", subject));
    fields.push(field("Body", text));
    fields
}

/// A row that exists to name something. With no name there is nothing to show:
/// printing the identifier instead is the defect these rows exist to avoid.
fn push_named(fields: &mut Vec<PresentationField>, label: &str, value: &str) {
    if !value.trim().is_empty() {
        fields.push(field(label, value));
    }
}

fn seal(
    intent: EmailSendIntent,
    idempotency_key: String,
    context: OwnerReviewContext,
    title: &str,
    prompt: &str,
    sections: Vec<PresentationSection>,
) -> Result<PreparedOwnerReview, EmailSendFailure> {
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 256 {
        return Err(EmailSendFailure::not_sent(
            "idempotency_key must contain 1 to 256 bytes",
        ));
    }
    PreparedOwnerReview::email(
        PreparedEmailAction {
            intent,
            idempotency_key,
            expires: Instant::now() + PREPARE_LIFETIME,
        },
        context,
        PresentationProjection {
            kind: ResultPresentationKind::new("email_confirmation").map_err(store_rejection)?,
            title: title.to_owned(),
            summary: Some("Confirm the exact mailbox, recipient and content shown below.".into()),
            sections,
        },
        prompt.to_owned(),
        "Send email".to_owned(),
    )
    .map_err(store_rejection)
}

/// Every rejection here happens before the review is even minted, so no claim
/// exists and nothing was handed to the mail provider.
fn store_rejection(error: ResultStoreError) -> EmailSendFailure {
    EmailSendFailure::not_sent(error.to_string())
}

fn fields_section(fields: Vec<PresentationField>) -> PresentationSection {
    PresentationSection::Fields {
        heading: None,
        fields: fields.into_iter().flat_map(review_fields).collect(),
    }
}

fn field(label: &str, value: impl Into<String>) -> PresentationField {
    let value = value.into();
    PresentationField {
        label: label.to_owned(),
        // The store rejects a blank field value. An empty subject or body is a
        // real thing to review before sending, so name it rather than drop the
        // row and leave the owner approving something they cannot see.
        value: if value.trim().is_empty() {
            "(empty)".to_owned()
        } else {
            value
        },
    }
}

/// The result store caps one presented field at 8 KiB. A long body is still
/// reviewed in full: splitting on UTF-8 boundaries preserves every byte while
/// satisfying that per-field limit.
fn review_fields(field: PresentationField) -> Vec<PresentationField> {
    const FIELD_BYTES: usize = 8 * 1024;
    if field.value.len() <= FIELD_BYTES {
        return vec![field];
    }
    let mut remaining = field.value.as_str();
    let mut result = Vec::new();
    while !remaining.is_empty() {
        let mut split = remaining.len().min(FIELD_BYTES);
        while !remaining.is_char_boundary(split) {
            split -= 1;
        }
        result.push(PresentationField {
            label: format!("{} (part {})", field.label, result.len() + 1),
            value: remaining[..split].to_owned(),
        });
        remaining = &remaining[split..];
    }
    result
}

fn validation_label(validation: EmailBatchValidation) -> &'static str {
    match validation {
        EmailBatchValidation::Strict => "strict",
        EmailBatchValidation::Permissive => "permissive",
    }
}

/// The batch card's sections: what is being sent and from where, who each
/// listed row reaches, and — when the batch runs past what the card can show —
/// that confirming still sends all of it.
fn batch_review_sections(
    mailbox: &MailboxDetail,
    input: &SendManyEmailInput,
    names: &HashMap<CreatorId, NamedCreator>,
) -> Vec<PresentationSection> {
    let fields = vec![
        sender_field(mailbox),
        field("Validation", validation_label(input.validation)),
        // The whole batch, not the listed prefix: this is the count that leaves.
        field("Recipients", input.items.len().to_string()),
    ];
    let mut sections = vec![fields_section(fields)];
    let listed = input
        .items
        .iter()
        .take(MAX_REVIEW_ITEMS)
        .map(|item| PresentationItem {
            title: recipient_label(item, names),
            description: Some(content_label(item)),
        })
        .collect::<Vec<_>>();
    if !listed.is_empty() {
        sections.push(PresentationSection::Items {
            heading: Some("Recipients".to_owned()),
            items: listed,
        });
    }
    if input.items.len() > MAX_REVIEW_ITEMS {
        sections.push(PresentationSection::Notice {
            heading: Some("List truncated".to_owned()),
            body: format!(
                "Showing the first {MAX_REVIEW_ITEMS} of {} recipients. Confirming sends the \
                 whole batch.",
                input.items.len()
            ),
            tone: NoticeTone::Warning,
        });
    }
    sections
}

/// The reconciliation card's sections. Nothing is sent here, so the card names
/// the mailbox whose provider outcomes are being recorded rather than a sender,
/// and lists those outcomes. The fingerprint binding this reconciliation to its
/// frozen request is internal bookkeeping, like the idempotency key beside it:
/// it decides nothing the reader is deciding, so the card does not show it.
fn reconcile_review_sections(
    mailbox: &MailboxDetail,
    input: &ReconcileEmailBatchInput,
) -> Vec<PresentationSection> {
    let fields = vec![
        field("Mailbox", sender_value(mailbox)),
        field("Reconciled items", input.items.len().to_string()),
    ];
    let mut sections = vec![fields_section(fields)];
    let listed = input
        .items
        .iter()
        .take(MAX_REVIEW_ITEMS)
        .map(|item| PresentationItem {
            title: format!("Item {}", item.input_index),
            description: Some(match &item.outcome {
                EmailBatchReconciliationOutcome::Accepted {
                    provider_message_id,
                } => format!("accepted as {provider_message_id}"),
                EmailBatchReconciliationOutcome::Rejected { message } => {
                    format!("rejected: {message}")
                }
            }),
        })
        .collect::<Vec<_>>();
    if !listed.is_empty() {
        sections.push(PresentationSection::Items {
            heading: Some("Outcomes".to_owned()),
            items: listed,
        });
    }
    sections
}

/// The unique Creators the card's listed rows name, in the order those rows
/// list them. Only the displayed prefix: `SendManyEmailInput::items` carries no
/// cap of its own, so resolving past the display cap would be unbounded work
/// for rows the card never shows.
fn listed_creator_ids(items: &[EmailBatchItemInput]) -> Vec<CreatorId> {
    let mut unique: Vec<CreatorId> = Vec::new();
    for item in items.iter().take(MAX_REVIEW_ITEMS) {
        let EmailBatchRecipient::CreatorRoute { creator_id, .. } = &item.recipient else {
            continue;
        };
        if !unique.contains(creator_id) {
            unique.push(creator_id.clone());
        }
    }
    unique
}

/// Reduce one read Creator record to what its batch rows show. The record
/// already carries the Creator's contact routes, so every route of theirs the
/// batch addresses is named out of this single read.
fn named_creator(record: CreatorRecord) -> NamedCreator {
    NamedCreator {
        display_name: record.creator.display_name,
        route_addresses: record
            .contact_routes
            .iter()
            .map(|route| (route.route.id.clone(), route_address(route).to_owned()))
            .collect(),
    }
}

/// Who one batch row reaches, as a reader knows them.
fn recipient_label(item: &EmailBatchItemInput, names: &HashMap<CreatorId, NamedCreator>) -> String {
    let label = match &item.recipient {
        EmailBatchRecipient::ExplicitAddress { to } => to.join(", "),
        EmailBatchRecipient::CreatorRoute {
            creator_id,
            contact_route_id,
        } => creator_route_label(names, creator_id, contact_route_id)
            .unwrap_or_else(|| UNNAMED_RECIPIENT.to_owned()),
    };
    if label.trim().is_empty() {
        "(no recipient)".to_owned()
    } else {
        label
    }
}

/// A saved Creator route as `Name <address>`, the shape the card's sender row
/// already uses. `None` when the Creator or the route could not be read, or
/// when neither carries anything to show: the identifiers that selected them
/// are never the answer.
fn creator_route_label(
    names: &HashMap<CreatorId, NamedCreator>,
    creator_id: &CreatorId,
    contact_route_id: &CreatorContactRouteId,
) -> Option<String> {
    let creator = names.get(creator_id)?;
    let address = creator.route_addresses.get(contact_route_id)?.trim();
    let name = creator.display_name.trim();
    match (name.is_empty(), address.is_empty()) {
        (false, false) => Some(format!("{name} <{address}>")),
        (false, true) => Some(name.to_owned()),
        (true, false) => Some(address.to_owned()),
        (true, true) => None,
    }
}

fn content_label(item: &EmailBatchItemInput) -> String {
    let label = match &item.content {
        EmailBatchContentInput::InlineRendered { subject, .. } => subject.clone(),
        EmailBatchContentInput::LocalTemplate {
            template_id,
            revision,
            ..
        } => format!(
            "local template {} revision {revision}",
            template_id.as_str()
        ),
        EmailBatchContentInput::PublishedTemplate {
            template_id_or_alias,
            ..
        } => format!("published template {template_id_or_alias}"),
    };
    if label.trim().is_empty() {
        "(no subject)".to_owned()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use onecrew_creator_runtime::{EmailBatchReconciliationItem, MailboxProvider};

    use super::*;

    /// The mailbox identifier from the reported card, which showed this value
    /// to the user under the label 邮箱标识 and named no sender at all.
    const MAILBOX_ID: &str = "96e1a8ec-2242-41ad-8816-70263a22a5d3";

    /// The Creator and contact-route identifiers a batch card printed verbatim:
    /// a reader confirming a mass send saw these and nothing else about who it
    /// would reach.
    const CREATOR_ID: &str = "0f0f1d1c-8d0a-4c0b-9f1e-1a2b3c4d5e6f";
    const ROUTE_ID: &str = "7a6b5c4d-3e2f-4a1b-8c9d-0e1f2a3b4c5d";

    /// A route the resolved Creator no longer holds.
    const MISSING_ROUTE_ID: &str = "11111111-2222-3333-4444-555555555555";

    /// Internal bookkeeping the reconciliation card used to print.
    const FINGERPRINT: &str = "b7c9f0e1a2d3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6f708192a3b4c5";

    fn mailbox(display_name: Option<&str>) -> MailboxDetail {
        MailboxDetail {
            mailbox_ref_id: MailboxRefId::new(MAILBOX_ID),
            provider: MailboxProvider::Resend,
            label: "Outreach".to_owned(),
            email_address: "outreach@example.com".to_owned(),
            display_name: display_name.map(str::to_owned),
            capabilities: Vec::new(),
            available: true,
            issues: Vec::new(),
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        }
    }

    fn value<'a>(fields: &'a [PresentationField], label: &str) -> Option<&'a str> {
        fields
            .iter()
            .find(|field| field.label == label)
            .map(|field| field.value.as_str())
    }

    #[test]
    fn a_reviewed_message_names_its_sender_and_prints_no_mailbox_identifier() {
        let fields = message_fields(
            &mailbox(Some("Reach")),
            "creator@example.com",
            "Hello",
            "Body",
        );

        assert_eq!(value(&fields, "From"), Some("Reach <outreach@example.com>"));
        assert_eq!(value(&fields, "To"), Some("creator@example.com"));
        assert!(
            fields.iter().all(|field| !field.value.contains(MAILBOX_ID)),
            "{fields:?}"
        );
    }

    #[test]
    fn a_mailbox_without_a_display_name_is_shown_by_address_alone() {
        let fields = message_fields(&mailbox(None), "creator@example.com", "Hello", "Body");

        assert_eq!(value(&fields, "From"), Some("outreach@example.com"));
        assert!(
            fields.iter().all(|field| !field.value.contains(MAILBOX_ID)),
            "{fields:?}"
        );
    }

    #[test]
    fn a_creator_send_shows_names_in_place_of_creator_and_route_identifiers() {
        let fields = direct_send_fields(
            &mailbox(Some("Reach")),
            "Creator 42",
            "creator@example.com",
            Some("Original subject"),
            "Hello",
            "Body",
        );

        assert_eq!(value(&fields, "From"), Some("Reach <outreach@example.com>"));
        assert_eq!(value(&fields, "Creator"), Some("Creator 42"));
        assert_eq!(value(&fields, "To"), Some("creator@example.com"));
        assert_eq!(value(&fields, "Reply to"), Some("Original subject"));
        assert!(
            !fields
                .iter()
                .any(|field| field.label == "Mailbox" || field.label == "Contact route"),
            "{fields:?}"
        );
        assert!(
            fields.iter().all(|field| !field.value.contains(MAILBOX_ID)),
            "{fields:?}"
        );
    }

    #[test]
    fn a_naming_row_is_dropped_rather_than_filled_with_an_identifier() {
        let fields = direct_send_fields(&mailbox(Some("Reach")), "   ", "", None, "Hello", "Body");

        assert!(
            !fields
                .iter()
                .any(|field| field.label == "Creator" || field.label == "To"),
            "{fields:?}"
        );
        assert_eq!(value(&fields, "From"), Some("Reach <outreach@example.com>"));
    }

    fn batch_item(recipient: EmailBatchRecipient, subject: &str) -> EmailBatchItemInput {
        EmailBatchItemInput {
            recipient,
            from_name: None,
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            content: EmailBatchContentInput::InlineRendered {
                subject: subject.to_owned(),
                text: Some("Body".to_owned()),
                html: None,
            },
        }
    }

    fn creator_route_item(contact_route_id: &str, subject: &str) -> EmailBatchItemInput {
        batch_item(
            EmailBatchRecipient::CreatorRoute {
                creator_id: CreatorId::new(CREATOR_ID),
                contact_route_id: CreatorContactRouteId::new(contact_route_id),
            },
            subject,
        )
    }

    fn batch(items: Vec<EmailBatchItemInput>) -> SendManyEmailInput {
        SendManyEmailInput {
            mailbox_ref_id: MailboxRefId::new(MAILBOX_ID),
            idempotency_key: "batch-1".to_owned(),
            validation: EmailBatchValidation::Strict,
            items,
        }
    }

    /// What one readable Creator read yields: their name, and the one route
    /// they still hold.
    fn resolved(display_name: &str) -> HashMap<CreatorId, NamedCreator> {
        HashMap::from([(
            CreatorId::new(CREATOR_ID),
            NamedCreator {
                display_name: display_name.to_owned(),
                route_addresses: HashMap::from([(
                    CreatorContactRouteId::new(ROUTE_ID),
                    "creator@example.com".to_owned(),
                )]),
            },
        )])
    }

    fn listed_items(sections: &[PresentationSection]) -> Vec<PresentationItem> {
        sections
            .iter()
            .find_map(|section| match section {
                PresentationSection::Items { items, .. } => Some(items.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn section_fields(sections: &[PresentationSection]) -> Vec<PresentationField> {
        sections
            .iter()
            .find_map(|section| match section {
                PresentationSection::Fields { fields, .. } => Some(fields.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Every string the card puts in front of a reader, whatever section it
    /// sits in. An identifier is a leak wherever it surfaces, so the leak tests
    /// read the whole card rather than the rows they expect it in.
    fn rendered_text(sections: &[PresentationSection]) -> Vec<String> {
        let mut text = Vec::new();
        for section in sections {
            match section {
                PresentationSection::Fields { heading, fields } => {
                    text.extend(heading.clone());
                    for field in fields {
                        text.push(field.label.clone());
                        text.push(field.value.clone());
                    }
                }
                PresentationSection::Items { heading, items } => {
                    text.extend(heading.clone());
                    for item in items {
                        text.push(item.title.clone());
                        text.extend(item.description.clone());
                    }
                }
                PresentationSection::Notice { heading, body, .. } => {
                    text.extend(heading.clone());
                    text.push(body.clone());
                }
                PresentationSection::CreatorSaveRows { .. } => {}
            }
        }
        text
    }

    #[test]
    fn only_the_listed_rows_are_resolved_and_each_creator_only_once() {
        let mixed = vec![
            creator_route_item(ROUTE_ID, "First"),
            // The same Creator on a second route: one read covers both rows.
            creator_route_item(MISSING_ROUTE_ID, "Second"),
            batch_item(
                EmailBatchRecipient::ExplicitAddress {
                    to: vec!["direct@example.com".to_owned()],
                },
                "Direct",
            ),
        ];

        assert_eq!(listed_creator_ids(&mixed), vec![CreatorId::new(CREATOR_ID)]);

        // A batch longer than the card shows resolves only the listed prefix,
        // however many items the uncapped input carries.
        let long = (0..MAX_REVIEW_ITEMS + 5)
            .map(|index| {
                batch_item(
                    EmailBatchRecipient::CreatorRoute {
                        creator_id: CreatorId::new(format!("creator-{index}")),
                        contact_route_id: CreatorContactRouteId::new(ROUTE_ID),
                    },
                    "Message",
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(listed_creator_ids(&long).len(), MAX_REVIEW_ITEMS);
    }

    #[test]
    fn a_batch_row_names_the_creator_and_the_address_its_route_reaches() {
        let input = batch(vec![creator_route_item(ROUTE_ID, "Spring launch")]);

        let sections =
            batch_review_sections(&mailbox(Some("Reach")), &input, &resolved("Nova Lin"));

        let items = listed_items(&sections);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Nova Lin <creator@example.com>");
        assert_eq!(items[0].description.as_deref(), Some("Spring launch"));
    }

    #[test]
    fn a_batch_card_prints_no_creator_or_contact_route_identifier() {
        let input = batch(vec![
            creator_route_item(ROUTE_ID, "Spring launch"),
            batch_item(
                EmailBatchRecipient::ExplicitAddress {
                    to: vec!["direct@example.com".to_owned()],
                },
                "Direct",
            ),
        ]);

        let sections =
            batch_review_sections(&mailbox(Some("Reach")), &input, &resolved("Nova Lin"));

        // An explicitly addressed row already read as an address and is left
        // exactly as it was.
        assert_eq!(listed_items(&sections)[1].title, "direct@example.com");
        for text in rendered_text(&sections) {
            assert!(!text.contains(CREATOR_ID), "{text:?}");
            assert!(!text.contains(ROUTE_ID), "{text:?}");
            assert!(!text.contains(MAILBOX_ID), "{text:?}");
        }
    }

    #[test]
    fn the_truncation_notice_still_separates_what_is_shown_from_what_is_sent() {
        let input = batch(
            (0..MAX_REVIEW_ITEMS + 5)
                .map(|index| creator_route_item(ROUTE_ID, &format!("Message {index}")))
                .collect(),
        );

        let sections =
            batch_review_sections(&mailbox(Some("Reach")), &input, &resolved("Nova Lin"));

        assert_eq!(listed_items(&sections).len(), MAX_REVIEW_ITEMS);
        // The count names the whole batch, not the shown prefix.
        assert_eq!(value(&section_fields(&sections), "Recipients"), Some("105"));
        assert_eq!(
            sections.last(),
            Some(&PresentationSection::Notice {
                heading: Some("List truncated".to_owned()),
                body: format!(
                    "Showing the first {MAX_REVIEW_ITEMS} of 105 recipients. Confirming sends \
                     the whole batch."
                ),
                tone: NoticeTone::Warning,
            })
        );
    }

    #[test]
    fn a_recipient_that_cannot_be_read_degrades_instead_of_failing_the_card() {
        let input = batch(vec![
            creator_route_item(ROUTE_ID, "Spring launch"),
            creator_route_item(MISSING_ROUTE_ID, "Spring launch"),
        ]);

        // The Creator itself cannot be read, so neither row can be named. The
        // card is still minted, with both rows present and neither identified.
        let gone = batch_review_sections(&mailbox(Some("Reach")), &input, &HashMap::new());
        let items = listed_items(&gone);
        assert_eq!(items.len(), 2);
        assert!(
            items.iter().all(|item| item.title == UNNAMED_RECIPIENT),
            "{items:?}"
        );

        // The Creator reads back but no longer holds the second route. The
        // readable row is named; only the unreadable one degrades.
        let partial = batch_review_sections(&mailbox(Some("Reach")), &input, &resolved("Nova Lin"));
        let items = listed_items(&partial);
        assert_eq!(items[0].title, "Nova Lin <creator@example.com>");
        assert_eq!(items[1].title, UNNAMED_RECIPIENT);
        for text in rendered_text(&gone)
            .into_iter()
            .chain(rendered_text(&partial))
        {
            assert!(!text.contains(CREATOR_ID), "{text:?}");
            assert!(!text.contains(ROUTE_ID), "{text:?}");
            assert!(!text.contains(MISSING_ROUTE_ID), "{text:?}");
        }
    }

    #[test]
    fn a_reconciliation_card_shows_no_internal_fingerprint() {
        let input = ReconcileEmailBatchInput {
            mailbox_ref_id: MailboxRefId::new(MAILBOX_ID),
            idempotency_key: "reconcile-1".to_owned(),
            logical_fingerprint: FINGERPRINT.to_owned(),
            items: vec![EmailBatchReconciliationItem {
                input_index: 0,
                outcome: EmailBatchReconciliationOutcome::Accepted {
                    provider_message_id: "provider-message-1".to_owned(),
                },
            }],
        };

        let sections = reconcile_review_sections(&mailbox(Some("Reach")), &input);

        assert_eq!(
            value(&section_fields(&sections), "Mailbox"),
            Some("Reach <outreach@example.com>")
        );
        assert_eq!(
            value(&section_fields(&sections), "Reconciled items"),
            Some("1")
        );
        for text in rendered_text(&sections) {
            assert!(!text.contains(FINGERPRINT), "{text:?}");
            assert!(!text.contains(MAILBOX_ID), "{text:?}");
        }
    }
}
