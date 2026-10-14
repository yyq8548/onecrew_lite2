//! The private IO seam of this workflow. Production calls typed toolkit
//! functions; offline workflow tests supply the same concrete effect semantics.

use async_trait::async_trait;
use onecrew_creator_runtime::{
    CreatorOutreachTarget, CreatorRouteOutreachContext, Platform, RelationshipMutation,
    RelationshipValue, TargetKind,
};
use onecrew_dm_contracts::{
    DmCapability, DmFailure, DmReceipt, DmSendError, DmSendRequest, DmTarget, DmTargetKind,
    DmThreadReadback,
};

use super::OutreachError;
use crate::{LocalPlatform, toolkits};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EffectFailure {
    Refused,
    Unknown,
}

#[derive(Clone, Debug)]
pub(super) struct RelationshipReadback {
    pub following: Option<bool>,
    pub can_message: Option<bool>,
    /// Present only when the provider reports its pending outgoing request.
    /// This native read fact is not a second persisted Creator relationship.
    pub pending_follow_request: Option<bool>,
}

pub(super) fn unfollowed_confirmation_required(
    follow_relevant: bool,
    following: RelationshipValue,
    allow_unfollowed: bool,
) -> bool {
    follow_relevant && following != RelationshipValue::Yes && !allow_unfollowed
}

#[derive(Clone, Debug)]
pub(super) struct ReplyReadback {
    pub id: String,
    pub body: String,
    pub author_external_id: String,
    pub reply_to_id: Option<String>,
    pub conversation_id: Option<String>,
}

#[async_trait]
pub(super) trait OutreachEffects: Send + Sync {
    async fn relationship_read(
        &self,
        operator: &str,
        target: &CreatorOutreachTarget,
    ) -> Result<RelationshipReadback, OutreachError>;
    async fn relationship_write(
        &self,
        context: &CreatorRouteOutreachContext,
        action: RelationshipMutation,
    ) -> Result<(), EffectFailure>;
    async fn reply_read(
        &self,
        operator_id: &str,
        content_id: &str,
    ) -> Result<Option<ReplyReadback>, OutreachError>;
    async fn reply_write(
        &self,
        operator_id: &str,
        reply_to_id: &str,
        body: &str,
    ) -> Result<String, EffectFailure>;
    async fn message_capability(
        &self,
        context: &CreatorRouteOutreachContext,
    ) -> Result<DmCapability, OutreachError>;
    async fn message_write(
        &self,
        context: &CreatorRouteOutreachContext,
        body: &str,
    ) -> Result<DmReceipt, DmSendError>;
    async fn message_read(
        &self,
        context: &CreatorRouteOutreachContext,
        thread_id: Option<&str>,
    ) -> Result<DmThreadReadback, OutreachError>;
}

#[async_trait]
impl OutreachEffects for LocalPlatform {
    async fn relationship_read(
        &self,
        operator: &str,
        target: &CreatorOutreachTarget,
    ) -> Result<RelationshipReadback, OutreachError> {
        let (external_id, following, can_message, pending_follow_request) = match target.platform {
            Platform::X => {
                let handle = target.handle.as_deref().ok_or(OutreachError::InvalidInput(
                    "X target handle is unavailable",
                ))?;
                let value = toolkits::x::relationship_get(
                    self,
                    operator,
                    &target.external_target_id,
                    handle,
                )
                .await
                .map_err(|error| {
                    OutreachError::DirectMessage(DmFailure {
                        stage: "relationship_read",
                        diagnostic: error.diagnostic("x"),
                    })
                })?;
                (value.external_id, value.following, value.can_message, None)
            }
            Platform::Instagram => {
                let value = toolkits::instagram::relationship_get(
                    self,
                    operator,
                    &target.external_target_id,
                )
                .await
                .map_err(|error| {
                    OutreachError::DirectMessage(DmFailure {
                        stage: "relationship_read",
                        diagnostic: error.diagnostic("instagram"),
                    })
                })?;
                let observed = instagram_relationship_readback(value);
                (
                    target.external_target_id.clone(),
                    observed.following,
                    observed.can_message,
                    observed.pending_follow_request,
                )
            }
            _ => return Err(OutreachError::WrongPlatform),
        };
        if external_id != target.external_target_id {
            return Err(OutreachError::IdentityMismatch);
        }
        Ok(RelationshipReadback {
            following,
            can_message,
            pending_follow_request,
        })
    }

