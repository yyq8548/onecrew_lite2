use crate::operation_registry::descriptor::{AuditMode, EffectClass};
use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::domains::onecrew_writes::creators::shared::{
    CREATOR_SURFACES, ConfirmationSpec, IdempotencySpec, ReadbackSpec, SafetySpec,
    classify_string_failure, metadata, policy,
};
use crate::operation_registry::execution::{
    BoundedPublicValue, ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective,
};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_creator_runtime::{
    ApproveDiscoveryCampaignInput, CreateDiscoveryCampaignInput, DiscoveryCampaign,
    IncreaseDiscoveryTargetInput, SendContentReplyInput,
};
use onecrew_local_runtime::{
    CreatorExportInput, CreatorExportResult, OpenCliDiscoveryPlatformSearchInput,
    OpenCliDiscoverySearchError, OpenCliDiscoverySearchInput, OpenCliDiscoverySearchResult,
    OpenCliDiscoveryVideoInput,
};

const EXPORT_FAILURE_CODE: &str = "creator_export_outcome_unknown";

pub(super) fn register_discovery_and_content(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    register_campaign(builder, owners.clone())?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::SubmitDiscoveryIdentityReviewInput, onecrew_creator_runtime::DiscoveryIdentityReviewReceipt, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_SUBMIT, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            idempotency: IdempotencySpec::Input(&["/pair/campaign_id", "/decision_id"]),
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_GET.as_str()] }),
            audit: AuditMode::OwnerRecorded,
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.submit_discovery_identity_review(input).await }
        },
        classify_string_failure("identity_review_outcome_unknown"),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::RevokeDiscoveryIdentityReviewInput, onecrew_creator_runtime::DiscoveryIdentityReviewReceipt, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_REVOKE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            idempotency: IdempotencySpec::Input(&["/campaign_id", "/decision_id"]),
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_GET.as_str()] }),
            audit: AuditMode::OwnerRecorded,
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.revoke_discovery_identity_review(input).await }
        },
        classify_string_failure("identity_review_outcome_unknown"),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::SubmitDiscoveryReviewInput, onecrew_creator_runtime::DiscoveryReviewReceipt, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_SUBMIT, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            idempotency: IdempotencySpec::Input(&["/campaign_id", "/request_id"]),
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_GET.as_str()] }),
            ..SafetySpec::local()
        }),
        move |_context,input| {
            let platform = platform.clone();
            async move { platform.submit_discovery_review(input).await }
        },
        classify_string_failure("review_submit_outcome_unknown"),
    )?;
    let platform = owners.platform.clone();
    builder.register_owner_review_operation::<SendContentReplyInput, onecrew_local_runtime::OutreachCompletion, onecrew_local_runtime::OutreachError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTENT_REPLY_SEND, "creators", CREATOR_SURFACES),
        policy(
            EffectClass::ExternalWrite,
            SafetySpec {
                confirmation: ConfirmationSpec::Attended("Confirm posting this reply to the prepared Creator content?"),
                idempotency: IdempotencySpec::Input(&["/idempotency_key"]),
                readback: ReadbackSpec::Inline,
                audit: AuditMode::OwnerRecorded,
                ..SafetySpec::local()
            },
        ),
        move |context, input| {
            let platform = platform.clone();
            async move { platform.prepare_outreach_content_reply(input, context.owner_review_context()?).await }
        },
        super::super::outreach_mail::classify_outreach_prepare_failure,
    )?;

    let platform = owners.platform;
    builder.register_operation::<CreatorExportInput, CreatorExportResult, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_EXPORT,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/format"]),
                readback: ReadbackSpec::Inline,
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.export_creators_to_system_desktop(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            EXPORT_FAILURE_CODE,
        )),
    )
}

