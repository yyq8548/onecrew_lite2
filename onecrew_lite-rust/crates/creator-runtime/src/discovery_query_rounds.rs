//! Additive automatic queries never change the approved qualification criteria.
use crate::{CreatorRepositoryCallError, CreatorRuntime, DiscoveryPlatformScope};

#[derive(Clone)]
pub struct AppendDiscoveryQueries {
    pub campaign_id: String,
    pub revision: u32,
    pub expected_scopes: Vec<DiscoveryPlatformScope>,
    pub additions: Vec<DiscoveryPlatformScope>,
}

impl CreatorRuntime {
    pub async fn append_discovery_queries_async(
        &self,
        account: String,
        input: AppendDiscoveryQueries,
    ) -> Result<(), CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            if account.trim().is_empty()
                || input.additions.is_empty()
                || input.additions.len() > input.expected_scopes.len()
            {
                return Err("query_round_invalid".into());
            }
            let mut platforms = std::collections::HashSet::new();
            for scope in &input.additions {
                if !platforms.insert((&scope.platform, &scope.profile_ref))
                    || scope.keywords.is_empty()
                    || scope.keywords.len() > 8
                {
                    return Err("query_round_invalid".into());
                }
                let prior = input
                    .expected_scopes
                    .iter()
                    .find(|s| s.platform == scope.platform && s.profile_ref == scope.profile_ref)
                    .ok_or("query_round_outside_scope")?;
                let mut seen: std::collections::HashSet<_> = prior
                    .keywords
                    .iter()
                    .map(|s| s.trim().to_lowercase())
                    .collect();
                for word in &scope.keywords {
                    if word.trim().is_empty()
                        || word.len() > 160
                        || word.chars().any(char::is_control)
                        || !seen.insert(word.trim().to_lowercase())
                    {
                        return Err("query_round_duplicate_or_invalid".into());
                    }
                }
            }
            owner.repository.append_discovery_queries(&account, &input)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CreateDiscoveryCampaignInput, DiscoveryStep, FinishDiscoverySearchInput};

    #[tokio::test]
    async fn query_round_preserves_filters_and_resumes_after_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
        let scopes = vec![DiscoveryPlatformScope {
            platform: "youtube".into(),
            profile_ref: "work".into(),
            keywords: vec!["headphone review".into()],
        }];
        let criteria = serde_json::json!({"min_subscribers":5000,"language":"English","automatic_query_expansion":true});
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "query-round".into(),
                brief: "Promote headphones".into(),
                opencli_version: "1.8.8".into(),
                criteria: criteria.clone(),
                scopes: scopes.clone(),
                target_count: 10,
                max_calls: Some(10),
            })
            .unwrap();
        owner.approve_discovery_campaign("query-round", 1).unwrap();
        let claim = owner
            .claim_discovery_search("query-round", 1, "youtube", "work")
            .unwrap();
        owner
            .finish_discovery_search(FinishDiscoverySearchInput {
                claim,
                status: "completed".into(),
                stop_code: None,
                search_hits: vec![],
            })
            .unwrap();
        assert_eq!(
            owner.plan_discovery_step("query-round", 1).unwrap(),
            DiscoveryStep::SearchExhausted
        );
        let input = AppendDiscoveryQueries {
            campaign_id: "query-round".into(),
            revision: 1,
            expected_scopes: scopes,
            additions: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "work".into(),
                keywords: vec!["headphones comfort test".into()],
            }],
        };
        let export = std::env::var_os("ONECREW_QUERY_ROUND_ORACLE_DIRECTORY").map(std::path::PathBuf::from);
        if let Some(path) = &export {
            std::fs::create_dir_all(path).unwrap();
            let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
            db.execute("VACUUM INTO ?1", [path.join("before.sqlite").to_str().unwrap()]).unwrap();
            std::fs::write(path.join("input.json"), serde_json::to_vec_pretty(&serde_json::json!({"campaign_id":input.campaign_id,"revision":input.revision,"expected_scopes":input.expected_scopes,"additions":input.additions})).unwrap()).unwrap();
        }
        owner
            .append_discovery_queries_async("account".into(), input.clone())
            .await
            .unwrap();
        if let Some(path) = &export {
            let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
            db.execute("VACUUM INTO ?1", [path.join("after.sqlite").to_str().unwrap()]).unwrap();
        }
        // A repeated request cannot append twice; the stale snapshot loses the CAS.
        assert!(
            owner
                .append_discovery_queries_async("account".into(), input)
                .await
                .is_err()
        );
        let after = owner.get_discovery_campaign("query-round").unwrap();
        assert_eq!(after.criteria, criteria);
        assert_eq!(after.revision, 1);
        assert_eq!(after.used_calls, 1);
        assert_eq!(after.max_calls, Some(10));
        assert!(matches!(
            owner.plan_discovery_step("query-round", 1).unwrap(),
            DiscoveryStep::Search { .. }
        ));
        let claim = owner
            .claim_discovery_search("query-round", 1, "youtube", "work")
            .unwrap();
        assert_eq!(claim.query, "headphones comfort test");
    }

    #[tokio::test]
    async fn query_round_rejects_manual_plan_and_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let owner = CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
        let scope = DiscoveryPlatformScope {
            platform: "youtube".into(),
            profile_ref: "work".into(),
            keywords: vec!["headphones".into()],
        };
        owner
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "manual".into(),
                brief: "headphones".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({}),
                scopes: vec![scope.clone()],
                target_count: 10,
                max_calls: None,
            })
            .unwrap();
        owner.approve_discovery_campaign("manual", 1).unwrap();
        let input = AppendDiscoveryQueries {
            campaign_id: "manual".into(),
            revision: 1,
            expected_scopes: vec![scope.clone()],
            additions: vec![scope.clone()],
        };
        assert!(
            owner
                .append_discovery_queries_async("account".into(), input.clone())
                .await
                .is_err()
        );
        let mut other = input;
        other.additions[0].keywords = vec!["headphones comparison".into()];
        assert!(
            owner
                .append_discovery_queries_async("account".into(), other)
                .await
                .is_err()
        );
        assert_eq!(
            owner.get_discovery_campaign("manual").unwrap().scopes,
            vec![scope]
        );
    }
}
