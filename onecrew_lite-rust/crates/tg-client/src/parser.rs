use grammers_client::message::Message;
use grammers_client::peer::User;
use grammers_client::tl;
use url::Url;

use crate::models::{TgLocator, TgLocatorKind, TgMessage, TgUser};

pub(crate) fn map_user(user: &User) -> TgUser {
    TgUser {
        id: user.id().to_string(),
        username: user.username().map(str::to_string),
        first_name: user.first_name().map(str::to_string),
        last_name: user.last_name().map(str::to_string),
        phone: user.phone().map(str::to_string),
        is_bot: user.is_bot(),
        verified: user.verified(),
    }
}

/// Map a raw TL user (as returned inside `channels.getParticipants`) to a model.
pub(crate) fn map_raw_user(user: &tl::types::User) -> TgUser {
    TgUser {
        id: user.id.to_string(),
        username: user.username.clone(),
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        phone: user.phone.clone(),
        is_bot: user.bot,
        verified: user.verified,
    }
}

pub(crate) fn map_message(message: &Message) -> TgMessage {
    TgMessage {
        id: message.id().to_string(),
        outgoing: message.outgoing(),
        text: message.text().to_string(),
        sender_id: message.sender_id().map(|id| id.to_string()),
        date: message.date().timestamp(),
    }
}

pub fn parse_locator(input: &str) -> TgLocator {
    let raw = input.trim().to_owned();
    if raw.is_empty() {
        return unknown(raw);
    }

    if let Some(username) = raw.strip_prefix('@').filter(|value| is_username(value)) {
        return public_username_locator(raw.clone(), username);
    }
    if !raw.contains(['/', ':', '?', '#']) && is_username(&raw) {
        return public_username_locator(raw.clone(), &raw);
    }
    if raw.to_ascii_lowercase().starts_with("tg:") {
        return TgLocator {
            kind: TgLocatorKind::SpecialLink,
            raw,
            username: None,
            invite_hash: None,
            canonical_url: None,
        };
    }

    let candidate = if raw.contains("://") {
        raw.clone()
    } else {
        format!("https://{raw}")
    };
    let Ok(url) = Url::parse(&candidate) else {
        return unknown(raw);
    };
    let Some(host) = url.host_str().map(|host| host.to_ascii_lowercase()) else {
        return unknown(raw);
    };
    if !matches!(
        host.as_str(),
        "t.me" | "www.t.me" | "telegram.me" | "www.telegram.me"
    ) {
        return unknown(raw);
    }
    let parts = url
        .path_segments()
        .map(|parts| parts.filter(|part| !part.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    if parts.is_empty() {
        return unknown(raw);
    }
    let canonical_url = Some(format!("https://t.me/{}", parts.join("/")));
    let first = parts[0];
    if let Some(hash) = first.strip_prefix('+').filter(|hash| !hash.is_empty()) {
        return invite_locator(raw, hash, canonical_url);
    }
    if first.eq_ignore_ascii_case("joinchat") {
        return parts
            .get(1)
            .filter(|hash| !hash.is_empty())
            .map(|hash| invite_locator(raw.clone(), hash, canonical_url.clone()))
            .unwrap_or_else(|| unknown(raw));
    }
    if first.eq_ignore_ascii_case("c") {
        return TgLocator {
            kind: TgLocatorKind::PrivateMessageLink,
            raw,
            username: None,
            invite_hash: None,
            canonical_url,
        };
    }
    if matches!(
        first.to_ascii_lowercase().as_str(),
        "share" | "proxy" | "socks" | "login" | "iv" | "addstickers" | "addemoji" | "setlanguage"
    ) || url
        .query_pairs()
        .any(|(key, _)| matches!(key.as_ref(), "start" | "startgroup" | "game" | "appname"))
    {
        return TgLocator {
            kind: TgLocatorKind::SpecialLink,
            raw,
            username: is_username(first).then(|| first.to_owned()),
            invite_hash: None,
            canonical_url,
        };
    }
    if first.eq_ignore_ascii_case("s") || parts.len() > 1 {
        let username = if first.eq_ignore_ascii_case("s") {
            parts.get(1).filter(|value| is_username(value)).copied()
        } else if is_username(first) {
            Some(first)
        } else {
            None
        };
        return TgLocator {
            kind: TgLocatorKind::MessageLink,
            raw,
            username: username.map(str::to_owned),
            invite_hash: None,
            canonical_url,
        };
    }
    if is_username(first) {
        return public_username_locator(raw, first);
    }
    unknown(raw)
}

pub fn public_username(input: &str) -> Result<String, String> {
    let locator = parse_locator(input);
    if locator.kind != TgLocatorKind::PublicUsername {
        return Err("Telegram locator is not a public username".into());
    }
    locator
        .username
        .ok_or_else(|| "Telegram public username is missing".into())
}

fn public_username_locator(raw: String, username: &str) -> TgLocator {
    TgLocator {
        kind: TgLocatorKind::PublicUsername,
        raw,
        username: Some(username.to_owned()),
        invite_hash: None,
        canonical_url: Some(format!("https://t.me/{username}")),
    }
}

fn invite_locator(raw: String, hash: &str, canonical_url: Option<String>) -> TgLocator {
    TgLocator {
        kind: TgLocatorKind::InviteLink,
        raw,
        username: None,
        invite_hash: Some(hash.to_owned()),
        canonical_url,
    }
}

fn unknown(raw: String) -> TgLocator {
    TgLocator {
        kind: TgLocatorKind::Unknown,
        raw,
        username: None,
        invite_hash: None,
        canonical_url: None,
    }
}

fn is_username(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .any(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_public_usernames_without_treating_nested_links_as_users() {
        for input in [
            "@creator",
            "creator",
            "https://t.me/creator",
            "telegram.me/creator",
        ] {
            let locator = parse_locator(input);
            assert_eq!(locator.kind, TgLocatorKind::PublicUsername, "{input}");
            assert_eq!(locator.username.as_deref(), Some("creator"));
        }
        assert_eq!(
            parse_locator("https://t.me/creator/123").kind,
            TgLocatorKind::MessageLink
        );
        assert_eq!(
            parse_locator("https://t.me/c/123/456").kind,
            TgLocatorKind::PrivateMessageLink
        );
    }

    #[test]
    fn numeric_peer_ids_are_never_treated_as_public_usernames() {
        assert_eq!(parse_locator("1688680661").kind, TgLocatorKind::Unknown);
        assert!(public_username("1688680661").is_err());
    }

    #[test]
    fn classifies_invites_and_special_links_without_public_username_fallback() {
        let invite = parse_locator("https://t.me/+invite_hash");
        assert_eq!(invite.kind, TgLocatorKind::InviteLink);
        assert_eq!(invite.invite_hash.as_deref(), Some("invite_hash"));
        assert_eq!(
            parse_locator("https://t.me/joinchat/legacy_hash").kind,
            TgLocatorKind::InviteLink
        );
        assert_eq!(
            parse_locator("https://t.me/share/url?url=https://example.com").kind,
            TgLocatorKind::SpecialLink
        );
        assert_eq!(
            parse_locator("https://t.me/some_bot?start=payload").kind,
            TgLocatorKind::SpecialLink
        );
        assert_eq!(
            parse_locator("https://example.com/creator").kind,
            TgLocatorKind::Unknown
        );
    }
}
