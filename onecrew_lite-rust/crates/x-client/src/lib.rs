#![deny(unreachable_pub)]

//! Typed X operations and provider-response normalization.
//!
//! The client is generic over the X transport boundary and owns operation
//! payloads, query resolution, parsing, and normalized provider models. It
//! does not own account persistence, Creator policy, or application UI state.

mod advanced_search;
mod client;
mod graphql;
mod models;
mod parser;

pub use advanced_search::{
    ADVANCED_SEARCH_DEFAULT_COUNT, ADVANCED_SEARCH_MAX_COUNT, ADVANCED_SEARCH_MIN_COUNT,
    AdvancedSearchCriteria, AdvancedSearchFilter, AdvancedSearchPage, AdvancedSearchResultMode,
    accepts_advanced_search_count,
};
pub use client::*;
pub use graphql::default_features;
pub use models::*;
pub use parser::{parse_bookmark_folders, parse_timeline, parse_tweet, parse_user, parse_users};
