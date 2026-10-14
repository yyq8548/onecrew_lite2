//! A model may suggest a Campaign, but cannot authorize or execute one.
//! These checks preserve the customer's exact request and expose unsupported
//! hard conditions before any draft is created.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedConstraint<T> {
    Specified { value: T, excerpt: String },
    Unrestricted { excerpt: String },
    Unspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuotedPlatform {
    pub platform: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuotedPreference {
    pub description: String,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuotedHardCondition {
    pub description: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryIntentProposal {
    pub original_request: String,
    pub goal: String,
    pub category: ProposedConstraint<String>,
    pub region: ProposedConstraint<String>,
    pub language: ProposedConstraint<String>,
    pub min_subscribers: ProposedConstraint<u64>,
    pub excluded_platforms: Vec<QuotedPlatform>,
    /// Any hard condition that the current Campaign owner cannot enforce
    /// blocks conversion to a draft until a supported mapping is available.
    pub other_hard_conditions: Vec<QuotedHardCondition>,
    pub soft_preferences: Vec<QuotedPreference>,
    pub suggested_keywords: Vec<String>,
}

/// Read-only proposal check. This does not create or approve a Campaign.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ValidateDiscoveryIntentInput {
    pub request: String,
    pub proposal: DiscoveryIntentProposal,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIntentValidationResult {
    pub valid: bool,
    pub issues: Vec<DiscoveryIntentIssue>,
    pub registered_source_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryIntentIssue {
    pub field: String,
    pub code: String,
}

fn issue(issues: &mut Vec<DiscoveryIntentIssue>, field: &str, code: &str) {
    issues.push(DiscoveryIntentIssue {
        field: field.into(),
        code: code.into(),
    });
}

fn excerpt_valid(request: &str, excerpt: &str) -> bool {
    !excerpt.trim().is_empty() && request.contains(excerpt)
}

fn validate_text_constraint(
    request: &str,
    field: &str,
    constraint: &ProposedConstraint<String>,
    issues: &mut Vec<DiscoveryIntentIssue>,
) {
    match constraint {
        ProposedConstraint::Specified { value, excerpt } => {
            if value.trim().is_empty() || value.len() > 160 {
                issue(issues, field, "invalid_value");
            }
            if !excerpt_valid(request, excerpt) {
                issue(issues, field, "excerpt_not_in_request");
            }
        }
        ProposedConstraint::Unrestricted { excerpt } => {
            if !excerpt_valid(request, excerpt) {
                issue(issues, field, "excerpt_not_in_request");
            }
        }
        ProposedConstraint::Unspecified => {}
    }
}

fn numbers_in_excerpt(excerpt: &str) -> Vec<u64> {
    let mut values = Vec::new();
    let mut digits = String::new();
    for character in excerpt.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit() {
            digits.push(character);
        } else if character == ',' && !digits.is_empty() {
            continue;
        } else if !digits.is_empty() {
            if let Ok(value) = digits.parse::<u64>() {
                let multiplier = match character {
                    '万' => 10_000,
                    '千' | 'k' | 'K' => 1_000,
                    _ => 1,
                };
                if let Some(normalized) = value.checked_mul(multiplier) {
                    values.push(normalized);
                }
            }
            digits.clear();
        }
    }
    values
}

/// Validation is a pre-review guard, never permission to approve a Campaign.
/// A semantically wrong but correctly quoted interpretation still needs the
/// customer's review before the existing Creator owner freezes hard criteria.
pub fn validate_discovery_intent_proposal(
    request: &str,
    proposal: &DiscoveryIntentProposal,
    registered_platforms: &[String],
) -> Vec<DiscoveryIntentIssue> {
    let mut issues = Vec::new();
    if request.trim().is_empty() || request.len() > 4_000 || proposal.original_request != request {
        issue(&mut issues, "original_request", "request_mismatch");
    }
    if proposal.excluded_platforms.len() > 64 {
        issue(&mut issues, "excluded_platforms", "too_many_values");
    }
    if proposal.other_hard_conditions.len() > 32 {
        issue(&mut issues, "other_hard_conditions", "too_many_values");
    }
    if proposal.soft_preferences.len() > 64 {
        issue(&mut issues, "soft_preferences", "too_many_values");
    }
    if proposal.goal.trim().is_empty() || proposal.goal.len() > 400 {
        issue(&mut issues, "goal", "invalid_value");
    }
    for (field, constraint) in [
        ("category", &proposal.category),
        ("region", &proposal.region),
        ("language", &proposal.language),
    ] {
        validate_text_constraint(request, field, constraint, &mut issues);
    }
    match &proposal.min_subscribers {
        ProposedConstraint::Specified { value, excerpt } => {
            if *value > 1_000_000_000 {
                issue(&mut issues, "min_subscribers", "invalid_value");
            }
            if !excerpt_valid(request, excerpt) {
                issue(&mut issues, "min_subscribers", "excerpt_not_in_request");
            } else if !numbers_in_excerpt(excerpt).contains(value) {
                issue(&mut issues, "min_subscribers", "number_not_in_excerpt");
            }
        }
        ProposedConstraint::Unrestricted { excerpt } => {
            if !excerpt_valid(request, excerpt) {
                issue(&mut issues, "min_subscribers", "excerpt_not_in_request");
            }
        }
        ProposedConstraint::Unspecified => {}
    }
    let registered: HashSet<&str> = registered_platforms.iter().map(String::as_str).collect();
    let mut excluded = HashSet::new();
    for platform in &proposal.excluded_platforms {
        if !registered.contains(platform.platform.as_str()) || !excluded.insert(&platform.platform)
        {
            issue(&mut issues, "excluded_platforms", "invalid_platform");
        }
        if !excerpt_valid(request, &platform.excerpt) {
            issue(&mut issues, "excluded_platforms", "excerpt_not_in_request");
        }
    }
    for condition in &proposal.other_hard_conditions {
        if !excerpt_valid(request, &condition.excerpt) || condition.description.trim().is_empty() {
            issue(&mut issues, "other_hard_conditions", "invalid_condition");
        } else {
            issue(
                &mut issues,
                "other_hard_conditions",
                "unsupported_hard_condition",
            );
        }
    }
    for preference in &proposal.soft_preferences {
        if preference.description.trim().is_empty() || preference.description.len() > 300 {
            issue(&mut issues, "soft_preferences", "invalid_value");
        }
        if preference
            .excerpt
            .as_ref()
            .is_some_and(|excerpt| !excerpt_valid(request, excerpt))
        {
            issue(&mut issues, "soft_preferences", "excerpt_not_in_request");
        }
    }
    if proposal.suggested_keywords.is_empty() || proposal.suggested_keywords.len() > 20 {
        issue(&mut issues, "suggested_keywords", "invalid_count");
    }
    let mut seen_keywords = HashSet::new();
    for keyword in &proposal.suggested_keywords {
        if keyword.trim().is_empty()
            || keyword.len() > 160
            || keyword.chars().any(char::is_control)
            || !seen_keywords.insert(keyword.trim().to_lowercase())
        {
            issue(&mut issues, "suggested_keywords", "invalid_keyword");
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shoe_proposal(request: &str) -> DiscoveryIntentProposal {
        DiscoveryIntentProposal {
            original_request: request.into(),
            goal: "寻找鞋类创作者".into(),
            category: ProposedConstraint::Specified {
                value: "鞋子".into(),
                excerpt: "鞋子".into(),
            },
            region: ProposedConstraint::Unrestricted {
                excerpt: "所在地不限".into(),
            },
            language: ProposedConstraint::Unrestricted {
                excerpt: "语言不限".into(),
            },
            min_subscribers: ProposedConstraint::Specified {
                value: 10_000,
                excerpt: "至少1万粉丝".into(),
            },
            excluded_platforms: vec![],
            other_hard_conditions: vec![],
            soft_preferences: vec![],
            suggested_keywords: vec!["鞋子测评".into(), "sneaker review".into()],
        }
    }

    #[test]
    fn accepts_quoted_shoe_requirements_without_adding_defaults() {
        let request = "找鞋子创作者，至少1万粉丝，语言不限，所在地不限";
        let proposal = shoe_proposal(request);
        assert!(validate_discovery_intent_proposal(request, &proposal, &[]).is_empty());
        let mut absent = proposal;
        absent.region = ProposedConstraint::Unspecified;
        assert!(validate_discovery_intent_proposal(request, &absent, &[]).is_empty());
        assert_eq!(absent.region, ProposedConstraint::Unspecified);
    }

    #[test]
    fn rejects_unquoted_or_changed_hard_requirements() {
        let request = "找鞋子创作者，至少5千粉丝，语言不限，所在地不限";
        let mut proposal = shoe_proposal(request);
        proposal.min_subscribers = ProposedConstraint::Specified {
            value: 10_000,
            excerpt: "至少5千粉丝".into(),
        };
        proposal.region = ProposedConstraint::Specified {
            value: "美国".into(),
            excerpt: "美国".into(),
        };
        let issues = validate_discovery_intent_proposal(request, &proposal, &[]);
        assert!(
            issues
                .iter()
                .any(|item| item.code == "number_not_in_excerpt")
        );
        assert!(
            issues
                .iter()
                .any(|item| item.field == "region" && item.code == "excerpt_not_in_request")
        );
    }

    #[test]
    fn blocks_unsupported_hard_conditions_and_unregistered_exclusions() {
        let request = "找鞋子创作者，至少1万粉丝，语言不限，所在地不限，排除已有合作对象和抖音";
        let mut proposal = shoe_proposal(request);
        proposal.excluded_platforms = vec![QuotedPlatform {
            platform: "douyin".into(),
            excerpt: "抖音".into(),
        }];
        proposal.other_hard_conditions = vec![QuotedHardCondition {
            description: "排除已有合作对象".into(),
            excerpt: "排除已有合作对象".into(),
        }];
        let issues = validate_discovery_intent_proposal(request, &proposal, &["youtube".into()]);
        assert!(issues.iter().any(|item| item.code == "invalid_platform"));
        assert!(
            issues
                .iter()
                .any(|item| item.code == "unsupported_hard_condition")
        );
    }
}
