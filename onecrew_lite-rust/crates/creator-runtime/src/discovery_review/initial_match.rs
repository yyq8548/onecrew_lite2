//! Versioned initial matching replaces the legacy proof checklist for scored rows.
//! The rubric is pinned to onecrew-plugins 0cebf122, not to provider instructions.
use super::DiscoveryReviewMaterial;
use super::submission::*;
use crate::discovery_qualification::Assessment;
use serde_json::{Value, json};

pub const INITIAL_MATCH_REVIEWER: &str = "onecrew-match-evaluation-0cebf122-initial-v1";
pub(crate) const POLICY: &str = "initial-match-evaluation-v1";

impl crate::CreatorRuntime {
    /// The returned count and preservation flag are derived from the same saved basis.
    pub async fn prepare_initial_match_material_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<
        (DiscoveryReviewMaterial, Option<u64>, bool),
        crate::CreatorRepositoryCallError<String>,
    > {
        let repository = self.repository.clone();
        let basis = self
            .repository_read_segment(move || {
                repository.read_discovery_review_basis(&campaign, revision, &hit)
            })
            .await?;
        self.repository_cpu_segment(move || {
            let evidence: Value = serde_json::from_str(&basis.evidence_json)
                .map_err(|_| "review_evidence_invalid".to_owned())?;
            let count = basis.native_followers.or_else(|| {
                crate::discovery_qualification::assess(
                    &serde_json::from_str(&basis.criteria_json).unwrap_or(Value::Null),
                    &evidence,
                    &basis.native_author_id,
                )
                .subscribers
            });
            let preserve = evidence["reviewed_match"]["policy_version"] == POLICY
                && evidence["reviewed_match"]["outcome"] == "supported";
            Ok((super::build_material(basis)?, count, preserve))
        })
        .await
    }
}

pub(super) fn is_initial(input: &SubmitDiscoveryReviewInput) -> bool {
    input.reviewer.kind == DiscoveryReviewerKind::Agent
        && input.reviewer.label == INITIAL_MATCH_REVIEWER
}

