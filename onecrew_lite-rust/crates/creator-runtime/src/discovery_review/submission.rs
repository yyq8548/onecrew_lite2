//! Local reviewer assertions are auditable judgments, never provider facts or arbitrary verdicts.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    DiscoveryReviewBasis, DiscoveryReviewCitation, DiscoveryReviewMaterial, build_material,
};
use crate::{
    CreatorRepositoryCallError, CreatorRuntime,
    discovery_qualification::{assess, assess_profile, assessed_evidence},
};

const POLICY: &str = "youtube-text-review-v1";
const APPAREL_POLICY: &str = "apparel-residence-review-v1";
const APPAREL_NATIVE_POLICY: &str = "apparel-native-review-v2";
const CATEGORY_POLICY: &str = "category-content-review-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryReviewerKind {
    Agent,
    Human,
}

/// The label/model is self-reported; authenticated account identity is injected separately.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryReviewer {
    pub kind: DiscoveryReviewerKind,
    #[schemars(length(min = 1, max = 256))]
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryReviewCriterion {
    OwnAiToolDemo,
    PublicToolUrl,
    ApparelBusinessContent,
    CategoryContent,
    CreatorResidenceOutsideMainland,
    ContentLanguage,
    MatchFit,
    /// 当前已批准 requirements 数组中的索引，随 criteria_revision 固定。
    Requirement(u8),
}

/// Evidence-based fit, independent of eligibility. Unknown fit has no level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryMatchLevel {
    Excellent,
    Good,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryReviewOutcome {
    Supported,
    Contradicted,
    Insufficient,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiscoveryReviewQuote {
    Description {
        quote: String,
    },
    SearchCaption {
        quote: String,
    },
    Transcript {
        segment_index: u32,
        quote: String,
    },
    NativeProfile {
        profile_url: String,
        quote: String,
    },
    NativeContent {
        quote: String,
    },
    PublicPage {
        requested_url: String,
        quote: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryReviewedTool {
    pub name: String,
    pub requested_url: String,
}

/// Supported demo requires first-hand actions/results in saved transcript, not a title,
/// advertisement, list, third-party retelling or mere tool mention. Text is not visual proof.
/// Supported tool requires an author-observed link and a successful target-page quotation.
/// Language requires exact saved author text; spoken language requires transcript.
/// MatchFit is optional: excellent = strong relevant evidence and an advantage;
/// good = clear supported relevance; medium = partial fit with a limitation;
/// low = a cited significant weakness. Missing optional data never lowers fit.
/// Fit cannot override required eligibility. Unknown fit has insufficient outcome
/// and no match_level. Absence in one sample cannot reject the whole creator.
/// match_fit 的 supported 表示证据支持所选等级，包括 low；
/// 未知匹配使用 insufficient 且不填等级，match_fit 不接受 contradicted。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryReviewFinding {
    pub criterion: DiscoveryReviewCriterion,
    pub outcome: DiscoveryReviewOutcome,
    #[schemars(length(min = 1, max = 4000))]
    pub rationale: String,
    #[schemars(length(max = 8))]
    pub citations: Vec<DiscoveryReviewQuote>,
    pub tool: Option<DiscoveryReviewedTool>,
    /// ISO 3166-1 alpha-2 location identified by the reviewer from cited evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_level: Option<DiscoveryMatchLevel>,
}

/// Judgments reference the previously read owner material. No account, policy, proof or verdict input.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubmitDiscoveryReviewInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub search_hit_id: String,
    pub material_hash: String,
    pub request_id: String,
    pub reviewer: DiscoveryReviewer,
    #[schemars(length(max = 40))]
    pub findings: Vec<DiscoveryReviewFinding>,
}

/// Account identity is injected by the authenticated runtime, never the caller.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetDiscoveryReviewInput {
    pub campaign_id: String,
    pub request_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryReviewJudgment {
    pub criterion: DiscoveryReviewCriterion,
    pub outcome: DiscoveryReviewOutcome,
}

/// A committed historical receipt, not a promise that the candidate has not since changed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryReviewReceipt {
    pub review_id: String,
    pub operator_account_id: String,
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub search_hit_id: String,
    pub candidate_id: String,
    pub native_author_id: String,
    pub profile_ref: String,
    pub request_id: String,
    pub material_hash: String,
    pub policy_version: String,
    pub judgments: Vec<DiscoveryReviewJudgment>,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub subscribers: Option<u64>,
    pub created_at: i64,
}

