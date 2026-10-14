use grammers_client::peer::Peer;
use grammers_client::session::types::{PeerAuth, PeerId, PeerKind, PeerRef};
use grammers_client::{Client, InvocationError, tl};
use onecrew_tg_transport::TgError;

use crate::models::{
    TgChat, TgCommunityTarget, TgLocator, TgLocatorKind, TgMessage, TgResolvedTarget, TgTargetKind,
    TgTargetResolution, TgUser, TgUserTarget,
};
use crate::parser::{map_message, map_raw_user, map_user, public_username};

/// The signed-in account.
pub async fn who_am_i(client: &Client) -> Result<TgUser, TgError> {
    let me = client.get_me().await.map_err(TgError::from_invocation)?;
    Ok(map_user(&me))
}

/// Resolve a `@username` to a user.
pub async fn user(client: &Client, username: &str) -> Result<TgUser, TgError> {
    let name = public_username(username).map_err(TgError::Other)?;
    match client
        .resolve_username(&name)
        .await
        .map_err(TgError::from_invocation)?
    {
        Some(Peer::User(user)) => Ok(map_user(&user)),
        Some(_) => Err(TgError::Other(format!("@{name} is not a user"))),
        None => Err(TgError::Other(format!("@{name} not found"))),
    }
}

/// Resolve one already-classified locator inside the signed-in account context.
/// Nested message links and special links never fall back to username lookup.
pub async fn resolve_target(
    client: &Client,
    locator: &TgLocator,
) -> Result<TgTargetResolution, TgError> {
    match locator.kind {
        TgLocatorKind::PublicUsername => {
            let username = locator
                .username
                .as_deref()
                .ok_or_else(|| TgError::Other("Telegram public username is missing".into()))?;
            Ok(
                match client
                    .resolve_username(username)
                    .await
                    .map_err(TgError::from_invocation)?
                {
                    Some(peer) => TgTargetResolution::Resolved {
                        target: resolved_peer(&peer, locator.canonical_url.clone()),
                    },
                    None => TgTargetResolution::NotFound,
                },
            )
        }
        TgLocatorKind::InviteLink => {
            let hash = locator
                .invite_hash
                .as_ref()
                .ok_or_else(|| TgError::Other("Telegram invite hash is missing".into()))?;
            let invite = client
                .invoke(&tl::functions::messages::CheckChatInvite { hash: hash.clone() })
                .await
                .map_err(TgError::from_invocation)?;
            Ok(match invite {
                tl::enums::ChatInvite::Already(value) => TgTargetResolution::Resolved {
                    target: resolved_peer(
                        &Peer::from_raw(client, value.chat),
                        locator.canonical_url.clone(),
                    ),
                },
                tl::enums::ChatInvite::Peek(value) => TgTargetResolution::Resolved {
                    target: resolved_peer(
                        &Peer::from_raw(client, value.chat),
                        locator.canonical_url.clone(),
                    ),
                },
                tl::enums::ChatInvite::Invite(value) => TgTargetResolution::Preview {
                    kind: invite_target_kind(value.broadcast, value.megagroup),
                    title: value.title,
                    canonical_url: locator
                        .canonical_url
                        .clone()
                        .unwrap_or_else(|| locator.raw.clone()),
                },
            })
        }
        _ => static_target_resolution(locator)
            .ok_or_else(|| TgError::Other("Telegram locator requires account resolution".into())),
    }
}

/// Resolves locator kinds that cannot use an account or make provider calls.
pub fn static_target_resolution(locator: &TgLocator) -> Option<TgTargetResolution> {
    let reason = match locator.kind {
        TgLocatorKind::PublicUsername | TgLocatorKind::InviteLink => return None,
        TgLocatorKind::MessageLink => "Telegram message links require a dedicated message resolver",
        TgLocatorKind::PrivateMessageLink => {
            "private Telegram message links require joined-chat context"
        }
        TgLocatorKind::SpecialLink => "special Telegram links are not Creator targets",
        TgLocatorKind::Unknown => "Telegram locator type is unknown",
    };
    Some(TgTargetResolution::BrowserOnly {
        reason: reason.into(),
    })
}

/// Smallest page size a public community search accepts.
pub const COMMUNITY_SEARCH_MIN_COUNT: u16 = 1;
/// Largest page size a public community search accepts.
pub const COMMUNITY_SEARCH_MAX_COUNT: u16 = 100;
/// Page size of a public community search that names none.
pub const COMMUNITY_SEARCH_DEFAULT_COUNT: u16 = 20;