pub(super) fn apply(
    material: &DiscoveryReviewMaterial,
    input: &SubmitDiscoveryReviewInput,
    evidence: &mut Value,
    review_id: &str,
) -> Result<(), String> {
    let finding = input
        .findings
        .first()
        .ok_or("initial_match_single_finding_required")?;
    if input.findings.len() > 2 {
        return Err("initial_match_single_finding_required".into());
    }
    if let Some(language) = input.findings.get(1)
        && (finding.outcome != DiscoveryReviewOutcome::Insufficient
            || language.criterion != DiscoveryReviewCriterion::ContentLanguage
            || language.outcome != DiscoveryReviewOutcome::Insufficient
            || language.match_level.is_some()
            || language.tool.is_some()
            || language.country_code.is_some()
            || language.rationale.trim().is_empty()
            || language.rationale.len() > 4000
            || language.citations.len() > 8
            || !material.criteria["language"]
                .as_str()
                .is_some_and(|v| !v.trim().is_empty())
            || !language
                .citations
                .iter()
                .all(|q| super::submission::citation_matches(material, q)))
    {
        return Err("initial_match_language_request_invalid".into());
    }
    let supported = finding.outcome == DiscoveryReviewOutcome::Supported;
    if finding.criterion != DiscoveryReviewCriterion::MatchFit
        || finding.outcome == DiscoveryReviewOutcome::Contradicted
        || supported != finding.match_level.is_some()
        || finding.rationale.trim().is_empty()
        || finding.rationale.len() > 4000
        || finding.tool.is_some()
        || finding.country_code.is_some()
        || finding.citations.len() > 8
        || !finding
            .citations
            .iter()
            .all(|quote| super::submission::citation_matches(material, quote))
    {
        return Err("initial_match_finding_invalid".into());
    }
    let minimum = material.criteria["min_subscribers"]
        .as_u64()
        .or_else(|| material.criteria["min_followers"].as_u64())
        .filter(|n| *n > 0);
    let count = material.native_followers.or_else(|| {
        evidence
            .pointer("/assessment/subscribers_normalized")
            .and_then(Value::as_u64)
    });
    let estimate = material.native_follower_estimate.as_ref();
    let below_minimum = if let Some(estimate) = estimate {
        minimum.is_some_and(|min| estimate.upper < min)
    } else {
        minimum.zip(count).is_some_and(|(min, count)| count < min)
    };
    let count_unknown = if let Some(estimate) = estimate {
        minimum.is_some_and(|min| estimate.lower < min && estimate.upper >= min)
    } else {
        count.is_none()
    };
    if input.findings.len() == 2 && minimum.is_some() && count_unknown {
        return Err("initial_match_language_request_invalid".into());
    }
    // An observed required failure has precedence over unknown information.
    if below_minimum && finding.match_level != Some(DiscoveryMatchLevel::Low) {
        return Err("initial_match_required_failure_must_be_low".into());
    }
    if !below_minimum
        && minimum.is_some()
        && count_unknown
        && supported
        && finding.match_level != Some(DiscoveryMatchLevel::Low)
    {
        return Err("initial_match_required_count_unknown".into());
    }
    if supported
        && !below_minimum
        && !finding.citations.iter().any(|quote| {
            matches!(
                quote,
                DiscoveryReviewQuote::Description { .. }
                    | DiscoveryReviewQuote::SearchCaption { .. }
                    | DiscoveryReviewQuote::Transcript { .. }
                    | DiscoveryReviewQuote::NativeContent { .. }
                    | DiscoveryReviewQuote::NativeProfile { .. }
            )
        })
    {
        return Err("initial_match_author_evidence_required".into());
    }
    // A native biography can establish topic context, never content language.
    // Keep the author pending until a real original-content/comment read supplies it.
    if supported
        && !below_minimum
        && finding.match_level != Some(DiscoveryMatchLevel::Low)
        && material.criteria["language"]
            .as_str()
            .is_some_and(|language| !language.trim().is_empty())
        && finding
            .citations
            .iter()
            .any(|quote| matches!(quote, DiscoveryReviewQuote::NativeProfile { .. }))
        && !finding.citations.iter().any(|quote| {
            matches!(
                quote,
                DiscoveryReviewQuote::Description { .. }
                    | DiscoveryReviewQuote::SearchCaption { .. }
                    | DiscoveryReviewQuote::Transcript { .. }
                    | DiscoveryReviewQuote::NativeContent { .. }
            )
        })
    {
        return Err("initial_match_content_language_unavailable".into());
    }
    validate_audience_language(material, finding)?;
    evidence["reviewed_match"] = json!({
        "verified": true, "native_author_id": material.native_author_id,
        "source_url": material.hit.source_url, "review_id": review_id,
        "material_hash": material.material_hash, "policy_version": POLICY,
        "outcome": finding.outcome, "level": finding.match_level, "reason": finding.rationale,
        "language_only_pending": input.findings.len() == 2,
    });
    Ok(())
}

