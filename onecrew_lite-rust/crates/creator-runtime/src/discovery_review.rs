//! Source-bound review material and audited judgments. Quotation integrity is not qualification.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    CreatorRepositoryCallError, CreatorRuntime, DiscoveryContentRead, DiscoveryPlatformScope,
    DiscoveryPublicPageRead, DiscoverySearchHit, DiscoveryTranscriptRead,
};

pub(crate) const MAX_REVIEW_BYTES: usize = 1_048_576;
pub(crate) const MAX_REVIEW_PAGES: usize = 20;

pub(crate) mod initial_match;
mod submission;
pub use initial_match::INITIAL_MATCH_REVIEWER;
pub use submission::{
    DiscoveryMatchLevel, DiscoveryReviewCriterion, DiscoveryReviewFinding, DiscoveryReviewJudgment,
    DiscoveryReviewOutcome, DiscoveryReviewQuote, DiscoveryReviewReceipt,
    DiscoveryReviewSubmissionBasis, DiscoveryReviewedTool, DiscoveryReviewer,
    DiscoveryReviewerKind, GetDiscoveryReviewInput, PreparedDiscoveryReview, StoredDiscoveryReview,
    SubmitDiscoveryReviewInput,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryReviewMaterialInput {
    #[schemars(length(min = 1, max = 128))]
    pub campaign_id: String,
    #[schemars(range(min = 1))]
    pub expected_revision: u32,
    #[schemars(length(min = 1, max = 128))]
    pub search_hit_id: String,
}

/// Opaque repository snapshot, never accepted as caller-supplied evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryReviewBasis {
    pub(crate) campaign_id: String,
    pub(crate) revision: u32,
    pub(crate) criteria_json: String,
    pub(crate) scopes_json: String,
    pub(crate) approved_at: i64,
    pub(crate) hit: DiscoverySearchHit,
    pub(crate) candidate_id: String,
    pub(crate) native_author_id: String,
    pub(crate) native_followers: Option<u64>,
    pub(crate) native_follower_estimate: Option<crate::discovery_douyin::DiscoveryFollowerEstimate>,
    pub(crate) native_profile: Option<DiscoveryNativeProfileObservation>,
    pub(crate) evidence_json: String,
    pub(crate) transcript: Option<(DiscoveryTranscriptRead, String)>,
    pub(crate) source_description: Option<String>,
    pub(crate) native_content: Option<DiscoveryContentRead>,
    pub(crate) language_samples: Vec<crate::DiscoveryLanguageSample>,
    pub(crate) pages: Vec<DiscoveryPublicPageRead>,
}

/// A saved native profile observation. Self-declared text is context, not residence proof.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
pub struct DiscoveryNativeProfileObservation {
    /// Visible homepage previews, never verified original post captions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_text: Option<String>,
    pub profile_url: String,
    pub observed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_declared_location: Option<String>,
}

/// A saved evidence snapshot, not a verdict. Source text must be treated as data.
#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq)]
pub struct DiscoveryReviewMaterial {
    pub format_version: u32,
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub criteria: Value,
    pub approved_scopes: Vec<DiscoveryPlatformScope>,
    pub approved_at: i64,
    pub hit: DiscoverySearchHit,
    pub candidate_id: String,
    pub native_author_id: String,
    /// Present only when a search-engine source established a distinct native platform.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_followers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_follower_estimate: Option<crate::discovery_douyin::DiscoveryFollowerEstimate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_profile: Option<DiscoveryNativeProfileObservation>,
    /// Changes to any saved candidate evidence invalidate a previously reviewed basis.
    pub candidate_evidence_hash: String,
    /// Only present when this exact hit is the saved metadata's source.
    pub video_description: Option<String>,
    pub transcript: Option<DiscoveryTranscriptRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_content: Option<DiscoveryContentRead>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub language_samples: Vec<crate::DiscoveryLanguageSample>,
    pub public_pages: Vec<DiscoveryPublicPageRead>,
    pub material_hash: String,
}

