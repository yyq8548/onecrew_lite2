use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

macro_rules! string_enum {
    ($name:ident { #[default] $default_variant:ident => $default_value:literal, $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(
            Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
            Deserialize, JsonSchema,
        )]
        pub enum $name {
            #[default]
            #[serde(rename = $default_value)]
            $default_variant,
            $(#[serde(rename = $value)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    Self::$default_variant => $default_value,
                    $(Self::$variant => $value),+
                }
            }

            pub fn parse(value: &str) -> Result<Self, String> {
                match value {
                    $default_value => Ok(Self::$default_variant),
                    $($value => Ok(Self::$variant)),+,
                    _ => Err(format!("unknown {}: {value}", stringify!($name))),
                }
            }
        }
    };
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
            JsonSchema,
        )]
        pub enum $name {
            $(#[serde(rename = $value)] $variant),+
        }

        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            pub fn parse(value: &str) -> Result<Self, String> {
                match value {
                    $($value => Ok(Self::$variant)),+,
                    _ => Err(format!("unknown {}: {value}", stringify!($name))),
                }
            }
        }
    };
}

string_enum!(Platform {
    X => "x",
    Instagram => "instagram",
    Telegram => "telegram",
    YouTube => "youtube",
    TikTok => "tiktok",
});

string_enum!(CreatorStage {
    Prospect => "prospect",
    Engaged => "engaged",
    Collaborating => "collaborating",
    Archived => "archived",
});

string_enum!(TargetKind {
    User => "user",
    Bot => "bot",
    Channel => "channel",
    Supergroup => "supergroup",
    Group => "group",
    Creator => "creator",
    Business => "business",
});

string_enum!(CommunityTargetKind {
    Channel => "channel",
    Supergroup => "supergroup",
});

impl From<CommunityTargetKind> for TargetKind {
    fn from(value: CommunityTargetKind) -> Self {
        match value {
            CommunityTargetKind::Channel => Self::Channel,
            CommunityTargetKind::Supergroup => Self::Supergroup,
        }
    }
}

impl TryFrom<TargetKind> for CommunityTargetKind {
    type Error = String;

    fn try_from(value: TargetKind) -> Result<Self, Self::Error> {
        match value {
            TargetKind::Channel => Ok(Self::Channel),
            TargetKind::Supergroup => Ok(Self::Supergroup),
            _ => Err("community target must be a Telegram Channel or Supergroup".into()),
        }
    }
}

string_enum!(CommunityAdministratorRole {
    Owner => "owner",
    Administrator => "administrator",
});

string_enum!(ContactChannel {
    Email => "email",
    Wechat => "wechat",
    X => "x",
    Instagram => "instagram",
    Telegram => "telegram",
    YouTube => "youtube",
    TikTok => "tiktok",
    Website => "website",
    LinkHub => "link_hub",
});

string_enum!(LocatorKind {
    EmailAddress => "email_address",
    Handle => "handle",
    Url => "url",
    InviteLink => "invite_link",
    ChannelId => "channel_id",
});

#[cfg(test)]
mod string_enum_tests {
    use super::super::{
        AccountRefId, CommunityAdministratorSource, CommunityReference, CreatorCandidateOrigin,
        CreatorContactRouteId, CreatorId, CreatorOrigin, CreatorOriginId, DiscoveryId,
        ObservationSourceId, ResolvedTargetId,
    };
    use super::{CommunityAdministratorRole, CommunityTargetKind, ContactChannel, Platform};

    #[test]
    fn serde_uses_the_declared_canonical_wire_value() {
        assert_eq!(serde_json::to_value(Platform::YouTube).unwrap(), "youtube");
        assert_eq!(
            serde_json::from_value::<Platform>(serde_json::json!("youtube")).unwrap(),
            Platform::YouTube
        );
        assert_eq!(
            serde_json::to_value(ContactChannel::YouTube).unwrap(),
            "youtube"
        );
        assert_eq!(
            serde_json::to_value(ContactChannel::LinkHub).unwrap(),
            "link_hub"
        );
    }

