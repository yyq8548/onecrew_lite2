//! Source-bound public-page evidence; reading a page does not qualify a candidate.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CreatorRepositoryCallError, CreatorRuntime};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryPublicPageInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub profile_ref: String,
    pub candidate_id: String,
    pub requested_url: String,
}

/// Read a saved receipt in its Campaign scope; never dispatch a provider call.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryPublicPageReadInput {
    pub campaign_id: String,
    pub candidate_id: String,
    pub requested_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryPublicPageClaim {
    pub input: DiscoveryPublicPageInput,
    pub search_hit_id: String,
    pub source_url: String,
    pub call_number: u32,
}

#[derive(Debug, Clone)]
pub struct FinishDiscoveryPublicPageInput {
    pub hidden_email_available: bool,
    pub claim: DiscoveryPublicPageClaim,
    pub document_url: Option<String>,
    pub title: String,
    pub markdown_excerpt: String,
    pub truncated: bool,
    pub stop_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryPublicPageRead {
    pub candidate_id: String,
    pub requested_url: String,
    pub search_hit_id: String,
    pub source_url: String,
    pub source_excerpt: String,
    pub profile_ref: String,
    pub platform_account_id: String,
    pub call_number: u32,
    pub status: String,
    pub document_url: Option<String>,
    pub title: String,
    pub markdown_excerpt: String,
    pub truncated: bool,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}

/// Exact observed token, not a substring of a longer or visibly shortened URL.
pub(crate) fn description_observes_link(description: &str, link: &str) -> bool {
    description.len() <= 16_000
        && description.split_whitespace().any(|token| {
            !token.contains("...")
                && !token.contains('…')
                && token.trim_matches(|c: char| {
                    matches!(
                        c,
                        '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.' | ';' | '!' | '"' | '\''
                    )
                }) == link
        })
}

/// A community invitation is not a tool-page candidate. Other URLs remain
/// unverified candidates, not certified tools; provider URL safety is separate.
pub(crate) fn is_community_invite_link(link: &str) -> bool {
    url::Url::parse(link).is_ok_and(|url| {
        matches!(url.host_str(), Some("discord.gg" | "www.discord.gg"))
            || (matches!(
                url.host_str(),
                Some("discord.com" | "www.discord.com" | "discordapp.com" | "www.discordapp.com")
            ) && (url.path() == "/invite" || url.path().starts_with("/invite/")))
    })
}

/// Product/affiliate destinations do not supply the creator's contact details.
/// Unknown shorteners remain eligible; no destination is guessed or fetched.
pub(crate) fn is_product_link(link: &str) -> bool {
    url::Url::parse(link).is_ok_and(|url| {
        let host = url.host_str().unwrap_or_default();
        matches!(host, "amzn.to" | "geni.us" | "a.co" | "amzn.eu")
            || host == "amazon.com"
            || host.ends_with(".amazon.com")
            || host == "amazon.co.uk"
            || host.ends_with(".amazon.co.uk")
            || (host.starts_with("shop.")
                && url.path().split('/').any(|part| part == "collections"))
            || url
                .path()
                .split('/')
                .any(|part| matches!(part, "dp" | "products"))
    })
}

/// Contact navigation observed on the linked website, never guessed paths.
/// Limit expansion to one same-origin hop; provider URL safety still applies.
pub(crate) fn contact_page_links(source: &str, markdown: &str) -> Vec<String> {
    if markdown.len() > 20_000 {
        return Vec::new();
    }
    let Ok(base) = url::Url::parse(source) else {
        return Vec::new();
    };
    let pattern = regex::Regex::new(r"https://[^\s<>\[\]()+]+|\]\((/[^\s()]+)\)")
        .expect("constant contact link regex");
    let mut links = Vec::new();
    for capture in pattern.captures_iter(markdown) {
        let raw = capture.get(1).or_else(|| capture.get(0)).unwrap().as_str();
        if raw.contains("...") || raw.contains('…') {
            continue;
        }
        let token = raw.trim_end_matches([',', '.', ';', '!', '\"', '\'']);
        let Ok(link) = base.join(token) else {
            continue;
        };
        let path = link.path().to_ascii_lowercase();
        if link.origin() != base.origin()
            || link.scheme() != "https"
            || !link.username().is_empty()
            || link.password().is_some()
            || link.port().is_some()
            || link.as_str().len() > 2048
            || token.contains("...")
            || token.contains('…')
            || !path.split('/').any(|part| {
                matches!(
                    part,
                    "contact"
                        | "contact-us"
                        | "contacts"
                        | "business"
                        | "cooperation"
                        | "collaboration"
                        | "partnerships"
                        | "kontakt"
                        | "contacto"
                        | "contato"
                        | "contatti"
                        | "kontaktieren"
                        | "contacter"
                )
            })
        {
            continue;
        }
        if !links.iter().any(|saved| saved == link.as_str()) {
            links.push(link.into());
        }
    }
    links
}

pub(crate) fn validate_page_result(input: &FinishDiscoveryPublicPageInput) -> Result<(), String> {
    if input.hidden_email_available
        && (input.stop_code.is_some()
            || input.document_url.as_deref() != Some(input.claim.input.requested_url.as_str())
            || !input
                .claim
                .input
                .requested_url
                .starts_with("https://www.youtube.com/channel/UC"))
    {
        return Err("Hidden email entry requires a successful native channel receipt".into());
    }
    if input.markdown_excerpt.len() > 20_000
        || input.title.chars().count() > 500
        || input.markdown_excerpt.contains('\0')
    {
        return Err("Public page result exceeds bounded evidence limits".into());
    }
    if let Some(code) = input.stop_code.as_deref() {
        if !matches!(
            code,
            "opencli_timeout"
                | "opencli_output_too_large"
                | "opencli_version_changed"
                | "opencli_daemon_unavailable"
                | "opencli_extension_unavailable"
                | "opencli_profile_unavailable"
                | "opencli_command_unavailable"
                | "opencli_invalid_output"
                | "opencli_invalid_input"
                | "opencli_unavailable"
                | "opencli_command_failed"
                | "opencli_risk_control"
                | "opencli_auth_required"
        ) || input.document_url.is_some()
            || !input.title.is_empty()
            || !input.markdown_excerpt.is_empty()
            || input.truncated
        {
            return Err("Failed public page reads cannot contain successful evidence".into());
        }
    } else if input.title.trim().is_empty()
        || input.markdown_excerpt.trim().is_empty()
        || !input.document_url.as_deref().is_some_and(|value| {
            value.len() <= 2048
                && url::Url::parse(value).is_ok_and(|url| {
                    url.scheme() == "https"
                        && url.host_str().is_some()
                        && url.username().is_empty()
                        && url.password().is_none()
                        && url.port().is_none()
                })
        })
    {
        return Err("Successful public page reads require attributed content".into());
    }
    Ok(())
}

impl CreatorRuntime {
    pub fn claim_discovery_public_page(
        &self,
        input: DiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageClaim, String> {
        self.repository.claim_discovery_public_page(input)
    }

    pub async fn claim_discovery_public_page_async(
        &self,
        input: DiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.claim_discovery_public_page(input))
            .await
    }

