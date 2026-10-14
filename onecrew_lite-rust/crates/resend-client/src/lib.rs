//! Official `resend-rs` typed client boundary.
//!
//! This crate owns no Creator policy, provider wire compatibility, or persistence.
//! `resend-rs` owns HTTP and endpoint serialization; the narrow transport crate
//! supplies only the shared client/admission seam.

use std::{collections::HashMap, sync::Arc};

use onecrew_email_contracts::{
    EmailBatchChunkResult, EmailBatchItem, EmailBatchItemOutcome, EmailBatchItemResult,
    EmailBatchRequest, EmailBatchResult, EmailBatchTemplate, EmailBatchValidation, EmailError,
    EmailReceipt, EmailSendProvider, OutboundEmail, ProviderErrorClass, ProviderFailure,
    ProviderRateLimitStatus,
};
use onecrew_resend_transport::{ResendOpPriority, ResendTransport};
use rand::Rng;
use resend_rs::{Resend, idempotent::IdempotentTrait, types::CreateEmailBaseOptions};
use tokio::time::Duration;
pub struct ResendClient {
    sdk: Resend,
    transport: Arc<ResendTransport>,
    api_key: String,
}

impl ResendClient {
    pub fn new(api_key: String, transport: Arc<ResendTransport>) -> Result<Self, EmailError> {
        if api_key.trim().is_empty() {
            return Err(EmailError::InvalidConfig(
                "Resend API key is required".into(),
            ));
        }
        let config = resend_rs::Config::builder(api_key.clone())
            .base_url(transport.sdk_base_url())
            .client(transport.sdk_http_client())
            .build();
        Ok(Self {
            sdk: Resend::with_config(config),
            transport,
            api_key,
        })
    }

    pub async fn send(&self, message: &OutboundEmail) -> Result<EmailReceipt, EmailError> {
        // Keep the typed client safe when called directly (the facade also
        // validates before dispatch). Invalid payloads must never consume a
        // provider request or an idempotency key.
        message.validate()?;
        let request = sdk_email_from_outbound(message)?;
        let mut attempt = 0;
        loop {
            attempt += 1;
            let permit = self.transport.acquire(ResendOpPriority::User).await;
            let result = self
                .sdk
                .emails
                .send(
                    request
                        .clone()
                        .with_idempotency_key(&message.idempotency_key),
                )
                .await;
            drop(permit);
            match result {
                Ok(response) => {
                    return Ok(EmailReceipt {
                        provider: "resend".into(),
                        message_id: Some(response.id.to_string()),
                        accepted: true,
                    });
                }
                Err(error) => {
                    let mapped = map_sdk_error(error, &self.api_key);
                    let rate_limited = mapped
                        .provider_failure()
                        .filter(|failure| failure.class == ProviderErrorClass::RateLimited)
                        .map(|failure| self.transport.report_rate_limited(failure.rate_limit));
                    if !mapped.is_transient() || attempt >= onecrew_resend_transport::MAX_ATTEMPTS {
                        return Err(mapped);
                    }
                    let wait = rate_limited.unwrap_or_else(|| {
                        onecrew_resend_transport::BACKOFF_BASE * 2_u32.saturating_pow(attempt - 1)
                    });
                    tokio::time::sleep(with_jitter(wait)).await;
                }
            }
        }
    }