/// Opaque persisted readback, parsed outside the repository lease.
pub struct StoredDiscoveryReview {
    pub(crate) request_hash: String,
    pub(crate) receipt_json: String,
}

/// Read atomically: a matching completed request wins over a changed candidate basis.
pub enum DiscoveryReviewSubmissionBasis {
    Committed(StoredDiscoveryReview),
    Material(Box<DiscoveryReviewBasis>),
}

/// Only the owner can construct the exact audited mutation and its immutable source basis.
pub struct PreparedDiscoveryReview {
    pub(crate) basis: DiscoveryReviewBasis,
    pub(crate) receipt: DiscoveryReviewReceipt,
    pub(crate) request_hash: String,
    pub(crate) material_json: String,
    pub(crate) reviewer_json: String,
    pub(crate) findings_json: String,
    pub(crate) receipt_json: String,
    pub(crate) evidence_json: String,
}

fn text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn encoded(value: &impl Serialize, max: usize) -> Result<String, String> {
    let value = serde_json::to_string(value).map_err(|_| "review_encoding_invalid")?;
    if value.len() > max {
        return Err("review_input_too_large".into());
    }
    Ok(value)
}

fn request_hash(account: &str, input: &SubmitDiscoveryReviewInput) -> Result<String, String> {
    if !text(account, 256)
        || !text(&input.campaign_id, 128)
        || !text(&input.search_hit_id, 128)
        || !text(&input.request_id, 128)
        || input.expected_revision == 0
        || !hash(&input.material_hash)
        || !text(&input.reviewer.label, 256)
        || input.findings.len() > 40
    {
        return Err("review_input_invalid".into());
    }
    let value = encoded(&(POLICY, account, input), 65_536)?;
    Ok(format!("{:x}", Sha256::digest(value.as_bytes())))
}

pub(super) fn citation_matches(
    material: &DiscoveryReviewMaterial,
    cite: &DiscoveryReviewQuote,
) -> bool {
    let reference = match cite {
        DiscoveryReviewQuote::Description { quote } => {
            DiscoveryReviewCitation::Description { quote }
        }
        DiscoveryReviewQuote::SearchCaption { quote } => {
            DiscoveryReviewCitation::SearchCaption { quote }
        }
        DiscoveryReviewQuote::Transcript {
            segment_index,
            quote,
        } => DiscoveryReviewCitation::Transcript {
            segment_index: *segment_index as usize,
            quote,
        },
        DiscoveryReviewQuote::NativeProfile { profile_url, quote } => {
            DiscoveryReviewCitation::NativeProfile { profile_url, quote }
        }
        DiscoveryReviewQuote::NativeContent { quote } => {
            DiscoveryReviewCitation::NativeContent { quote }
        }
        DiscoveryReviewQuote::PublicPage {
            requested_url,
            quote,
        } => DiscoveryReviewCitation::PublicPage {
            requested_url,
            quote,
        },
    };
    material.contains_citation(reference)
}

