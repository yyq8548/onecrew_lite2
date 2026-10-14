//! Flat model transport for quoted constraints. Domain validation stays unchanged.
use onecrew_creator_runtime::{
    DiscoveryIntentProposal, ProposedConstraint, QuotedHardCondition, QuotedPlatform,
    QuotedPreference,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::super::{CodexDraftPlan, DiscoveryDraftError, DraftScope};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum State {
    Specified,
    Unrestricted,
    Unspecified,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Constraint<T> {
    state: State,
    value: Option<T>,
    excerpt: Option<String>,
}
impl<T> Constraint<T> {
    fn into_domain(self) -> Result<ProposedConstraint<T>, DiscoveryDraftError> {
        match (self.state, self.value, self.excerpt) {
            (State::Specified, Some(value), Some(excerpt)) => {
                Ok(ProposedConstraint::Specified { value, excerpt })
            }
            (State::Unrestricted, None, Some(excerpt)) => {
                Ok(ProposedConstraint::Unrestricted { excerpt })
            }
            (State::Unspecified, None, None) => Ok(ProposedConstraint::Unspecified),
            _ => Err(DiscoveryDraftError::InvalidPlan),
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Intent {
    original_request: String,
    goal: String,
    category: Constraint<String>,
    region: Constraint<String>,
    language: Constraint<String>,
    min_subscribers: Constraint<u64>,
    excluded_platforms: Vec<QuotedPlatform>,
    other_hard_conditions: Vec<QuotedHardCondition>,
    soft_preferences: Vec<QuotedPreference>,
    suggested_keywords: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct Plan {
    target_count: Constraint<u64>,
    max_calls: Constraint<u64>,
    intent: Intent,
    scopes: Vec<DraftScope>,
    #[schemars(length(min = 3, max = 6))]
    contact_search_terms: Vec<String>,
}
impl Plan {
    pub(super) fn into_domain(self) -> Result<CodexDraftPlan, DiscoveryDraftError> {
        if !(3..=6).contains(&self.contact_search_terms.len()) {
            return Err(DiscoveryDraftError::InvalidPlan);
        }
        let intent = self.intent;
        Ok(CodexDraftPlan {
            table_subject: String::new(),
            target_count: self.target_count.into_domain()?,
            max_calls: self.max_calls.into_domain()?,
            intent: DiscoveryIntentProposal {
                original_request: intent.original_request,
                goal: intent.goal,
                category: intent.category.into_domain()?,
                region: intent.region.into_domain()?,
                language: intent.language.into_domain()?,
                min_subscribers: intent.min_subscribers.into_domain()?,
                excluded_platforms: intent.excluded_platforms,
                other_hard_conditions: intent.other_hard_conditions,
                soft_preferences: intent.soft_preferences,
                suggested_keywords: intent.suggested_keywords,
            },
            scopes: self.scopes,
            contact_search_terms: self.contact_search_terms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_constraints_preserve_quotes_and_reject_inconsistent_states() {
        let language: Constraint<String> = serde_json::from_str(
            r#"{"state":"specified","value":"中文","excerpt":"Content language: 中文"}"#,
        )
        .unwrap();
        assert_eq!(
            language.into_domain().unwrap(),
            ProposedConstraint::Specified {
                value: "中文".into(),
                excerpt: "Content language: 中文".into()
            }
        );
        let threshold: Constraint<u64> =
            serde_json::from_str(r#"{"state":"specified","value":8765,"excerpt":"粉丝至少 8765"}"#)
                .unwrap();
        assert_eq!(
            threshold.into_domain().unwrap(),
            ProposedConstraint::Specified {
                value: 8765,
                excerpt: "粉丝至少 8765".into()
            }
        );
        for input in [
            r#"{"state":"specified","value":null,"excerpt":"中文"}"#,
            r#"{"state":"unrestricted","value":"中文","excerpt":"语言不限"}"#,
            r#"{"state":"unspecified","value":null,"excerpt":"中文"}"#,
        ] {
            let constraint: Constraint<String> = serde_json::from_str(input).unwrap();
            assert_eq!(
                constraint.into_domain(),
                Err(DiscoveryDraftError::InvalidPlan)
            );
        }
        for (input, expected) in [
            (
                r#"{"state":"unspecified","value":null,"excerpt":null}"#,
                ProposedConstraint::Unspecified,
            ),
            (
                r#"{"state":"unrestricted","value":null,"excerpt":"语言不限"}"#,
                ProposedConstraint::Unrestricted {
                    excerpt: "语言不限".into(),
                },
            ),
        ] {
            let constraint: Constraint<String> = serde_json::from_str(input).unwrap();
            assert_eq!(constraint.into_domain().unwrap(), expected);
        }
    }
}