/// The model identifies language; the owner verifies the cited sample boundary.
/// No later comment, duplicate text, or second post can supply the missing votes.
fn validate_audience_language(
    material: &DiscoveryReviewMaterial,
    finding: &DiscoveryReviewFinding,
) -> Result<(), String> {
    if finding.outcome != DiscoveryReviewOutcome::Supported
        || finding.match_level == Some(DiscoveryMatchLevel::Low)
    {
        return Ok(());
    }
    let audience_claim = finding.rationale.contains("粉丝评论语言符合")
        || finding
            .rationale
            .contains("Audience comment language matches");
    let comment_only_citation = finding.citations.iter().any(|citation| {
        let DiscoveryReviewQuote::NativeContent { quote } = citation else {
            return false;
        };
        material.language_samples.iter().any(|sample| {
            sample
                .comments
                .iter()
                .any(|comment| comment.text.contains(quote))
        }) && !material
            .language_samples
            .iter()
            .any(|sample| sample.text.contains(quote))
            && !material
                .native_content
                .as_ref()
                .and_then(|read| read.native_content.as_ref())
                .and_then(|content| {
                    content.get(if material.hit.platform == "bilibili" {
                        "description"
                    } else {
                        "text"
                    })
                })
                .and_then(Value::as_str)
                .is_some_and(|text| text.contains(quote))
    });
    if !audience_claim && !comment_only_citation {
        return Ok(());
    }
    if material.criteria["language"]
        .as_str()
        .is_some_and(|language| {
            let language = language.to_lowercase();
            ["spoken", "speech", "口语", "口播", "语音"]
                .iter()
                .any(|word| language.contains(word))
        })
    {
        return Err("initial_match_audience_language_invalid".into());
    }
    if !audience_claim || finding.match_level != Some(DiscoveryMatchLevel::Good) {
        return Err("initial_match_audience_language_invalid".into());
    }
    let matched_source = material.matched_language_source();
    let enough = material.language_samples.iter().any(|sample| {
        if sample.author_id != material.native_author_id || sample.source_url != matched_source {
            return false;
        }
        let mut texts = std::collections::HashSet::new();
        let mut sources = std::collections::HashSet::new();
        let cited = sample
            .comments
            .iter()
            .take(5)
            .filter(|comment| {
                let text = comment.text.trim();
                // Minimal substance check. Semantic language/name checks remain in
                // the evaluator; URL-only and emoji-only comments cannot vote.
                let substance = text.chars().filter(|c| c.is_alphabetic()).count() >= 3
                    && !text.starts_with("https://")
                    && !text.starts_with("http://");
                substance
                    && texts.insert(text.to_lowercase())
                    && sources.insert(&comment.source_url)
                    && finding.citations.iter().any(|citation| {
                        matches!(citation,
                    DiscoveryReviewQuote::NativeContent { quote } if quote.trim() == text)
                    })
            })
            .count();
        cited >= 3
    });
    if !enough {
        return Err("initial_match_audience_sample_insufficient".into());
    }
    Ok(())
}

pub(crate) fn assessment(
    evidence: &Value,
    account_id: &str,
    subscribers: Option<u64>,
) -> Option<Assessment> {
    let proof = &evidence["reviewed_match"];
    if proof["verified"] != true
        || proof["policy_version"] != POLICY
        || proof["native_author_id"].as_str() != Some(account_id)
        || !proof["review_id"].as_str().is_some_and(|s| !s.is_empty())
        || !proof["reason"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 4000)
        || !evidence["source_urls"]
            .as_array()
            .is_some_and(|urls| urls.iter().any(|url| *url == proof["source_url"]))
    {
        return None;
    }
    let (verdict, reasons) = match (proof["outcome"].as_str(), proof["level"].as_str()) {
        (Some("supported"), Some("excellent" | "good" | "medium")) => ("qualified", vec![]),
        (Some("supported"), Some("low")) => ("rejected", vec!["initial_match_low"]),
        (Some("insufficient"), None) => ("pending", vec!["initial_match_unrated"]),
        _ => return None,
    };
    Some(Assessment {
        verdict,
        reasons,
        subscribers,
    })
}

#[cfg(test)]
mod audience_tests {
    use super::*;
    use crate::{DiscoveryLanguageComment, DiscoveryLanguageSample, DiscoverySearchHit};