fn apply_findings(
    material: &DiscoveryReviewMaterial,
    input: &SubmitDiscoveryReviewInput,
    evidence: &mut Value,
    review_id: &str,
) -> Result<(), String> {
    apply_language_and_match(material, input, evidence, review_id)?;
    let object = evidence.as_object_mut().ok_or("review_evidence_invalid")?;
    let apparel_policy = if matches!(material.hit.platform.as_str(), "twitter" | "tiktok") {
        APPAREL_NATIVE_POLICY
    } else {
        APPAREL_POLICY
    };
    // Never inherit a copied verified boolean as authority for this review.
    object.remove("own_ai_tool_demo");
    object.remove("public_tool_url");
    for (criterion, key) in [
        (
            DiscoveryReviewCriterion::OwnAiToolDemo,
            "requires_ai_tool_demo",
        ),
        (
            DiscoveryReviewCriterion::PublicToolUrl,
            "requires_public_tool_url",
        ),
    ] {
        let findings: Vec<_> = input
            .findings
            .iter()
            .filter(|finding| finding.criterion == criterion)
            .collect();
        let required = material.criteria.get(key).and_then(Value::as_bool) != Some(false);
        if findings.len() != usize::from(required) {
            return Err("review_conditions_mismatch".into());
        }
        let Some(finding) = findings.first() else {
            continue;
        };
        if finding.country_code.is_some()
            || !text(&finding.rationale, 4000)
            || finding.citations.len() > 8
            || !finding
                .citations
                .iter()
                .all(|cite| citation_matches(material, cite))
            || (finding.outcome != DiscoveryReviewOutcome::Insufficient
                && finding.citations.is_empty())
        {
            return Err("review_citation_invalid".into());
        }
        if finding.outcome != DiscoveryReviewOutcome::Supported {
            if finding.tool.is_some() {
                return Err("review_tool_unexpected".into());
            }
            continue;
        }
        let mut proof = json!({"verified":true, "source_url":material.hit.source_url,
            "review_id":review_id, "material_hash":material.material_hash, "policy_version":POLICY});
        match criterion {
            DiscoveryReviewCriterion::OwnAiToolDemo => {
                if finding.tool.is_some()
                    || !finding
                        .citations
                        .iter()
                        .any(|cite| matches!(cite, DiscoveryReviewQuote::Transcript { .. }))
                {
                    return Err("review_first_hand_evidence_required".into());
                }
                proof["channel_id"] = json!(material.native_author_id);
                object.insert("own_ai_tool_demo".into(), proof);
            }
            DiscoveryReviewCriterion::PublicToolUrl => {
                let tool = finding.tool.as_ref().ok_or("review_tool_required")?;
                let page = material
                    .public_pages
                    .iter()
                    .find(|page| {
                        page.requested_url == tool.requested_url
                            && page.status == "completed"
                            && page.stop_code.is_none()
                            && page.source_excerpt.contains(&tool.requested_url)
                    })
                    .ok_or("review_tool_source_unavailable")?;
                let observed_name = page.title.to_lowercase().contains(&tool.name.to_lowercase())
                    || finding.citations.iter().any(|cite| matches!(cite,
                        DiscoveryReviewQuote::PublicPage { requested_url, quote }
                            if requested_url == &page.requested_url && quote.to_lowercase().contains(&tool.name.to_lowercase())));
                if !text(&tool.name, 256) || !observed_name || !finding.citations.iter().any(|cite|
                    matches!(cite, DiscoveryReviewQuote::PublicPage { requested_url, .. } if requested_url == &page.requested_url)) {
                    return Err("review_tool_citation_required".into());
                }
                proof["tool_name"] = json!(tool.name);
                proof["url"] = json!(page.requested_url);
                object.insert("public_tool_url".into(), proof);
            }
            _ => return Err("review_conditions_mismatch".into()),
        }
    }
    for (criterion, required) in [
        (
            DiscoveryReviewCriterion::ApparelBusinessContent,
            material.criteria["category"] == "服装",
        ),
        (
            DiscoveryReviewCriterion::CategoryContent,
            material.criteria["category"]
                .as_str()
                .is_some_and(|category| !category.is_empty() && category != "服装"),
        ),
        (
            DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
            material.criteria["region"] == "中国大陆以外",
        ),
    ] {
        let findings: Vec<_> = input
            .findings
            .iter()
            .filter(|finding| finding.criterion == criterion)
            .collect();
        if findings.len() != usize::from(required) {
            return Err("review_conditions_mismatch".into());
        }
        let Some(finding) = findings.first() else {
            continue;
        };
        if !text(&finding.rationale, 4000)
            || finding.citations.len() > 8
            || !finding
                .citations
                .iter()
                .all(|cite| citation_matches(material, cite))
            || (finding.outcome != DiscoveryReviewOutcome::Insufficient
                && finding.citations.is_empty())
            || finding.tool.is_some()
        {
            return Err("review_citation_invalid".into());
        }
        if finding.outcome != DiscoveryReviewOutcome::Supported {
            if finding.country_code.is_some() {
                return Err("review_country_unexpected".into());
            }
            if finding.outcome == DiscoveryReviewOutcome::Contradicted {
                // An affirmative residence conflict invalidates an earlier
                // overseas claim. An unrelated apparel video does not erase
                // a supported clothing video from the same author.
                match criterion {
                    DiscoveryReviewCriterion::CreatorResidenceOutsideMainland => {
                        object.remove("reviewed_residence");
                    }
                    DiscoveryReviewCriterion::ApparelBusinessContent
                        if object["reviewed_apparel_content"]["source_url"]
                            == material.hit.source_url =>
                    {
                        object.remove("reviewed_apparel_content");
                    }
                    DiscoveryReviewCriterion::CategoryContent
                        if object["reviewed_category_content"]["source_url"]
                            == material.hit.source_url =>
                    {
                        object.remove("reviewed_category_content");
                    }
                    _ => {}
                }
            }
            continue;
        }
        match criterion {
            DiscoveryReviewCriterion::ApparelBusinessContent => {
                if finding.country_code.is_some()
                    || !finding.citations.iter().any(|cite| {
                        matches!(
                            cite,
                            DiscoveryReviewQuote::Transcript { .. }
                                | DiscoveryReviewQuote::NativeContent { .. }
                                | DiscoveryReviewQuote::SearchCaption { .. }
                        )
                    })
                {
                    return Err("review_first_hand_evidence_required".into());
                }
                object.insert(
                    "reviewed_apparel_content".into(),
                    json!({
                        "verified": true, "native_author_id": material.native_author_id,
                        "source_url": material.hit.source_url, "review_id": review_id,
                        "material_hash": material.material_hash, "policy_version": apparel_policy,
                    }),
                );
            }
            DiscoveryReviewCriterion::CategoryContent => {
                if finding.country_code.is_some()
                    || !finding.citations.iter().any(|cite| {
                        matches!(
                            cite,
                            DiscoveryReviewQuote::Description { .. }
                                | DiscoveryReviewQuote::Transcript { .. }
                                | DiscoveryReviewQuote::NativeContent { .. }
                        ) || (material.hit.platform == "tiktok"
                            && matches!(cite, DiscoveryReviewQuote::SearchCaption { .. }))
                    })
                {
                    return Err("review_first_hand_evidence_required".into());
                }
                object.insert(
                    "reviewed_category_content".into(),
                    json!({
                        "verified": true, "native_author_id": material.native_author_id,
                        "category": material.criteria["category"],
                        "source_url": material.hit.source_url, "review_id": review_id,
                        "material_hash": material.material_hash, "policy_version": CATEGORY_POLICY,
                    }),
                );
            }
            DiscoveryReviewCriterion::CreatorResidenceOutsideMainland => {
                let country = finding
                    .country_code
                    .as_deref()
                    .ok_or("review_country_required")?;
                if country.len() != 2
                    || country == "CN"
                    || !country.bytes().all(|byte| byte.is_ascii_uppercase())
                {
                    return Err("review_country_invalid".into());
                }
                // Residence requires a first-person statement and a distinct,
                // successfully read page linked from that author's content.
                if !finding.citations.iter().any(|cite| {
                    matches!(
                        cite,
                        DiscoveryReviewQuote::Transcript { .. }
                            | DiscoveryReviewQuote::NativeContent { .. }
                    )
                }) {
                    return Err("review_residence_statement_required".into());
                }
                let page_url = finding
                    .citations
                    .iter()
                    .find_map(|cite| match cite {
                        DiscoveryReviewQuote::PublicPage { requested_url, .. } => {
                            Some(requested_url.as_str())
                        }
                        _ => None,
                    })
                    .ok_or("review_residence_corroboration_required")?;
                let page = material
                    .public_pages
                    .iter()
                    .find(|page| {
                        page.requested_url == page_url
                            && page.status == "completed"
                            && page.stop_code.is_none()
                            && page.source_excerpt.contains(page_url)
                    })
                    .ok_or("review_residence_corroboration_unavailable")?;
                object.insert(
                    "reviewed_residence".into(),
                    json!({
                        "verified": true, "native_author_id": material.native_author_id,
                        "country_code": country, "source_url": material.hit.source_url,
                        "corroboration_url": page.requested_url, "review_id": review_id,
                        "material_hash": material.material_hash, "policy_version": apparel_policy,
                    }),
                );
            }
            _ => return Err("review_conditions_mismatch".into()),
        }
    }
    let sources = object
        .get_mut("source_urls")
        .and_then(Value::as_array_mut)
        .ok_or("review_evidence_invalid")?;
    if !sources
        .iter()
        .any(|url| url.as_str() == Some(&material.hit.source_url))
    {
        sources.push(json!(material.hit.source_url));
    }
    Ok(())
}

