use crate::email::{
    RecoveredEmailAction, VerifiedEmailAction, canonical_email_request_fingerprint,
};
use crate::{
    CreatorChangeScope, EmailActionKind, EmailAddress, EmailDirection, EmailDraft,
    EmailDraftSendClaim, EmailDraftSendFingerprint, EmailDraftSendResult, EmailDraftState,
    EmailSendFailure, EmailThread, MailboxDetail, PrepareEmailInput, PrepareManualEmailInput,
    RemoteAcceptedEmailContext, RemoteEmailRecovery, RemoteStartedEmailAction,
    ReserveEmailDraftSendInput, SendEmailDraftInput,
};
use onecrew_email_contracts::PlatformEmailRequest;

struct DraftDelivery {
    mailbox: MailboxDetail,
    recipient: String,
    thread: Option<EmailThread>,
    reply_to: Option<crate::EmailMessageView>,
    creator_id: Option<crate::CreatorId>,
    contact_route_id: Option<crate::CreatorContactRouteId>,
    locator_id: Option<crate::ContactLocatorId>,
}

impl crate::CreatorRuntime {
    pub async fn send_email_draft(
        &self,
        input: SendEmailDraftInput,
    ) -> Result<EmailDraftSendResult, EmailSendFailure> {
        crate::email::validate_idempotency_key(&input.idempotency_key)
            .map_err(EmailSendFailure::not_sent)?;
        let claim_runtime = self.clone();
        let (claim, replayed) = self
            .repository_write_segment(move || claim_runtime.claim_email_draft_send(input))
            .await
            .map_err(draft_claim_segment_error)?;
        self.resume_email_draft_claim(claim, replayed).await
    }

    fn claim_email_draft_send(
        &self,
        input: SendEmailDraftInput,
    ) -> Result<(EmailDraftSendClaim, bool), String> {
        let claim_input = ReserveEmailDraftSendInput {
            draft_id: input.draft_id.clone(),
            expected_revision: input.expected_revision,
            idempotency_key: input.idempotency_key.clone(),
        };
        if let Some(claim) = self.repository.find_email_draft_send(claim_input.clone())? {
            return Ok((claim, true));
        }
        let draft = self.repository.get_email_draft(&input.draft_id)?;
        if draft.revision != input.expected_revision {
            return Err(format!(
                "email draft revision conflict: expected {}, found {}",
                input.expected_revision, draft.revision
            ));
        }
        if draft.state != EmailDraftState::Draft {
            return Err("trashed email draft must be restored before sending".into());
        }
        if draft.sending {
            return Err("email draft is reserved for sending".into());
        }
        validate_send_body(&draft.subject, &draft.text, draft.html.as_deref())?;
        let preparation = self.prepare_draft_delivery(&draft)?;
        let (in_reply_to, references) = crate::email::reply_headers(preparation.reply_to.as_ref());
        let subject = draft.subject.trim().to_owned();
        let text = draft.text.trim().to_owned();
        // 关联头让客户端把消息归成一条线程;引文让**人**看得出这是在回
        // 哪一句。两件事都要,而且触发条件相同,所以在同一处完成。
        let (text, html) = crate::email::quote_original(
            preparation.reply_to.as_ref(),
            (!text.is_empty()).then_some(text),
            draft
                .html
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        );
        let request = PlatformEmailRequest {
            to: vec![preparation.recipient.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: in_reply_to.clone(),
            references: references.clone(),
            subject,
            text,
            html,
            idempotency_key: input.idempotency_key.clone(),
        };
        let provider_thread_key = preparation
            .thread
            .as_ref()
            .map(|thread| thread.provider_thread_key.clone())
            .unwrap_or_else(
                // 草稿没有指明在回哪一封,那它就是一次新的触达,归它自己
                // 的线程;idempotency_key 是这次意图的标识,重试落回同一条。
                || match (&preparation.creator_id, &preparation.contact_route_id) {
                    (Some(creator_id), Some(contact_route_id)) => format!(
                        "creator:{}:route:{}:send:{}",
                        creator_id.as_str(),
                        contact_route_id.as_str(),
                        input.idempotency_key.trim()
                    ),
                    _ => format!(
                        "manual:{}:{}:send:{}",
                        preparation.mailbox.mailbox_ref_id.as_str(),
                        preparation.recipient,
                        input.idempotency_key.trim()
                    ),
                },
            );
        let request_fingerprint = serde_json::to_string(&EmailDraftSendFingerprint::new(
            &draft.mailbox_ref_id,
            &draft.id,
            draft.revision,
        ))
        .map_err(|error| error.to_string())?;
        let recovery = RemoteStartedEmailAction {
            recovery: RemoteEmailRecovery {
                action: EmailActionKind::Draft,
                request_fingerprint,
                canonical_request_fingerprint: canonical_email_request_fingerprint(&request)?,
                mailbox: crate::email::persisted_mailbox_snapshot(preparation.mailbox),
                expected_recipient: EmailAddress::parse(&preparation.recipient)?,
                request,
                provider_thread_key,
                in_reply_to,
                references,
                context: RemoteAcceptedEmailContext::Draft {
                    draft_id: input.draft_id.clone(),
                    expected_revision: input.expected_revision,
                    creator_id: preparation.creator_id,
                    contact_route_id: preparation.contact_route_id,
                    contact_locator_id: preparation.locator_id,
                },
            },
        };
        let claim = self
            .repository
            .claim_remote_started_email_draft_send(claim_input, recovery)?;
        self.publish_changes([
            CreatorChangeScope::EmailDrafts,
            CreatorChangeScope::EmailFolders,
        ]);
        Ok((claim, false))
    }

    async fn resume_email_draft_claim(
        &self,
        claim: EmailDraftSendClaim,
        replayed: bool,
    ) -> Result<EmailDraftSendResult, EmailSendFailure> {
        match claim {
            EmailDraftSendClaim::Completed(mut result) => {
                result.replayed = true;
                self.publish_changes([
                    CreatorChangeScope::EmailDrafts,
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]);
                Ok(*result)
            }
            // A durable `failed` claim records a terminal provider rejection,
            // so replaying it sends nothing.
            EmailDraftSendClaim::Failed(value) => Err(EmailSendFailure::not_sent(
                crate::email::failed_replay(value),
            )),
            EmailDraftSendClaim::RemoteStarted(recovery) => {
                match self
                    .resume_remote_started_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Draft(result) => Ok(result),
                    // The recovery already succeeded through the provider; only
                    // this shape assertion failed.
                    _ => Err(EmailSendFailure::sent(
                        "draft email recovery returned the wrong action kind",
                    )),
                }
            }
            EmailDraftSendClaim::RemoteAccepted(recovery) => {
                match self
                    .recover_remote_accepted_email_action(recovery, replayed)
                    .await?
                {
                    RecoveredEmailAction::Draft(result) => Ok(result),
                    _ => Err(EmailSendFailure::sent(
                        "draft email recovery returned the wrong action kind",
                    )),
                }
            }
            EmailDraftSendClaim::RemoteVerified(commit) => {
                match self
                    .complete_verified_email_action_segment(
                        VerifiedEmailAction::Draft(commit),
                        replayed,
                    )
                    .await?
                {
                    RecoveredEmailAction::Draft(result) => Ok(result),
                    _ => Err(EmailSendFailure::sent(
                        "draft email recovery returned the wrong action kind",
                    )),
                }
            }
        }
    }

