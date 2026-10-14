//! Deterministic Campaign verdicts. Provider text is evidence, never an instruction.

use serde_json::{Value, json};
use url::Url;

pub(crate) fn required_language(criteria: &Value) -> Option<&str> {
    criteria["language"]
        .as_str()
        .map(str::trim)
        .filter(|value| {
            !matches!(
                value.to_lowercase().as_str(),
                "" | "不限" | "语言不限" | "any" | "all" | "unrestricted"
            )
        })
}

pub(crate) struct Assessment {
    pub verdict: &'static str,
    pub reasons: Vec<&'static str>,
    pub subscribers: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
struct SubscriberEstimate {
    nominal: u64,
    lower: u64,
    upper: u64,
}

pub(crate) fn assess_douyin_profile(
    criteria: &Value,
    evidence: &Value,
    account_id: &str,
    raw: Option<&str>,
) -> Assessment {
    let estimate = raw.and_then(crate::discovery_douyin::follower_estimate);
    let mut result = assess_with_count(
        criteria,
        evidence,
        account_id,
        estimate.as_ref().map(|v| SubscriberEstimate {
            nominal: v.nominal,
            lower: v.lower,
            upper: v.upper,
        }),
        false,
    );
    if let Some(estimate) = estimate {
        let minimum = criteria["min_subscribers"]
            .as_u64()
            .or_else(|| criteria["min_followers"].as_u64());
        if minimum.is_some_and(|min| estimate.lower < min && estimate.upper >= min)
            && !result.reasons.contains(&"subscriber_count_near_threshold")
        {
            result.reasons.push("subscriber_count_near_threshold");
        }
    }
    result
}

pub(crate) fn assess(criteria: &Value, evidence: &Value, account_id: &str) -> Assessment {
    let sources = evidence.get("source_urls").and_then(Value::as_array);
    let source_matches = |key: &str| {
        evidence
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|source| {
                sources.is_some_and(|sources| {
                    sources.iter().any(|value| value.as_str() == Some(source))
                })
            })
    };
    let raw = evidence.get("subscribers_raw").and_then(Value::as_str);
    let subscriber_estimate = if source_matches("subscribers_source_url")
        && evidence
            .get("captured_at")
            .and_then(Value::as_u64)
            .is_some_and(|time| time > 0)
        && evidence
            .get("subscribers_source_url")
            .and_then(Value::as_str)
            .is_some_and(is_youtube_video)
    {
        raw.and_then(parse_subscribers)
    } else {
        None
    };
    assess_with_count(criteria, evidence, account_id, subscriber_estimate, true)
}

/// The caller must obtain this exact count from a validated, author-bound native
/// receipt. This is not a second public evidence-input path. Native profile
/// text and YouTube proof can never stand in for this author's content proof.
pub(crate) fn assess_profile(
    criteria: &Value,
    evidence: &Value,
    account_id: &str,
    followers: u64,
) -> Assessment {
    assess_with_count(
        criteria,
        evidence,
        account_id,
        Some(SubscriberEstimate {
            nominal: followers,
            lower: followers,
            upper: followers,
        }),
        false,
    )
}

pub(crate) fn assess_profile_without_count(
    criteria: &Value,
    evidence: &Value,
    account_id: &str,
) -> Assessment {
    assess_with_count(criteria, evidence, account_id, None, false)
}

