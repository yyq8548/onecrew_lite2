use crate::*;

fn setup(path: &std::path::Path) -> CreatorRuntime {
    let owner = CreatorRuntime::open(path, CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "target-campaign".into(),
            brief: "Shoe creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_subscribers":10000,"category":"鞋子"}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shoes".into()],
            }],
            target_count: 30,
            max_calls: Some(400),
        })
        .unwrap();
    owner
        .approve_discovery_campaign("target-campaign", 1)
        .unwrap();
    owner
}

fn input() -> IncreaseDiscoveryTargetInput {
    IncreaseDiscoveryTargetInput {
        campaign_id: "target-campaign".into(),
        expected_revision: 1,
        expected_target_count: 30,
        new_target_count: 200,
        decision_id: "target-200".into(),
    }
}

#[test]
fn target_decrease_is_audited_and_preserves_budget() {
    let directory = tempfile::tempdir().unwrap();
    let owner = setup(directory.path());
    let before = owner.get_discovery_campaign("target-campaign").unwrap();
    let mut request = input();
    request.new_target_count = 8;
    let after = owner.increase_discovery_target("account", request.clone()).unwrap();
    assert_eq!(after.target_count, 8);
    assert_eq!(after.criteria, before.criteria);
    assert_eq!(after.max_calls, before.max_calls);
    assert_eq!(after.used_calls, before.used_calls);
    assert_eq!(owner.increase_discovery_target("account", request).unwrap(), after);
}

#[test]
fn target_increase_preserves_criteria_sources_and_budget_across_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let owner = setup(directory.path());
    let before = owner.get_discovery_campaign("target-campaign").unwrap();
    let updated = owner.increase_discovery_target("account", input()).unwrap();
    assert_eq!(updated.target_count, 200);
    assert_eq!(updated.criteria, before.criteria);
    assert_eq!(updated.scopes, before.scopes);
    assert_eq!(updated.platform_runs, before.platform_runs);
    assert_eq!(updated.max_calls, Some(400));
    assert_eq!(updated.used_calls, before.used_calls);
    assert_eq!(updated.revision, before.revision);
    assert_eq!(
        owner.increase_discovery_target("account", input()).unwrap(),
        updated
    );
    assert!(
        owner
            .increase_discovery_target("other-account", input())
            .is_err()
    );
    let mut stale = input();
    stale.decision_id = "new-intent".into();
    assert!(owner.increase_discovery_target("account", stale).is_err());
    drop(owner);
    let reopened = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_campaign("target-campaign").unwrap(),
        updated
    );
    let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    let ledger: (i64, i64, i64, i64) = db.query_row(
        "SELECT count(*),previous_target_count,new_target_count,used_calls_at_decision FROM discovery_target_increases",
        [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(ledger, (1, 30, 200, 0));
}

#[test]
fn target_increase_rejects_invalid_actor_decrease_and_revision() {
    let directory = tempfile::tempdir().unwrap();
    let owner = setup(directory.path());
    assert!(owner.increase_discovery_target("", input()).is_err());
    for (expected, new, revision) in [
        (30, 30, 1),
        (30, 0, 1),
        (30, 10001, 1),
        (30, 200, 2),
        (20, 200, 1),
    ] {
        let mut request = input();
        request.expected_target_count = expected;
        request.new_target_count = new;
        request.expected_revision = revision;
        assert!(owner.increase_discovery_target("account", request).is_err());
    }
    assert_eq!(
        owner
            .get_discovery_campaign("target-campaign")
            .unwrap()
            .target_count,
        30
    );
}

#[test]
fn target_change_does_not_race_an_active_provider_read() {
    let directory = tempfile::tempdir().unwrap();
    let owner = setup(directory.path());
    let claim = owner
        .claim_discovery_search("target-campaign", 1, "youtube", "dedicated")
        .unwrap();
    assert!(owner.increase_discovery_target("account", input()).is_err());
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![],
        })
        .unwrap();
    let updated = owner.increase_discovery_target("account", input()).unwrap();
    assert_eq!(updated.used_calls, 1);
    assert_eq!(updated.max_calls, Some(400));
    assert_eq!(updated.target_count, 200);
}
