//! Reviewer judgments are source-bound assertions, never caller-supplied native facts.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{
    DiscoveryIdentityMergeMaterial, DiscoveryIdentityMergeMaterialInput, prepare_pair, validate,
};
use crate::{
    CreatorRepositoryCallError, CreatorRuntime, DiscoveryReviewBasis, DiscoveryReviewCitation,
    DiscoveryReviewMaterial, DiscoveryReviewer,
};

pub(super) const POLICY: &str = "reciprocal-native-account-review-v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIdentityDecision {
    SameAuthor,
    DifferentAuthors,
    Insufficient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIdentityReviewKind {
    SameAuthor,
    DifferentAuthors,
    Insufficient,
    Revoke,
}

impl DiscoveryIdentityReviewKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SameAuthor => "same_author",
            Self::DifferentAuthors => "different_authors",
            Self::Insufficient => "insufficient",
            Self::Revoke => "revoke",
        }
    }
}

/// Self-reported reviewer interpretation of a quoted relationship, not a native fact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIdentityRelationship {
    SelfOwnedAccount,
    OtherPerson,
    Unclear,
}

/// Only saved text owned by this exact native author can support account ownership.
/// External target pages, search names, product URLs and sponsor URLs are excluded.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscoveryIdentityQuote {
    Description { quote: String },
    Transcript { segment_index: u32, quote: String },
    NativeContent { quote: String },
    NativeProfileBio { quote: String },
}

