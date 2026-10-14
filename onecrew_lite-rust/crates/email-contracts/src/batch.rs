//! Provider-neutral native batch preparation and result contracts.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::EmailError;

/// Validation mode passed to a provider-native batch endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailBatchValidation {
    Strict,
    Permissive,
}

/// Either a fully rendered local payload or a provider-published template.
/// Published templates cannot carry inline text/html/react content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmailBatchTemplate {
    /// The Creator owner rendered and froze the final subject/body before provider dispatch.
    Local {
        template_id: String,
        revision: u64,
        /// Explicit variables are retained in the frozen provider-neutral
        /// payload so the logical fingerprint covers template identity and
        /// inputs, even when two renderings happen to produce equal text.
        variables: BTreeMap<String, String>,
    },
    /// Resend-hosted published template. Its id/alias and variables remain a
    /// provider-specific payload and cannot carry inline body fields.
    Published {
        template_id_or_alias: String,
        variables: BTreeMap<String, serde_json::Value>,
        from_override: Option<String>,
        subject_override: Option<String>,
        reply_to_override: Vec<String>,
    },
}

/// One ordered, fully rendered batch item. `subject` is optional only when a
/// published provider template supplies it or an explicit override is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchItem {
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub template: Option<EmailBatchTemplate>,
}