    async fn relationship_write(
        &self,
        context: &CreatorRouteOutreachContext,
        action: RelationshipMutation,
    ) -> Result<(), EffectFailure> {
        let operator = context.operator_account.id.as_str();
        let target = &context.target;
        match target.platform {
            Platform::X => toolkits::x::relationship_update(
                self,
                operator,
                match action {
                    RelationshipMutation::Follow => onecrew_x::RelationshipAction::Follow,
                    RelationshipMutation::Unfollow => onecrew_x::RelationshipAction::Unfollow,
                },
                &target.external_target_id,
            )
            .await
            .map_err(x_write_failure),
            Platform::Instagram => toolkits::instagram::relationship_update(
                self,
                operator,
                match action {
                    RelationshipMutation::Follow => onecrew_ig::IgRelationshipAction::Follow,
                    RelationshipMutation::Unfollow => onecrew_ig::IgRelationshipAction::Unfollow,
                },
                &target.external_target_id,
            )
            .await
            .map_err(ig_write_failure),
            _ => Err(EffectFailure::Refused),
        }
    }

    async fn reply_read(
        &self,
        operator_id: &str,
        content_id: &str,
    ) -> Result<Option<ReplyReadback>, OutreachError> {
        toolkits::x::reply_readback(self, operator_id, content_id)
            .await
            .map(|reply| {
                reply.map(|reply| ReplyReadback {
                    id: reply.id,
                    body: reply.text,
                    author_external_id: reply.author.id,
                    reply_to_id: reply.in_reply_to_status_id,
                    conversation_id: reply.conversation_id,
                })
            })
            .map_err(|_| OutreachError::ProviderRead("content"))
    }

    async fn reply_write(
        &self,
        operator_id: &str,
        reply_to_id: &str,
        body: &str,
    ) -> Result<String, EffectFailure> {
        toolkits::x::reply(self, operator_id, reply_to_id, body)
            .await
            .map(|receipt| receipt.external_id)
            .map_err(x_write_failure)
    }

    async fn message_capability(
        &self,
        context: &CreatorRouteOutreachContext,
    ) -> Result<DmCapability, OutreachError> {
        let target = dm_target(&context.target)?;
        let operator = context.operator_account.id.as_str();
        let result = match context.target.platform {
            Platform::X => toolkits::x::message_capability(self, operator, &target).await,
            Platform::Instagram => {
                toolkits::instagram::message_capability(self, operator, &target).await
            }
            Platform::Telegram => {
                toolkits::telegram::message_capability(self, operator, &target).await
            }
            _ => return Err(OutreachError::WrongPlatform),
        };
        result.map_err(|error| {
            OutreachError::DirectMessage(DmFailure {
                stage: "message_capability",
                diagnostic: error.diagnostic(dm_platform(context.target.platform)),
            })
        })
    }

    async fn message_write(
        &self,
        context: &CreatorRouteOutreachContext,
        body: &str,
    ) -> Result<DmReceipt, DmSendError> {
        let request = DmSendRequest {
            target: dm_target(&context.target).map_err(|_| {
                DmSendError::refused(onecrew_dm_contracts::DmDiagnostic::local(
                    dm_platform(context.target.platform),
                    "onecrew_outreach",
                    "invalid_target",
                    "Direct-message target is invalid",
                ))
            })?,
            body: body.to_owned(),
        };
        let operator = context.operator_account.id.as_str();
        let result = match context.target.platform {
            Platform::X => toolkits::x::message_send(self, operator, request).await,
            Platform::Instagram => toolkits::instagram::message_send(self, operator, request).await,
            Platform::Telegram => toolkits::telegram::message_send(self, operator, request).await,
            _ => {
                return Err(DmSendError::refused(
                    onecrew_dm_contracts::DmDiagnostic::local(
                        dm_platform(context.target.platform),
                        "onecrew_outreach",
                        "unsupported_platform",
                        "This platform does not support direct messages",
                    ),
                ));
            }
        };
        result.map_err(|error| {
            let diagnostic = error.diagnostic(dm_platform(context.target.platform));
            match error.send_failure_kind() {
                onecrew_dm_contracts::DmSendFailureKind::Refused => {
                    DmSendError::refused(diagnostic)
                }
                onecrew_dm_contracts::DmSendFailureKind::Unknown => {
                    DmSendError::unknown(diagnostic)
                }
            }
        })
    }

