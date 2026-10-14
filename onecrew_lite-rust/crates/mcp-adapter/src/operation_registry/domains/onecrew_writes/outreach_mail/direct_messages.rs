//! The platform toolkits' own direct-message operations.
//!
//! Each toolkit owns the actual contact — the send and the thread sync — while
//! the Creator runtime stays the one durable ledger behind them. Every one of
//! these carries its account inside the conversation it names, so none of them
//! declares a connection, and each refuses a conversation that belongs to
//! another toolkit before it can reach a provider.

use onecrew_creator_runtime::{
    ConversationPlatformError, ConversationSyncInput, ConversationView, Platform,
    SendConversationReplyInput,
};
use onecrew_local_runtime::ToolkitId;

use super::{
    super::super::RegistryOwners,
    outreach::open_world_safety,
    owner_review::{classify_prepare_failure, review_policy},
    shared::{metadata, owner_guarded, read_status},
};
use crate::operation_registry::{
    descriptor::{EffectClass, ReadbackMode, SurfaceId},
    execution::{ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective},
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Outreach];
const CONVERSATION_GET: &str =
    onecrew_local_runtime::OperationId::ONECREW_OUTREACH_CONVERSATION_GET.as_str();
const FAILURE_CODE: &str = "outreach_operation_failed";
/// OneCrew's own refusal rather than a platform's: the named conversation
/// belongs to a different toolkit, so this is the wrong operation for it.
const PLATFORM_MISMATCH_CODE: &str = "conversation_platform_mismatch";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    for (toolkit, platform, approval) in [
        (
            ToolkitId::X,
            Platform::X,
            "Approve this X reply before sending it.",
        ),
        (
            ToolkitId::Instagram,
            Platform::Instagram,
            "Approve this Instagram reply before sending it.",
        ),
        (
            ToolkitId::Telegram,
            Platform::Telegram,
            "Approve this Telegram reply before sending it.",
        ),
    ] {
        let runtime = owners.platform.clone();
        builder.register_owner_review_operation::<
            SendConversationReplyInput,
            onecrew_local_runtime::OutreachCompletion,
            onecrew_local_runtime::OutreachError,
            _,
            _,
            _,
        >(
            metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_MESSAGE_SEND, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_MESSAGE_SEND, ToolkitId::Telegram => onecrew_local_runtime::OperationId::TELEGRAM_MESSAGE_SEND, _ => unreachable!() }, "outreach", SURFACES),
            review_policy(approval),
            move |context, input| {
                let runtime = runtime.clone();
                async move { runtime.prepare_outreach_conversation_reply(input, Some(platform), context.owner_review_context()?).await }
            },
            classify_prepare_failure,
        )?;

        let creators = owners.creators.clone();
        builder.register_operation::<
            ConversationSyncInput,
            Option<ConversationView>,
            ConversationPlatformError,
            _,
            _,
            _,
        >(
            metadata(match toolkit { ToolkitId::X => onecrew_local_runtime::OperationId::X_CONVERSATION_SYNC, ToolkitId::Instagram => onecrew_local_runtime::OperationId::INSTAGRAM_CONVERSATION_SYNC, ToolkitId::Telegram => onecrew_local_runtime::OperationId::TELEGRAM_CONVERSATION_SYNC, _ => unreachable!() }, "outreach", SURFACES),
            owner_guarded(
                EffectClass::SyncOpenWorld,
                open_world_safety(
                    ReadbackMode::InlineAndFollowUp,
                    &[CONVERSATION_GET],
                    &["/pending_triage"],
                    &[],
                ),
            ),
            move |_context, input| {
                let creators = creators.clone();
                async move { creators.sync_conversation(input, Some(platform)).await }
            },
            classify(CONVERSATION_GET),
        )?;
    }
    Ok(())
}

fn classify(
    recovery_operation: &'static str,
) -> impl Fn(ConversationPlatformError) -> ClassifiedOwnerFailure {
    move |error| match error {
        // Naming the wrong toolkit changed nothing anywhere; the fix is the
        // input, not a retry.
        ConversationPlatformError::WrongPlatform => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse(PLATFORM_MISMATCH_CODE)
                .expect("static public failure code is valid"),
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        ConversationPlatformError::ProviderRead(failure) => {
            ClassifiedOwnerFailure::known_no_effect(
                PublicFailureCode::parse(FAILURE_CODE)
                    .expect("static public failure code is valid"),
                false,
                None,
            )
            .with_message(failure.diagnostic.message)
            .with_dm_failure(failure)
        }
        ConversationPlatformError::GatewayUnavailable
        | ConversationPlatformError::InvalidSnapshot
        | ConversationPlatformError::RecordNotStarted => ClassifiedOwnerFailure::known_no_effect(
            PublicFailureCode::parse(FAILURE_CODE).expect("static public failure code is valid"),
            false,
            Some(read_status(recovery_operation)),
        ),
        // Local readback may follow a committed snapshot. Its stage text does
        // not prove no effect, and an uncertain commit must remain uncertain.
        ConversationPlatformError::LocalRead(_) | ConversationPlatformError::RecordUnknown => {
            ClassifiedOwnerFailure::unknown(
                PublicFailureCode::parse(FAILURE_CODE)
                    .expect("static public failure code is valid"),
                Some(read_status(recovery_operation)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_registry::execution::{
        ExecuteEffectState, project_classified_owner_failure,
    };

    #[test]
    fn sync_projects_native_evidence_but_local_commit_uncertainty_stays_unknown() {
        let mut diagnostic = onecrew_local_runtime::DmDiagnostic::provider(
            "telegram",
            "mtproto",
            "login 429 sent successfully timeout",
        );
        diagnostic.code = Some("PEER_FLOOD".into());
        diagnostic.rpc_code = Some(400);
        let failure = onecrew_local_runtime::DmFailure {
            stage: "thread_sync",
            diagnostic,
        };
        let wire = project_classified_owner_failure(classify(CONVERSATION_GET)(
            ConversationPlatformError::ProviderRead(failure.clone()),
        ));
        assert_eq!(wire.dm_failure.as_deref(), Some(&failure));
        assert_eq!(wire.effect, ExecuteEffectState::KnownNoEffect);
        assert!(!wire.retry_safe);
        assert_eq!(wire.recovery, None);
        for error in [
            ConversationPlatformError::RecordUnknown,
            ConversationPlatformError::LocalRead("triage readback"),
        ] {
            let wire = project_classified_owner_failure(classify(CONVERSATION_GET)(error));
            assert_eq!(wire.effect, ExecuteEffectState::Unknown);
            assert!(!wire.retry_safe);
            assert!(wire.dm_failure.is_none());
        }
    }
}
