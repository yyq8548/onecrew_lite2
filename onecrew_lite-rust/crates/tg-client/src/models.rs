use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TgLocatorKind {
    PublicUsername,
    InviteLink,
    MessageLink,
    PrivateMessageLink,
    SpecialLink,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TgLocator {
    pub kind: TgLocatorKind,
    pub raw: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TgTargetKind {
    User,
    Bot,
    Channel,
    Supergroup,
    Group,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TgCommunityKind {
    Channel,
    Supergroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TgCommunityReference {
    pub external_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub kind: TgCommunityKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TgResolvedTarget {
    pub kind: TgTargetKind,
    pub external_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TgTargetResolution {
    Resolved {
        target: TgResolvedTarget,
    },
    Preview {
        kind: TgTargetKind,
        title: String,
        canonical_url: String,
    },
    BrowserOnly {
        reason: String,
    },
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TgUserTarget {
    pub external_id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TgUser {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    pub is_bot: bool,
    pub verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TgAdministratorRole {
    Owner,
    Administrator,
}

#[derive(Debug, Clone, Serialize)]
pub struct TgCommunityAdministrator {
    pub role: TgAdministratorRole,
    pub user: TgUser,
}

#[derive(Debug, Clone, Serialize)]
pub struct TgCommunityAdministrators {
    pub community: TgCommunityReference,
    pub administrators: Vec<TgCommunityAdministrator>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TgMessage {
    pub id: String,
    /// True if this message was sent by the logged-in account.
    pub outgoing: bool,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    /// Unix timestamp (seconds).
    pub date: i64,
}

/// Public search metadata without access hashes, phone numbers or session data.
#[derive(Debug, Clone, Serialize)]
pub struct TgChat {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// "channel" (broadcast), "supergroup" (megagroup), or "group".
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub participant_count: Option<i32>,
    pub verified: bool,
    pub scam: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TgCommunityTarget {
    pub external_id: String,
    pub kind: TgCommunityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_hash: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::TgChat;

    #[test]
    fn public_search_chats_preserve_lossless_ids_and_only_public_metadata() {
        for kind in ["channel", "supergroup", "group"] {
            let chat = TgChat {
                id: "9007199254740993".into(),
                title: "Public Rust Community".into(),
                username: Some("rust_public".into()),
                kind: kind.into(),
                participant_count: Some(2500),
                verified: true,
                scam: false,
            };
            let public = serde_json::to_value(&chat).unwrap();
            assert_eq!(public["id"], "9007199254740993");
            assert_eq!(public["kind"], kind);
            assert_eq!(public["participant_count"], 2500);
            assert_eq!(
                public
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                [
                    "id",
                    "kind",
                    "participant_count",
                    "scam",
                    "title",
                    "username",
                    "verified"
                ],
            );
            for private in ["access_hash", "phone", "session", "api_hash"] {
                assert!(public.get(private).is_none(), "{private}");
                assert!(!format!("{chat:?}").contains(private), "{private}");
            }
        }
    }
}