fn assess_with_count(
    criteria: &Value,
    evidence: &Value,
    account_id: &str,
    subscriber_estimate: Option<SubscriberEstimate>,
    youtube_content_supported: bool,
) -> Assessment {
    if let Some(assessment) = crate::discovery_review::initial_match::assessment(
        evidence,
        account_id,
        subscriber_estimate
            .as_ref()
            .map(|estimate| estimate.nominal),
    ) {
        return assessment;
    }
    let mut reasons = Vec::new();
    let sources = evidence.get("source_urls").and_then(Value::as_array);
    let minimum = criteria
        .get("min_subscribers")
        .or_else(|| criteria.get("min_followers"))
        .and_then(Value::as_u64)
        .filter(|minimum| *minimum > 0);
    let below_minimum = match (minimum, subscriber_estimate.as_ref()) {
        (Some(minimum), Some(estimate)) if estimate.upper < minimum => true,
        (Some(minimum), Some(estimate)) if estimate.lower < minimum => {
            reasons.push("subscriber_count_near_threshold");
            false
        }
        (Some(_), None) => {
            reasons.push("subscriber_count_unverified");
            false
        }
        _ => false,
    };
    if below_minimum {
        reasons.push("below_min_subscribers");
    }

    if criteria
        .get("requires_ai_tool_demo")
        .and_then(Value::as_bool)
        != Some(false)
    {
        let demo = evidence.get("own_ai_tool_demo");
        let verified = youtube_content_supported
            && demo
                .and_then(|item| item.get("verified"))
                .and_then(Value::as_bool)
                == Some(true)
            && demo
                .and_then(|item| item.get("channel_id"))
                .and_then(Value::as_str)
                == Some(account_id)
            && demo
                .and_then(|item| item.get("source_url"))
                .and_then(Value::as_str)
                .is_some_and(|url| is_youtube_video(url) && source_in_list(sources, url));
        if !verified {
            reasons.push("own_ai_tool_demo_unverified");
        }
    }
    if criteria
        .get("requires_public_tool_url")
        .and_then(Value::as_bool)
        != Some(false)
    {
        let tool = evidence.get("public_tool_url");
        let verified = tool
            .and_then(|item| item.get("verified"))
            .and_then(Value::as_bool)
            == Some(true)
            && tool
                .and_then(|item| item.get("tool_name"))
                .and_then(Value::as_str)
                .is_some_and(|name| !name.trim().is_empty())
            && tool
                .and_then(|item| item.get("url"))
                .and_then(Value::as_str)
                .is_some_and(is_public_https_url)
            && tool
                .and_then(|item| item.get("source_url"))
                .and_then(Value::as_str)
                .is_some_and(|url| source_in_list(sources, url));
        if !verified {
            reasons.push("public_tool_url_unverified");
        }
    }
    // A search title, profile locale or provider-supplied `verified` label
    // never certifies a Campaign's reviewed conditions.
    if criteria
        .get("region")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        let residence = &evidence["reviewed_residence"];
        let supported = criteria["region"] == "中国大陆以外"
            && residence["verified"] == true
            && residence["native_author_id"].as_str() == Some(account_id)
            && residence["country_code"].as_str().is_some_and(|code| {
                code.len() == 2
                    && code != "CN"
                    && code.bytes().all(|byte| byte.is_ascii_uppercase())
            })
            && residence["source_url"]
                .as_str()
                .is_some_and(|url| source_in_list(sources, url))
            && residence["corroboration_url"]
                .as_str()
                .is_some_and(is_public_https_url)
            && residence["review_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && matches!(
                residence["policy_version"].as_str(),
                Some("apparel-residence-review-v1" | "apparel-native-review-v2")
            );
        if !supported {
            reasons.push("region_unverified");
        }
    }
    let mut language_conflict = false;
    if required_language(criteria).is_some() {
        let proof = &evidence["reviewed_language"];
        let bound = proof["verified"] == true
            && proof["native_author_id"].as_str() == Some(account_id)
            && proof["language"] == criteria["language"]
            && proof["policy_version"] == "language-match-review-v1"
            && proof["review_id"].as_str().is_some_and(|id| !id.is_empty())
            && proof["source_url"]
                .as_str()
                .is_some_and(|url| source_in_list(sources, url));
        language_conflict = bound && proof["outcome"] == "contradicted";
        if language_conflict {
            reasons.push("language_mismatch");
        } else if !(bound && proof["outcome"] == "supported") {
            reasons.push("language_unverified");
        }
    }
    if criteria
        .get("category")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        let category = criteria["category"].as_str().unwrap_or_default();
        let content = if category == "服装" {
            &evidence["reviewed_apparel_content"]
        } else {
            &evidence["reviewed_category_content"]
        };
        let supported = content["verified"] == true
            && content["native_author_id"].as_str() == Some(account_id)
            && (category == "服装" || content["category"].as_str() == Some(category))
            && content["source_url"]
                .as_str()
                .is_some_and(|url| source_in_list(sources, url))
            && content["review_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
            && if category == "服装" {
                matches!(
                    content["policy_version"].as_str(),
                    Some("apparel-residence-review-v1" | "apparel-native-review-v2")
                )
            } else {
                content["policy_version"].as_str() == Some("category-content-review-v1")
            };
        if !supported {
            reasons.push("category_unverified");
        }
    }
    // Custom hard constraints cannot silently disappear from qualification.
    if criteria.as_object().is_some_and(|object| {
        object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "min_subscribers"
                    | "min_followers"
                    | "requires_ai_tool_demo"
                    | "requires_public_tool_url"
                    | "region"
                    | "language"
                    | "category"
            )
        })
    }) {
        reasons.push("unsupported_criterion");
    }
    let verdict = if below_minimum || language_conflict {
        "rejected"
    } else if reasons.is_empty() {
        "qualified"
    } else {
        "pending"
    };
    Assessment {
        verdict,
        reasons,
        subscribers: subscriber_estimate.map(|estimate| estimate.nominal),
    }
}