/// Whether `count` is inside the accepted community-search page-size range.
pub fn accepts_community_search_count(count: u16) -> bool {
    (COMMUNITY_SEARCH_MIN_COUNT..=COMMUNITY_SEARCH_MAX_COUNT).contains(&count)
}

/// Whether `query` is a community-search query: it is not blank.
pub fn accepts_community_search_query(query: &str) -> bool {
    !query.trim().is_empty()
}

/// Global public search for channels and groups (like the in-app search bar).
pub async fn search_chats(
    client: &Client,
    query: &str,
    limit: i32,
) -> Result<Vec<TgChat>, TgError> {
    if !accepts_community_search_query(query)
        || !u16::try_from(limit).is_ok_and(accepts_community_search_count)
    {
        return Err(TgError::Other(
            "invalid Telegram community search input".into(),
        ));
    }
    let request = tl::functions::contacts::Search {
        q: query.to_string(),
        limit,
        bots: false,
        broadcasts: false,
    };
    let tl::enums::contacts::Found::Found(found) = client
        .invoke(&request)
        .await
        .map_err(TgError::from_invocation)?;
    let mut out = Vec::new();
    for chat in found.chats {
        if let Some(mapped) = map_chat(chat) {
            out.push(mapped);
        }
    }
    Ok(out)
}

fn map_chat(chat: tl::enums::Chat) -> Option<TgChat> {
    match chat {
        tl::enums::Chat::Channel(channel) => Some(TgChat {
            id: channel.id.to_string(),
            title: channel.title,
            username: channel.username,
            kind: if channel.broadcast {
                "channel"
            } else if channel.megagroup {
                "supergroup"
            } else {
                "group"
            }
            .into(),
            participant_count: channel.participants_count,
            verified: channel.verified,
            scam: channel.scam || channel.fake,
        }),
        tl::enums::Chat::Chat(chat) => Some(TgChat {
            id: chat.id.to_string(),
            title: chat.title,
            username: None,
            kind: "group".into(),
            participant_count: Some(chat.participants_count),
            verified: false,
            scam: false,
        }),
        _ => None,
    }
}

