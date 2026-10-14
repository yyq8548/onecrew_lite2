use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Only an unresolved language requirement may be waived by this entry point.
pub(crate) fn language_only_pending(
    criteria: &serde_json::Value,
    evidence: &serde_json::Value,
    author: &str,
    verdict: &str,
) -> bool {
    use serde_json::Value;
    let Some(language) = crate::discovery_qualification::required_language(criteria) else {
        return false;
    };
    if verdict != "pending" {
        return false;
    }
    let bound = |proof: &Value| {
        proof["verified"] == true
            && proof["native_author_id"].as_str() == Some(author)
            && proof["review_id"].as_str().is_some_and(|id| !id.is_empty())
            && evidence["source_urls"]
                .as_array()
                .is_some_and(|urls| urls.iter().any(|url| url == &proof["source_url"]))
    };
    let language_proof = &evidence["reviewed_language"];
    if bound(language_proof)
        && language_proof["language"].as_str() == Some(language)
        && matches!(
            language_proof["outcome"].as_str(),
            Some("contradicted" | "supported")
        )
    {
        return false;
    }
    let minimum = criteria["min_subscribers"]
        .as_u64()
        .or_else(|| criteria["min_followers"].as_u64())
        .unwrap_or(0);
    if minimum > 0
        && !evidence["assessment"]["subscribers_normalized"]
            .as_u64()
            .is_some_and(|count| count >= minimum)
    {
        return false;
    }
    let Some(reasons) = evidence["assessment"]["reasons"].as_array() else {
        return false;
    };
    let matched = &evidence["reviewed_match"];
    if bound(matched) && matched["policy_version"] == "initial-match-evaluation-v1" {
        return matched["outcome"] == "insufficient"
            && matched["language_only_pending"] == true
            && reasons.iter().all(|reason| {
                matches!(
                    reason.as_str(),
                    Some("language_unverified" | "initial_match_unrated")
                )
            });
    }
    reasons.len() == 1
        && reasons[0] == "language_unverified"
        && criteria["requires_ai_tool_demo"] != true
        && criteria["requires_public_tool_url"] != true
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApproveDiscoveryCandidateInput {
    pub campaign_id: String,
    pub expected_revision: u32,
    pub candidate_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ListDiscoveryCandidatesInput;

    #[test]
    fn manual_language_policy_excludes_other_gaps_and_explicit_language_results() {
        use serde_json::json;
        let criteria = json!({"min_followers":8000,"language":"英文","category":"鞋子"});
        let mut evidence = json!({"source_urls":["https://example.com/post"],
            "assessment":{"subscribers_normalized":10000,"reasons":["initial_match_unrated"]},
            "reviewed_match":{"verified":true,"native_author_id":"author","review_id":"review",
                "source_url":"https://example.com/post","policy_version":"initial-match-evaluation-v1",
                "outcome":"insufficient","language_only_pending":true}});
        assert!(language_only_pending(
            &criteria, &evidence, "author", "pending"
        ));
        assert!(!language_only_pending(
            &criteria, &evidence, "author", "rejected"
        ));
        for reason in [
            "category_unverified",
            "region_unverified",
            "language_mismatch",
            "subscriber_count_near_threshold",
            "own_ai_tool_demo_unverified",
        ] {
            evidence["assessment"]["reasons"] = json!(["initial_match_unrated", reason]);
            assert!(
                !language_only_pending(&criteria, &evidence, "author", "pending"),
                "{reason}"
            );
        }
        evidence["assessment"]["reasons"] = json!(["initial_match_unrated"]);
        evidence["assessment"]["subscribers_normalized"] = json!(7000);
        assert!(!language_only_pending(
            &criteria, &evidence, "author", "pending"
        ));
        evidence["assessment"]["subscribers_normalized"] = json!(10000);
        for outcome in ["contradicted", "supported"] {
            evidence["reviewed_language"] = json!({"verified":true,"native_author_id":"author",
                "review_id":"language-review","source_url":"https://example.com/post",
                "language":"英文","outcome":outcome});
            // Reject even if an older initial-match assessment still says pending.
            assert!(!language_only_pending(
                &criteria, &evidence, "author", "pending"
            ));
        }
    }

    #[test]
    fn approval_endpoint_rejects_bound_candidates_with_other_unverified_requirements() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, _) = crate::discovery_review::tests::setup(dir.path());
        let candidate = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "review".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .remove(0);
        assert_eq!(
            owner
                .approve_discovery_candidate(ApproveDiscoveryCandidateInput {
                    campaign_id: "review".into(),
                    expected_revision: 1,
                    candidate_id: candidate.id,
                })
                .unwrap_err(),
            "manual_approval_retired_for_contact_recommendations"
        );
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM discovery_manual_approvals", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn human_approval_cannot_claim_table_inclusion_without_native_source_binding() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, _) = crate::discovery_review::tests::setup(dir.path());
        let candidate = owner
            .list_discovery_candidates(ListDiscoveryCandidatesInput {
                campaign_id: "review".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .remove(0);
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        db.execute("DELETE FROM discovery_video_identity_bindings", [])
            .unwrap();
        assert_eq!(
            owner
                .approve_discovery_candidate(ApproveDiscoveryCandidateInput {
                    campaign_id: "review".into(),
                    expected_revision: 1,
                    candidate_id: candidate.id,
                })
                .unwrap_err(),
            "manual_approval_source_binding_missing"
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM discovery_manual_approvals", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn human_approval_is_idempotent_and_preserves_automatic_evidence_and_budget() {
        let dir = tempfile::tempdir().unwrap();
        let (owner, _) = crate::discovery_review::tests::setup_with_options(
            dir.path(),
            serde_json::json!({"min_followers":10000,"language":"中文",
                "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            "UC1234567890",
            4,
        );
        crate::discovery_contacts::legacy_approval_fixture(dir.path());
        let page = ListDiscoveryCandidatesInput {
            campaign_id: "review".into(),
            after_id: None,
            limit: 10,
        };
        let before = owner.list_discovery_candidates(page.clone()).unwrap();
        let campaign = owner.get_discovery_campaign("review").unwrap();
        let input = ApproveDiscoveryCandidateInput {
            campaign_id: "review".into(),
            expected_revision: 1,
            candidate_id: before[0].id.clone(),
        };
        let first = owner.approve_discovery_candidate(input.clone()).unwrap();
        assert_eq!(
            owner.approve_discovery_candidate(input.clone()).unwrap(),
            first
        );
        let mut after = owner.list_discovery_candidates(page).unwrap();
        assert_eq!(
            after[0].evidence["manual_approval"]["approved_at"],
            first.approved_at
        );
        after[0]
            .evidence
            .as_object_mut()
            .unwrap()
            .remove("manual_approval");
        assert_eq!(after, before);
        assert_eq!(owner.get_discovery_campaign("review").unwrap(), campaign);
        let publication = owner
            .prepare_discovery_library_publication("review", 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(row.verdict, "qualified");
        assert!(row.reasons.iter().any(|reason| reason == "manual_approved"));
        assert!(
            row.reasons
                .iter()
                .any(|reason| reason.ends_with("unverified"))
        );
        assert!(row.match_rating.is_none());
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            1
        );
        assert_eq!(
            owner.count_discovery_candidates("review").unwrap().pending,
            0
        );
        let mut stale = input.clone();
        stale.expected_revision = 2;
        assert!(
            owner
                .approve_discovery_candidate(stale)
                .unwrap_err()
                .contains("stale")
        );
        let mut other = input;
        other.campaign_id = "other".into();
        assert!(owner.approve_discovery_candidate(other).is_err());
        drop(owner);
        let owner =
            crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            owner
                .approve_discovery_candidate(ApproveDiscoveryCandidateInput {
                    campaign_id: first.campaign_id.clone(),
                    expected_revision: first.criteria_revision,
                    candidate_id: first.candidate_id.clone()
                })
                .unwrap(),
            first
        );
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM discovery_manual_approvals", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveryManualApproval {
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub candidate_id: String,
    pub approved_at: i64,
}

impl crate::CreatorRuntime {
    pub async fn approve_discovery_candidate_async(
        &self,
        input: ApproveDiscoveryCandidateInput,
    ) -> Result<DiscoveryManualApproval, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.approve_discovery_candidate(input))
            .await
    }

    pub fn approve_discovery_candidate(
        &self,
        input: ApproveDiscoveryCandidateInput,
    ) -> Result<DiscoveryManualApproval, String> {
        if input.expected_revision == 0
            || input.expected_revision > i32::MAX as u32
            || [&input.campaign_id, &input.candidate_id]
                .iter()
                .any(|id| id.is_empty() || id.len() > 128 || id.trim() != id.as_str())
        {
            return Err("manual_approval_input_invalid".into());
        }
        self.repository.approve_discovery_candidate(input)
    }
}
