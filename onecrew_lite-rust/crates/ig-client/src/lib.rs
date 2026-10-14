//! Typed Instagram operations and response normalization.
//!
//! The client consumes the Instagram transport and exposes provider models
//! without taking ownership of session persistence, Creator policy, or UI state.

mod client;
mod models;
mod parser;

pub use client::{
    IgClient, USER_SEARCH_DEFAULT_COUNT, USER_SEARCH_MAX_COUNT, USER_SEARCH_MIN_COUNT,
    accepts_user_search_count, accepts_user_search_query,
};
pub use models::{
    IgComment, IgCommentPage, IgMedia, IgMediaPage, IgMessage, IgRelationshipStatus, IgThread,
    IgUser, IgUserPage,
};