pub(crate) fn assessed_evidence(mut evidence: Value, assessment: &Assessment) -> Value {
    let fit = &evidence["reviewed_match"];
    let rating = if (assessment.verdict == "qualified"
        || (assessment.verdict == "rejected"
            && fit["policy_version"] == "initial-match-evaluation-v1"
            && fit["level"] == "low"))
        && fit["verified"] == true
        && matches!(
            fit["policy_version"].as_str(),
            Some("language-match-review-v1" | "initial-match-evaluation-v1")
        )
        && fit["outcome"] == "supported"
        && fit["reason"]
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty() && reason.len() <= 4000)
        && fit["source_url"]
            .as_str()
            .is_some_and(|source| source_in_list(evidence["source_urls"].as_array(), source))
        && matches!(
            fit["level"].as_str(),
            Some("excellent" | "good" | "medium" | "low")
        ) {
        json!({"level":fit["level"], "reason":fit["reason"], "source_url":fit["source_url"]})
    } else {
        Value::Null
    };
    if let Some(object) = evidence.as_object_mut() {
        object.insert(
            "assessment".into(),
            json!({
                "reasons": assessment.reasons,
                "subscribers_normalized": assessment.subscribers,
                "match_rating": rating,
            }),
        );
    }
    evidence
}

/// A second video from the same channel must add evidence, not erase the first
/// video's sources or a reviewed proof attached to them.
pub(crate) fn merge_pending_evidence(mut current: Value, previous: &Value) -> Value {
    let Some(current_object) = current.as_object_mut() else {
        return current;
    };
    let Some(previous_object) = previous.as_object() else {
        return current;
    };
    for key in ["source_urls", "public_links"] {
        let Some(current_values) = current_object.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        if let Some(previous_values) = previous_object.get(key).and_then(Value::as_array) {
            for value in previous_values {
                if current_values.len() == 20 {
                    break;
                }
                if value.is_string() && !current_values.contains(value) {
                    current_values.push(value.clone());
                }
            }
        }
    }
    // Only legacy observations are copied here. Reviewed apparel and residence
    // proof cannot be inherited by a generic candidate update without checking
    // its audit receipt; a later review may restore it from current material.
    for key in [
        "own_ai_tool_demo",
        "public_tool_url",
        "verified_region",
        "verified_language",
    ] {
        if !current_object.contains_key(key)
            && let Some(value) = previous_object.get(key)
        {
            current_object.insert(key.into(), value.clone());
        }
    }
    if current_object
        .get("subscribers_raw")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        for key in ["subscribers_raw", "subscribers_source_url", "captured_at"] {
            if let Some(value) = previous_object.get(key) {
                current_object.insert(key.into(), value.clone());
            }
        }
    }
    // Keep sources referenced by retained proof even when the display summary
    // reaches its cap. A copied URL is eligible only if it was actually present
    // in one of the source lists; a proof field cannot invent provenance.
    let mut sources = Vec::new();
    if let Some(latest) = current_object
        .get("source_urls")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
    {
        sources.push(latest.clone());
    }
    let retained_sources = [
        current_object.get("subscribers_source_url"),
        current_object
            .get("own_ai_tool_demo")
            .and_then(|proof| proof.get("source_url")),
        current_object
            .get("public_tool_url")
            .and_then(|proof| proof.get("source_url")),
    ];
    for source in retained_sources.into_iter().flatten() {
        let observed = [
            current_object.get("source_urls"),
            previous_object.get("source_urls"),
        ]
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .any(|values| values.contains(source));
        if source.is_string() && observed && !sources.contains(source) {
            sources.push(source.clone());
        }
    }
    if let Some(values) = current_object.get("source_urls").and_then(Value::as_array) {
        for source in values {
            if sources.len() >= 20 {
                break;
            }
            if !sources.contains(source) {
                sources.push(source.clone());
            }
        }
        current_object.insert("source_urls".into(), Value::Array(sources));
    }
    current
}