fn apply_language_and_match(
    material: &DiscoveryReviewMaterial,
    input: &SubmitDiscoveryReviewInput,
    evidence: &mut Value,
    review_id: &str,
) -> Result<(), String> {
    use DiscoveryReviewCriterion::{ContentLanguage, MatchFit};
    let language = crate::discovery_qualification::required_language(&material.criteria);
    for criterion in [ContentLanguage, MatchFit] {
        let findings: Vec<_> = input
            .findings
            .iter()
            .filter(|f| f.criterion == criterion)
            .collect();
        if findings.len() > 1
            || (criterion == ContentLanguage && language.is_none() && !findings.is_empty())
        {
            return Err("review_conditions_mismatch".into());
        }
        let Some(finding) = findings.first() else {
            continue;
        };
        let author_content = |cite: &DiscoveryReviewQuote| {
            matches!(
                cite,
                DiscoveryReviewQuote::Transcript { .. }
                    | DiscoveryReviewQuote::NativeContent { .. }
                    | DiscoveryReviewQuote::Description { .. }
            ) || (material.hit.platform == "tiktok"
                && matches!(cite, DiscoveryReviewQuote::SearchCaption { .. }))
        };
        if !text(&finding.rationale, 4000)
            || finding.tool.is_some()
            || finding.country_code.is_some()
            || finding.citations.len() > 8
            || !finding
                .citations
                .iter()
                .all(|cite| citation_matches(material, cite))
            || (finding.outcome != DiscoveryReviewOutcome::Insufficient
                && !finding.citations.iter().any(author_content))
        {
            return Err("review_first_hand_evidence_required".into());
        }
        // Spoken-language requests require original speech transcripts. Captions,
        // interface language and profile location do not establish spoken language.
        if criterion == ContentLanguage
            && finding.outcome != DiscoveryReviewOutcome::Insufficient
            && language.is_some_and(|value| {
                let value = value.to_lowercase();
                ["spoken", "speech", "口语", "口播", "语音"]
                    .iter()
                    .any(|word| value.contains(word))
            })
            && !finding
                .citations
                .iter()
                .any(|cite| matches!(cite, DiscoveryReviewQuote::Transcript { .. }))
        {
            return Err("review_transcript_required".into());
        }
        if (criterion != MatchFit && finding.match_level.is_some())
            || (criterion == MatchFit
                && (finding.outcome == DiscoveryReviewOutcome::Supported)
                    != finding.match_level.is_some())
            || (criterion == MatchFit && finding.outcome == DiscoveryReviewOutcome::Contradicted)
        {
            return Err("review_match_level_invalid".into());
        }
        let key = if criterion == ContentLanguage {
            "reviewed_language"
        } else {
            "reviewed_match"
        };
        // An explicit reassessment replaces a former assertion. Missing optional
        // match findings preserve the earlier audited rating.
        evidence
            .as_object_mut()
            .ok_or("review_evidence_invalid")?
            .remove(key);
        if finding.outcome != DiscoveryReviewOutcome::Insufficient {
            evidence[key] = json!({
                "verified": true, "native_author_id": material.native_author_id,
                "source_url": material.hit.source_url, "review_id": review_id,
                "material_hash": material.material_hash, "policy_version": "language-match-review-v1",
                "language": material.criteria["language"], "outcome": finding.outcome,
                "level": finding.match_level, "reason": finding.rationale,
            });
        }
    }
    if input
        .findings
        .iter()
        .any(|f| f.criterion != MatchFit && f.match_level.is_some())
    {
        return Err("review_match_level_invalid".into());
    }
    Ok(())
}

