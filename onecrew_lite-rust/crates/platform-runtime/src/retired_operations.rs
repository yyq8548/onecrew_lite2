use crate::OperationId;

pub const RETIRED_OPERATION_CODE: &str = "operation_retired";

const RETIRED_OPERATIONS: &[OperationId] = &[
    OperationId::EMAIL_CREDENTIAL_CONNECT,
    OperationId::EMAIL_CREDENTIAL_DELETE,
    OperationId::EMAIL_CREDENTIAL_ROTATE,
    OperationId::EMAIL_DIRECT_PREPARE,
    OperationId::EMAIL_DIRECT_SEND,
    OperationId::EMAIL_MANUAL_PREPARE,
    OperationId::EMAIL_MANUAL_SEND,
    OperationId::EMAIL_DRAFT_RESTORE,
    OperationId::EMAIL_DRAFT_SAVE,
    OperationId::EMAIL_DRAFT_SEND,
    OperationId::EMAIL_DRAFT_TRASH,
    OperationId::EMAIL_SEND_MANY,
    OperationId::EMAIL_SEND_MANY_RECONCILE,
    OperationId::EMAIL_TEMPLATE_DELETE,
    OperationId::EMAIL_TEMPLATE_RENDER,
    OperationId::EMAIL_TEMPLATE_SAVE,
    OperationId::INSTAGRAM_CONVERSATION_SYNC,
    OperationId::INSTAGRAM_MESSAGE_SEND,
    OperationId::TELEGRAM_CONVERSATION_SYNC,
    OperationId::TELEGRAM_MESSAGE_SEND,
    OperationId::X_CONVERSATION_SYNC,
    OperationId::X_MESSAGE_SEND,
    OperationId::X_CHAT_PASSCODE_CLEAR,
    OperationId::X_CHAT_PASSCODE_SET,
    OperationId::ONECREW_CREATOR_CONTENT_REPLY_PREPARE,
    OperationId::ONECREW_CREATOR_CONTENT_REPLY_SEND,
    OperationId::ONECREW_OUTREACH_MESSAGE_PREPARE,
    OperationId::ONECREW_OUTREACH_MESSAGE_SEND,
    OperationId::ONECREW_OUTREACH_RELATIONSHIP_PREPARE,
    OperationId::ONECREW_OUTREACH_RELATIONSHIP_UPDATE,
];

pub fn is_retired_operation(id: &str) -> bool {
    RETIRED_OPERATIONS.iter().any(|operation| operation.as_str() == id)
}

pub fn is_retired_result(snapshot: &crate::ResultSnapshot) -> bool {
    match snapshot.presentation.kind.as_str() {
        "creator_save_batch_confirmation" | "creator_save_batch_outcome"
        | "creator_batch_confirmation" | "creator_batch_outcome" | "search_plan_editor" => false,
        "outreach_confirmation" | "outreach_outcome" | "outreach_draft"
        | "email_confirmation" | "email_outcome" => true,
        _ => snapshot.batch.outcomes.iter().any(|outcome| is_retired_operation(&outcome.operation_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retirement_uses_real_catalog_identities_and_preserves_public_reads() {
        let catalog = crate::CapabilityCatalog::build().unwrap();
        assert_eq!(RETIRED_OPERATIONS.len(), 30);
        for operation in RETIRED_OPERATIONS {
            assert!(catalog.operation(operation.as_str()).is_some());
        }
        for operation in [
            OperationId::X_PROFILE_GET,
            OperationId::X_POST_THREAD_GET,
            OperationId::INSTAGRAM_PROFILE_GET,
            OperationId::TELEGRAM_COMMUNITY_GET,
            OperationId::TELEGRAM_COMMUNITY_ADMINISTRATORS_LIST,
            OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE,
            OperationId::ONECREW_SEARCH_PLAN_PREPARE,
            OperationId::ONECREW_ASK_USER_QUESTION,
            OperationId::ONECREW_OUTREACH_CONVERSATION_GET,
            OperationId::EMAIL_THREAD_GET,
            OperationId::EMAIL_MESSAGE_GET,
        ] {
            assert!(catalog.operation(operation.as_str()).is_some());
            assert!(!is_retired_operation(operation.as_str()));
        }
    }
}
