use crate::{
    EmailActionKind, EmailActionResult, EmailAddress, EmailCapability, EmailPreparation,
    EmailSendFailure, ManualEmailActionResult, ManualEmailPreparation, PrepareEmailInput,
    PrepareManualEmailInput, RecoverableActionClaim, RemoteAcceptedEmailContext,
    RemoteEmailRecovery, RemoteStartedEmailAction, SendEmailInput, SendManualEmailInput,
};
use onecrew_email_contracts::PlatformEmailRequest;

use super::{
    EMAIL_ACTION, MANUAL_EMAIL_ACTION, canonical_email_body, canonical_email_request_fingerprint,
    ensure_email_route, ensure_mailbox, normalize_email_address, persisted_mailbox_snapshot,
    quote_original, reply_headers, validate_idempotency_key, validate_remote_started_email_action,
};

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EmailIntentFingerprint {
    Creator {
        mailbox_ref_id: String,
        creator_id: String,
        contact_route_id: String,
        reply_to_email_id: Option<String>,
        subject: String,
        text: Option<String>,
        html: Option<String>,
    },
    Manual {
        mailbox_ref_id: String,
        to: String,
        subject: String,
        text: Option<String>,
        html: Option<String>,
    },
}

impl crate::CreatorRuntime {
    pub fn prepare_email(&self, input: PrepareEmailInput) -> Result<EmailPreparation, String> {
        let creator = self.require_active_creator(&input.creator_id)?;
        let route = creator
            .contact_routes
            .iter()
            .find(|record| record.route.id == input.contact_route_id)
            .cloned()
            .ok_or_else(|| "email route does not belong to this Creator".to_string())?;
        ensure_email_route(&route)?;
        let mailbox = self.require_mailbox_ref(&input.mailbox_ref_id)?;
        ensure_mailbox(&mailbox, EmailCapability::Send)?;
        ensure_mailbox(&mailbox, EmailCapability::SentReadback)?;

        let (thread, reply_to) = if let Some(email_id) = input.reply_to_email_id.as_ref() {
            ensure_mailbox(&mailbox, EmailCapability::Reply)?;
            let message = self.repository.get_email_message(email_id)?;
            let thread = self
                .repository
                .get_email_thread_record(&message.thread_id)?;
            if thread.mailbox_ref_id != mailbox.mailbox_ref_id
                || thread.creator_id.as_ref() != Some(&input.creator_id)
                || thread.contact_route_id.as_ref() != Some(&input.contact_route_id)
            {
                return Err(
                    "reply email is outside the selected Creator, route, or mailbox".into(),
                );
            }
            let reply_address = message.reply_to.first().unwrap_or(&message.from);
            if normalize_email_address(reply_address)? != route.locator.normalized_value {
                return Err("reply target is not the selected saved email contact route".into());
            }
            if message
                .internet_message_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("reply email has no Internet Message-ID".into());
            }
            (Some(thread), Some(message.view()))
        } else {
            // 没有指明在回哪一封,这就是一次新的触达,归它自己的线程。
            //
            // RFC 5322 里线程关系由 `In-Reply-To`/`References` 决定,主题
            // 只是没有这些头时的兜底启发式;收件人相同**从来不是**判据。
            // 按收件人合并的后果是:再给同一个人写一封新的信,它会被吞进
            // 上一条线程,连那条线程的已归档、已删除状态一起继承——用户
            // 刚发出去的信直接躺进回收站,而「已发送」是空的。
            (None, None)
        };