/// Members of a group/supergroup (bots skipped).
pub async fn members(client: &Client, source: &str, limit: usize) -> Result<Vec<TgUser>, TgError> {
    let peer = client
        .resolve_username(&public_username(source).map_err(TgError::Other)?)
        .await
        .map_err(TgError::from_invocation)?
        .ok_or_else(|| TgError::Other(format!("{source} not found")))?;
    let peer_ref = peer
        .to_ref()
        .await
        .map_err(|error| TgError::Other(error.to_string()))?
        .ok_or_else(|| TgError::Other("cannot reference this chat".into()))?;
    let mut iter = client.iter_participants(peer_ref);
    let mut out = Vec::new();
    while let Some(participant) = iter.next().await.map_err(TgError::from_invocation)? {
        if !participant.user.is_bot() {
            out.push(map_user(&participant.user));
        }
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}

/// Admins and the creator/owner of a channel or supergroup. Telegram may require
/// the signed-in operator to be an administrator of the target community, and
/// its `CHAT_ADMIN_REQUIRED` refusal stays a Telegram RPC error.
pub async fn admins(
    client: &Client,
    target: &TgCommunityTarget,
) -> Result<crate::TgCommunityAdministrators, TgError> {
    let expected_id = target
        .external_id
        .parse::<i64>()
        .map_err(|_| TgError::Other("Telegram community external_id must be numeric".into()))?;
    let (peer_ref, canonical_peer) = if let Some(username) = target.username.as_deref() {
        let username = public_username(username).map_err(TgError::Other)?;
        let peer = client
            .resolve_username(&username)
            .await
            .map_err(TgError::from_invocation)?
            .ok_or_else(|| TgError::Other(format!("@{username} not found")))?;
        if peer.id().bare_id() != Some(expected_id) {
            return Err(TgError::Other(
                "Telegram community username no longer matches its stable id".into(),
            ));
        }
        let peer_ref = peer
            .to_ref()
            .await
            .map_err(|error| TgError::Other(error.to_string()))?
            .ok_or_else(|| TgError::Other("cannot reference this Telegram community".into()))?;
        (peer_ref, peer)
    } else {
        let peer_ref = numeric_community_peer_ref(expected_id, target.access_hash.as_deref())?;
        let peer = client
            .resolve_peer(peer_ref)
            .await
            .map_err(TgError::from_invocation)?;
        (peer_ref, peer)
    };
    if peer_ref.id.kind() != PeerKind::Channel {
        return Err(TgError::Other(
            "admins are only available for channels/supergroups".into(),
        ));
    }
    let community = canonical_admin_community_from_peer(&canonical_peer, expected_id, target.kind)?;
    administrators_for_peer(client, peer_ref, community).await
}

pub(super) async fn administrators_for_peer(
    client: &Client,
    peer_ref: PeerRef,
    community: crate::TgCommunityReference,
) -> Result<crate::TgCommunityAdministrators, TgError> {
    let request = tl::functions::channels::GetParticipants {
        channel: (&peer_ref).into(),
        filter: tl::enums::ChannelParticipantsFilter::ChannelParticipantsAdmins,
        offset: 0,
        limit: 200,
        hash: 0,
    };
    let tl::enums::channels::ChannelParticipants::Participants(result) = client
        .invoke(&request)
        .await
        .map_err(TgError::from_invocation)?
    else {
        return Err(TgError::Other(
            "Telegram did not return administrator participants".into(),
        ));
    };
    Ok(administrator_result(
        community,
        &result.participants,
        &result.users,
    ))
}

fn administrator_result(
    community: crate::TgCommunityReference,
    participants: &[tl::enums::ChannelParticipant],
    users: &[tl::enums::User],
) -> crate::TgCommunityAdministrators {
    let roles = participants
        .iter()
        .filter_map(|participant| match participant {
            tl::enums::ChannelParticipant::Creator(owner) => {
                Some((owner.user_id, crate::TgAdministratorRole::Owner))
            }
            tl::enums::ChannelParticipant::Admin(administrator) => Some((
                administrator.user_id,
                crate::TgAdministratorRole::Administrator,
            )),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut out = Vec::new();
    for user in users {
        if let tl::enums::User::User(raw) = user
            && let Some(role) = roles.get(&raw.id).copied()
        {
            out.push(crate::TgCommunityAdministrator {
                role,
                user: map_raw_user(raw),
            });
        }
    }
    crate::TgCommunityAdministrators {
        community,
        administrators: out,
    }
}

pub(super) fn canonical_admin_community_from_peer(
    peer: &Peer,
    expected_id: i64,
    expected_kind: crate::TgCommunityKind,
) -> Result<crate::TgCommunityReference, TgError> {
    let actual_kind = match peer {
        Peer::Channel(_) => crate::TgCommunityKind::Channel,
        Peer::Group(group) if group.id().kind() == PeerKind::Channel => {
            crate::TgCommunityKind::Supergroup
        }
        _ => {
            return Err(TgError::Other(
                "Telegram administrator target is not a Channel or Supergroup".into(),
            ));
        }
    };
    canonical_admin_community(
        peer.id()
            .bare_id()
            .ok_or_else(|| TgError::Other("Telegram canonical community id is invalid".into()))?,
        peer.name().unwrap_or_default(),
        peer.username(),
        actual_kind,
        expected_id,
        expected_kind,
    )
}

fn canonical_admin_community(
    actual_id: i64,
    title: &str,
    username: Option<&str>,
    actual_kind: crate::TgCommunityKind,
    expected_id: i64,
    expected_kind: crate::TgCommunityKind,
) -> Result<crate::TgCommunityReference, TgError> {
    if actual_id != expected_id {
        return Err(TgError::Other(
            "Telegram canonical community id does not match the requested stable id".into(),
        ));
    }
    if actual_kind != expected_kind {
        return Err(TgError::Other(
            "Telegram canonical community kind does not match the requested target".into(),
        ));
    }
    let title = title.trim();
    if title.is_empty() {
        return Err(TgError::Other(
            "Telegram canonical community title is empty".into(),
        ));
    }
    Ok(crate::TgCommunityReference {
        external_id: actual_id.to_string(),
        title: title.to_owned(),
        username: username
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        kind: actual_kind,
    })
}

fn numeric_community_peer_ref(
    external_id: i64,
    access_hash: Option<&str>,
) -> Result<PeerRef, TgError> {
    let access_hash = access_hash
        .ok_or_else(|| {
            TgError::Other(
                "Telegram community has no public username or reusable peer reference".into(),
            )
        })?
        .parse::<i64>()
        .map_err(|_| TgError::Other("Telegram community access_hash is invalid".into()))?;
    Ok(PeerRef {
        id: PeerId::channel(external_id)
            .ok_or_else(|| TgError::Other("Telegram community id is invalid".into()))?,
        auth: PeerAuth::from_hash(access_hash),
    })
}

/// Read the most recent messages of the conversation with a `@username`
/// (newest first), the same shape the X direct-message read returns. No
/// inbox listing on purpose.
pub async fn thread(
    client: &Client,
    username: &str,
    count: usize,
) -> Result<Vec<TgMessage>, TgError> {
    let name = public_username(username).map_err(TgError::Other)?;
    let peer = client
        .resolve_username(&name)
        .await
        .map_err(TgError::from_invocation)?
        .ok_or_else(|| TgError::Other(format!("@{name} not found")))?;
    let peer_ref = peer
        .to_ref()
        .await
        .map_err(|error| TgError::Other(error.to_string()))?
        .ok_or_else(|| TgError::Other("cannot reference this peer".into()))?;
    let mut iter = client.iter_messages(peer_ref).limit(count);
    let mut out = Vec::new();
    while let Some(message) = iter.next().await.map_err(TgError::from_invocation)? {
        out.push(map_message(&message));
    }
    Ok(out)
}

/// Send to a previously resolved User target. The endpoint re-resolves the
/// username and verifies both peer kind and stable ID before the network write.
pub async fn send_dm(
    client: &Client,
    target: &TgUserTarget,
    text: &str,
) -> Result<String, TgError> {
    let peer_ref = resolved_user_ref(client, target).await?;
    let message = client
        .send_message(peer_ref, text)
        .await
        .map_err(TgError::from_invocation)?;
    Ok(message.id().to_string())
}

/// Reads a human-user thread after one alias resolution whose immutable ID
/// is verified before using that same peer reference for the history request.
pub async fn user_thread(
    client: &Client,
    target: &TgUserTarget,
    count: usize,
) -> Result<Vec<TgMessage>, TgError> {
    let peer_ref = resolved_user_ref(client, target).await?;
    let mut iter = client.iter_messages(peer_ref).limit(count);
    let mut messages = Vec::new();
    while let Some(message) = iter.next().await.map_err(TgError::from_invocation)? {
        messages.push(map_message(&message));
    }
    Ok(messages)
}

async fn resolved_user_ref(client: &Client, target: &TgUserTarget) -> Result<PeerRef, TgError> {
    let name = public_username(&target.username).map_err(TgError::Other)?;
    let user = match client
        .resolve_username(&name)
        .await
        .map_err(TgError::from_invocation)?
    {
        Some(Peer::User(user)) if !user.is_bot() => user,
        Some(Peer::User(_)) => {
            return Err(TgError::Other(format!(
                "@{name} is a bot and cannot be used as a Creator DM target"
            )));
        }
        Some(_) => return Err(TgError::Other(format!("@{name} is not a user"))),
        None => return Err(TgError::Other(format!("@{name} not found"))),
    };
    let actual_id = user.id().bare_id_unchecked().to_string();
    if actual_id != target.external_id {
        return Err(TgError::Other(format!(
            "@{name} now resolves to another Telegram user"
        )));
    }
    user.to_ref()
        .await
        .map_err(|error| match error.downcast::<InvocationError>() {
            Ok(error) => TgError::from_invocation(*error),
            // Grammers also reads the local session cache here. Its boxed
            // storage errors carry no proven RPC/auth evidence or safe text.
            Err(_) => TgError::Other("Telegram peer reference could not be recovered".into()),
        })?
        .ok_or_else(|| TgError::Other("cannot reference this user".into()))
}

fn resolved_peer(peer: &Peer, fallback_url: Option<String>) -> TgResolvedTarget {
    let kind = match peer {
        Peer::User(user) if user.is_bot() => TgTargetKind::Bot,
        Peer::User(_) => TgTargetKind::User,
        Peer::Channel(_) => TgTargetKind::Channel,
        Peer::Group(group) if group.is_megagroup() => TgTargetKind::Supergroup,
        Peer::Group(_) => TgTargetKind::Group,
    };
    let username = peer.username().map(str::to_owned);
    TgResolvedTarget {
        kind,
        external_id: peer.id().bare_id_unchecked().to_string(),
        username: username.clone(),
        display_name: peer.name().map(str::to_owned),
        canonical_url: username
            .map(|username| format!("https://t.me/{username}"))
            .or(fallback_url),
    }
}

fn invite_target_kind(broadcast: bool, megagroup: bool) -> TgTargetKind {
    if broadcast {
        TgTargetKind::Channel
    } else if megagroup {
        TgTargetKind::Supergroup
    } else {
        TgTargetKind::Group
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_client::sender::RpcError;

    fn rpc_error(code: i32, name: &str) -> InvocationError {
        InvocationError::Rpc(RpcError {
            code,
            name: name.into(),
            value: None,
            caused_by: None,
        })
    }

    #[test]
    fn community_search_accepts_a_nonblank_query_and_page_sizes_from_1_through_100() {
        assert_eq!(
            (
                COMMUNITY_SEARCH_MIN_COUNT,
                COMMUNITY_SEARCH_MAX_COUNT,
                COMMUNITY_SEARCH_DEFAULT_COUNT
            ),
            (1, 100, 20)
        );
        for (count, accepted) in [(0, false), (1, true), (20, true), (100, true), (101, false)] {
            assert_eq!(accepts_community_search_count(count), accepted, "{count}");
        }
        assert!(accepts_community_search_count(
            COMMUNITY_SEARCH_DEFAULT_COUNT
        ));
        for (query, accepted) in [
            ("rust", true),
            (" rust ", true),
            ("", false),
            (" \t\n", false),
        ] {
            assert_eq!(accepts_community_search_query(query), accepted, "{query:?}");
        }
    }

    #[test]
    fn invite_flags_map_to_explicit_target_kinds() {
        assert_eq!(invite_target_kind(true, false), TgTargetKind::Channel);
        assert_eq!(invite_target_kind(false, true), TgTargetKind::Supergroup);
        assert_eq!(invite_target_kind(false, false), TgTargetKind::Group);
    }

    #[test]
    fn numeric_community_requires_lossless_peer_authorization() {
        let peer = numeric_community_peer_ref(1_688_680_661, Some("-9223372036854775000"))
            .expect("typed peer ref");
        assert_eq!(peer.id.bare_id(), Some(1_688_680_661));
        assert_eq!(peer.auth.hash(), -9_223_372_036_854_775_000);
        assert!(numeric_community_peer_ref(1_688_680_661, None).is_err());
    }

    #[test]
    fn administrator_participant_refusals_keep_telegram_rpc_code_and_name() {
        // `administrators_for_peer` reports its participants request failure as
        // this invocation error, so an operator who does not administer the
        // community keeps Telegram's own `CHAT_ADMIN_REQUIRED` answer.
        for (code, name) in [
            (400, "CHAT_ADMIN_REQUIRED"),
            (403, "CHAT_ADMIN_REQUIRED"),
            (400, "CHANNEL_INVALID"),
        ] {
            assert!(
                matches!(
                    TgError::from_invocation(rpc_error(code, name)),
                    TgError::Rpc { code: actual, name: Some(actual_name) }
                        if actual == code && actual_name == name
                ),
                "{code} {name}"
            );
        }
    }

    #[test]
    fn canonical_admin_community_uses_provider_metadata_and_checks_identity() {
        let community = canonical_admin_community(
            42,
            " Provider title ",
            Some("provider_handle"),
            crate::TgCommunityKind::Supergroup,
            42,
            crate::TgCommunityKind::Supergroup,
        )
        .expect("canonical community");
        assert_eq!(community.external_id, "42");
        assert_eq!(community.title, "Provider title");
        assert_eq!(community.username.as_deref(), Some("provider_handle"));
        assert_eq!(community.kind, crate::TgCommunityKind::Supergroup);

        assert!(
            canonical_admin_community(
                42,
                "Provider title",
                None,
                crate::TgCommunityKind::Channel,
                42,
                crate::TgCommunityKind::Supergroup,
            )
            .is_err()
        );
    }

    #[test]
    fn administrator_result_does_not_require_redundant_response_chats() {
        let community = crate::TgCommunityReference {
            external_id: "42".into(),
            title: "Provider title".into(),
            username: Some("provider_handle".into()),
            kind: crate::TgCommunityKind::Supergroup,
        };

        // channels.channelParticipants.chats only contains chats mentioned by
        // the response. A valid administrator response may therefore omit the
        // requested community entirely.
        let result = administrator_result(community.clone(), &[], &[]);

        assert_eq!(result.community, community);
        assert!(result.administrators.is_empty());
    }
}