    pub fn finish_discovery_public_page(
        &self,
        input: FinishDiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageRead, String> {
        validate_page_result(&input)?;
        self.repository.finish_discovery_public_page(input)
    }

    /// Settle a provider response without leaving its reservation busy when the
    /// returned document fails evidence validation. Authorization is rechecked
    /// by the same owner; stale or changed claims still fail closed.
    pub fn settle_discovery_public_page(
        &self,
        input: FinishDiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageRead, String> {
        let claim = input.claim.clone();
        match self.finish_discovery_public_page(input) {
            Ok(read) => Ok(read),
            Err(original) => self
                .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
                    hidden_email_available: false,
                    claim,
                    document_url: None,
                    title: String::new(),
                    markdown_excerpt: String::new(),
                    truncated: false,
                    stop_code: Some("opencli_invalid_output".into()),
                })
                .map_err(|_| original),
        }
    }

    pub async fn finish_discovery_public_page_async(
        &self,
        input: FinishDiscoveryPublicPageInput,
    ) -> Result<DiscoveryPublicPageRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.settle_discovery_public_page(input))
            .await
    }

    pub fn get_discovery_public_page(
        &self,
        campaign: &str,
        candidate: &str,
        url: &str,
    ) -> Result<DiscoveryPublicPageRead, String> {
        self.repository
            .get_discovery_public_page(campaign, candidate, url)
    }

    pub async fn get_discovery_public_page_async(
        &self,
        campaign: String,
        candidate: String,
        url: String,
    ) -> Result<DiscoveryPublicPageRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || {
            owner.get_discovery_public_page(&campaign, &candidate, &url)
        })
        .await
    }
}