        Ok(EmailPreparation {
            creator,
            route,
            mailbox,
            thread,
            reply_to,
            confirmation_required: true,
            confirmation_reason: "sending email is an external write".into(),
        })
    }

    pub fn prepare_manual_email(
        &self,
        input: PrepareManualEmailInput,
    ) -> Result<ManualEmailPreparation, String> {
        let mailbox = self.require_mailbox_ref(&input.mailbox_ref_id)?;
        ensure_mailbox(&mailbox, EmailCapability::Send)?;
        ensure_mailbox(&mailbox, EmailCapability::SentReadback)?;
        Ok(ManualEmailPreparation {
            mailbox,
            to: normalize_email_address(&input.to)?,
            confirmation_required: true,
            confirmation_reason: "sending email is an external write".into(),
        })
    }

    pub async fn send_email(
        &self,
        input: SendEmailInput,
    ) -> Result<EmailActionResult, EmailSendFailure> {
        let (subject, text, html) = canonical_email_body(&input.subject, &input.text, &input.html)
            .map_err(EmailSendFailure::not_sent)?;
        validate_idempotency_key(&input.idempotency_key).map_err(EmailSendFailure::not_sent)?;
        let request_fingerprint = serde_json::to_string(&EmailIntentFingerprint::Creator {
            mailbox_ref_id: input.mailbox_ref_id.as_str().to_owned(),
            creator_id: input.creator_id.as_str().to_owned(),
            contact_route_id: input.contact_route_id.as_str().to_owned(),
            reply_to_email_id: input
                .reply_to_email_id
                .as_ref()
                .map(|id| id.as_str().to_owned()),
            subject: subject.clone(),
            text: text.clone(),
            html: html.clone(),
        })
        .map_err(|error| EmailSendFailure::not_sent(error.to_string()))?;
        let claim_runtime = self.clone();
        let (claim, replayed) = self
            .repository_write_segment(move || {
                claim_runtime.claim_creator_email_send(
                    input,
                    subject,
                    text,
                    html,
                    request_fingerprint,
                )
            })
            .await
            .map_err(email_claim_segment_error)?;
        self.resume_creator_email_claim(claim, replayed).await
    }

    pub async fn send_manual_email(
        &self,
        input: SendManualEmailInput,
    ) -> Result<ManualEmailActionResult, EmailSendFailure> {
        let (subject, text, html) = canonical_email_body(&input.subject, &input.text, &input.html)
            .map_err(EmailSendFailure::not_sent)?;
        let recipient = normalize_email_address(&input.to).map_err(EmailSendFailure::not_sent)?;
        validate_idempotency_key(&input.idempotency_key).map_err(EmailSendFailure::not_sent)?;
        let request_fingerprint = serde_json::to_string(&EmailIntentFingerprint::Manual {
            mailbox_ref_id: input.mailbox_ref_id.as_str().to_owned(),
            to: recipient.clone(),
            subject: subject.clone(),
            text: text.clone(),
            html: html.clone(),
        })
        .map_err(|error| EmailSendFailure::not_sent(error.to_string()))?;
        let claim_runtime = self.clone();
        let (claim, replayed) = self
            .repository_write_segment(move || {
                claim_runtime.claim_manual_email_send(
                    input,
                    recipient,
                    subject,
                    text,
                    html,
                    request_fingerprint,
                )
            })
            .await
            .map_err(email_claim_segment_error)?;
        self.resume_manual_email_claim(claim, replayed).await
    }

    fn claim_creator_email_send(
        &self,
        input: SendEmailInput,
        subject: String,
        text: Option<String>,
        html: Option<String>,
        request_fingerprint: String,
    ) -> Result<(RecoverableActionClaim, bool), String> {
        if let Some(claim) = self.repository.find_recoverable_action(
            &input.idempotency_key,
            EMAIL_ACTION,
            Some(&input.mailbox_ref_id),
            Some(&input.contact_route_id),
            &request_fingerprint,
        )? {
            return Ok((claim, true));
        }
        let preparation = self.prepare_email(PrepareEmailInput {
            creator_id: input.creator_id.clone(),
            contact_route_id: input.contact_route_id.clone(),
            mailbox_ref_id: input.mailbox_ref_id.clone(),
            reply_to_email_id: input.reply_to_email_id.clone(),
        })?;
        let (in_reply_to, references) = reply_headers(preparation.reply_to.as_ref());
        let (text, html) = quote_original(preparation.reply_to.as_ref(), text, html);
        let request = PlatformEmailRequest {
            to: vec![preparation.route.locator.normalized_value.clone()],
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
            .unwrap_or_else(|| {
                format!(
                    "creator:{}:route:{}:send:{}",
                    input.creator_id.as_str(),
                    input.contact_route_id.as_str(),
                    input.idempotency_key.trim()
                )
            });
        let recovery = RemoteStartedEmailAction {
            recovery: RemoteEmailRecovery {
                action: EmailActionKind::Creator,
                request_fingerprint: request_fingerprint.clone(),
                canonical_request_fingerprint: canonical_email_request_fingerprint(&request)?,
                mailbox: persisted_mailbox_snapshot(preparation.mailbox.clone()),
                expected_recipient: EmailAddress::parse(
                    &preparation.route.locator.normalized_value,
                )?,
                request,
                provider_thread_key,
                in_reply_to,
                references,
                context: RemoteAcceptedEmailContext::Creator {
                    preparation: Box::new(preparation),
                },
            },
        };
        validate_remote_started_email_action(&recovery)?;
        let claim = self.repository.claim_remote_started_action(
            &input.idempotency_key,
            EMAIL_ACTION,
            Some(&input.mailbox_ref_id),
            Some(&input.contact_route_id),
            &request_fingerprint,
            serde_json::to_value(&recovery).map_err(|error| error.to_string())?,
        )?;
        Ok((claim, false))
    }

    fn claim_manual_email_send(
        &self,
        input: SendManualEmailInput,
        recipient: String,
        subject: String,
        text: Option<String>,
        html: Option<String>,
        request_fingerprint: String,
    ) -> Result<(RecoverableActionClaim, bool), String> {
        if let Some(claim) = self.repository.find_recoverable_action(
            &input.idempotency_key,
            MANUAL_EMAIL_ACTION,
            Some(&input.mailbox_ref_id),
            None,
            &request_fingerprint,
        )? {
            return Ok((claim, true));
        }
        let preparation = self.prepare_manual_email(PrepareManualEmailInput {
            mailbox_ref_id: input.mailbox_ref_id.clone(),
            to: recipient,
        })?;
        let request = PlatformEmailRequest {
            to: vec![preparation.to.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject,
            text,
            html,
            idempotency_key: input.idempotency_key.clone(),
        };
        let recovery = RemoteStartedEmailAction {
            recovery: RemoteEmailRecovery {
                action: EmailActionKind::Manual,
                request_fingerprint: request_fingerprint.clone(),
                canonical_request_fingerprint: canonical_email_request_fingerprint(&request)?,
                mailbox: persisted_mailbox_snapshot(preparation.mailbox.clone()),
                expected_recipient: EmailAddress::parse(&preparation.to)?,
                request,
                provider_thread_key: format!(
                    "manual:{}:{}:send:{}",
                    preparation.mailbox.mailbox_ref_id.as_str(),
                    preparation.to,
                    input.idempotency_key.trim()
                ),
                in_reply_to: None,
                references: Vec::new(),
                context: RemoteAcceptedEmailContext::Manual { preparation },
            },
        };
        validate_remote_started_email_action(&recovery)?;
        let claim = self.repository.claim_remote_started_action(
            &input.idempotency_key,
            MANUAL_EMAIL_ACTION,
            Some(&input.mailbox_ref_id),
            None,
            &request_fingerprint,
            serde_json::to_value(&recovery).map_err(|error| error.to_string())?,
        )?;
        Ok((claim, false))
    }
}

/// Every claim-segment outcome precedes the provider. The durable claim itself
/// may be uncertain, but nothing has been handed to the mail provider yet, so
/// no send can have occurred.
fn email_claim_segment_error(error: crate::CreatorRepositoryCallError<String>) -> EmailSendFailure {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => EmailSendFailure::not_sent(error),
        crate::CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            EmailSendFailure::not_sent(format!(
                "email claim did not start: {error}; no provider send occurred"
            ))
        }
        crate::CreatorRepositoryCallError::Dispatch(error) => EmailSendFailure::not_sent(format!(
            "email claim outcome is unknown: {error}; no provider send occurred yet; retry only \
             with the same action and idempotency key to read back the durable claim"
        )),
    }
}