    fn prepare_draft_delivery(&self, draft: &EmailDraft) -> Result<DraftDelivery, String> {
        match (&draft.creator_id, &draft.contact_route_id) {
            (Some(creator_id), Some(contact_route_id)) => {
                let preparation = self.prepare_email(PrepareEmailInput {
                    creator_id: creator_id.clone(),
                    contact_route_id: contact_route_id.clone(),
                    mailbox_ref_id: draft.mailbox_ref_id.clone(),
                    reply_to_email_id: draft.reply_to_email_id.clone(),
                })?;
                let recipient = preparation.route.locator.normalized_value.clone();
                if recipient != draft.to {
                    return Err(
                        "draft recipient no longer matches the current email contact route".into(),
                    );
                }
                Ok(DraftDelivery {
                    mailbox: preparation.mailbox,
                    recipient,
                    thread: preparation.thread,
                    reply_to: preparation.reply_to,
                    creator_id: Some(creator_id.clone()),
                    contact_route_id: Some(contact_route_id.clone()),
                    locator_id: Some(preparation.route.locator.id),
                })
            }
            (None, None) => {
                let preparation = self.prepare_manual_email(PrepareManualEmailInput {
                    mailbox_ref_id: draft.mailbox_ref_id.clone(),
                    to: draft.to.clone(),
                })?;
                let (thread, reply_to) = if let Some(email_id) = draft.reply_to_email_id.as_ref() {
                    crate::email::ensure_mailbox(
                        &preparation.mailbox,
                        crate::EmailCapability::Reply,
                    )?;
                    let message = self.repository.get_email_message(email_id)?;
                    let thread = self
                        .repository
                        .get_email_thread_record(&message.thread_id)?;
                    if thread.mailbox_ref_id != preparation.mailbox.mailbox_ref_id
                        || thread.creator_id.is_some()
                        || thread.contact_route_id.is_some()
                    {
                        return Err(
                            "manual draft reply must belong to an unowned thread in the same mailbox"
                                .into(),
                        );
                    }
                    if message.direction != EmailDirection::Inbound {
                        return Err("manual draft reply target must be an inbound email".into());
                    }
                    let recipient = crate::email::normalize_email_address(
                        message.reply_to.first().unwrap_or(&message.from),
                    )?;
                    if recipient != preparation.to {
                        return Err(
                            "manual draft recipient no longer matches its reply email".into()
                        );
                    }
                    if message
                        .internet_message_id
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                    {
                        return Err("manual draft reply email has no Internet Message-ID".into());
                    }
                    (Some(thread), Some(message.view()))
                } else {
                    (None, None)
                };
                Ok(DraftDelivery {
                    mailbox: preparation.mailbox,
                    recipient: preparation.to,
                    thread,
                    reply_to,
                    creator_id: None,
                    contact_route_id: None,
                    locator_id: None,
                })
            }
            _ => Err("email draft has invalid ownership".into()),
        }
    }
}