impl EmailBatchItem {
    pub fn validate(&self) -> Result<(), EmailError> {
        if self.from_email.trim().is_empty() {
            return Err(EmailError::InvalidMessage("from_email is required".into()));
        }
        if self.to.is_empty() || self.to.iter().any(|address| address.trim().is_empty()) {
            return Err(EmailError::InvalidMessage(
                "at least one recipient is required".into(),
            ));
        }
        match &self.template {
            Some(EmailBatchTemplate::Local {
                template_id,
                revision,
                ..
            }) => {
                if template_id.trim().is_empty() {
                    return Err(EmailError::InvalidMessage(
                        "local template id is required".into(),
                    ));
                }
                if *revision == 0 {
                    return Err(EmailError::InvalidMessage(
                        "local template revision must be positive".into(),
                    ));
                }
                if self
                    .subject
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                    || (self.text.as_deref().is_none_or(str::is_empty)
                        && self.html.as_deref().is_none_or(str::is_empty))
                {
                    return Err(EmailError::InvalidMessage(
                        "local template must carry its rendered subject and body".into(),
                    ));
                }
            }
            Some(EmailBatchTemplate::Published {
                template_id_or_alias,
                from_override,
                subject_override,
                reply_to_override,
                ..
            }) => {
                if template_id_or_alias.trim().is_empty() {
                    return Err(EmailError::InvalidMessage(
                        "published template id or alias is required".into(),
                    ));
                }
                if self.text.is_some() || self.html.is_some() {
                    return Err(EmailError::InvalidMessage(
                        "published template cannot include inline text or html".into(),
                    ));
                }
                if self.subject.is_some() && subject_override.is_some() {
                    return Err(EmailError::InvalidMessage(
                        "published template cannot duplicate a subject override".into(),
                    ));
                }
                if from_override
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
                    || reply_to_override
                        .iter()
                        .any(|value| value.trim().is_empty())
                {
                    return Err(EmailError::InvalidMessage(
                        "published template override is empty".into(),
                    ));
                }
            }
            None => {
                if self
                    .subject
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty())
                {
                    return Err(EmailError::InvalidMessage("subject is required".into()));
                }
                if self.text.as_deref().is_none_or(str::is_empty)
                    && self.html.as_deref().is_none_or(str::is_empty)
                {
                    return Err(EmailError::InvalidMessage(
                        "text or html body is required".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// One prepared native-batch intent. The logical key covers ordered payload,
/// while each provider chunk derives a stable child key from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchRequest {
    pub idempotency_key: String,
    pub validation: EmailBatchValidation,
    pub items: Vec<EmailBatchItem>,
}

impl EmailBatchRequest {
    pub const MAX_ITEMS_PER_PROVIDER_REQUEST: usize = 100;
    /// Keep native-batch child idempotency keys within the 256-byte limit
    /// required by the shipped provider contract.
    pub const MAX_IDEMPOTENCY_KEY_LENGTH: usize = 256;

    pub fn validate(&self) -> Result<(), EmailError> {
        let key = self.idempotency_key.trim();
        if key.is_empty() || key.len() > Self::MAX_IDEMPOTENCY_KEY_LENGTH {
            return Err(EmailError::InvalidMessage(
                "batch idempotency_key must contain 1-256 characters".into(),
            ));
        }
        if self.items.is_empty() {
            return Err(EmailError::InvalidMessage(
                "batch must contain at least one item".into(),
            ));
        }
        Ok(())
    }

    /// Fingerprint of the ordered, fully rendered payload, excluding caller
    /// identity and execution policy so retries can prove payload identity
    /// independently. In particular, strict versus permissive validation is
    /// not a payload change: it only controls how the provider validates it.
    pub fn logical_fingerprint(&self) -> Result<String, EmailError> {
        let bytes = serde_json::to_vec(&self.items)
            .map_err(|error| EmailError::InvalidMessage(error.to_string()))?;
        Ok(hex_digest(&bytes))
    }

    pub fn chunk_idempotency_key(&self, logical_fingerprint: &str, chunk_index: usize) -> String {
        // Keep the caller key readable when there is room, but derive the
        // suffix from the complete caller key, payload fingerprint, and
        // chunk index. This makes truncating a long caller key safe while
        // keeping every child key within the provider's 256-byte limit.
        let caller_key = self.idempotency_key.trim();
        let child_digest =
            hex_digest(format!("{caller_key}\0{logical_fingerprint}\0{chunk_index}").as_bytes());
        let suffix = format!("chunk-{chunk_index}-{child_digest}");
        let separator_len = usize::from(!caller_key.is_empty());
        let prefix_budget =
            Self::MAX_IDEMPOTENCY_KEY_LENGTH.saturating_sub(separator_len + suffix.len());
        let caller_prefix = truncate_utf8(caller_key, prefix_budget);
        if caller_prefix.is_empty() {
            suffix
        } else {
            format!("{caller_prefix}:{suffix}")
        }
    }
}

/// Truncate a UTF-8 string by bytes without splitting a code point. Provider
/// keys are normally ASCII, but the contract should remain panic-free for a
/// caller that supplies Unicode before validation is applied.
fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmailBatchItemOutcome {
    Accepted { provider_message_id: String },
    Rejected { message: String },
    Unknown { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchItemResult {
    pub input_index: usize,
    pub outcome: EmailBatchItemOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchChunkResult {
    pub chunk_index: usize,
    pub idempotency_key: String,
    pub outcomes: Vec<EmailBatchItemResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchResult {
    pub logical_fingerprint: String,
    pub chunks: Vec<EmailBatchChunkResult>,
    pub outcomes: Vec<EmailBatchItemResult>,
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(index: usize) -> EmailBatchItem {
        EmailBatchItem {
            from_name: None,
            from_email: "sender@example.com".into(),
            to: vec![format!("recipient-{index}@example.com")],
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject: Some(format!("Subject {index}")),
            text: Some("Body".into()),
            html: None,
            template: None,
        }
    }

    #[test]
    fn fingerprint_is_payload_bound_and_chunk_keys_are_stable() {
        let request = EmailBatchRequest {
            idempotency_key: "intent".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![item(0), item(1)],
        };
        let fingerprint = request.logical_fingerprint().unwrap();
        assert_eq!(fingerprint, request.logical_fingerprint().unwrap());
        assert_ne!(
            fingerprint,
            EmailBatchRequest {
                items: vec![item(1), item(0)],
                ..request.clone()
            }
            .logical_fingerprint()
            .unwrap()
        );
        assert_ne!(
            request.chunk_idempotency_key(&fingerprint, 0),
            request.chunk_idempotency_key(&fingerprint, 1)
        );

        // The logical payload identity must not depend on the caller's
        // retry key; changing that key changes the child key prefix but not
        // the payload fingerprint suffix.
        let same_payload_different_key = EmailBatchRequest {
            idempotency_key: "another-intent".into(),
            ..request.clone()
        };
        assert_eq!(
            fingerprint,
            same_payload_different_key.logical_fingerprint().unwrap()
        );
        let permissive = EmailBatchRequest {
            validation: EmailBatchValidation::Permissive,
            ..request.clone()
        };
        assert_eq!(fingerprint, permissive.logical_fingerprint().unwrap());
        assert_eq!(
            request.chunk_idempotency_key(&fingerprint, 0),
            permissive.chunk_idempotency_key(&fingerprint, 0)
        );
        assert_ne!(
            request.chunk_idempotency_key(&fingerprint, 0),
            same_payload_different_key.chunk_idempotency_key(&fingerprint, 0)
        );
    }

    #[test]
    fn chunk_keys_are_bounded_and_include_full_parent_identity() {
        let request = EmailBatchRequest {
            idempotency_key: "k".repeat(256),
            validation: EmailBatchValidation::Strict,
            items: vec![item(0)],
        };
        let fingerprint = request.logical_fingerprint().unwrap();
        let key = request.chunk_idempotency_key(&fingerprint, 0);
        assert!(key.len() <= EmailBatchRequest::MAX_IDEMPOTENCY_KEY_LENGTH);

        // Distinct parent keys that share the visible prefix still derive
        // different children because the digest covers the complete key.
        let mut twin = request.clone();
        twin.idempotency_key.replace_range(255.., "x");
        assert_ne!(
            key,
            twin.chunk_idempotency_key(&fingerprint, 0),
            "long-key truncation must not alias idempotency children"
        );

        // The public helper is defensive for malformed/non-hex input too;
        // callers normally pass the 64-character SHA-256 above.
        let short = request.chunk_idempotency_key("short", 0);
        assert!(short.len() <= EmailBatchRequest::MAX_IDEMPOTENCY_KEY_LENGTH);
    }

    #[test]
    fn permissive_request_keeps_item_validation_for_provider_partial_results() {
        let mut invalid = item(0);
        invalid.text = None;
        invalid.subject = None;
        let request = EmailBatchRequest {
            idempotency_key: "intent".into(),
            validation: EmailBatchValidation::Permissive,
            items: vec![invalid],
        };
        assert!(request.validate().is_ok());
        assert!(request.items[0].validate().is_err());
    }

    #[test]
    fn batch_outcomes_round_trip_all_terminal_states_with_input_indices() {
        let result = EmailBatchResult {
            logical_fingerprint: "a".repeat(64),
            chunks: vec![EmailBatchChunkResult {
                chunk_index: 0,
                idempotency_key: "intent:chunk-0-a".into(),
                outcomes: vec![
                    EmailBatchItemResult {
                        input_index: 0,
                        outcome: EmailBatchItemOutcome::Accepted {
                            provider_message_id: "provider-0".into(),
                        },
                    },
                    EmailBatchItemResult {
                        input_index: 1,
                        outcome: EmailBatchItemOutcome::Rejected {
                            message: "invalid recipient".into(),
                        },
                    },
                    EmailBatchItemResult {
                        input_index: 2,
                        outcome: EmailBatchItemOutcome::Unknown {
                            message: "response omitted item".into(),
                        },
                    },
                ],
            }],
            outcomes: vec![
                EmailBatchItemResult {
                    input_index: 0,
                    outcome: EmailBatchItemOutcome::Accepted {
                        provider_message_id: "provider-0".into(),
                    },
                },
                EmailBatchItemResult {
                    input_index: 1,
                    outcome: EmailBatchItemOutcome::Rejected {
                        message: "invalid recipient".into(),
                    },
                },
                EmailBatchItemResult {
                    input_index: 2,
                    outcome: EmailBatchItemOutcome::Unknown {
                        message: "response omitted item".into(),
                    },
                },
            ],
        };
        let encoded = serde_json::to_string(&result).unwrap();
        let decoded: EmailBatchResult = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, result);
    }
}

#[cfg(test)]
#[path = "batch_storage_goldens.rs"]
mod storage_goldens;