fn apply_requirements(
    material: &DiscoveryReviewMaterial,
    input: &SubmitDiscoveryReviewInput,
    evidence: &mut Value,
    review_id: &str,
) -> Result<(), String> {
    let requirements = match material.criteria.get("requirements") {
        None => &[][..],
        Some(value) => value.as_array().ok_or("review_requirements_invalid")?.as_slice(),
    };
    if requirements.len() > 32 {
        return Err("review_requirements_invalid".into());
    }
    let findings: Vec<_> = input.findings.iter().filter(|finding| {
        matches!(finding.criterion, DiscoveryReviewCriterion::Requirement(_))
    }).collect();
    if findings.len() != requirements.len() {
        return Err("review_requirements_incomplete".into());
    }
    let mut proofs = Vec::with_capacity(requirements.len());
    for (index, requirement) in requirements.iter().enumerate() {
        let matching: Vec<_> = findings.iter().filter(|finding| {
            finding.criterion == DiscoveryReviewCriterion::Requirement(index as u8)
        }).collect();
        if matching.len() != 1 {
            return Err("review_requirements_mismatch".into());
        }
        let finding = matching[0];
        if !text(&finding.rationale, 4000)
            || finding.citations.len() > 8
            || finding.tool.is_some()
            || finding.country_code.is_some()
            || finding.match_level.is_some()
            || !finding.citations.iter().all(|cite| citation_matches(material, cite))
            || (finding.outcome != DiscoveryReviewOutcome::Insufficient && finding.citations.is_empty())
        {
            return Err("review_requirement_citation_invalid".into());
        }
        proofs.push(json!({
            "requirement": requirement, "index": index, "outcome": finding.outcome,
            "reason": finding.rationale, "native_author_id": material.native_author_id,
            "source_url": material.hit.source_url, "review_id": review_id,
            "material_hash": material.material_hash, "criteria_revision": input.expected_revision,
        }));
    }
    evidence.as_object_mut().ok_or("review_evidence_invalid")?
        .insert("reviewed_requirements".into(), json!(proofs));
    Ok(())
}