    pub async fn send_many(
        &self,
        request: &EmailBatchRequest,
    ) -> Result<EmailBatchResult, EmailError> {
        request.validate()?;
        let fingerprint = request.logical_fingerprint()?;
        let mut all_outcomes = Vec::with_capacity(request.items.len());
        let mut valid_indices = Vec::with_capacity(request.items.len());
        for (index, item) in request.items.iter().enumerate() {
            match item.validate() {
                Ok(()) => valid_indices.push(index),
                Err(error) if request.validation == EmailBatchValidation::Permissive => {
                    all_outcomes.push(EmailBatchItemResult {
                        input_index: index,
                        outcome: EmailBatchItemOutcome::Rejected {
                            message: error.to_string(),
                        },
                    });
                }
                Err(error) => return Err(error),
            }
        }
        let mut chunks = Vec::new();
        for (chunk_index, valid_chunk) in valid_indices
            .chunks(EmailBatchRequest::MAX_ITEMS_PER_PROVIDER_REQUEST)
            .enumerate()
        {
            let chunk_key = request.chunk_idempotency_key(&fingerprint, chunk_index);
            let sdk_items = valid_chunk
                .iter()
                .map(|index| sdk_email_from_batch_item(&request.items[*index]))
                .collect::<Result<Vec<_>, _>>()?;
            let validation = match request.validation {
                EmailBatchValidation::Strict => resend_rs::types::BatchValidation::Strict,
                EmailBatchValidation::Permissive => resend_rs::types::BatchValidation::Permissive,
            };
            let mut chunk = self
                .send_batch_chunk(sdk_items, validation, &chunk_key, chunk_index)
                .await?;
            for outcome in &mut chunk.outcomes {
                outcome.input_index = valid_chunk[outcome.input_index];
            }
            all_outcomes.extend(chunk.outcomes.clone());
            chunks.push(chunk);
        }
        all_outcomes.sort_by_key(|outcome| outcome.input_index);
        Ok(EmailBatchResult {
            logical_fingerprint: fingerprint,
            chunks,
            outcomes: all_outcomes,
        })
    }

