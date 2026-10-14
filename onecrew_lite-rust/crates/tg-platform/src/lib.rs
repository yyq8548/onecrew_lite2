//! Application-facing Telegram facade.

mod account;
mod community;
mod direct_messages;

pub use account::{
    CodeStep, PasswordStep, QrStart, QrStep, TgAccount, TgLogin, TgPassword, TgQrLogin,
};
pub use community::{
    TgPublicAdministrator, TgPublicAdministrators, TgPublicUser, community_read_evidence,
};
pub use direct_messages::{TgAccountSource, TgDirectMessages, evidence};
pub use onecrew_tg_client::{
    COMMUNITY_SEARCH_DEFAULT_COUNT, COMMUNITY_SEARCH_MAX_COUNT, COMMUNITY_SEARCH_MIN_COUNT,
    TgAdministratorRole, TgChat, TgCommunityAdministrator, TgCommunityAdministrators,
    TgCommunityKind, TgCommunityReadError, TgCommunityReference, TgCommunityTarget, TgLocator,
    TgLocatorKind, TgMessage, TgResolvedTarget, TgTargetKind, TgTargetResolution, TgUser,
    TgUserTarget, accepts_community_search_count, accepts_community_search_query, parse_locator,
    public_username, static_target_resolution,
};
pub use onecrew_tg_client::{TgCommunityLocator, TgCommunityReference as TgPublicCommunity};
pub use onecrew_tg_transport::TgError;
pub use onecrew_tg_transport::{LoginCodeDelivery, LoginCodeMethod, QrLoginChallenge};