fn prepare(
    account: String,
    input: SubmitDiscoveryReviewInput,
    request_hash: String,
    basis: DiscoveryReviewBasis,
) -> Result<PreparedDiscoveryReview, String> {
    let material = build_material(basis.clone())?;
    if input.material_hash != material.material_hash {
        return Err("review_material_changed".into());
    }
    let review_id = uuid::Uuid::new_v4().to_string();
    let mut evidence: Value =
        serde_json::from_str(&basis.evidence_json).map_err(|_| "review_evidence_invalid")?;
    let initial_match = super::initial_match::is_initial(&input);
    if initial_match {
        super::initial_match::apply(&material, &input, &mut evidence, &review_id)?;
    } else {
        apply_findings(&material, &input, &mut evidence, &review_id)?;
    }
    apply_requirements(&material, &input, &mut evidence, &review_id)?;
    let assessment =
        if material.hit.platform == "douyin" && material.native_follower_estimate.is_some() {
            crate::discovery_qualification::assess_douyin_profile(
                &material.criteria,
                &evidence,
                &material.native_author_id,
                material
                    .native_follower_estimate
                    .as_ref()
                    .map(|estimate| estimate.display.as_str()),
            )
        } else if matches!(material.hit.platform.as_str(), "reddit" | "facebook") {
            match material.native_followers {
                Some(count) => assess_profile(
                    &material.criteria,
                    &evidence,
                    &material.native_author_id,
                    count,
                ),
                None => crate::discovery_qualification::assess_profile_without_count(
                    &material.criteria,
                    &evidence,
                    &material.native_author_id,
                ),
            }
        } else if matches!(
            material.hit.platform.as_str(),
            "twitter" | "tiktok" | "bilibili"
        ) {
            assess_profile(
                &material.criteria,
                &evidence,
                &material.native_author_id,
                material
                    .native_followers
                    .ok_or("review_native_followers_unavailable")?,
            )
        } else {
            assess(&material.criteria, &evidence, &material.native_author_id)
        };
    let receipt = DiscoveryReviewReceipt {
        review_id,
        operator_account_id: account,
        campaign_id: basis.campaign_id.clone(),
        criteria_revision: basis.revision,
        search_hit_id: basis.hit.id.clone(),
        candidate_id: basis.candidate_id.clone(),
        native_author_id: basis.native_author_id.clone(),
        profile_ref: basis.hit.profile_ref.clone(),
        request_id: input.request_id,
        material_hash: material.material_hash.clone(),
        policy_version: if initial_match {
            super::initial_match::POLICY
        } else if input.findings.iter().any(|f| {
            matches!(
                f.criterion,
                DiscoveryReviewCriterion::ContentLanguage | DiscoveryReviewCriterion::MatchFit
            )
        }) {
            "language-match-review-v1"
        } else if material.criteria["category"]
            .as_str()
            .is_some_and(|category| !category.is_empty() && category != "服装")
        {
            CATEGORY_POLICY
        } else if material.criteria["category"] == "服装"
            || material.criteria["region"] == "中国大陆以外"
        {
            if matches!(material.hit.platform.as_str(), "twitter" | "tiktok") {
                APPAREL_NATIVE_POLICY
            } else {
                APPAREL_POLICY
            }
        } else {
            POLICY
        }
        .into(),
        judgments: input
            .findings
            .iter()
            .map(|finding| DiscoveryReviewJudgment {
                criterion: finding.criterion,
                outcome: finding.outcome,
            })
            .collect(),
        verdict: assessment.verdict.into(),
        reasons: assessment.reasons.iter().map(|s| (*s).into()).collect(),
        subscribers: assessment.subscribers,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "review_clock_invalid")?
            .as_secs()
            .try_into()
            .map_err(|_| "review_clock_invalid")?,
    };
    Ok(PreparedDiscoveryReview {
        basis,
        request_hash,
        material_json: encoded(&material, super::MAX_REVIEW_BYTES)?,
        reviewer_json: encoded(&input.reviewer, 2048)?,
        findings_json: encoded(&input.findings, 65_536)?,
        receipt_json: encoded(&receipt, 16_384)?,
        evidence_json: encoded(
            &assessed_evidence(evidence, &assessment),
            super::MAX_REVIEW_BYTES,
        )?,
        receipt,
    })
}

