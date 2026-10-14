use crate::*;
use serde_json::json;

#[test]
fn explicit_unlimited_authorization_survives_old_cap_restart_and_source_exhaustion() {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "unlimited".into(),
            brief: "Keyboards".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":8000}),
            target_count: 30,
            max_calls: None,
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "fixture".into(),
                keywords: vec!["keyboards".into()],
            }],
        })
        .unwrap();
    owner.approve_discovery_campaign("unlimited", 1).unwrap();
    // A historical amount of consumed reads is a statistic, never an implied cap.
    rusqlite::Connection::open(dir.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE discovery_campaigns SET used_calls=10001 WHERE id='unlimited'",
            [],
        )
        .unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.get_discovery_campaign("unlimited").unwrap().max_calls,
        None
    );
    assert!(matches!(
        owner.plan_discovery_step("unlimited", 1).unwrap(),
        DiscoveryStep::Search { .. }
    ));
    let claim = owner
        .claim_discovery_search("unlimited", 1, "youtube", "fixture")
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
        owner
            .get_discovery_campaign("unlimited")
            .unwrap()
            .used_calls,
        10002
    );
    assert!(matches!(
        owner.plan_discovery_step("unlimited", 1).unwrap(),
        DiscoveryStep::SearchExhausted
    ));
    assert!(
        owner
            .increase_discovery_budget(
                "operator",
                IncreaseDiscoveryBudgetInput {
                    campaign_id: "unlimited".into(),
                    expected_revision: 1,
                    expected_max_calls: 10000,
                    new_max_calls: 10001,
                    decision_id: "do-not-convert-unlimited".into(),
                }
            )
            .is_err()
    );
    assert_eq!(
        owner.get_discovery_campaign("unlimited").unwrap().max_calls,
        None
    );
}