    #[test]
    fn creator_origins_use_strict_tagged_wire_shapes() {
        let search = CreatorCandidateOrigin::PlatformSearch {
            discovery_id: DiscoveryId::new("discovery-search"),
            account_ref_id: AccountRefId::new("account-x"),
            query: "crypto".into(),
            search_kind: "people".into(),
        };
        assert_eq!(
            serde_json::to_value(search).unwrap(),
            serde_json::json!({
                "kind": "platform_search",
                "discovery_id": "discovery-search",
                "account_ref_id": "account-x",
                "query": "crypto",
                "search_kind": "people"
            })
        );
        let search_origin = CreatorOrigin::PlatformSearch {
            id: CreatorOriginId::new("origin-search"),
            discovery_id: DiscoveryId::new("discovery-search"),
            account_ref_id: AccountRefId::new("account-x"),
            creator_id: CreatorId::new("creator-search"),
            source_id: ObservationSourceId::new("source-search"),
            discovered_at: 41,
        };
        assert_eq!(
            serde_json::to_value(search_origin).unwrap(),
            serde_json::json!({
                "kind": "platform_search",
                "id": "origin-search",
                "discovery_id": "discovery-search",
                "account_ref_id": "account-x",
                "creator_id": "creator-search",
                "source_id": "source-search",
                "discovered_at": 41
            })
        );
        assert_eq!(
            serde_json::to_value(CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: AccountRefId::new("account-tg"),
            })
            .unwrap(),
            serde_json::json!({
                "kind": "operator_account",
                "operator_account_ref_id": "account-tg"
            })
        );

        let administrator = CreatorOrigin::CommunityAdministrator {
            id: CreatorOriginId::new("origin-admin"),
            discovery_id: DiscoveryId::new("discovery-admin"),
            creator_id: CreatorId::new("creator-admin"),
            source_id: ObservationSourceId::new("source-admin"),
            role: CommunityAdministratorRole::Owner,
            community: CommunityReference {
                external_id: "community-42".into(),
                title: "Crypto Owners".into(),
                handle: Some("crypto_owners".into()),
                target_kind: CommunityTargetKind::Supergroup,
            },
            source: CommunityAdministratorSource::CreatorContactRoute {
                source_creator_id: CreatorId::new("creator-community"),
                source_contact_route_id: Some(CreatorContactRouteId::new("route-tg")),
                operator_account_ref_id: AccountRefId::new("account-tg"),
                source_resolved_target_id: ResolvedTargetId::new("target-community"),
            },
            discovered_at: 42,
        };
        assert_eq!(
            serde_json::to_value(administrator).unwrap(),
            serde_json::json!({
                "kind": "community_administrator",
                "id": "origin-admin",
                "discovery_id": "discovery-admin",
                "creator_id": "creator-admin",
                "source_id": "source-admin",
                "role": "owner",
                "community": {
                    "external_id": "community-42",
                    "title": "Crypto Owners",
                    "handle": "crypto_owners",
                    "target_kind": "supergroup"
                },
                "source": {
                    "kind": "creator_contact_route",
                    "source_creator_id": "creator-community",
                    "source_contact_route_id": "route-tg",
                    "operator_account_ref_id": "account-tg",
                    "source_resolved_target_id": "target-community"
                },
                "discovered_at": 42
            })
        );
        assert_eq!(
            serde_json::to_value(CommunityAdministratorSource::CreatorContactRoute {
                source_creator_id: CreatorId::new("creator-community"),
                source_contact_route_id: None,
                operator_account_ref_id: AccountRefId::new("account-tg"),
                source_resolved_target_id: ResolvedTargetId::new("target-community"),
            })
            .unwrap(),
            serde_json::json!({
                "kind": "creator_contact_route",
                "source_creator_id": "creator-community",
                "source_contact_route_id": null,
                "operator_account_ref_id": "account-tg",
                "source_resolved_target_id": "target-community"
            })
        );

        let manual = CreatorOrigin::ManualEntry {
            id: CreatorOriginId::new("origin-manual"),
            creator_id: CreatorId::new("creator-manual"),
            source_id: ObservationSourceId::new("source-manual"),
            created_at: 43,
        };
        assert_eq!(
            serde_json::to_value(manual).unwrap(),
            serde_json::json!({
                "kind": "manual_entry",
                "id": "origin-manual",
                "creator_id": "creator-manual",
                "source_id": "source-manual",
                "created_at": 43
            })
        );

        assert!(
            serde_json::from_value::<CreatorCandidateOrigin>(serde_json::json!({
                "kind": "community_administrator",
                "discovery_id": "discovery-admin",
                "role": "owner",
                "community": {
                    "external_id": "community-42",
                    "title": "Crypto Owners",
                    "handle": null,
                    "target_kind": "channel",
                    "unexpected": true
                },
                "source": {
                    "kind": "operator_account",
                    "operator_account_ref_id": "account-tg"
                }
            }))
            .is_err()
        );
    }
}

string_enum!(SourceKind {
    InitialPlatformAccount => "initial_platform_account",
    ProfileBio => "profile_bio",
    ProfileWebsite => "profile_website",
    ProfileLocation => "profile_location",
    VideoDescription => "video_description",
    ChannelDescription => "channel_description",
    LinkHub => "link_hub",
    ManualBusinessEmail => "manual_business_email",
    ManualEntry => "manual_entry",
    CommunityAdministrator => "community_administrator",
    ContactRouteResolution => "contact_route_resolution",
    CreatorImport => "creator_import",
});

string_enum!(LinkState {
    Claimed => "claimed",
    Confirmed => "confirmed",
    Rejected => "rejected",
});