impl DiscoveryIdentityQuote {
    fn text(&self) -> &str {
        match self {
            Self::Description { quote }
            | Self::Transcript { quote, .. }
            | Self::NativeContent { quote }
            | Self::NativeProfileBio { quote } => quote,
        }
    }
    fn present_in(&self, account: &DiscoveryReviewMaterial) -> bool {
        match self {
            Self::Description { quote } => {
                account.contains_citation(DiscoveryReviewCitation::Description { quote })
            }
            Self::Transcript {
                segment_index,
                quote,
            } => account.contains_citation(DiscoveryReviewCitation::Transcript {
                segment_index: *segment_index as usize,
                quote,
            }),
            Self::NativeContent { quote } => {
                account.contains_citation(DiscoveryReviewCitation::NativeContent { quote })
            }
            Self::NativeProfileBio { quote } => {
                text(quote, 4000)
                    && account
                        .native_profile
                        .as_ref()
                        .and_then(|p| p.bio.as_ref())
                        .is_some_and(|bio| bio.contains(quote))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryIdentityFinding {
    /// One of the pair's saved hit IDs; the target is always the other bound account.
    pub source_search_hit_id: String,
    pub relationship: DiscoveryIdentityRelationship,
    pub citation: DiscoveryIdentityQuote,
}

/// Same-author needs reciprocal, author-owned account references and explicit reviewer
/// ownership judgments. A matching URL alone is not an ownership determination.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitDiscoveryIdentityReviewInput {
    pub pair: DiscoveryIdentityMergeMaterialInput,
    pub material_hash: String,
    pub decision_id: String,
    pub reviewer: DiscoveryReviewer,
    pub decision: DiscoveryIdentityDecision,
    pub rationale: String,
    pub findings: Vec<DiscoveryIdentityFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIdentityReviewReceipt {
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub decision_id: String,
    pub operator_account_id: String,
    pub policy_version: String,
    pub kind: DiscoveryIdentityReviewKind,
    pub revokes_decision_id: Option<String>,
    pub candidate_ids: [String; 2],
    pub native_author_ids: [String; 2],
    pub platforms: [String; 2],
    pub material_hash: String,
    pub created_at: i64,
}

pub struct StoredDiscoveryIdentityReview {
    pub(crate) request_hash: String,
    pub(crate) receipt_json: String,
}

pub enum DiscoveryIdentitySubmissionBasis {
    Committed(StoredDiscoveryIdentityReview),
    Material(Box<[DiscoveryReviewBasis; 2]>),
}

/// Opaque owner preparation; no caller can inject source snapshots or account facts.
pub struct PreparedDiscoveryIdentityReview {
    pub(crate) basis: [DiscoveryReviewBasis; 2],
    pub(crate) request_hash: String,
    pub(crate) input_json: String,
    pub(crate) material_json: String,
    pub(crate) receipt_json: String,
    pub(crate) receipt: DiscoveryIdentityReviewReceipt,
}

pub(super) fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max
        && !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}

pub(super) fn encoded<T: Serialize>(value: &T, max: usize) -> Result<String, String> {
    let json = serde_json::to_string(value).map_err(|_| "identity_review_encoding_invalid")?;
    if json.len() > max {
        return Err("identity_review_too_large".into());
    }
    Ok(json)
}

pub(super) fn request_hash(
    account: &str,
    input: &SubmitDiscoveryIdentityReviewInput,
) -> Result<String, String> {
    validate(&input.pair)?;
    if !text(account, 256)
        || account.chars().any(char::is_control)
        || !text(&input.decision_id, 128)
        || input.decision_id.chars().any(char::is_control)
        || input.material_hash.len() != 64
        || !input.material_hash.bytes().all(|b| b.is_ascii_hexdigit())
        || !text(&input.reviewer.label, 256)
        || !text(&input.rationale, 4000)
        || input.findings.len() > 8
    {
        return Err("identity_review_input_invalid".into());
    }
    encoded(input, 65536)?;
    crate::discovery_library::digest(&(account, POLICY, input))
}

fn native_profile_url(account: &DiscoveryReviewMaterial) -> Result<Url, String> {
    let raw = if account.author_platform() == "youtube" {
        format!(
            "https://www.youtube.com/channel/{}",
            account.native_author_id
        )
    } else {
        account
            .native_profile
            .as_ref()
            .ok_or("identity_review_target_profile_unavailable")?
            .profile_url
            .clone()
    };
    let url = Url::parse(&raw).map_err(|_| "identity_review_target_profile_invalid")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("identity_review_target_profile_invalid".into());
    }
    Ok(url)
}

fn references_profile(quote: &str, expected: &Url) -> bool {
    // Exact URL tokens, not substring prefixes (e.g. /UC123-impersonator).
    quote.split_whitespace().any(|token| {
        let token = token.trim_matches(['(', ')', '[', ']', '<', '>', '"', '\'', ',', '.', ';']);
        Url::parse(token).is_ok_and(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.host_str() == expected.host_str()
                && url.path().trim_end_matches('/') == expected.path().trim_end_matches('/')
        })
    })
}

pub(super) fn validate_findings(
    input: &SubmitDiscoveryIdentityReviewInput,
    pair: &DiscoveryIdentityMergeMaterial,
) -> Result<(), String> {
    let mut own = [false; 2];
    let mut other = false;
    for finding in &input.findings {
        let index = pair
            .accounts
            .iter()
            .position(|a| a.hit.id == finding.source_search_hit_id)
            .ok_or("identity_review_quote_source_invalid")?;
        if !finding.citation.present_in(&pair.accounts[index]) {
            return Err("identity_review_quote_not_found".into());
        }
        if !references_profile(
            finding.citation.text(),
            &native_profile_url(&pair.accounts[1 - index])?,
        ) {
            return Err("identity_review_quote_target_mismatch".into());
        }
        match finding.relationship {
            DiscoveryIdentityRelationship::SelfOwnedAccount => own[index] = true,
            DiscoveryIdentityRelationship::OtherPerson => other = true,
            DiscoveryIdentityRelationship::Unclear => {}
        }
    }
    match input.decision {
        DiscoveryIdentityDecision::SameAuthor
            if own != [true, true]
                || other
                || input
                    .findings
                    .iter()
                    .any(|f| f.relationship == DiscoveryIdentityRelationship::Unclear) =>
        {
            Err("identity_review_reciprocal_ownership_required".into())
        }
        DiscoveryIdentityDecision::DifferentAuthors if !other || own.contains(&true) => {
            Err("identity_review_separation_evidence_required".into())
        }
        _ => Ok(()),
    }
}

fn prepare(
    account: String,
    input: SubmitDiscoveryIdentityReviewInput,
    hash: String,
    basis: [DiscoveryReviewBasis; 2],
) -> Result<PreparedDiscoveryIdentityReview, String> {
    let pair = prepare_pair(
        &input.pair,
        crate::discovery_review::build_material(basis[0].clone())?,
        crate::discovery_review::build_material(basis[1].clone())?,
    )?;
    if pair.material_hash != input.material_hash {
        return Err("identity_review_material_changed".into());
    }
    validate_findings(&input, &pair)?;
    let receipt = DiscoveryIdentityReviewReceipt {
        campaign_id: input.pair.campaign_id.clone(),
        criteria_revision: input.pair.expected_revision,
        decision_id: input.decision_id.clone(),
        operator_account_id: account,
        policy_version: POLICY.into(),
        kind: match input.decision {
            DiscoveryIdentityDecision::SameAuthor => DiscoveryIdentityReviewKind::SameAuthor,
            DiscoveryIdentityDecision::DifferentAuthors => {
                DiscoveryIdentityReviewKind::DifferentAuthors
            }
            DiscoveryIdentityDecision::Insufficient => DiscoveryIdentityReviewKind::Insufficient,
        },
        revokes_decision_id: None,
        candidate_ids: pair.accounts.each_ref().map(|a| a.candidate_id.clone()),
        native_author_ids: pair.accounts.each_ref().map(|a| a.native_author_id.clone()),
        platforms: pair
            .accounts
            .each_ref()
            .map(|a| a.author_platform().to_owned()),
        material_hash: pair.material_hash.clone(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "identity_review_clock_invalid")?
            .as_secs()
            .try_into()
            .map_err(|_| "identity_review_clock_invalid")?,
    };
    Ok(PreparedDiscoveryIdentityReview {
        basis,
        request_hash: hash,
        input_json: encoded(&input, 65536)?,
        material_json: encoded(&pair, 3145728)?,
        receipt_json: encoded(&receipt, 65536)?,
        receipt,
    })
}

pub(super) fn decode(
    stored: StoredDiscoveryIdentityReview,
    account: &str,
    hash: Option<&str>,
) -> Result<DiscoveryIdentityReviewReceipt, String> {
    if hash.is_some_and(|h| h != stored.request_hash) {
        return Err("identity_review_idempotency_conflict".into());
    }
    let receipt: DiscoveryIdentityReviewReceipt = serde_json::from_str(&stored.receipt_json)
        .map_err(|_| "identity_review_receipt_invalid")?;
    if receipt.operator_account_id != account {
        return Err("identity_review_account_mismatch".into());
    }
    Ok(receipt)
}

impl CreatorRuntime {
    /// Trusted authenticated account is a composition argument, never accepted in the DTO.
    pub fn submit_discovery_identity_review(
        &self,
        account: &str,
        input: SubmitDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, String> {
        let hash = request_hash(account, &input)?;
        let basis = self
            .repository
            .read_discovery_identity_submission(&input.pair, &input.decision_id)?;
        let basis = match basis {
            DiscoveryIdentitySubmissionBasis::Committed(stored) => {
                return decode(stored, account, Some(&hash));
            }
            DiscoveryIdentitySubmissionBasis::Material(basis) => *basis,
        };
        let prepared = prepare(account.into(), input, hash.clone(), basis)?;
        decode(
            self.repository
                .commit_discovery_identity_review(&prepared)?,
            account,
            Some(&hash),
        )
    }

    pub async fn submit_discovery_identity_review_async(
        &self,
        account: String,
        input: SubmitDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, CreatorRepositoryCallError<String>> {
        let (account, input, hash) = self
            .repository_cpu_segment(move || {
                let hash = request_hash(&account, &input)?;
                Ok((account, input, hash))
            })
            .await?;
        let repository = self.repository.clone();
        let (account, input, hash, basis) = self
            .repository_read_segment(move || {
                let basis = repository
                    .read_discovery_identity_submission(&input.pair, &input.decision_id)?;
                Ok((account, input, hash, basis))
            })
            .await?;
        let basis = match basis {
            DiscoveryIdentitySubmissionBasis::Committed(stored) => {
                return self
                    .repository_cpu_segment(move || decode(stored, &account, Some(&hash)))
                    .await;
            }
            DiscoveryIdentitySubmissionBasis::Material(basis) => *basis,
        };
        let trusted_account = account.clone();
        let prepared = self
            .repository_cpu_segment(move || prepare(account, input, hash, basis))
            .await?;
        let repository = self.repository.clone();
        let stored = self
            .repository_write_segment(move || {
                repository.commit_discovery_identity_review(&prepared)
            })
            .await?;
        self.repository_cpu_segment(move || decode(stored, &trusted_account, None))
            .await
    }

    /// Readback of an uncertain write; never retries a decision or spends provider budget.
    pub fn get_discovery_identity_review(
        &self,
        account: &str,
        campaign: &str,
        decision: &str,
    ) -> Result<Option<DiscoveryIdentityReviewReceipt>, String> {
        if !text(account, 256) || !text(campaign, 128) || !text(decision, 128) {
            return Err("identity_review_input_invalid".into());
        }
        self.repository
            .find_discovery_identity_review(campaign, decision)?
            .map(|stored| decode(stored, account, None))
            .transpose()
    }

    pub async fn get_discovery_identity_review_async(
        &self,
        account: String,
        campaign: String,
        decision: String,
    ) -> Result<Option<DiscoveryIdentityReviewReceipt>, CreatorRepositoryCallError<String>> {
        if !text(&account, 256) || !text(&campaign, 128) || !text(&decision, 128) {
            return Err(CreatorRepositoryCallError::Owner(
                "identity_review_input_invalid".into(),
            ));
        }
        let repository = self.repository.clone();
        let stored = self
            .repository_read_segment(move || {
                repository.find_discovery_identity_review(&campaign, &decision)
            })
            .await?;
        self.repository_cpu_segment(move || {
            stored.map(|row| decode(row, &account, None)).transpose()
        })
        .await
    }
}

#[cfg(test)]
pub(super) mod tests;
