use schemars::JsonSchema;
use serde::Deserialize;

use crate::operation_registry::{
    descriptor::{
        ConnectionRef, ConnectionRequirement, EffectClass, MailboxToolkitId, OperationRegistration,
        OperatorToolkitId, ServiceToolkitId, SurfaceId, SurfaceSet,
    },
    execution::{
        BindingContext, BlockingTaskFailure, BoundedPublicValue, ClassifiedOwnerFailure,
        ExecutionDeclaration, PublicFailureCode, RecoveryDirective,
    },
    registration::OperationPolicy,
};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyInput {}

#[derive(Debug)]
pub(super) enum ConnectionBindingError {
    Selection,
    Owner(String),
}

/// A lost blocking-pool result is an owner-side detail read failure; the
/// classifier already projects `Owner` without its cause.
impl From<BlockingTaskFailure> for ConnectionBindingError {
    fn from(failure: BlockingTaskFailure) -> Self {
        Self::Owner(String::from(failure))
    }
}

pub(super) fn operator_id(
    context: BindingContext,
    expected: OperatorToolkitId,
) -> Result<String, ConnectionBindingError> {
    match context.connection {
        Some(ConnectionRef::OperatorAccount {
            toolkit,
            operator_account_id,
        }) if toolkit == expected => Ok(operator_account_id),
        _ => Err(ConnectionBindingError::Selection),
    }
}

pub(super) fn mailbox_id(context: BindingContext) -> Result<String, ConnectionBindingError> {
    match context.connection {
        Some(ConnectionRef::Mailbox {
            toolkit: MailboxToolkitId::Email,
            mailbox_ref_id,
        }) => Ok(mailbox_ref_id),
        _ => Err(ConnectionBindingError::Selection),
    }
}

pub(super) fn youtube_id(context: BindingContext) -> Result<String, ConnectionBindingError> {
    match context.connection {
        Some(ConnectionRef::ServiceCredential {
            toolkit: ServiceToolkitId::Youtube,
            credential_id,
        }) => Ok(credential_id),
        _ => Err(ConnectionBindingError::Selection),
    }
}

pub(super) fn metadata(id: onecrew_local_runtime::OperationId) -> OperationRegistration {
    OperationRegistration {
        id,
        domain_id: "accounts".to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new([
            SurfaceId::Full,
            SurfaceId::Compact,
            SurfaceId::Accounts,
            SurfaceId::Mail,
        ])
        .expect("connection operations have fixed surfaces"),
    }
}

pub(super) fn policy(connection: ConnectionRequirement) -> OperationPolicy {
    OperationPolicy::read_only(
        EffectClass::ReadLocal,
        connection,
        ExecutionDeclaration::Sequential,
    )
}

pub(super) fn classify_detail_failure(error: ConnectionBindingError) -> ClassifiedOwnerFailure {
    match error {
        ConnectionBindingError::Selection => known_local_failure(
            "connection_selection_invalid",
            Some(RecoveryDirective::CorrectInput),
        ),
        ConnectionBindingError::Owner(_cause) => known_local_failure(
            "connection_detail_failed",
            Some(connection_inventory_recovery()),
        ),
    }
}

fn known_local_failure(code: &str, recovery: Option<RecoveryDirective>) -> ClassifiedOwnerFailure {
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static public failure code is valid"),
        false,
        recovery,
    )
}

fn connection_inventory_recovery() -> RecoveryDirective {
    RecoveryDirective::ReadStatus {
        reference: BoundedPublicValue::parse("onecrew_connection_list")
            .expect("fixed public meta-tool reference is valid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_distinguishes_input_free_selection_and_owner_failures() {
        assert_eq!(
            classify_detail_failure(ConnectionBindingError::Selection).recovery(),
            Some(&RecoveryDirective::CorrectInput)
        );
        assert_eq!(
            classify_detail_failure(ConnectionBindingError::Owner(
                "sentinel-owner-cause".to_owned()
            ))
            .recovery(),
            Some(&connection_inventory_recovery())
        );
    }
}