fn validate_send_body(subject: &str, text: &str, html: Option<&str>) -> Result<(), String> {
    if subject.trim().is_empty() {
        return Err("email subject is required".into());
    }
    if text.trim().is_empty() && html.map(str::trim).unwrap_or_default().is_empty() {
        return Err("email text or html body is required".into());
    }
    Ok(())
}

/// The draft claim segment runs entirely before the provider, so every outcome
/// here leaves the mail unsent.
fn draft_claim_segment_error(error: crate::CreatorRepositoryCallError<String>) -> EmailSendFailure {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => EmailSendFailure::not_sent(error),
        crate::CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            EmailSendFailure::not_sent(format!(
                "email draft claim did not start: {error}; no provider send occurred"
            ))
        }
        crate::CreatorRepositoryCallError::Dispatch(error) => EmailSendFailure::not_sent(format!(
            "email draft claim outcome is unknown: {error}; no provider send occurred yet; retry \
             only with the same draft, revision, and idempotency key"
        )),
    }
}

#[cfg(test)]
mod dispatch_contract {
    use std::collections::BTreeSet;
    use syn::visit::Visit;

    #[derive(Default)]
    struct Calls {
        depth: usize,
        segments: usize,
        inside: BTreeSet<String>,
        outside: BTreeSet<String>,
    }

    impl<'ast> Visit<'ast> for Calls {
        fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
            let method = expression.method.to_string();
            if method == "repository_write_segment" {
                self.segments += 1;
                self.depth += 1;
                for argument in &expression.args {
                    self.visit_expr(argument);
                }
                self.depth -= 1;
                return;
            }
            if self.depth == 0 {
                self.outside.insert(method);
            } else {
                self.inside.insert(method);
            }
            syn::visit::visit_expr_method_call(self, expression);
        }
    }

    fn method<'a>(file: &'a syn::File, name: &str) -> &'a syn::ImplItemFn {
        file.items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Impl(item) => Some(item),
                _ => None,
            })
            .flat_map(|item| &item.items)
            .find_map(|item| match item {
                syn::ImplItem::Fn(method) if method.sig.ident == name => Some(method),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn draft_find_prepare_reservation_and_claim_stay_in_one_writer_segment() {
        let file = syn::parse_file(include_str!("draft_send.rs")).unwrap();
        let outer = method(&file, "send_email_draft");
        let mut calls = Calls::default();
        calls.visit_block(&outer.block);
        assert_eq!(calls.segments, 1);
        assert!(calls.inside.contains("claim_email_draft_send"));
        assert!(calls.outside.contains("resume_email_draft_claim"));
        for forbidden in [
            "find_email_draft_send",
            "get_email_draft",
            "prepare_draft_delivery",
            "claim_remote_started_email_draft_send",
            "publish_changes",
        ] {
            assert!(!calls.outside.contains(forbidden));
        }
        let helper = method(&file, "claim_email_draft_send");
        assert!(helper.sig.asyncness.is_none());
        let mut helper_calls = Calls::default();
        helper_calls.visit_block(&helper.block);
        for required in [
            "find_email_draft_send",
            "get_email_draft",
            "prepare_draft_delivery",
            "claim_remote_started_email_draft_send",
            "publish_changes",
        ] {
            assert!(
                helper_calls.outside.contains(required),
                "missing {required}"
            );
        }
    }
}
