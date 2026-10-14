//! Public-web capabilities independent of accounts, Creator data and routing.

mod fetch;
mod parse;

pub use fetch::LinkHubExtractError;
pub use parse::{LinkHubEmail, LinkHubExtraction, LinkHubLayer, LinkHubLink};

pub(crate) async fn extract(url: &str) -> Result<LinkHubExtraction, LinkHubExtractError> {
    let page = fetch::fetch(url).await?;
    let body = std::str::from_utf8(&page.body).map_err(|_| LinkHubExtractError::InvalidUtf8)?;
    Ok(parse::parse_link_hub(
        body,
        &page.initial_url,
        &page.final_url,
    ))
}
