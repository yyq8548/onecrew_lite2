//! Public Telegram community reads intentionally exclude private user fields.
//! A failed community read explains itself through protocol-native evidence.

use onecrew_dm_contracts::DmDiagnostic;
use onecrew_tg_client::{TgCommunityAdministrators, TgCommunityReadError, TgUser};
use serde::Serialize;

use crate::direct_messages::local_diagnostic;

/// Public administrator identity; never carries phone numbers or peer hashes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TgPublicUser {
    pub id: String,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub is_bot: bool,
    pub verified: bool,
}

impl From<TgUser> for TgPublicUser {
    fn from(user: TgUser) -> Self {
        Self {
            id: user.id,
            username: user.username,
            first_name: user.first_name,
            last_name: user.last_name,
            is_bot: user.is_bot,
            verified: user.verified,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TgPublicAdministrator {
    pub role: crate::TgAdministratorRole,
    pub user: TgPublicUser,
}

/// One bounded administrator response; absence is not proof of nonmembership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TgPublicAdministrators {
    pub community: crate::TgPublicCommunity,
    pub administrators: Vec<TgPublicAdministrator>,
    /// Provider request limit. This API has no continuation cursor.
    pub page_limit: u16,
}

impl From<TgCommunityAdministrators> for TgPublicAdministrators {
    fn from(value: TgCommunityAdministrators) -> Self {
        Self {
            community: value.community,
            administrators: value
                .administrators
                .into_iter()
                .map(|admin| TgPublicAdministrator {
                    role: admin.role,
                    user: admin.user.into(),
                })
                .collect(),
            page_limit: 200,
        }
    }
}

/// The protocol-native evidence of a failed community read. A conclusion the
/// client reached itself (an invalid locator, no such community, a target that
/// is not a channel or supergroup, a changed identity or kind, or a peer whose
/// authorization cannot be recovered) is local evidence with a Telegram-owned
/// code and explanation; a Telegram failure is its [`crate::evidence`].
pub fn community_read_evidence(error: &TgCommunityReadError) -> DmDiagnostic {
    match error {
        TgCommunityReadError::InvalidLocator => local_diagnostic(
            "invalid_community_locator",
            "Telegram community locator is invalid",
        ),
        TgCommunityReadError::NotFound => {
            local_diagnostic("community_not_found", "Telegram community was not found")
        }
        TgCommunityReadError::UnsupportedKind => local_diagnostic(
            "unsupported_community_kind",
            "Telegram target must be a channel or supergroup",
        ),
        TgCommunityReadError::IdentityMismatch => local_diagnostic(
            "community_identity_mismatch",
            "Telegram community identity or kind no longer matches",
        ),
        TgCommunityReadError::PeerUnavailable => local_diagnostic(
            "community_peer_unavailable",
            "Telegram community authorization could not be recovered; stable-ID recovery checks this session and at most 2000 dialogs",
        ),
        TgCommunityReadError::Provider(error) => crate::evidence(error),
    }
}

#[cfg(test)]
mod tests {
    use onecrew_dm_contracts::DmDiagnosticOrigin;
    use onecrew_tg_transport::TgError;

    use super::*;

    #[test]
    fn administrator_output_drops_phone_in_serialization_and_debug() {
        let user = TgPublicUser::from(TgUser {
            id: "123".into(),
            username: Some("person".into()),
            first_name: Some("Name".into()),
            last_name: None,
            phone: Some("PHONE_SENTINEL".into()),
            is_bot: false,
            verified: true,
        });
        let json = serde_json::to_value(&user).unwrap();
        assert!(json.get("phone").is_none());
        assert!(json.get("access_hash").is_none());
        assert_eq!(json.as_object().unwrap().len(), 6);
        assert!(!format!("{user:?}").contains("SENTINEL"));
    }

    /// Names every community read failure, so a new failure fails to compile
    /// here until it gains an arm. The match does not add the failure to the
    /// samples below; its author must add that sample and its code.
    fn covered(error: &TgCommunityReadError) {
        match error {
            TgCommunityReadError::InvalidLocator
            | TgCommunityReadError::NotFound
            | TgCommunityReadError::UnsupportedKind
            | TgCommunityReadError::IdentityMismatch
            | TgCommunityReadError::PeerUnavailable
            | TgCommunityReadError::Provider(_) => {}
        }
    }

    #[test]
    fn local_community_conclusions_are_local_evidence_with_telegram_codes() {
        for (error, code) in [
            (
                TgCommunityReadError::InvalidLocator,
                "invalid_community_locator",
            ),
            (TgCommunityReadError::NotFound, "community_not_found"),
            (
                TgCommunityReadError::UnsupportedKind,
                "unsupported_community_kind",
            ),
            (
                TgCommunityReadError::IdentityMismatch,
                "community_identity_mismatch",
            ),
            (
                TgCommunityReadError::PeerUnavailable,
                "community_peer_unavailable",
            ),
        ] {
            covered(&error);
            let evidence = community_read_evidence(&error);
            assert_eq!(evidence.origin, DmDiagnosticOrigin::Local, "{error:?}");
            assert_eq!(evidence.code.as_deref(), Some(code), "{error:?}");
            assert_eq!(
                (evidence.platform, evidence.protocol),
                ("telegram", "mtproto")
            );
            assert_eq!((evidence.http_status, evidence.rpc_code), (None, None));
            assert!(evidence.message.starts_with("Telegram "), "{error:?}");
        }
    }

    #[test]
    fn a_telegram_failure_keeps_its_rpc_code_and_name() {
        // An operator who does not administer the community is refused by
        // Telegram itself, so its answer keeps the native name too.
        for (code, name) in [(420, "FLOOD_WAIT"), (400, "CHAT_ADMIN_REQUIRED")] {
            let rpc = || TgError::Rpc {
                code,
                name: Some(name.into()),
            };
            let error = TgCommunityReadError::Provider(rpc());
            covered(&error);
            let evidence = community_read_evidence(&error);
            assert_eq!(evidence, crate::evidence(&rpc()), "{name}");
            assert_eq!(evidence.origin, DmDiagnosticOrigin::Provider, "{name}");
            assert_eq!(evidence.rpc_code, Some(code), "{name}");
            assert_eq!(evidence.code.as_deref(), Some(name));
            assert_eq!(evidence.http_status, None, "{name}");
        }
    }

    #[test]
    fn failures_discard_every_opaque_diagnostic() {
        for error in [
            TgError::Session("SESSION_SENTINEL".into()),
            TgError::Auth("AUTH_SENTINEL".into()),
            TgError::Network("socks5://USER:PASS_SENTINEL@host".into()),
            TgError::Permission("PHONE_SENTINEL +15551234567".into()),
            TgError::Other("HASH_SENTINEL".into()),
        ] {
            let evidence = community_read_evidence(&TgCommunityReadError::Provider(error));
            for view in [
                format!("{evidence:?}"),
                serde_json::to_string(&evidence).unwrap(),
            ] {
                for absent in ["SENTINEL", "socks5://", "USER:", "+15551234567"] {
                    assert!(!view.contains(absent), "{absent}: {view}");
                }
            }
        }
    }
}
