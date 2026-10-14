//! Application-facing Instagram facade.
//!
//! The facade binds one restored Instagram session to typed client operations.
//! Account persistence and workflow policy remain in their owning runtimes.

mod account;
mod direct_messages;
mod error;
mod locator;

pub use account::{IgAccount, IgRelationshipAction, accepts_stable_user_id};
pub use direct_messages::{IgAccountSource, IgDirectMessages, evidence};
pub use error::IgWriteFailure;
pub use locator::parse_profile_handle;
pub use onecrew_ig_client::{
    IgMedia, IgMessage, IgRelationshipStatus, IgThread, IgUser, USER_SEARCH_DEFAULT_COUNT,
    USER_SEARCH_MAX_COUNT, USER_SEARCH_MIN_COUNT, accepts_user_search_count,
    accepts_user_search_query,
};
pub use onecrew_ig_transport::{IgError, IgGeo, IgSession};