    async fn message_read(
        &self,
        context: &CreatorRouteOutreachContext,
        thread_id: Option<&str>,
    ) -> Result<DmThreadReadback, OutreachError> {
        let target = dm_target(&context.target)?;
        let operator = context.operator_account.id.as_str();
        let result = match context.target.platform {
            Platform::X => {
                toolkits::x::message_thread_readback(self, operator, &target, thread_id, 100).await
            }
            Platform::Instagram => {
                toolkits::instagram::message_thread_readback(
                    self, operator, &target, thread_id, 100,
                )
                .await
            }
            Platform::Telegram => {
                toolkits::telegram::message_thread_readback(self, operator, &target, thread_id, 100)
                    .await
            }
            _ => return Err(OutreachError::WrongPlatform),
        };
        result.map_err(|error| {
            OutreachError::DirectMessage(DmFailure {
                stage: "message_readback",
                diagnostic: error.diagnostic(dm_platform(context.target.platform)),
            })
        })
    }
}

fn instagram_relationship_readback(
    status: onecrew_ig::IgRelationshipStatus,
) -> RelationshipReadback {
    // An outgoing request proves acceptance only. Pending approval is neither
    // a verified follow nor permission to skip the unfollowed-recipient gate.
    RelationshipReadback {
        following: Some(status.following),
        can_message: None,
        pending_follow_request: Some(status.outgoing_request),
    }
}

fn dm_target(target: &CreatorOutreachTarget) -> Result<DmTarget, OutreachError> {
    Ok(DmTarget {
        handle: target.handle.clone().ok_or(OutreachError::InvalidInput(
            "direct-message target handle is unavailable",
        ))?,
        external_id: target.external_target_id.clone(),
        kind: match target.target_kind {
            TargetKind::User => DmTargetKind::User,
            TargetKind::Bot => DmTargetKind::Bot,
            TargetKind::Channel => DmTargetKind::Channel,
            TargetKind::Supergroup => DmTargetKind::Supergroup,
            TargetKind::Group => DmTargetKind::Group,
            TargetKind::Creator => DmTargetKind::Creator,
            TargetKind::Business => DmTargetKind::Business,
        },
    })
}

fn x_write_failure(error: toolkits::x::XToolkitWriteError) -> EffectFailure {
    match error {
        toolkits::x::XToolkitWriteError::Admission(_)
        | toolkits::x::XToolkitWriteError::Provider(onecrew_x::XWriteFailure::Refused) => {
            EffectFailure::Refused
        }
        toolkits::x::XToolkitWriteError::Provider(onecrew_x::XWriteFailure::Unknown) => {
            EffectFailure::Unknown
        }
    }
}

fn ig_write_failure(error: toolkits::instagram::IgToolkitWriteError) -> EffectFailure {
    match error {
        toolkits::instagram::IgToolkitWriteError::Admission(_)
        | toolkits::instagram::IgToolkitWriteError::Provider(onecrew_ig::IgWriteFailure::Refused) => {
            EffectFailure::Refused
        }
        toolkits::instagram::IgToolkitWriteError::Provider(onecrew_ig::IgWriteFailure::Unknown) => {
            EffectFailure::Unknown
        }
    }
}