string_enum!(Confidence {
    Exact => "exact",
    Asserted => "asserted",
    Possible => "possible",
});

string_enum!(ResolutionState {
    Active => "active",
    Invalidated => "invalidated",
});

string_enum!(TargetResolutionStatus {
    Resolved => "resolved",
    RequiresOperatorAuth => "requires_operator_auth",
    BrowserOnly => "browser_only",
    Unsupported => "unsupported",
    NotFound => "not_found",
    Conflict => "conflict",
});

string_enum!(ContactAction {
    OpenProfile => "open_profile",
    ReadPublic => "read_public",
    Follow => "follow",
    DirectMessage => "direct_message",
    OpenCommunity => "open_community",
    ExtractAdministrators => "extract_administrators",
    SendEmail => "send_email",
});

string_enum!(EmailCapability {
    Send => "send",
    SentReadback => "sent_readback",
    Inbox => "inbox",
    Reply => "reply",
});

string_enum!(EmailDirection {
    Inbound => "inbound",
    Outbound => "outbound",
});

string_enum!(EmailFolder {
    #[default]
    All => "all",
    Inbox => "inbox",
    Sent => "sent",
    Drafts => "drafts",
    Archive => "archive",
    Deleted => "deleted",
});

string_enum!(EmailThreadFolder {
    Inbox => "inbox",
    Sent => "sent",
    Archive => "archive",
    Deleted => "deleted",
});

string_enum!(EmailThreadDispositionAction {
    Archive => "archive",
    Unarchive => "unarchive",
    Restore => "restore",
    Delete => "delete",
});

string_enum!(EmailDraftState {
    Draft => "draft",
    Trashed => "trashed",
});

string_enum!(EmailRecipientKind {
    From => "from",
    To => "to",
    Cc => "cc",
    Bcc => "bcc",
    ReceivedFor => "received_for",
    ReplyTo => "reply_to",
});

string_enum!(ConflictState {
    Open => "open",
    Resolved => "resolved",
    Dismissed => "dismissed",
});

string_enum!(RelationshipKind {
    Following => "following",
    FollowedBy => "followed_by",
    Blocked => "blocked",
    CanMessage => "can_message",
});

string_enum!(RelationshipValue {
    Yes => "yes",
    No => "no",
    Unknown => "unknown",
});

string_enum!(DirectMessageCapabilityState {
    Allowed => "allowed",
    Unavailable => "unavailable",
    Unknown => "unknown",
});

string_enum!(DirectMessageCapabilityScope {
    ExistingConversation => "existing_conversation",
    NewMessageRequest => "new_message_request",
    Unknown => "unknown",
});

string_enum!(DirectMessageCapabilityReasonCode {
    TargetDmBlocking => "target_dm_blocking",
    PlatformDmNotAllowed => "platform_dm_not_allowed",
    OperatorPremiumRequired => "operator_premium_required",
    OperatorChatPasscodeMissing => "operator_chat_passcode_missing",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectMessageCapability {
    pub state: DirectMessageCapabilityState,
    pub scope: DirectMessageCapabilityScope,
    pub reason_code: Option<DirectMessageCapabilityReasonCode>,
    pub detail: Option<String>,
}

#[cfg(test)]
mod direct_message_capability_tests {
    use super::{
        DirectMessageCapability, DirectMessageCapabilityReasonCode, DirectMessageCapabilityScope,
        DirectMessageCapabilityState,
    };

    #[test]
    fn capability_has_a_strict_stable_wire_shape() {
        let capability = DirectMessageCapability {
            state: DirectMessageCapabilityState::Unavailable,
            scope: DirectMessageCapabilityScope::NewMessageRequest,
            reason_code: Some(DirectMessageCapabilityReasonCode::OperatorPremiumRequired),
            detail: None,
        };
        assert_eq!(
            serde_json::to_value(capability).unwrap(),
            serde_json::json!({
                "state": "unavailable",
                "scope": "new_message_request",
                "reason_code": "operator_premium_required",
                "detail": null
            })
        );
        assert!(
            serde_json::from_value::<DirectMessageCapability>(serde_json::json!({
                "state": "blocked",
                "scope": "new_message_request",
                "reason_code": null,
                "detail": null
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<DirectMessageCapability>(serde_json::json!({
                "state": "unknown",
                "scope": "unknown",
                "reason_code": null,
                "detail": null,
                "guessed_reason": "not_following"
            }))
            .is_err()
        );
    }
}

string_enum!(RelationshipMutation {
    Follow => "follow",
    Unfollow => "unfollow",
});

string_enum!(ActionFailureKind {
    RemoteWriteUnverified => "remote_write_unverified",
    RemoteWriteRejected => "remote_write_rejected",
    LocalCommitFailed => "local_commit_failed",
});
