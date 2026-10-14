//! Application-facing X facade.
//!
//! The facade binds one restored X session to the typed Web and encrypted-chat
//! clients. It exposes provider capabilities without owning profile storage,
//! Creator workflow policy, or adapter wire contracts.

mod account;
mod direct_messages;
mod error;
mod locator;

pub use account::{XAccount, XRelationshipStatus, XReplyReceipt};
pub use direct_messages::{XAccountSource, XChatPasscode, XDirectMessages};
pub use error::{XError, XWriteFailure};
pub use locator::parse_profile_handle;

pub use onecrew_x_chat::{
    XChatConversation, XChatDmCapability, XChatDmCapabilityProbe, XChatError, XChatMessage,
    XChatParticipant, XChatSendResult,
};
pub use onecrew_x_client::{
    ADVANCED_SEARCH_DEFAULT_COUNT, ADVANCED_SEARCH_MAX_COUNT, ADVANCED_SEARCH_MIN_COUNT,
    AdvancedSearchCriteria, AdvancedSearchFilter, AdvancedSearchPage, AdvancedSearchResultMode,
    Article, Author, BookmarkFolder, BookmarkFolderPage, Media, Metrics, POST_THREAD_DEFAULT_COUNT,
    POST_THREAD_MAX_COUNT, POST_THREAD_MIN_COUNT, RelationshipAction, TimelineKind, TimelinePage,
    Tweet, TweetAction, User, UserListKind, UserPage, XClientError, accepts_advanced_search_count,
    accepts_post_thread_count,
};
pub use onecrew_x_transport::{TransportError, XSession};
