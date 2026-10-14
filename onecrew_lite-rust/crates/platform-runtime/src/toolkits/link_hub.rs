//! Typed application entry for one allowlisted public link-hub page.
//!
//! The private public-web capability owns HTTP bounds and page parsing. Links
//! and emails retain their source layer; contact classification belongs to
//! the Creator save workflow.

use crate::capabilities::public_web;

// These types form the toolkit's public result and failure contract, used by
// the MCP operation. The implementation tree remains private.
pub use public_web::{
    LinkHubEmail, LinkHubExtractError, LinkHubExtraction, LinkHubLayer, LinkHubLink,
};

/// Fetch one supported public link-hub page and extract what it publishes.
///
/// # Errors
///
/// Returns [`LinkHubExtractError`] for refused URLs or redirects, failed
/// requests, and exceeded timeout, redirect or response-body bounds.
pub async fn extract(url: &str) -> Result<LinkHubExtraction, LinkHubExtractError> {
    public_web::extract(url).await
}
