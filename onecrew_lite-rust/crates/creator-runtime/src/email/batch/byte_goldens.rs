//! Capture the real Creator batch envelopes and fingerprint functions before
//! replacing their nested duplicate provider contracts. No provider is opened.
use super::*;
use crate::{EmailCapability, MailboxProvider};
use onecrew_email_contracts::{
    EmailBatchChunkResult as ProviderEmailBatchChunkResult,
    EmailBatchItemResult as ProviderEmailBatchItemResult,
};

fn provider_items() -> Vec<EmailBatchItem> {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../email-contracts/src/testdata/batch-items.json"
    )))
    .unwrap()
}

fn business_item(index: usize, item: &EmailBatchItem) -> EmailBatchItemInput {
    let content = match &item.template {
        None => EmailBatchContentInput::InlineRendered {
            subject: item.subject.clone().unwrap(),
            text: item.text.clone(),
            html: item.html.clone(),
        },
        Some(EmailBatchTemplate::Local {
            template_id,
            revision,
            variables,
        }) => EmailBatchContentInput::LocalTemplate {
            template_id: EmailTemplateId::new(template_id),
            revision: *revision,
            variables: variables.clone(),
        },
        Some(EmailBatchTemplate::Published {
            template_id_or_alias,
            variables,
            from_override,
            subject_override,
            reply_to_override,
        }) => EmailBatchContentInput::PublishedTemplate {
            template_id_or_alias: template_id_or_alias.clone(),
            variables: variables.clone(),
            from_override: from_override.clone(),
            subject_override: subject_override.clone(),
            reply_to_override: reply_to_override.clone(),
        },
    };
    EmailBatchItemInput {
        recipient: if index == 1 {
            EmailBatchRecipient::CreatorRoute {
                creator_id: CreatorId::new("creator-local"),
                contact_route_id: CreatorContactRouteId::new("route-local"),
            }
        } else {
            EmailBatchRecipient::ExplicitAddress {
                to: item.to.clone(),
            }
        },
        from_name: item.from_name.clone(),
        cc: item.cc.clone(),
        bcc: item.bcc.clone(),
        reply_to: item.reply_to.clone(),
        in_reply_to: item.in_reply_to.clone(),
        references: item.references.clone(),
        content,
    }
}

fn fixture(
    validation: EmailBatchValidation,
    rejected_item: bool,
) -> (
    SendManyEmailInput,
    RemoteStartedEmailBatchAction,
    RemoteAcceptedEmailBatchAction,
) {
    let mailbox = MailboxDetail {
        mailbox_ref_id: MailboxRefId::new("mailbox-fixture"),
        provider: MailboxProvider::Resend,
        label: "Fixture mailbox".into(),
        email_address: "sender@example.invalid".into(),
        display_name: Some("Fixture sender".into()),
        capabilities: vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
        ],
        available: true,
        issues: vec![],
        created_at: 1_700_000_001,
        updated_at: 1_700_000_002,
    };
    let items = provider_items();
    let mut input = SendManyEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        idempotency_key: "fixture-intent-世界".into(),
        validation,
        items: items
            .iter()
            .enumerate()
            .map(|(index, item)| business_item(index, item))
            .collect(),
    };
    let mut frozen: Vec<_> = items
        .into_iter()
        .enumerate()
        .map(|(index, request)| FrozenEmailBatchItem {
            creator_id: (index == 1).then(|| CreatorId::new("creator-local")),
            contact_route_id: (index == 1).then(|| CreatorContactRouteId::new("route-local")),
            preflight_rejection: None,
            request,
        })
        .collect();
    if rejected_item {
        input.items.push(EmailBatchItemInput {
            recipient: EmailBatchRecipient::CreatorRoute {
                creator_id: CreatorId::new("creator-rejected"),
                contact_route_id: CreatorContactRouteId::new("route-rejected"),
            },
            from_name: None,
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            in_reply_to: None,
            references: vec![],
            content: EmailBatchContentInput::InlineRendered {
                subject: "Rejected item".into(),
                text: Some("Synthetic body".into()),
                html: None,
            },
        });
        frozen.push(FrozenEmailBatchItem {
            creator_id: Some(CreatorId::new("creator-rejected")),
            contact_route_id: Some(CreatorContactRouteId::new("route-rejected")),
            preflight_rejection: Some("synthetic route preflight rejection".into()),
            request: EmailBatchItem {
                from_name: None,
                from_email: mailbox.email_address.clone(),
                to: vec![],
                cc: vec![],
                bcc: vec![],
                reply_to: vec![],
                in_reply_to: None,
                references: vec![],
                subject: Some(String::new()),
                text: None,
                html: None,
                template: None,
            },
        });
    }
    let request = EmailBatchRequest {
        idempotency_key: input.idempotency_key.clone(),
        validation: match validation {
            EmailBatchValidation::Strict => onecrew_email_contracts::EmailBatchValidation::Strict,
            EmailBatchValidation::Permissive => {
                onecrew_email_contracts::EmailBatchValidation::Permissive
            }
        },
        items: frozen.iter().map(|item| item.request.clone()).collect(),
    };
    let provider_fingerprint = logical_fingerprint(&request).unwrap();
    let fingerprint = action_fingerprint(&input, &provider_fingerprint).unwrap();
    let started = RemoteStartedEmailBatchAction {
        mailbox,
        request,
        logical_fingerprint: provider_fingerprint.clone(),
        action_fingerprint: fingerprint,
        items: frozen,
    };
    let mut outcomes = vec![
        ProviderEmailBatchItemResult {
            input_index: 2,
            outcome: EmailBatchItemOutcome::Unknown {
                message: "synthetic missing readback".into(),
            },
        },
        ProviderEmailBatchItemResult {
            input_index: 0,
            outcome: EmailBatchItemOutcome::Accepted {
                provider_message_id: "provider-message-0".into(),
            },
        },
        ProviderEmailBatchItemResult {
            input_index: 1,
            outcome: EmailBatchItemOutcome::Rejected {
                message: "synthetic rejection".into(),
            },
        },
    ];
    if rejected_item {
        outcomes.push(ProviderEmailBatchItemResult {
            input_index: 3,
            outcome: EmailBatchItemOutcome::Accepted {
                provider_message_id: "ignored-preflight-rejection".into(),
            },
        });
    }
    let result = EmailBatchResult {
        logical_fingerprint: provider_fingerprint,
        chunks: vec![
            ProviderEmailBatchChunkResult {
                chunk_index: 0,
                idempotency_key: "fixture:chunk-0".into(),
                outcomes: outcomes[..2].to_vec(),
            },
            ProviderEmailBatchChunkResult {
                chunk_index: 1,
                idempotency_key: "fixture:chunk-1".into(),
                outcomes: outcomes[2..].to_vec(),
            },
        ],
        outcomes,
    };
    let accepted = RemoteAcceptedEmailBatchAction {
        started: started.clone(),
        result,
    };
    (input, started, accepted)
}

