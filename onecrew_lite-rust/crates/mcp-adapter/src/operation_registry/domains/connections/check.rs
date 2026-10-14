//! Live connection self-checks. One typed platform-runtime owner answers
//! whether the network or proxy is reachable and whether the provider still
//! accepts the stored session or key; provider reads that fail without an
//! HTTP response point the model here through their recovery.

use onecrew_local_runtime::{ConnectionCheck, ConnectionCheckError, OperatorAccountPlatform};

use super::{
    super::RegistryOwners,
    shared::{EmptyInput, mailbox_id, metadata, operator_id, youtube_id},
};
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, ConnectionRequirement, EffectClass, MailboxToolkitId, OperationRegistration,
        OperationSafety, OperationViewKind, OperatorToolkitId, SecretClass, ServiceToolkitId,
        SurfaceId, SurfaceSet,
    },
    execution::{
        BoundedPublicValue, ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode,
        RecoveryDirective,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};

/// A toolkit whose connections have a live self-check operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::operation_registry::domains) enum CheckedToolkit {
    X,
    Instagram,
    Telegram,
    Youtube,
    Email,
}

impl CheckedToolkit {
    pub(in crate::operation_registry::domains) const ALL: [Self; 5] = [
        Self::X,
        Self::Instagram,
        Self::Telegram,
        Self::Youtube,
        Self::Email,
    ];

    pub(in crate::operation_registry::domains) const fn operation_id(self) -> &'static str {
        match self {
            Self::X => onecrew_local_runtime::OperationId::X_CONNECTION_CHECK.as_str(),
            Self::Instagram => {
                onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_CHECK.as_str()
            }
            Self::Telegram => {
                onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_CHECK.as_str()
            }
            Self::Youtube => onecrew_local_runtime::OperationId::YOUTUBE_CONNECTION_CHECK.as_str(),
            Self::Email => onecrew_local_runtime::OperationId::EMAIL_CONNECTION_CHECK.as_str(),
        }
    }

    /// The toolkit whose self-check covers a binding's declared connection;
    /// `None` for a connection no self-check covers.
    pub(in crate::operation_registry::domains) fn for_connection(
        connection: &ConnectionRequirement,
    ) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|toolkit| &toolkit.connection() == connection)
    }

    /// The recovery that sends the model to this toolkit's self-check.
    pub(in crate::operation_registry::domains) fn recovery(self) -> RecoveryDirective {
        RecoveryDirective::ReadStatus {
            reference: BoundedPublicValue::parse(self.operation_id())
                .expect("static connection check reference is bounded"),
        }
    }

    /// A check runs on every surface where a recovery can name it: the
    /// connection surfaces, plus Creators, where the X, Instagram, Telegram,
    /// and YouTube provider reads that recover through it are registered.
    fn metadata(self) -> OperationRegistration {
        let mut surfaces = vec![
            SurfaceId::Full,
            SurfaceId::Compact,
            SurfaceId::Accounts,
            SurfaceId::Mail,
        ];
        if self != Self::Email {
            surfaces.push(SurfaceId::Creators);
        }
        OperationRegistration {
            surfaces: SurfaceSet::new(surfaces).expect("connection checks have fixed surfaces"),
            ..metadata(onecrew_local_runtime::OperationId::from_static(
                self.operation_id(),
            ))
        }
    }

    fn connection(self) -> ConnectionRequirement {
        match self {
            Self::X => ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::X,
            },
            Self::Instagram => ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::Instagram,
            },
            Self::Telegram => ConnectionRequirement::OperatorAccount {
                toolkit: OperatorToolkitId::Telegram,
            },
            Self::Youtube => ConnectionRequirement::ServiceCredential {
                toolkit: ServiceToolkitId::Youtube,
            },
            Self::Email => ConnectionRequirement::Mailbox {
                toolkit: MailboxToolkitId::Email,
            },
        }
    }

    /// Operator checks only read the vault. Service-credential checks record
    /// the credential's health locally, so they synchronize open-world state.
    const fn effect(self) -> EffectClass {
        match self {
            Self::X | Self::Instagram | Self::Telegram => EffectClass::ReadOpenWorld,
            Self::Youtube | Self::Email => EffectClass::SyncOpenWorld,
        }
    }

    fn policy(self) -> OperationPolicy {
        OperationPolicy {
            effect: self.effect(),
            secret: SecretClass::None,
            approval: ApprovalClass::None,
            connection: self.connection(),
            sensitive_input_paths: Vec::new(),
            view_kind: OperationViewKind::SimpleOperation,
            approval_request: None,
            safety: OperationSafety::read_only(),
            execution: ExecutionDeclaration::Sequential,
        }
    }
}