fn decode(
    stored: StoredDiscoveryReview,
    expected_hash: Option<&str>,
) -> Result<DiscoveryReviewReceipt, String> {
    if expected_hash.is_some_and(|hash| stored.request_hash != hash) {
        return Err("review_idempotency_conflict".into());
    }
    serde_json::from_str(&stored.receipt_json).map_err(|_| "review_receipt_invalid".into())
}

impl CreatorRuntime {
    /// Account is a trusted composition argument, not a field accepted from HTTP/MCP JSON.
    /// A same-key replay returns the immutable historical receipt and never rewrites a candidate.
    pub fn submit_discovery_review(
        &self,
        account: &str,
        input: SubmitDiscoveryReviewInput,
    ) -> Result<DiscoveryReviewReceipt, String> {
        let hash = request_hash(account, &input)?;
        let basis = match self.repository.read_discovery_review_submission(
            account,
            &input.campaign_id,
            &input.request_id,
            input.expected_revision,
            &input.search_hit_id,
        )? {
            DiscoveryReviewSubmissionBasis::Committed(stored) => {
                return decode(stored, Some(&hash));
            }
            DiscoveryReviewSubmissionBasis::Material(basis) => *basis,
        };
        let prepared = prepare(account.into(), input, hash.clone(), basis)?;
        decode(
            self.repository.commit_discovery_review(&prepared)?,
            Some(&hash),
        )
    }

