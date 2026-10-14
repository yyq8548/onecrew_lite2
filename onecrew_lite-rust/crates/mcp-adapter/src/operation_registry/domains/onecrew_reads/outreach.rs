use onecrew_creator_runtime::{
    ContactRouteLastContactAccount, ContactRouteLastContactAccountInput, ConversationGetInput,
    ConversationSummary, ConversationView, ListConversationsInput, ListReplyTriageInput,
    OutreachActionStatusInput, Page, ReplyTriageView,
};

use super::{
    super::RegistryOwners,
    shared::{classify_string_failure, metadata, read_policy},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    execution::run_blocking,
    registration::{RegistryBuildError, RegistryBuilder},
};

const OUTREACH_SURFACES: [SurfaceId; 3] =
    [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];
const TRIAGE_SURFACES: [SurfaceId; 4] = [
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Outreach,
    SurfaceId::Mail,
];

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let creators = owners.creators.clone();
    builder.register_operation::<
        ContactRouteLastContactAccountInput,
        Option<ContactRouteLastContactAccount>, _,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_ROUTE_LAST_CONTACT_ACCOUNT, "outreach", OUTREACH_SURFACES),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.contact_route_last_contact_account_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure("outreach_read_unavailable")),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ConversationGetInput, Option<ConversationView>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONVERSATION_GET,
            "outreach",
            OUTREACH_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.get_conversation_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            "outreach_read_unavailable",
        )),
    )?;

    let creators = owners.creators.clone();
    builder.register_operation::<ListConversationsInput, Page<ConversationSummary>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONVERSATION_LIST,
            "outreach",
            OUTREACH_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_dm_conversations_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            "outreach_read_unavailable",
        )),
    )?;

    let platform = owners.platform.clone();
    builder
        .register_operation::<OutreachActionStatusInput, onecrew_creator_runtime::CreatorOutreachStatus, onecrew_local_runtime::OutreachError, _, _, _>(
            metadata(onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS, "outreach", OUTREACH_SURFACES),
            read_policy(),
            move |_context, input| {
                let platform = platform.clone();
                async move { platform.outreach_action_status(input).await }
            },
            |error| crate::operation_registry::execution::ClassifiedOwnerFailure::known_no_effect(
                crate::operation_registry::execution::PublicFailureCode::parse("outreach_read_unavailable").expect("static status code"),
                false, None,
            ).with_message(&error.to_string()),
        )?;

    let creators = owners.creators;
    builder.register_operation::<ListReplyTriageInput, Page<ReplyTriageView>, String, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST,
            "triage",
            TRIAGE_SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { run_blocking(move || creators.list_reply_triage(input)).await }
        },
        classify_string_failure("triage_read_unavailable"),
    )
}