fn dm_platform(platform: Platform) -> &'static str {
    match platform {
        Platform::X => "x",
        Platform::Instagram => "instagram",
        Platform::Telegram => "telegram",
        Platform::YouTube => "youtube",
        Platform::TikTok => "tiktok",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{PlatformConnectionResolutionError, SearchPlanApprovalError};
    use onecrew_ig::IgError;
    use onecrew_x::{TransportError, XClientError, XError};
    use toolkits::PlatformReadError;

    const CONNECTION_FAILURES: [PlatformConnectionResolutionError; 4] = [
        PlatformConnectionResolutionError::InvalidIdentifier,
        PlatformConnectionResolutionError::NotFoundOrMismatch,
        PlatformConnectionResolutionError::Unavailable,
        PlatformConnectionResolutionError::Internal,
    ];

    fn refusal_name(reason: SearchPlanApprovalError) -> &'static str {
        match reason {
            SearchPlanApprovalError::ConfirmationRequired => "ConfirmationRequired",
            SearchPlanApprovalError::InvalidOrExpired => "InvalidOrExpired",
            SearchPlanApprovalError::ScopeMismatch => "ScopeMismatch",
            SearchPlanApprovalError::Unavailable => "Unavailable",
        }
    }

    fn connection_variant(error: PlatformConnectionResolutionError) -> String {
        let name = match error {
            PlatformConnectionResolutionError::InvalidIdentifier => "InvalidIdentifier",
            PlatformConnectionResolutionError::NotFoundOrMismatch => "NotFoundOrMismatch",
            PlatformConnectionResolutionError::Unavailable => "Unavailable",
            PlatformConnectionResolutionError::Internal => "Internal",
        };
        format!("Connection.{name}")
    }

    /// Compares diagnostics keyed `platform/variant` with the frozen JSON,
    /// which keeps protocol, origin, native or local code and HTTP status.
    fn assert_frozen_diagnostics(
        platform: &str,
        diagnostics: impl IntoIterator<Item = (String, onecrew_dm_contracts::DmDiagnostic)>,
    ) {
        let mut actual = BTreeMap::new();
        for (variant, diagnostic) in diagnostics {
            let key = format!("{platform}/{variant}");
            let wire = serde_json::to_value(diagnostic).expect("a DM diagnostic serializes");
            assert!(
                actual.insert(key.clone(), wire).is_none(),
                "duplicate row {key}"
            );
        }
        let frozen: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(include_str!("fixtures/read_failure_dm_diagnostics.json"))
                .expect("frozen DM diagnostics are JSON");
        let expected = frozen
            .into_iter()
            .filter(|(key, _)| key.split('/').next() == Some(platform))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            actual.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>(),
            "{platform}: frozen diagnostic keys"
        );
        for (key, wire) in &actual {
            assert_eq!(wire, &expected[key], "{key}");
        }
    }

    /// Every local read outcome, with `input_invalid` as the owner's input
    /// explanation.
    fn local_read_errors(input_invalid: &'static str) -> Vec<PlatformReadError> {
        let mut local = vec![
            PlatformReadError::InputInvalid(input_invalid),
            PlatformReadError::SessionUnavailable,
            PlatformReadError::CredentialUnavailable,
            PlatformReadError::CredentialHealthCommitUnknown,
            PlatformReadError::Interrupted,
            PlatformReadError::NativeDataNotObject,
        ];
        local.extend(SearchPlanApprovalError::ALL.map(PlatformReadError::SearchPlanRefused));
        local.extend(CONNECTION_FAILURES.map(PlatformReadError::Connection));
        local
    }

    /// A local read outcome keyed by its variant; platform failures are keyed
    /// by the platform failure whose evidence they carry.
    fn local_read_variant(error: &PlatformReadError) -> String {
        match error {
            PlatformReadError::InputInvalid(_) => "InputInvalid".to_owned(),
            PlatformReadError::SearchPlanRefused(reason) => {
                format!("SearchPlanRefused.{}", refusal_name(*reason))
            }
            PlatformReadError::Connection(error) => connection_variant(*error),
            PlatformReadError::SessionUnavailable => "SessionUnavailable".to_owned(),
            PlatformReadError::CredentialUnavailable => "CredentialUnavailable".to_owned(),
            PlatformReadError::CredentialHealthCommitUnknown => {
                "CredentialHealthCommitUnknown".to_owned()
            }
            PlatformReadError::Interrupted => "Interrupted".to_owned(),
            PlatformReadError::NativeDataNotObject => "NativeDataNotObject".to_owned(),
            PlatformReadError::Platform { .. } => {
                unreachable!("platform failures are keyed by their platform failure")
            }
        }
    }

    /// Local outcomes name themselves through the shared read diagnostic; X
    /// failures keep X's own evidence: HTTP status and native code, a local
    /// code for a precondition, or neither for a missing response.
    #[test]
    fn x_relationship_read_failure_dm_diagnostics_match_the_frozen_json() {
        let local = local_read_errors("X stable target ID and handle are required");
        let http = |status, code| TransportError::Http {
            status,
            code,
            message: "provider body".into(),
        };
        let platform = [
            (
                "Transport.Network",
                XError::Transport(TransportError::Network("connect".into())),
            ),
            (
                "Transport.MissingCredential",
                XError::Transport(TransportError::MissingCredential),
            ),
            ("Transport.Http@401", XError::Transport(http(401, Some(32)))),
            ("Transport.Http@429", XError::Transport(http(429, Some(88)))),
            (
                "Transport.Http@503",
                XError::Transport(http(503, Some(130))),
            ),
            (
                "Transport.ApiResponse@200",
                XError::Transport(TransportError::ApiResponse {
                    status: 200,
                    code: Some(214),
                }),
            ),
            (
                "Client.Input",
                XError::Client(XClientError::Input(
                    "X relationship target stable ID changed".into(),
                )),
            ),
            (
                "Client.Response",
                XError::Client(XClientError::Response("unexpected".into())),
            ),
        ];
        let rows = local
            .into_iter()
            .map(|error| (local_read_variant(&error), error))
            .chain(platform.into_iter().map(|(name, error)| {
                (
                    format!("Platform.{name}"),
                    PlatformReadError::Platform {
                        evidence: error.evidence(),
                        after_local_commit: false,
                    },
                )
            }));
        assert_frozen_diagnostics(
            "x",
            rows.map(|(variant, error)| (variant, error.diagnostic("x"))),
        );
    }

    /// Local outcomes name themselves through the shared read diagnostic;
    /// Instagram failures keep Instagram's own evidence: HTTP status and
    /// `error_type`, a local code for a precondition or unusable response, or
    /// neither when no native fact was retained.
    #[test]
    fn instagram_relationship_read_failure_dm_diagnostics_match_the_frozen_json() {
        let local = local_read_errors("Instagram stable user ID is required");
        let platform = [
            ("Session", IgError::Session("expired".into())),
            ("InvalidHeader", IgError::InvalidHeader),
            ("Network", IgError::Network("connect".into())),
            ("Api", IgError::Api("user not found: creator".into())),
            ("Challenge", IgError::Challenge("checkpoint".into())),
            (
                "Response@404",
                IgError::Response {
                    status: 404,
                    code: Some("user_not_found".to_owned()),
                },
            ),
            (
                "Response@429",
                IgError::Response {
                    status: 429,
                    code: None,
                },
            ),
        ];
        let rows = local
            .into_iter()
            .map(|error| (local_read_variant(&error), error))
            .chain(platform.into_iter().map(|(name, error)| {
                (
                    format!("Platform.{name}"),
                    PlatformReadError::Platform {
                        evidence: onecrew_ig::evidence(&error),
                        after_local_commit: false,
                    },
                )
            }));
        assert_frozen_diagnostics(
            "instagram",
            rows.map(|(variant, error)| (variant, error.diagnostic("instagram"))),
        );
    }

    #[test]
    fn instagram_pending_follow_does_not_verify_following_or_skip_explicit_permission() {
        let observed = instagram_relationship_readback(onecrew_ig::IgRelationshipStatus {
            following: false,
            outgoing_request: true,
            followed_by: false,
            blocking: false,
        });
        assert!(!crate::outreach::recovery::relationship_satisfied(
            &observed,
            RelationshipMutation::Follow
        ));
        assert!(!crate::outreach::recovery::relationship_satisfied(
            &observed,
            RelationshipMutation::Unfollow
        ));
        assert!(unfollowed_confirmation_required(
            true,
            RelationshipValue::Unknown,
            false
        ));
        assert!(!unfollowed_confirmation_required(
            true,
            RelationshipValue::Unknown,
            true
        ));
        assert_eq!(observed.can_message, None);
    }

    #[test]
    fn instagram_unfollow_requires_the_outgoing_request_to_be_cancelled() {
        let observed = instagram_relationship_readback(onecrew_ig::IgRelationshipStatus {
            following: false,
            outgoing_request: false,
            followed_by: false,
            blocking: false,
        });
        assert!(crate::outreach::recovery::relationship_satisfied(
            &observed,
            RelationshipMutation::Unfollow
        ));
    }
}