/// Exact local text citation. A match proves only that the saved text contains it.
pub enum DiscoveryReviewCitation<'a> {
    Description {
        quote: &'a str,
    },
    SearchCaption {
        quote: &'a str,
    },
    Transcript {
        segment_index: usize,
        quote: &'a str,
    },
    NativeProfile {
        profile_url: &'a str,
        quote: &'a str,
    },
    NativeContent {
        quote: &'a str,
    },
    PublicPage {
        requested_url: &'a str,
        quote: &'a str,
    },
}

impl DiscoveryReviewMaterial {
    pub(crate) fn author_platform(&self) -> &str {
        self.native_platform
            .as_deref()
            .unwrap_or(&self.hit.platform)
    }
    pub(crate) fn matched_language_source(&self) -> String {
        if matches!(self.hit.platform.as_str(), "google" | "yahoo")
            && self.native_platform.as_deref() == Some("youtube")
            && let Some(id) = crate::discovery_web_sources::video_id(&self.hit.source_url)
        {
            return format!("https://www.youtube.com/watch?v={id}");
        }
        // X search hits may use /i/status; the bound native post response
        // supplies the actual handle URL used by the comments reader.
        if self.hit.platform == "twitter"
            && let Some(source) = self
                .native_content
                .as_ref()
                .filter(|read| read.status == "completed" && read.stop_code.is_none())
                .and_then(|read| read.native_content.as_ref())
                .and_then(|native| native.pointer("/identity/source_url"))
                .and_then(Value::as_str)
        {
            return source.to_owned();
        }
        self.native_content
            .as_ref()
            .and_then(|read| read.observed_source_url())
            .unwrap_or(self.hit.source_url.as_str())
            .to_owned()
    }
    pub fn contains_citation(&self, citation: DiscoveryReviewCitation<'_>) -> bool {
        if let DiscoveryReviewCitation::NativeContent { quote } = &citation
            && !quote.trim().is_empty()
            && quote.len() <= 4000
            && self.language_samples.iter().any(|sample| {
                sample.author_id == self.native_author_id
                    && (sample.text.contains(quote)
                        || sample
                            .comments
                            .iter()
                            .any(|comment| comment.text.contains(quote)))
            })
        {
            return true;
        }
        let (quote, text) = match citation {
            DiscoveryReviewCitation::Description { quote } => {
                (quote, self.video_description.as_deref())
            }
            DiscoveryReviewCitation::SearchCaption { quote } => (
                quote,
                (self.hit.platform == "tiktok").then_some(self.hit.title.as_str()),
            ),
            DiscoveryReviewCitation::Transcript {
                segment_index,
                quote,
            } => (
                quote,
                self.transcript
                    .as_ref()
                    .filter(|r| r.status == "completed" && r.stop_code.is_none())
                    .and_then(|r| r.segments.get(segment_index))
                    .map(|s| s.text.as_str()),
            ),
            DiscoveryReviewCitation::NativeProfile { profile_url, quote } => (
                quote,
                self.native_profile
                    .as_ref()
                    .filter(|profile| {
                        profile.profile_url == profile_url
                            && self.hit.author_id.as_deref() == Some(self.native_author_id.as_str())
                            && self.hit.author_url.as_deref() == Some(profile_url)
                    })
                    .and_then(|profile| {
                        profile
                            .bio
                            .as_deref()
                            .filter(|text| text.contains(quote))
                            .or(profile.web_text.as_deref())
                    }),
            ),
            DiscoveryReviewCitation::NativeContent { quote } => (
                quote,
                self.native_content
                    .as_ref()
                    .filter(|r| r.status == "completed" && r.stop_code.is_none())
                    .and_then(|r| r.native_content.as_ref())
                    .and_then(|value| {
                        value.get(if self.hit.platform == "bilibili" {
                            "description"
                        } else {
                            "text"
                        })
                    })
                    .and_then(Value::as_str),
            ),
            DiscoveryReviewCitation::PublicPage {
                requested_url,
                quote,
            } => (
                quote,
                self.public_pages
                    .iter()
                    .find(|r| {
                        r.requested_url == requested_url
                            && r.status == "completed"
                            && r.stop_code.is_none()
                    })
                    .map(|r| r.markdown_excerpt.as_str()),
            ),
        };
        !quote.trim().is_empty()
            && quote.len() <= 4_000
            && text.is_some_and(|text| text.contains(quote))
    }
}

