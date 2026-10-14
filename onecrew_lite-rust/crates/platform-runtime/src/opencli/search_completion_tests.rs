use super::*;
use onecrew_creator_runtime::{
    CreateDiscoveryCampaignInput, DiscoveryPlatformScope, ListDiscoverySearchHitsInput,
};

fn campaign(directory: &std::path::Path) -> onecrew_creator_runtime::CreatorRuntime {
    let creators = LocalPlatform::open(directory).unwrap().creators().unwrap();
    creators
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "completion".into(),
            brief: "Shoe designers".into(),
            opencli_version: VERIFIED_OPENCLI_VERSION.into(),
            criteria: json!({}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "dribbble-designers".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shoes".into(), "sneakers".into()],
            }],
            target_count: 30,
            max_calls: Some(4),
        })
        .unwrap();
    creators
        .approve_discovery_campaign("completion", 1)
        .unwrap();
    creators
}

fn hit() -> DiscoverySearchHitInput {
    DiscoverySearchHitInput {
        author_id: None,
        author_url: None,
        source_url: "https://dribbble.com/artist".into(),
        title: "Designer".into(),
        author_label: "artist".into(),
    }
}

#[tokio::test]
async fn rejected_search_completion_continues_to_the_next_approved_keyword() {
    let directory = tempfile::tempdir().unwrap();
    let creators = campaign(directory.path());
    let claim = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    let mut invalid = hit();
    invalid.author_url = Some(invalid.source_url.clone());
    let stopped = persist_search_hits(&creators, claim, vec![invalid])
        .await
        .expect("a saved failure must return a confirmed checkpoint to the runner");
    assert_eq!(stopped.returned_hits, 0);
    assert!(stopped.returned_sources.is_empty());
    assert_eq!(stopped.campaign.used_calls, 1);
    assert!(matches!(
        creators.plan_discovery_step("completion", 1).unwrap(),
        DiscoveryStep::Search { .. }
    ));
    let next = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    assert_ne!(next.query, "shoes");
    assert_eq!(next.call_number, 2);
}

#[tokio::test]
async fn profile_only_lead_is_saved_without_inventing_an_author_identity() {
    let directory = tempfile::tempdir().unwrap();
    let creators = campaign(directory.path());
    let claim = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    let result = persist_search_hits(&creators, claim, vec![hit()])
        .await
        .unwrap();
    assert_eq!(result.returned_hits, 1);
    assert_eq!(result.search_claim.query, "shoes");
    assert_eq!(result.returned_sources, vec![hit()]);
    assert_eq!(result.campaign.used_calls, 1);
    assert_eq!(result.campaign.platform_runs[0].status, "completed");
    let hits = creators
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "completion".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_url, "https://dribbble.com/artist");
    assert!(hits[0].author_id.is_none() && hits[0].author_url.is_none());
    assert_eq!(
        creators
            .count_discovery_candidates("completion")
            .unwrap()
            .qualified,
        0
    );
}

#[tokio::test]
async fn each_query_returns_its_order_even_when_saved_sources_are_deduplicated() {
    let directory = tempfile::tempdir().unwrap();
    let creators = campaign(directory.path());
    let first = hit();
    let mut second = hit();
    second.source_url = "https://dribbble.com/second".into();
    for (query, rows) in [
        ("shoes", vec![first.clone(), second.clone()]),
        ("sneakers", vec![second.clone(), first.clone()]),
    ] {
        let claim = creators
            .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
            .unwrap();
        let result = persist_search_hits(&creators, claim, rows.clone())
            .await
            .unwrap();
        assert_eq!(result.search_claim.query, query);
        assert_eq!(result.returned_sources, rows);
        assert_eq!(result.returned_hits, 2);
    }
    let saved = creators
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "completion".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    assert_eq!(saved.len(), 2);
}

#[tokio::test]
async fn rejected_results_end_the_reserved_attempt_without_saving_partial_hits() {
    let directory = tempfile::tempdir().unwrap();
    let creators = campaign(directory.path());
    let claim = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    let mut invalid = hit();
    invalid.author_url = Some(invalid.source_url.clone());
    let stopped = persist_search_hits(&creators, claim, vec![invalid])
        .await
        .unwrap();
    assert_eq!(stopped.returned_hits, 0);
    let result = creators.get_discovery_campaign("completion").unwrap();
    assert_eq!(result.used_calls, 1);
    assert_eq!(result.platform_runs[0].status, "technical_stop");
    assert_eq!(
        result.platform_runs[0].stop_code.as_deref(),
        Some("search_completion_failed")
    );
    assert!(
        creators
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "completion".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_stale_completion_cannot_stop_the_newer_inflight_attempt() {
    let directory = tempfile::tempdir().unwrap();
    let creators = campaign(directory.path());
    let old = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    persist_search_hits(&creators, old.clone(), vec![hit()])
        .await
        .unwrap();
    let next = creators
        .claim_discovery_search("completion", 1, "dribbble-designers", "dedicated")
        .unwrap();
    assert_eq!(next.call_number, 2);
    assert!(
        persist_search_hits(&creators, old, vec![hit()])
            .await
            .is_err()
    );
    let current = creators.get_discovery_campaign("completion").unwrap();
    assert_eq!(current.used_calls, 2);
    assert_eq!(current.platform_runs[0].status, "searching");
    assert_eq!(current.platform_runs[0].calls_used, next.call_number);
}