#[cfg(test)]
mod dispatch_contract {
    use std::collections::BTreeSet;

    use syn::visit::Visit;

    #[derive(Default)]
    struct SegmentCalls {
        segment_count: usize,
        in_segment: usize,
        inside: BTreeSet<String>,
        outside: BTreeSet<String>,
    }

    impl<'ast> Visit<'ast> for SegmentCalls {
        fn visit_expr_method_call(&mut self, expression: &'ast syn::ExprMethodCall) {
            let method = expression.method.to_string();
            if method == "repository_write_segment" {
                self.segment_count += 1;
                self.in_segment += 1;
                for argument in &expression.args {
                    self.visit_expr(argument);
                }
                self.in_segment -= 1;
                return;
            }
            if self.in_segment == 0 {
                self.outside.insert(method);
            } else {
                self.inside.insert(method);
            }
            syn::visit::visit_expr_method_call(self, expression);
        }
    }

    fn runtime_method<'a>(file: &'a syn::File, name: &str) -> &'a syn::ImplItemFn {
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
            .unwrap_or_else(|| panic!("missing runtime method {name}"))
    }

    #[test]
    fn direct_and_manual_find_prepare_claim_stay_inside_the_writer_segment() {
        let file = syn::parse_file(include_str!("send.rs")).unwrap();
        for (public_method, claim_helper) in [
            ("send_email", "claim_creator_email_send"),
            ("send_manual_email", "claim_manual_email_send"),
        ] {
            let method = runtime_method(&file, public_method);
            let mut calls = SegmentCalls::default();
            calls.visit_block(&method.block);
            assert_eq!(calls.segment_count, 1);
            assert!(calls.inside.contains(claim_helper));
            for forbidden in [
                "find_recoverable_action",
                "prepare_email",
                "prepare_manual_email",
                "claim_remote_started_action",
            ] {
                assert!(!calls.outside.contains(forbidden));
            }
            let helper = runtime_method(&file, claim_helper);
            assert!(helper.sig.asyncness.is_none());
        }
    }
}