    async fn send_batch_chunk(
        &self,
        request: Vec<CreateEmailBaseOptions>,
        validation: resend_rs::types::BatchValidation,
        idempotency_key: &str,
        chunk_index: usize,
    ) -> Result<EmailBatchChunkResult, EmailError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let permit = self.transport.acquire(ResendOpPriority::User).await;
            let result = self
                .sdk
                .batch
                .send_with_batch_validation(
                    request.clone().with_idempotency_key(idempotency_key),
                    validation,
                )
                .await;
            drop(permit);
            match result {
                Ok(response) => {
                    let mut rejected = HashMap::with_capacity(response.errors.len());
                    let mut malformed_error_index = false;
                    for error in response.errors {
                        let Some(index) = usize::try_from(error.index)
                            .ok()
                            .filter(|index| *index < request.len())
                        else {
                            malformed_error_index = true;
                            continue;
                        };
                        if rejected
                            .insert(
                                index,
                                EmailBatchItemOutcome::Rejected {
                                    message: error.message,
                                },
                            )
                            .is_some()
                        {
                            // Duplicate indices cannot be correlated safely;
                            // do not expose a guessed acceptance.
                            malformed_error_index = true;
                        }
                    }

                    let accepted = response
                        .data
                        .into_iter()
                        .map(|item| {
                            let provider_message_id = item.id.to_string();
                            if provider_message_id.trim().is_empty() {
                                EmailBatchItemOutcome::Unknown {
                                    message: "Resend batch response returned an empty item id"
                                        .into(),
                                }
                            } else {
                                EmailBatchItemOutcome::Accepted {
                                    provider_message_id,
                                }
                            }
                        })
                        .collect::<Vec<_>>();

                    // A provider response with an invalid error index or too
                    // many data entries is not safely correlated to inputs.
                    // Return explicit unknowns rather than shifting IDs onto
                    // the wrong recipient.
                    let malformed_response = malformed_error_index
                        || accepted.len() > request.len().saturating_sub(rejected.len());
                    let mut accepted = accepted.into_iter();
                    let outcomes = (0..request.len())
                        .map(|index| EmailBatchItemResult {
                            input_index: index,
                            outcome: if malformed_response {
                                EmailBatchItemOutcome::Unknown {
                                    message: "Resend batch response could not be correlated".into(),
                                }
                            } else {
                                rejected
                                    .get(&index)
                                    .cloned()
                                    .or_else(|| accepted.next())
                                    .unwrap_or(EmailBatchItemOutcome::Unknown {
                                        message: "Resend batch response omitted this item".into(),
                                    })
                            },
                        })
                        .collect();
                    return Ok(EmailBatchChunkResult {
                        chunk_index,
                        idempotency_key: idempotency_key.to_owned(),
                        outcomes,
                    });
                }
                Err(error) => {
                    let mapped = map_sdk_error(error, &self.api_key);
                    if let Some(failure) = mapped.provider_failure()
                        && failure.class == ProviderErrorClass::RateLimited
                    {
                        self.transport.report_rate_limited(failure.rate_limit);
                    }
                    if mapped.is_transient() && attempt < onecrew_resend_transport::MAX_ATTEMPTS {
                        tokio::time::sleep(with_jitter(
                            onecrew_resend_transport::BACKOFF_BASE
                                * 2_u32.saturating_pow(attempt - 1),
                        ))
                        .await;
                        continue;
                    }
                    // A terminal provider response (auth, quota, payload
                    // validation, or idempotency conflict) is a whole-request
                    // failure, not a per-item rejection. Propagating it lets
                    // the adapter update credential health and preserves the
                    // distinction from a transient response that exhausted
                    // retries after the request may have been accepted.
                    if !mapped.is_transient() {
                        return Err(mapped);
                    }
                    let outcome = EmailBatchItemOutcome::Unknown {
                        message: mapped.to_string(),
                    };
                    return Ok(EmailBatchChunkResult {
                        chunk_index,
                        idempotency_key: idempotency_key.to_owned(),
                        outcomes: (0..request.len())
                            .map(|input_index| EmailBatchItemResult {
                                input_index,
                                outcome: outcome.clone(),
                            })
                            .collect(),
                    });
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl EmailSendProvider for ResendClient {
    async fn send(&self, message: &OutboundEmail) -> Result<EmailReceipt, EmailError> {
        Self::send(self, message).await
    }

    async fn send_many(&self, request: &EmailBatchRequest) -> Result<EmailBatchResult, EmailError> {
        Self::send_many(self, request).await
    }
}

fn sdk_email_from_outbound(message: &OutboundEmail) -> Result<CreateEmailBaseOptions, EmailError> {
    let from = format_from(message.from_name.as_deref(), &message.from_email);
    let mut request =
        CreateEmailBaseOptions::new(from, message.to.clone(), message.subject.clone());
    if let Some(text) = message.text.as_deref() {
        request = request.with_text(text);
    }
    if let Some(html) = message.html.as_deref() {
        request = request.with_html(html);
    }
    for address in &message.cc {
        request = request.with_cc(address);
    }
    for address in &message.bcc {
        request = request.with_bcc(address);
    }
    if !message.reply_to.is_empty() {
        request = request.with_reply_multiple(&message.reply_to);
    }
    if let Some(value) = message.in_reply_to.as_deref() {
        request = request.with_header("In-Reply-To", value);
    }
    if !message.references.is_empty() {
        request = request.with_header("References", &message.references.join(" "));
    }
    Ok(request)
}

fn sdk_email_from_batch_item(item: &EmailBatchItem) -> Result<CreateEmailBaseOptions, EmailError> {
    item.validate()?;
    let (from, subject) = match &item.template {
        Some(EmailBatchTemplate::Published {
            from_override,
            subject_override,
            ..
        }) => (
            from_override
                .clone()
                .unwrap_or_else(|| format_from(item.from_name.as_deref(), &item.from_email)),
            subject_override
                .clone()
                .or_else(|| item.subject.clone())
                .unwrap_or_default(),
        ),
        Some(EmailBatchTemplate::Local { .. }) | None => (
            format_from(item.from_name.as_deref(), &item.from_email),
            item.subject.clone().unwrap_or_default(),
        ),
    };
    let mut request = CreateEmailBaseOptions::new(from, item.to.clone(), subject);
    match &item.template {
        Some(EmailBatchTemplate::Published {
            template_id_or_alias,
            variables,
            reply_to_override,
            ..
        }) => {
            let variables = variables
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<HashMap<_, _>>();
            request = request.with_template(
                resend_rs::types::EmailTemplate::new(template_id_or_alias)
                    .with_variables(variables),
            );
            let reply_to = if reply_to_override.is_empty() {
                &item.reply_to
            } else {
                reply_to_override
            };
            if !reply_to.is_empty() {
                request = request.with_reply_multiple(reply_to);
            }
        }
        Some(EmailBatchTemplate::Local { .. }) | None => {
            if let Some(text) = item.text.as_deref() {
                request = request.with_text(text);
            }
            if let Some(html) = item.html.as_deref() {
                request = request.with_html(html);
            }
            if !item.reply_to.is_empty() {
                request = request.with_reply_multiple(&item.reply_to);
            }
        }
    }
    for address in &item.cc {
        request = request.with_cc(address);
    }
    for address in &item.bcc {
        request = request.with_bcc(address);
    }
    if let Some(value) = item.in_reply_to.as_deref() {
        request = request.with_header("In-Reply-To", value);
    }
    if !item.references.is_empty() {
        request = request.with_header("References", &item.references.join(" "));
    }
    Ok(request)
}

fn format_from(name: Option<&str>, address: &str) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| format!("{name} <{address}>"))
        .unwrap_or_else(|| address.to_owned())
}

fn map_sdk_error(error: resend_rs::Error, api_key: &str) -> EmailError {
    match error {
        resend_rs::Error::RateLimit {
            ratelimit_limit,
            ratelimit_remaining,
            ratelimit_reset,
        } => EmailError::Provider(ProviderFailure {
            status: 429,
            class: ProviderErrorClass::RateLimited,
            message: "Resend rate limit exceeded".into(),
            rate_limit: ProviderRateLimitStatus {
                limit: ratelimit_limit,
                remaining: ratelimit_remaining,
                reset_seconds: ratelimit_reset,
                retry_after_seconds: None,
            },
        }),
        resend_rs::Error::Resend(response) => EmailError::Provider(ProviderFailure {
            status: response.status_code,
            class: class_from_name(&response.name),
            message: redact(&response.message, api_key),
            rate_limit: ProviderRateLimitStatus {
                limit: None,
                remaining: None,
                reset_seconds: None,
                retry_after_seconds: None,
            },
        }),
        resend_rs::Error::Http(error) => EmailError::Transport(redact(&error.to_string(), api_key)),
        resend_rs::Error::Parse { message, .. } | resend_rs::Error::Other(message) => {
            EmailError::Transport(redact(&message, api_key))
        }
    }
}

fn class_from_name(name: &str) -> ProviderErrorClass {
    match name {
        "missing_api_key" | "invalid_api_key" | "restricted_api_key" | "invalid_access" => {
            ProviderErrorClass::Auth
        }
        "rate_limit_exceeded" => ProviderErrorClass::RateLimited,
        "daily_quota_exceeded" => ProviderErrorClass::DailyQuotaExceeded,
        "monthly_quota_exceeded" => ProviderErrorClass::MonthlyQuotaExceeded,
        "validation_error"
        | "invalid_parameter"
        | "invalid_region"
        | "missing_required_field"
        | "invalid_attachment"
        | "invalid_from_address"
        | "invalid_idempotency_key"
        | "method_not_allowed" => ProviderErrorClass::Validation,
        "not_found" => ProviderErrorClass::NotFound,
        "invalid_idempotent_request" => ProviderErrorClass::IdempotencyPayloadMismatch,
        "concurrent_idempotent_requests" => ProviderErrorClass::IdempotencyRace,
        "internal_server_error" | "application_error" | "security_error" => {
            ProviderErrorClass::ProviderFault
        }
        _ => ProviderErrorClass::Unrecognized,
    }
}

fn redact(message: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        message.to_owned()
    } else {
        message.replace(api_key, "<redacted>")
    }
}

fn with_jitter(base: Duration) -> Duration {
    let jitter_ceiling = base.as_millis() as u64 / 2;
    if jitter_ceiling == 0 {
        return base;
    }
    base + Duration::from_millis(rand::thread_rng().gen_range(0..=jitter_ceiling))
}