    pub async fn submit_discovery_review_async(
        &self,
        account: String,
        input: SubmitDiscoveryReviewInput,
    ) -> Result<DiscoveryReviewReceipt, CreatorRepositoryCallError<String>> {
        let (account, input, hash) = self
            .repository_cpu_segment(move || {
                let hash = request_hash(&account, &input)?;
                Ok((account, input, hash))
            })
            .await?;
        let repository = self.repository.clone();
        let (account, input, hash, basis) = self
            .repository_read_segment(move || {
                let basis = repository.read_discovery_review_submission(
                    &account,
                    &input.campaign_id,
                    &input.request_id,
                    input.expected_revision,
                    &input.search_hit_id,
                )?;
                Ok((account, input, hash, basis))
            })
            .await?;
        let basis = match basis {
            DiscoveryReviewSubmissionBasis::Committed(stored) => {
                return self
                    .repository_cpu_segment(move || decode(stored, Some(&hash)))
                    .await;
            }
            DiscoveryReviewSubmissionBasis::Material(basis) => *basis,
        };
        let prepared = self
            .repository_cpu_segment(move || prepare(account, input, hash, basis))
            .await?;
        let repository = self.repository.clone();
        let stored = self
            .repository_write_segment(move || repository.commit_discovery_review(&prepared))
            .await?;
        self.repository_cpu_segment(move || decode(stored, None))
            .await
    }

    /// Readback before retrying an uncertain submission; does not replay a write.
    pub fn get_discovery_review(
        &self,
        account: &str,
        campaign: &str,
        request: &str,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        if !text(account, 256) || !text(campaign, 128) || !text(request, 128) {
            return Err("review_input_invalid".into());
        }
        self.repository
            .find_discovery_review(account, campaign, request)?
            .map(|stored| decode(stored, None))
            .transpose()
    }

    pub async fn get_discovery_review_async(
        &self,
        account: String,
        campaign: String,
        request: String,
    ) -> Result<Option<DiscoveryReviewReceipt>, CreatorRepositoryCallError<String>> {
        let repository = self.repository.clone();
        let stored = self
            .repository_read_segment(move || {
                if !text(&account, 256) || !text(&campaign, 128) || !text(&request, 128) {
                    return Err("review_input_invalid".into());
                }
                repository.find_discovery_review(&account, &campaign, &request)
            })
            .await?;
        self.repository_cpu_segment(move || stored.map(|stored| decode(stored, None)).transpose())
            .await
    }
}

#[cfg(test)]
mod tests;