#[derive(Debug)]
enum CheckBindingError {
    Selection,
    Owner(ConnectionCheckError),
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    for toolkit in CheckedToolkit::ALL {
        let platform = owners.platform.clone();
        builder.register_operation::<EmptyInput, ConnectionCheck, CheckBindingError, _, _, _>(
            toolkit.metadata(),
            toolkit.policy(),
            move |context, _input| {
                let platform = platform.clone();
                async move {
                    let owner_result = match toolkit {
                        CheckedToolkit::X => {
                            let id = operator_id(context, OperatorToolkitId::X)
                                .map_err(|_| CheckBindingError::Selection)?;
                            platform
                                .operator_connection_check(OperatorAccountPlatform::X, &id)
                                .await
                        }
                        CheckedToolkit::Instagram => {
                            let id = operator_id(context, OperatorToolkitId::Instagram)
                                .map_err(|_| CheckBindingError::Selection)?;
                            platform
                                .operator_connection_check(OperatorAccountPlatform::Instagram, &id)
                                .await
                        }
                        CheckedToolkit::Telegram => {
                            let id = operator_id(context, OperatorToolkitId::Telegram)
                                .map_err(|_| CheckBindingError::Selection)?;
                            platform
                                .operator_connection_check(OperatorAccountPlatform::Telegram, &id)
                                .await
                        }
                        CheckedToolkit::Youtube => {
                            let id =
                                youtube_id(context).map_err(|_| CheckBindingError::Selection)?;
                            platform.youtube_connection_check(&id).await
                        }
                        CheckedToolkit::Email => {
                            let id =
                                mailbox_id(context).map_err(|_| CheckBindingError::Selection)?;
                            platform.mailbox_connection_check(&id).await
                        }
                    };
                    owner_result.map_err(CheckBindingError::Owner)
                }
            },
            classify_check_failure,
        )?;
    }
    Ok(())
}

fn classify_check_failure(error: CheckBindingError) -> ClassifiedOwnerFailure {
    match error {
        CheckBindingError::Selection => ClassifiedOwnerFailure::known_no_effect(
            failure_code("connection_selection_invalid"),
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        CheckBindingError::Owner(ConnectionCheckError::ConnectionUnavailable) => {
            ClassifiedOwnerFailure::known_no_effect(
                failure_code("connection_unavailable"),
                false,
                Some(connection_inventory_recovery()),
            )
        }
        // A service-credential check may already have recorded health, so a
        // local failure after the provider call leaves the effect unknown.
        CheckBindingError::Owner(ConnectionCheckError::Internal) => {
            ClassifiedOwnerFailure::unknown(
                failure_code("connection_check_unavailable"),
                Some(connection_inventory_recovery()),
            )
        }
    }
}

fn failure_code(code: &str) -> PublicFailureCode {
    PublicFailureCode::parse(code).expect("static connection check failure code is valid")
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
    fn owner_failures_are_safe_and_point_to_the_connection_inventory() {
        assert_eq!(
            classify_check_failure(CheckBindingError::Selection).recovery(),
            Some(&RecoveryDirective::CorrectInput)
        );
        assert_eq!(
            classify_check_failure(CheckBindingError::Owner(
                ConnectionCheckError::ConnectionUnavailable
            )),
            ClassifiedOwnerFailure::known_no_effect(
                failure_code("connection_unavailable"),
                false,
                Some(connection_inventory_recovery()),
            )
        );
        assert_eq!(
            classify_check_failure(CheckBindingError::Owner(ConnectionCheckError::Internal)),
            ClassifiedOwnerFailure::unknown(
                failure_code("connection_check_unavailable"),
                Some(connection_inventory_recovery()),
            )
        );
    }

    #[test]
    fn every_checked_toolkit_recovers_to_its_own_check_operation() {
        for toolkit in CheckedToolkit::ALL {
            assert_eq!(
                toolkit.recovery(),
                RecoveryDirective::ReadStatus {
                    reference: BoundedPublicValue::parse(toolkit.operation_id()).unwrap(),
                }
            );
            assert!(toolkit.operation_id().ends_with(".connection.check"));
        }
    }
}
