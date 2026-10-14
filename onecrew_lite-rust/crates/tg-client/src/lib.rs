#![deny(unreachable_pub)]

//! Telegram read/write operations over a grammers client, with typed models.

mod client;
mod community;
mod models;
mod parser;

pub use client::{
    COMMUNITY_SEARCH_DEFAULT_COUNT, COMMUNITY_SEARCH_MAX_COUNT, COMMUNITY_SEARCH_MIN_COUNT,
    accepts_community_search_count, accepts_community_search_query, admins, members,
    resolve_target, search_chats, send_dm, static_target_resolution, thread, user, user_thread,
    who_am_i,
};
pub use community::{TgCommunityLocator, TgCommunityReadError, community, community_admins};
pub use models::{
    TgAdministratorRole, TgChat, TgCommunityAdministrator, TgCommunityAdministrators,
    TgCommunityKind, TgCommunityReference, TgCommunityTarget, TgLocator, TgLocatorKind, TgMessage,
    TgResolvedTarget, TgTargetKind, TgTargetResolution, TgUser, TgUserTarget,
};
pub use parser::{parse_locator, public_username};