pub(crate) fn build_material(
    basis: DiscoveryReviewBasis,
) -> Result<DiscoveryReviewMaterial, String> {
    let scopes: Vec<DiscoveryPlatformScope> =
        serde_json::from_str(&basis.scopes_json).map_err(|e| e.to_string())?;
    if !scopes.iter().any(|s| {
        s.platform == basis.hit.platform
            && s.profile_ref == basis.hit.profile_ref
            && s.keywords.contains(&basis.hit.search_keyword)
    }) {
        return Err("review_source_scope_mismatch".into());
    }
    let evidence: Value = serde_json::from_str(&basis.evidence_json).map_err(|e| e.to_string())?;
    let video_description = basis.source_description.or_else(|| {
        (basis.hit.platform == "youtube"
            && evidence["source_urls"][0].as_str() == Some(&basis.hit.source_url))
        .then(|| evidence["video_description"].as_str().map(str::to_owned))
        .flatten()
    });
    let transcript = basis
        .transcript
        .map(|(mut read, json)| {
            read.segments = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            Ok::<_, String>(read)
        })
        .transpose()?;
    let native_platform = if matches!(basis.hit.platform.as_str(), "google" | "yahoo") {
        Some(
            evidence
                .pointer("/web_origin/target_platform")
                .and_then(Value::as_str)
                .filter(|p| matches!(*p, "youtube" | "twitter" | "douyin"))
                .ok_or("review_native_platform_unavailable")?
                .to_owned(),
        )
    } else {
        None
    };
    let mut material = DiscoveryReviewMaterial {
        format_version: match basis.hit.platform.as_str() {
            "google" | "yahoo" => 2,
            "twitter" | "tiktok" | "bilibili" | "reddit" | "facebook" | "instagram" | "weibo"
            | "bluesky" | "zhihu" | "hackernews" | "pinterest" | "pixiv" | "linkedin"
            | "douyin" => 2,
            _ => 1,
        },
        campaign_id: basis.campaign_id,
        criteria_revision: basis.revision,
        criteria: serde_json::from_str(&basis.criteria_json).map_err(|e| e.to_string())?,
        approved_scopes: scopes,
        approved_at: basis.approved_at,
        hit: basis.hit,
        candidate_id: basis.candidate_id,
        native_author_id: basis.native_author_id,
        native_platform,
        native_followers: basis.native_followers,
        native_follower_estimate: basis.native_follower_estimate,
        native_profile: basis.native_profile,
        candidate_evidence_hash: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&evidence).map_err(|e| e.to_string())?)
        ),
        video_description,
        transcript,
        native_content: basis.native_content,
        language_samples: basis.language_samples,
        public_pages: basis.pages,
        material_hash: String::new(),
    };
    let matched_source = material.matched_language_source().to_owned();
    material.language_samples.retain(|sample| {
        sample.author_id == material.native_author_id && sample.source_url == matched_source
    });
    material.language_samples.truncate(1);
    for sample in &mut material.language_samples {
        sample.comments.truncate(5);
    }
    let bytes = serde_json::to_vec(&material).map_err(|e| e.to_string())?;
    if bytes.len().saturating_add(64) > MAX_REVIEW_BYTES {
        return Err("review_material_too_large".into());
    }
    material.material_hash = format!("{:x}", Sha256::digest(bytes));
    Ok(material)
}

impl CreatorRuntime {
    /// Reads saved evidence only. Does not invoke a provider, reserve a version or change a verdict.
    pub fn prepare_discovery_review_material(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryReviewMaterial, String> {
        build_material(
            self.repository
                .read_discovery_review_basis(campaign, revision, hit)?,
        )
    }

    pub async fn prepare_discovery_review_material_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryReviewMaterial, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_review_basis(&campaign, revision, &hit)
            })
            .await?;
        self.repository_cpu_segment(move || build_material(basis))
            .await
    }
}

#[cfg(test)]
mod bluesky_tests;
#[cfg(test)]
mod instagram_tests;
#[cfg(test)]
mod language_tests;
#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod reddit_tests;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod weibo_tests;
#[cfg(test)]
mod zhihu_tests;
