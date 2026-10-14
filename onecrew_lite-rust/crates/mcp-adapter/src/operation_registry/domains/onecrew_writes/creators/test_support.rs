use crate::accounts::{OperatorAccountWorkflow, PlatformRuntimeOperatorAccountWorkflow};
use crate::operation_registry::descriptor::{EntryId, OperationEntry, RegistryEntry, SurfaceId};
use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::execution::{
    ExecuteFailure, ExecutionContext, dispatch_prepared, prepare_invocation,
};
use crate::operation_registry::registration::OperationRegistry;
use std::future::Future;
use std::sync::Arc;

fn owners(platform: Arc<onecrew_local_runtime::LocalPlatform>) -> RegistryOwners {
    let operator_accounts: Arc<dyn OperatorAccountWorkflow> = Arc::new(
        PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    RegistryOwners {
        creators: platform.creators().expect("OneCrew runtime"),
        platform,
        operator_accounts,
    }
}

pub(super) fn registry(platform: Arc<onecrew_local_runtime::LocalPlatform>) -> OperationRegistry {
    let owners = owners(platform);
    let expected = crate::operation_registry::catalog_test_support::catalog(&[
        onecrew_local_runtime::OperationId::EMAIL_DIRECT_PREPARE.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_DRAFT_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_DRAFT_LIST.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_FOLDER_LIST.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_MAILBOX_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_MAILBOX_SYNC_STATUS.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_MANUAL_PREPARE.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_MESSAGE_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_LIST.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_TEMPLATE_RENDER.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_THREAD_GET.as_str(),
        onecrew_local_runtime::OperationId::EMAIL_THREAD_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_ARCHIVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_BATCH_CREATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_ADD.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_DELETE_CONFIRM.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_DELETE_PREPARE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTACT_ROUTE_UPDATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTENT_REPLY_PREPARE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CONTENT_REPLY_SEND.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_DELETE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_EXPORT.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_BULK_MOVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_CREATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_DELETE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_SET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_GROUP_UPDATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_IDENTITY_RESOLVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_MANUAL_CREATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_STAGE_UPDATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_SOURCES_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_INTENT_VALIDATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_CREATE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_APPROVE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CANDIDATES_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CANDIDATES_COUNT.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_HITS_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_SEARCH_ONCE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_ADVANCE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_TRANSCRIPT_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_PUBLIC_PAGE_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_MATERIAL_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_MATERIAL_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_GROUPS_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_SUBMIT.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_REVOKE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_SUBMIT.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_TARGET_INCREASE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_PROFILE_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CONTENT_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_YOUTUBE_SEARCH_ONCE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_YOUTUBE_VERIFY_HIT_ONCE.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_ACTION_STATUS.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONTACT_ROUTE_LAST_CONTACT_ACCOUNT
            .as_str(),
        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONVERSATION_GET.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONVERSATION_LIST.as_str(),
        onecrew_local_runtime::OperationId::ONECREW_TRIAGE_LIST.as_str(),
    ]);
    let mut builder = crate::operation_registry::registration::RegistryBuilder::new(expected);
    crate::operation_registry::domains::onecrew_reads::register(&mut builder, owners.clone())
        .expect("OneCrew reads");
    crate::operation_registry::domains::onecrew_writes::creators::register(&mut builder, owners)
        .expect("Creator writes");
    builder.build().expect("Creator registry")
}

pub(super) fn operation<'a>(registry: &'a OperationRegistry, id: &str) -> &'a OperationEntry {
    let id = EntryId::parse(id).expect("fixture operation ID");
    match registry
        .entry(&id, SurfaceId::Full)
        .expect("operation exists")
    {
        RegistryEntry::Operation(operation) => operation,
        RegistryEntry::Playbook(_) | RegistryEntry::ToolkitGuide(_) => {
            panic!("fixture ID resolved to a non-operation")
        }
    }
}

fn run<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Runtime::new()
        .expect("Tokio runtime")
        .block_on(future)
}

pub(super) fn execute_current(
    operation: &OperationEntry,
    context: ExecutionContext<'_>,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, ExecuteFailure> {
    let prepared = prepare_invocation(operation, context, operation.input.revision(), arguments)?;
    run(dispatch_prepared(prepared))
}