fn register_campaign(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_local_runtime::OpenCliDiscoveryAdvanceInput, onecrew_local_runtime::OpenCliDiscoveryAdvanceResult, OpenCliDiscoverySearchError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_ADVANCE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str()] }),
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.advance_discovery_campaign(input).await }
        },
        classify_campaign_read_failure,
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<CreateDiscoveryCampaignInput, DiscoveryCampaign, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_CREATE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                idempotency: IdempotencySpec::Input(&["/id"]),
                readback: ReadbackSpec::InlineAndFollowUp(
                    const {
                        &[
                            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET
                                .as_str(),
                        ]
                    },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.create_discovery_campaign_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            "discovery_create_outcome_unknown",
        )),
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<IncreaseDiscoveryTargetInput, DiscoveryCampaign, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_TARGET_INCREASE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(EffectClass::WriteLocal, SafetySpec {
            confirmation: ConfirmationSpec::Attended(
                "Adjust this Campaign's target without changing its criteria or read budget?",
            ),
            idempotency: IdempotencySpec::Input(&["/decision_id"]),
            readback: ReadbackSpec::InlineAndFollowUp(const {
                &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str()]
            }),
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.increase_discovery_target(input).await }
        },
        classify_target_increase_failure,
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ApproveDiscoveryCampaignInput, DiscoveryCampaign, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_APPROVE,
            "creators",
            CREATOR_SURFACES,
        ),
        policy(
            EffectClass::WriteLocal,
            SafetySpec {
                confirmation: ConfirmationSpec::Attended(
                    "Approve this Campaign's exact criteria, profile, platforms, and read budget?",
                ),
                idempotency: IdempotencySpec::Input(&["/campaign_id", "/expected_revision"]),
                readback: ReadbackSpec::InlineAndFollowUp(
                    const {
                        &[
                            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET
                                .as_str(),
                        ]
                    },
                ),
                ..SafetySpec::local()
            },
        ),
        move |_context, input| {
            let creators = creators.clone();
            async move {
                creators
                    .approve_discovery_campaign_async(input.campaign_id, input.expected_revision)
                    .await
            }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            "discovery_approval_outcome_unknown",
        )),
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<OpenCliDiscoverySearchInput, OpenCliDiscoverySearchResult, OpenCliDiscoverySearchError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_YOUTUBE_SEARCH_ONCE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str()] }),
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.search_discovery_youtube_once(&input.campaign_id, input.expected_revision, &input.profile_ref, input.limit).await }
        },
        classify_campaign_read_failure,
    )?;

    let platform = owners.platform.clone();
    builder.register_operation::<OpenCliDiscoveryPlatformSearchInput, OpenCliDiscoverySearchResult, OpenCliDiscoverySearchError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_SEARCH_ONCE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str()] }),
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.search_discovery_once(&input.campaign_id, input.expected_revision, &input.platform, &input.profile_ref, input.limit).await }
        },
        classify_campaign_read_failure,
    )?;

    let platform = owners.platform;
    builder.register_operation::<OpenCliDiscoveryVideoInput, DiscoveryCampaign, OpenCliDiscoverySearchError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_YOUTUBE_VERIFY_HIT_ONCE, "creators", CREATOR_SURFACES),
        policy(EffectClass::WriteLocal, SafetySpec {
            readback: ReadbackSpec::InlineAndFollowUp(const { &[onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str()] }),
            ..SafetySpec::local()
        }),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.verify_discovery_youtube_hit_once(&input.campaign_id, input.expected_revision, &input.profile_ref, &input.search_hit_id).await }
        },
        classify_campaign_read_failure,
    )
}

fn classify_target_increase_failure(error: String) -> ClassifiedOwnerFailure {
    match error.as_str() {
        "library_identity_unavailable"
        | "target_increase_input_invalid"
        | "target_increase_idempotency_conflict"
        | "target_increase_campaign_changed"
        | "target_increase_campaign_unavailable" => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse(&error).expect("closed target failure code"),
            false,
            None,
        ),
        _ => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("target_increase_outcome_unknown").unwrap(),
            Some(RecoveryDirective::ReadStatus {
                reference: BoundedPublicValue::parse("onecrew.discovery.campaign.get").unwrap(),
            }),
        ),
    }
}

fn classify_campaign_read_failure(error: OpenCliDiscoverySearchError) -> ClassifiedOwnerFailure {
    let readback = || {
        Some(RecoveryDirective::ReadStatus {
            reference: BoundedPublicValue::parse("onecrew.discovery.campaign.get")
                .expect("static operation ID"),
        })
    };
    match error {
        OpenCliDiscoverySearchError::Advance { failure, .. } => {
            classify_campaign_read_failure(*failure)
        }
        OpenCliDiscoverySearchError::Dependency(_) => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse("discovery_dependency_unavailable").unwrap(),
            false,
            None,
        ),
        OpenCliDiscoverySearchError::Campaign(_) => ClassifiedOwnerFailure::unknown(
            PublicFailureCode::parse("discovery_read_outcome_unknown").unwrap(),
            readback(),
        ),
        OpenCliDiscoverySearchError::Provider(_) => ClassifiedOwnerFailure::applied(
            PublicFailureCode::parse("discovery_provider_read_stopped").unwrap(),
            readback(),
        ),
    }
}