fn persisted(value: &impl Serialize) -> String {
    serde_json::to_string(&serde_json::to_value(value).unwrap()).unwrap()
}

fn snapshot() -> BTreeMap<String, String> {
    let mut observed = BTreeMap::new();
    for (name, validation, rejection) in [
        ("strict", EmailBatchValidation::Strict, false),
        ("permissive", EmailBatchValidation::Permissive, false),
        (
            "permissive_rejected",
            EmailBatchValidation::Permissive,
            true,
        ),
    ] {
        let (input, started, accepted) = fixture(validation, rejection);
        observed.insert(
            format!("{name}.business_input.bytes"),
            serde_json::to_string(&input).unwrap(),
        );
        observed.insert(
            format!("{name}.provider_fingerprint"),
            logical_fingerprint(&started.request).unwrap(),
        );
        observed.insert(
            format!("{name}.action_fingerprint"),
            action_fingerprint(&input, &started.logical_fingerprint).unwrap(),
        );
        observed.insert(
            format!("{name}.action_identity.bytes"),
            serde_json::to_string(&EmailBatchActionIdentity {
                mailbox_ref_id: &input.mailbox_ref_id,
                validation: input.validation,
                items: &input.items,
            })
            .unwrap(),
        );
        observed.insert(
            format!("{name}.started.bytes"),
            serde_json::to_string(&started).unwrap(),
        );
        observed.insert(format!("{name}.started.persisted"), persisted(&started));
        observed.insert(
            format!("{name}.accepted.bytes"),
            serde_json::to_string(&accepted).unwrap(),
        );
        observed.insert(format!("{name}.accepted.persisted"), persisted(&accepted));
        observed.insert(
            format!("{name}.frozen_items.persisted"),
            persisted(&started.items),
        );
        let normalized = normalize_provider_result(
            &accepted.result,
            &started.request,
            &started.logical_fingerprint,
            &started.action_fingerprint,
            &started.items,
        );
        observed.insert(
            format!("{name}.normalized_result.persisted"),
            persisted(&normalized),
        );
        for outcome in normalized.outcomes {
            observed.insert(
                format!("{name}.item.{}.fingerprint", outcome.input_index),
                outcome.payload_fingerprint,
            );
        }
    }
    observed
}

#[test]
fn creator_email_batch_storage_goldens() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/email/batch/testdata/recovery-v1.json");
    let expected: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(path).expect("install the captured pre-cutover golden first"),
    )
    .unwrap();
    assert_eq!(snapshot(), expected);
    for name in ["strict", "permissive", "permissive_rejected"] {
        let started: RemoteStartedEmailBatchAction =
            serde_json::from_str(&expected[&format!("{name}.started.persisted")]).unwrap();
        let accepted: RemoteAcceptedEmailBatchAction =
            serde_json::from_str(&expected[&format!("{name}.accepted.persisted")]).unwrap();
        assert_eq!(
            persisted(&started),
            expected[&format!("{name}.started.persisted")]
        );
        assert_eq!(
            persisted(&accepted),
            expected[&format!("{name}.accepted.persisted")]
        );
    }
    assert_eq!(
        expected["strict.provider_fingerprint"],
        expected["permissive.provider_fingerprint"]
    );
    assert_ne!(
        expected["strict.action_fingerprint"],
        expected["permissive.action_fingerprint"]
    );
    let normalized: EmailBatchActionResult =
        serde_json::from_str(&expected["permissive_rejected.normalized_result.persisted"]).unwrap();
    assert!(
        matches!(&normalized.outcomes[3].outcome,EmailBatchOutcome::Rejected {message} if message=="synthetic route preflight rejection")
    );
}