fn source_in_list(sources: Option<&Vec<Value>>, url: &str) -> bool {
    sources.is_some_and(|sources| sources.iter().any(|value| value.as_str() == Some(url)))
}

fn is_public_https_url(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some())
}

fn is_youtube_video(value: &str) -> bool {
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && matches!(url.host_str(), Some("youtube.com" | "www.youtube.com"))
            && (url.path() == "/watch" || url.path().starts_with("/shorts/"))
    })
}

/// Approximate, hidden, or malformed counts are unknown rather than rounded up.
pub(crate) fn subscriber_display(raw: &str) -> Option<String> {
    let count = parse_subscribers(raw)?;
    Some(if count.lower == count.upper { count.nominal.to_string() } else { format!("~{}", count.nominal) })
}

fn parse_subscribers(raw: &str) -> Option<SubscriberEstimate> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.contains(['~', '<', '>', '+'])
        || raw.contains("约")
        || raw.to_ascii_lowercase().contains("about")
    {
        return None;
    }
    let start = raw.find(|ch: char| ch.is_ascii_digit())?;
    let number = &raw[start..];
    let end = number
        .find(|ch: char| !(ch.is_ascii_digit() || ch == ',' || ch == '.'))
        .unwrap_or(number.len());
    let number = &number[..end];
    let suffix = raw[start + end..].trim_start();
    let multiplier = if suffix.starts_with('万') {
        10_000
    } else if suffix.starts_with('亿') {
        100_000_000
    } else if suffix.starts_with(['K', 'k', '千']) {
        1_000
    } else if suffix.starts_with(['M', 'm']) {
        1_000_000
    } else if suffix.starts_with(['B', 'b']) {
        1_000_000_000
    } else {
        1
    };
    let normalized = number.replace(',', "");
    let (whole, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 6
        || (multiplier == 1 && !fraction.is_empty())
    {
        return None;
    }
    let whole: u64 = whole.parse().ok()?;
    let factor = 10_u64.checked_pow(fraction.len() as u32)?;
    let fraction: u64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    let nominal = whole
        .checked_mul(multiplier)?
        .checked_add(fraction.checked_mul(multiplier)? / factor)?;
    let resolution = if multiplier == 1 {
        0
    } else {
        multiplier / factor
    };
    let margin = resolution.saturating_add(1) / 2;
    Some(SubscriberEstimate {
        nominal,
        lower: nominal.saturating_sub(margin),
        upper: nominal.saturating_add(margin),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn unrestricted_language_is_not_a_gate_and_unknown_language_never_publishes_fit() {
        let mut criteria = json!({"min_followers":8000,"requires_ai_tool_demo":false,"requires_public_tool_url":false});
        for language in ["", "不限", "语言不限", "any", " Unrestricted "] {
            criteria["language"] = json!(language);
            assert_eq!(
                assess_profile(&criteria, &json!({}), "author", 9000).verdict,
                "qualified"
            );
        }
        criteria["language"] = json!("English");
        let evidence = json!({"source_urls":["https://example.com/post"],"reviewed_match":{
            "verified":true,"level":"excellent","outcome":"supported","reason":"Direct product relevance",
            "policy_version":"language-match-review-v1","source_url":"https://example.com/post"}});
        let assessment = assess_profile(&criteria, &evidence, "author", 9000);
        assert_eq!(assessment.verdict, "pending");
        assert!(assessed_evidence(evidence, &assessment)["assessment"]["match_rating"].is_null());
    }
    use super::*;

    #[test]
    fn native_profile_count_does_not_replace_content_or_customer_constraints() {
        let evidence = json!({"source_urls":["https://www.youtube.com/watch?v=LR04bU_yV5k"],
            "own_ai_tool_demo":{"verified":true,"channel_id":"123","source_url":"https://www.youtube.com/watch?v=LR04bU_yV5k"}});
        let strict = assess_profile(
            &json!({"min_followers":10000,"region":"US","language":"English","category":"beauty"}),
            &evidence,
            "123",
            10000,
        );
        assert_eq!(strict.verdict, "pending");
        assert_eq!(
            strict.reasons,
            vec![
                "own_ai_tool_demo_unverified",
                "public_tool_url_unverified",
                "region_unverified",
                "language_unverified",
                "category_unverified"
            ]
        );
        // A Campaign explicitly limited to exact follower count is a different,
        // legitimate requirement; do not add the AI-demo requirement silently.
        let count_only = json!({"min_followers":10000,"requires_ai_tool_demo":false,"requires_public_tool_url":false});
        assert_eq!(
            assess_profile(&count_only, &json!({}), "123", 10000).verdict,
            "qualified"
        );
        assert_eq!(
            assess_profile(&count_only, &json!({}), "123", 9999).verdict,
            "rejected"
        );
    }

    #[test]
    fn shoe_category_needs_bound_reviewed_content_before_qualification() {
        let criteria = json!({"min_subscribers":10000,"category":"鞋子",
            "requires_ai_tool_demo":false,"requires_public_tool_url":false});
        let source = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let mut evidence = json!({"source_urls":[source]});
        let pending = assess_profile(&criteria, &evidence, "channel-1", 15000);
        assert_eq!(pending.verdict, "pending");
        assert_eq!(pending.reasons, vec!["category_unverified"]);
        evidence["reviewed_category_content"] = json!({"verified":true,
            "native_author_id":"channel-1","category":"鞋子","source_url":source,
            "review_id":"review-1","policy_version":"category-content-review-v1"});
        assert_eq!(
            assess_profile(&criteria, &evidence, "channel-1", 15000).verdict,
            "qualified"
        );
        evidence["reviewed_category_content"]["category"] = json!("服装");
        assert_eq!(
            assess_profile(&criteria, &evidence, "channel-1", 15000).verdict,
            "pending"
        );
        evidence["reviewed_category_content"]["category"] = json!("鞋子");
        assert_eq!(
            assess_profile(&criteria, &evidence, "other-channel", 15000).verdict,
            "pending"
        );
        assert_eq!(
            assess_profile(&criteria, &evidence, "channel-1", 9999).verdict,
            "rejected"
        );
    }

    #[test]
    fn subscriber_counts_are_conservative() {
        assert_eq!(parse_subscribers("1.16万位订阅者").unwrap().lower, 11_550);
        assert_eq!(
            parse_subscribers("12.4K subscribers").unwrap().lower,
            12_350
        );
        assert_eq!(
            parse_subscribers("1,234 subscribers").unwrap().nominal,
            1_234
        );
        assert_eq!(parse_subscribers("10K subscribers").unwrap().lower, 9_500);
        assert_eq!(parse_subscribers("约1万位订阅者"), None);
        assert_eq!(parse_subscribers("hidden"), None);
    }

    #[test]
    fn missing_proof_is_pending_and_hard_failure_is_unqualified() {
        let criteria = json!({"min_subscribers": 10000, "requires_ai_tool_demo": true, "requires_public_tool_url": true});
        let mut evidence = json!({"source_urls": ["https://www.youtube.com/watch?v=LR04bU_yV5k"],
            "subscribers_raw": "1.16万位订阅者"});
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").verdict,
            "pending"
        );
        evidence["subscribers_source_url"] = json!("https://www.youtube.com/watch?v=LR04bU_yV5k");
        evidence["captured_at"] = json!(1);
        evidence["subscribers_raw"] = json!("9.9K subscribers");
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").verdict,
            "rejected"
        );
    }

    #[test]
    fn rounded_threshold_is_pending_not_qualified_or_rejected() {
        let video = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let criteria = json!({"min_subscribers": 10000,
            "requires_ai_tool_demo": false, "requires_public_tool_url": false});
        let evidence = json!({"source_urls": [video], "subscribers_raw": "10K subscribers",
            "subscribers_source_url": video, "captured_at": 1});
        let result = assess(&criteria, &evidence, "UC1234567890");
        assert_eq!(result.verdict, "pending");
        assert_eq!(result.reasons, vec!["subscriber_count_near_threshold"]);
    }

    #[test]
    fn all_frozen_hard_constraints_need_sourced_proof() {
        let video = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let criteria = json!({"min_subscribers": 10000, "requires_ai_tool_demo": true,
            "requires_public_tool_url": true, "region": "US", "language": "English"});
        let mut evidence = json!({"source_urls": [video], "subscribers_raw": "12.4K subscribers",
            "subscribers_source_url": video, "captured_at": 1,
            "own_ai_tool_demo": {"verified": true, "channel_id": "UC1234567890", "source_url": video},
            "public_tool_url": {"verified": true, "tool_name": "Example Tool", "url": "https://example.com/tool", "source_url": video}});
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").verdict,
            "pending"
        );
        // A matching free-form label cannot establish the hard constraint.
        evidence["verified_region"] = json!("US");
        evidence["verified_language"] = json!("English");
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").verdict,
            "pending"
        );
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").reasons,
            vec!["region_unverified", "language_unverified"]
        );
        evidence["own_ai_tool_demo"]["channel_id"] = json!("UCwrongchannel");
        assert_eq!(
            assess(&criteria, &evidence, "UC1234567890").verdict,
            "pending"
        );
    }

    #[test]
    fn second_video_keeps_first_video_proof() {
        let first = "https://www.youtube.com/watch?v=LR04bU_yV5k";
        let second = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        let old = json!({"source_urls": [first], "public_links": ["https://example.com/tool"],
            "own_ai_tool_demo": {"source_url": first, "verified": true},
            "reviewed_residence": {"verified": true}, "reviewed_apparel_content": {"verified": true},
            "reviewed_category_content": {"verified": true}});
        let merged =
            merge_pending_evidence(json!({"source_urls": [second], "public_links": []}), &old);
        assert_eq!(merged["source_urls"], json!([second, first]));
        assert_eq!(merged["public_links"], json!(["https://example.com/tool"]));
        assert_eq!(merged["own_ai_tool_demo"]["source_url"], first);
        assert!(merged.get("reviewed_residence").is_none());
        assert!(merged.get("reviewed_apparel_content").is_none());
        assert!(merged.get("reviewed_category_content").is_none());
    }

    #[test]
    fn bounded_summary_retains_proof_sources_without_inventing_them() {
        let first = "https://www.youtube.com/watch?v=00000000000";
        let mut evidence = json!({"source_urls": [first], "subscribers_raw": "12.4K subscribers",
            "subscribers_source_url": first, "captured_at": 1,
            "own_ai_tool_demo": {"verified": true, "channel_id": "UC1234567890", "source_url": first},
            "public_tool_url": {"verified": true, "tool_name": "Tool", "url": "https://example.com", "source_url": first}});
        for n in 1..30 {
            evidence = merge_pending_evidence(
                json!({"source_urls": [format!("https://www.youtube.com/watch?v={n:011}")]}),
                &evidence,
            );
            assert_eq!(
                assess(&json!({"min_followers": 10000}), &evidence, "UC1234567890").verdict,
                "qualified"
            );
        }
        assert_eq!(evidence["source_urls"].as_array().unwrap().len(), 20);
        evidence["own_ai_tool_demo"]["source_url"] =
            json!("https://www.youtube.com/watch?v=unseen00000");
        let merged = merge_pending_evidence(json!({"source_urls": [first]}), &evidence);
        assert!(
            !merged["source_urls"]
                .as_array()
                .unwrap()
                .contains(&json!("https://www.youtube.com/watch?v=unseen00000"))
        );
        assert_eq!(
            assess(&json!({}), &merged, "UC1234567890").verdict,
            "pending"
        );
    }
}
