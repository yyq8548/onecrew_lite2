//! Safe community locators and observed-session peer recovery.

use grammers_client::{
    Client,
    peer::Peer,
    session::types::{PeerId, PeerKind, PeerRef},
};
use onecrew_tg_transport::{Connection, TgError};
use serde::{Deserialize, Serialize};

use crate::client::{administrators_for_peer, canonical_admin_community_from_peer};
use crate::{TgCommunityAdministrators, TgCommunityKind, TgCommunityReference, public_username};

/// Public alias or stable community identity, without peer authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TgCommunityLocator {
    PublicUsername {
        username: String,
    },
    StableCommunity {
        external_id: String,
        kind: TgCommunityKind,
    },
}

/// Typed provider outcome; callers must discard diagnostics at public boundaries.
#[derive(Debug)]
pub enum TgCommunityReadError {
    InvalidLocator,
    NotFound,
    UnsupportedKind,
    IdentityMismatch,
    PeerUnavailable,
    Provider(TgError),
}

impl From<TgError> for TgCommunityReadError {
    fn from(error: TgError) -> Self {
        Self::Provider(error)
    }
}

impl TgCommunityLocator {
    /// Validate before account admission or network access.
    ///
    /// # Errors
    /// Returns `InvalidLocator` for malformed usernames or noncanonical IDs.
    pub fn validate(&self) -> Result<(), TgCommunityReadError> {
        match self {
            Self::PublicUsername { username } => {
                public_username(username).map_err(|_| TgCommunityReadError::InvalidLocator)?;
            }
            Self::StableCommunity { external_id, .. } => {
                stable_peer_id(external_id)?;
            }
        }
        Ok(())
    }
}

fn stable_peer_id(value: &str) -> Result<PeerId, TgCommunityReadError> {
    let id = value
        .parse::<i64>()
        .map_err(|_| TgCommunityReadError::InvalidLocator)?;
    if id.to_string() != value {
        return Err(TgCommunityReadError::InvalidLocator);
    }
    PeerId::channel(id).ok_or(TgCommunityReadError::InvalidLocator)
}

// Bound recovery independently from the administrator page: a missing peer is
// unavailable within this read, never proof that the account cannot access it.
const PEER_RECOVERY_DIALOG_LIMIT: usize = 2_000;

async fn resolved_community(
    connection: &Connection,
    locator: &TgCommunityLocator,
) -> Result<(PeerRef, TgCommunityReference), TgCommunityReadError> {
    locator.validate()?;
    let client = connection.client();
    let (peer, expected) = match locator {
        TgCommunityLocator::PublicUsername { username } => {
            let username =
                public_username(username).map_err(|_| TgCommunityReadError::InvalidLocator)?;
            let peer = client
                .resolve_username(&username)
                .await
                .map_err(TgError::from_invocation)?
                .ok_or(TgCommunityReadError::NotFound)?;
            (peer, None)
        }
        TgCommunityLocator::StableCommunity { external_id, kind } => {
            let id = stable_peer_id(external_id)?;
            let peer = if let Some(reference) = connection.cached_peer_ref(id).await? {
                client
                    .resolve_peer(reference)
                    .await
                    .map_err(TgError::from_invocation)?
            } else {
                recover_dialog_peer(client, id).await?
            };
            (peer, Some((id, *kind)))
        }
    };
    let kind = match &peer {
        Peer::Channel(_) => TgCommunityKind::Channel,
        Peer::Group(group) if group.id().kind() == PeerKind::Channel => TgCommunityKind::Supergroup,
        _ => return Err(TgCommunityReadError::UnsupportedKind),
    };
    if let Some((expected_id, expected_kind)) = expected
        && (peer.id() != expected_id || kind != expected_kind)
    {
        return Err(TgCommunityReadError::IdentityMismatch);
    }
    let reference = peer
        .to_ref()
        .await
        .map_err(|_| TgCommunityReadError::PeerUnavailable)?
        .ok_or(TgCommunityReadError::PeerUnavailable)?;
    let id = peer
        .id()
        .bare_id()
        .ok_or(TgCommunityReadError::UnsupportedKind)?;
    let community = canonical_admin_community_from_peer(&peer, id, kind)?;
    Ok((reference, community))
}

async fn recover_dialog_peer(client: &Client, id: PeerId) -> Result<Peer, TgCommunityReadError> {
    let mut dialogs = client.iter_dialogs().limit(PEER_RECOVERY_DIALOG_LIMIT);
    while let Some(dialog) = dialogs.next().await.map_err(TgError::from_invocation)? {
        if dialog.peer_id() == id {
            return Ok(dialog.peer().clone());
        }
    }
    Err(TgCommunityReadError::PeerUnavailable)
}

/// Read a community using only a public alias or stable identity.
///
/// # Errors
/// Fails for invalid/missing/noncommunity targets, mismatches, provider failures,
/// or when no observed authorization can be recovered in 2,000 dialogs.
pub async fn community(
    connection: &Connection,
    locator: &TgCommunityLocator,
) -> Result<TgCommunityReference, TgCommunityReadError> {
    resolved_community(connection, locator)
        .await
        .map(|(_, community)| community)
}

/// Read one administrator page (at most 200 entries, no cursor).
///
/// # Errors
/// Returns the community resolution failure or Telegram's own error, such as
/// the `CHAT_ADMIN_REQUIRED` RPC error when the operator does not administer
/// the community.
pub async fn community_admins(
    connection: &Connection,
    locator: &TgCommunityLocator,
) -> Result<TgCommunityAdministrators, TgCommunityReadError> {
    let (reference, community) = resolved_community(connection, locator).await?;
    administrators_for_peer(connection.client(), reference, community)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locators_reject_message_links_and_noncanonical_stable_ids() {
        for value in [
            "https://t.me/name/123",
            "tg://join?invite=secret",
            "12345",
            "",
        ] {
            assert!(
                TgCommunityLocator::PublicUsername {
                    username: value.into()
                }
                .validate()
                .is_err()
            );
        }
        for value in ["+123", "00123", "-100123", "0", "", "9223372036854775808"] {
            assert!(stable_peer_id(value).is_err(), "{value}");
        }
        assert!(stable_peer_id("123").is_ok());
    }

    #[test]
    fn locator_wire_never_accepts_peer_authorization() {
        assert!(serde_json::from_str::<TgCommunityLocator>(r#"{"type":"stable_community","external_id":"123","kind":"channel","access_hash":"SECRET"}"#).is_err());
    }
}