    fn fixture() -> (DiscoveryReviewMaterial, DiscoveryReviewFinding) {
        let source = "https://x.com/creator/status/123";
        let comments: Vec<_> = [
            "この説明はとても分かりやすいです",
            "この製品を使ってみたいです",
            "紹介してくれてありがとうございます",
            "Thanks for sharing",
            "Interesting product",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, text)| DiscoveryLanguageComment {
            source_url: format!("https://x.com/viewer/status/{}", 200 + index),
            text: text.into(),
        })
        .collect();
        let finding = DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "粉丝评论语言符合：日语；作者语言未核实".into(),
            citations: comments
                .iter()
                .take(3)
                .map(|c| DiscoveryReviewQuote::NativeContent {
                    quote: c.text.clone(),
                })
                .collect(),
            tool: None,
            country_code: None,
            match_level: Some(DiscoveryMatchLevel::Good),
        };
        let material = DiscoveryReviewMaterial {
            format_version: 2,
            campaign_id: "test".into(),
            criteria_revision: 1,
            criteria: json!({"language":"日语"}),
            approved_scopes: vec![],
            approved_at: 1,
            hit: DiscoverySearchHit {
                id: "hit".into(),
                campaign_id: "test".into(),
                platform: "twitter".into(),
                profile_ref: "test".into(),
                search_keyword: "keyboard".into(),
                source_url: source.into(),
                title: "Keyboard".into(),
                author_label: "creator".into(),
                captured_at: 1,
                author_id: Some("100".into()),
                author_url: Some("https://x.com/creator".into()),
                author_observed_at: Some(1),
            },
            candidate_id: "candidate".into(),
            native_author_id: "100".into(),
            native_platform: None,
            native_followers: Some(10000),
            native_follower_estimate: None,
            native_profile: None,
            candidate_evidence_hash: "hash".into(),
            video_description: None,
            transcript: None,
            native_content: None,
            public_pages: vec![],
            material_hash: "hash".into(),
            language_samples: vec![DiscoveryLanguageSample {
                author_id: "100".into(),
                source_url: source.into(),
                text: "Original English content".into(),
                comments,
            }],
        };
        (material, finding)
    }

    #[test]
    fn audience_votes_require_three_distinct_full_comments_from_matched_post_first_five() {
        let (material, finding) = fixture();
        assert!(validate_audience_language(&material, &finding).is_ok());
        let mut two = finding.clone();
        two.citations.pop();
        assert_eq!(
            validate_audience_language(&material, &two).unwrap_err(),
            "initial_match_audience_sample_insufficient"
        );
        let mut duplicate = material.clone();
        duplicate.language_samples[0].comments[2].text =
            duplicate.language_samples[0].comments[0].text.clone();
        assert!(validate_audience_language(&duplicate, &finding).is_err());
        let mut sixth = material.clone();
        let third = sixth.language_samples[0].comments.remove(2);
        sixth.language_samples[0]
            .comments
            .push(DiscoveryLanguageComment {
                source_url: "https://x.com/viewer/status/300".into(),
                text: "A different comment".into(),
            });
        sixth.language_samples[0].comments.push(third);
        assert!(validate_audience_language(&sixth, &finding).is_err());
        let mut cross_post = material.clone();
        let third = cross_post.language_samples[0].comments.pop().unwrap();
        let required = cross_post.language_samples[0].comments.remove(2);
        let mut other = cross_post.language_samples[0].clone();
        other.source_url = "https://x.com/creator/status/999".into();
        other.comments = vec![required, third];
        cross_post.language_samples.push(other);
        assert!(validate_audience_language(&cross_post, &finding).is_err());
        let mut excellent = finding.clone();
        excellent.match_level = Some(DiscoveryMatchLevel::Excellent);
        assert!(validate_audience_language(&material, &excellent).is_err());
        let mut spoken = material.clone();
        spoken.criteria["language"] = json!("日语口播");
        assert!(validate_audience_language(&spoken, &finding).is_err());
        let mut unlabeled = finding;
        unlabeled.rationale = "作者使用日语".into();
        assert!(validate_audience_language(&material, &unlabeled).is_err());
    }
}
