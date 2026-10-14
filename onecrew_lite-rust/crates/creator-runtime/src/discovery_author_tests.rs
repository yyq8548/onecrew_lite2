use crate::*;

#[tokio::test]
async fn library_names_use_subject_and_requested_count() {
    let (directory, owner) = setup();
    let mut input = CreateDiscoveryCampaignInput {
        id: "z-first".into(),
        brief: "虚拟货币\nContent language: 中文\nContent category: 稳定币".into(),
        opencli_version: "1.8.8".into(),
        criteria: serde_json::json!({"min_subscribers":8000}),
        scopes: vec![DiscoveryPlatformScope {
            platform: "youtube".into(),
            profile_ref: "chosen".into(),
            keywords: vec!["stablecoin".into()],
        }],
        target_count: 5,
        max_calls: Some(100),
    };
    owner.create_discovery_campaign(input.clone()).unwrap();
    input.id = "a-second".into();
    input.criteria["table_subject"] = serde_json::json!("鼠标");
    input.target_count = 10;
    owner.create_discovery_campaign(input.clone()).unwrap();
    input.id = "next-day".into();
    input.criteria["table_subject"] = serde_json::json!("美妆博主");
    input.target_count = 50;
    owner.create_discovery_campaign(input).unwrap();
    let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaigns SET created_at=strftime('%s','2026-09-30 12:00:00','utc') WHERE id='authors'", []).unwrap();
    db.execute("UPDATE discovery_campaigns SET created_at=strftime('%s','2026-10-02 12:00:00','utc') WHERE id IN ('z-first','a-second')", []).unwrap();
    db.execute("UPDATE discovery_campaigns SET created_at=strftime('%s','2026-10-03 12:00:00','utc') WHERE id='next-day'", []).unwrap();
    for (id, expected) in [
        ("z-first", "虚拟货币5个"),
        ("a-second", "鼠标10个"),
        ("next-day", "美妆博主50个"),
    ] {
        assert_eq!(
            owner
                .discovery_library_table_name_async(id.into())
                .await
                .unwrap()
                .name,
            expected
        );
        assert_eq!(owner.get_discovery_campaign(id).unwrap().used_calls, 0);
    }
    let reopened = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .discovery_library_table_name_async("z-first".into())
            .await
            .unwrap()
            .name,
        "虚拟货币5个"
    );
}

fn setup() -> (tempfile::TempDir, CreatorRuntime) {
    let directory = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "authors".into(),
            brief: "Search creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"min_followers":10000}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "bilibili".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["first".into(), "second".into(), "third".into()],
            }],
            target_count: 30,
            max_calls: Some(5),
        })
        .unwrap();
    owner.approve_discovery_campaign("authors", 1).unwrap();
    (directory, owner)
}

fn hit(video: &str, author: Option<&str>) -> DiscoverySearchHitInput {
    DiscoverySearchHitInput {
        source_url: format!("https://www.bilibili.com/video/{video}"),
        title: "demo".into(),
        author_label: "Same name".into(),
        author_id: author.map(str::to_owned),
        author_url: author.map(|id| format!("https://space.bilibili.com/{id}")),
    }
}

fn list(owner: &CreatorRuntime) -> Vec<DiscoverySearchHit> {
    owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "authors".into(),
            after_id: None,
            limit: 100,
        })
        .unwrap()
}

fn finish(owner: &CreatorRuntime, hits: Vec<DiscoverySearchHitInput>) {
    let claim = owner
        .claim_discovery_search("authors", 1, "bilibili", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: hits,
        })
        .unwrap();
}

#[test]
fn search_author_provenance_survives_restart_without_merging_names_or_qualifying() {
    let (directory, owner) = setup();
    finish(
        &owner,
        vec![
            hit("BV1RYqHYnEw8", Some("1")),
            hit("BV1RYqHYnEw9", Some("2")),
            hit("BV1RYqHYnEw0", None),
        ],
    );
    let before = list(&owner);
    assert_eq!(before.len(), 3);
    for row in &before {
        assert_eq!(row.author_label, "Same name");
        assert_eq!(row.author_id.is_some(), row.author_url.is_some());
        assert_eq!(row.author_id.is_some(), row.author_observed_at.is_some());
        if row.author_id.is_some() {
            assert_eq!(row.author_observed_at, Some(row.captured_at));
        }
    }
    assert_eq!(
        before
            .iter()
            .filter_map(|h| h.author_id.as_deref())
            .collect::<std::collections::BTreeSet<_>>(),
        ["1", "2"].into_iter().collect()
    );
    assert_eq!(
        owner
            .count_discovery_candidates("authors")
            .unwrap()
            .qualified,
        0
    );
    drop(owner);
    let reopened = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    assert_eq!(list(&reopened), before);
    finish(&reopened, vec![hit("BV1RYqHYnEw8", Some("1"))]);
    assert_eq!(list(&reopened), before);
    assert_eq!(
        reopened
            .get_discovery_campaign("authors")
            .unwrap()
            .used_calls,
        2
    );
}

#[test]
fn conflicting_author_rolls_back_hits_completion_and_checkpoint_without_refunding_read() {
    let (_directory, owner) = setup();
    finish(&owner, vec![hit("BV1RYqHYnEw8", Some("1"))]);
    let before = list(&owner);
    let claim = owner
        .claim_discovery_search("authors", 1, "bilibili", "dedicated")
        .unwrap();
    let result = owner.finish_discovery_search(FinishDiscoverySearchInput {
        claim: claim.clone(),
        status: "completed".into(),
        stop_code: None,
        search_hits: vec![
            hit("BV1RYqHYnEw9", Some("2")),
            hit("BV1RYqHYnEw8", Some("3")),
        ],
    });
    assert!(result.is_err());
    assert_eq!(list(&owner), before);
    let campaign = owner.get_discovery_campaign("authors").unwrap();
    assert_eq!(campaign.used_calls, 2);
    assert_eq!(campaign.platform_runs[0].status, "searching");
    // Complete the original claim; never dispatch a second read or refund its budget.
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "technical_stop".into(),
            stop_code: Some("opencli_invalid_output".into()),
            search_hits: vec![],
        })
        .unwrap();
    assert_eq!(list(&owner), before);
}

#[test]
fn incomplete_or_unsafe_author_is_refused_and_unknown_is_not_filled_from_label() {
    let (_directory, owner) = setup();
    let claim = owner
        .claim_discovery_search("authors", 1, "bilibili", "dedicated")
        .unwrap();
    for (id, url) in [
        (Some("1"), None),
        (None, Some("https://space.bilibili.com/1")),
        (Some(""), Some("https://space.bilibili.com/1")),
        (Some("1"), Some("https://secret@example.com/a")),
        (Some("1"), Some("https://space.bilibili.com/1?token=hidden")),
    ] {
        let mut invalid = hit("BV1RYqHYnEw8", None);
        invalid.author_id = id.map(str::to_owned);
        invalid.author_url = url.map(str::to_owned);
        assert!(
            owner
                .finish_discovery_search(FinishDiscoverySearchInput {
                    claim: claim.clone(),
                    status: "completed".into(),
                    stop_code: None,
                    search_hits: vec![invalid]
                })
                .is_err()
        );
        assert!(list(&owner).is_empty());
    }
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![hit("BV1RYqHYnEw8", None)],
        })
        .unwrap();
    assert_eq!(list(&owner)[0].author_id, None);
}

#[test]
fn excluded_unsearched_source_does_not_block_next_approved_keyword() {
    let (_directory, owner) = setup();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "excluded-source".into(),
            brief: "mouse".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"language":"English"}),
            scopes: vec![
                DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["mouse".into(), "wireless mouse".into()],
                },
                DiscoveryPlatformScope {
                    platform: "tiktok".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["mouse".into()],
                },
            ],
            target_count: 2,
            max_calls: None,
        })
        .unwrap();
    owner
        .approve_discovery_campaign("excluded-source", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("excluded-source", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![],
        })
        .unwrap();
    let step = owner
        .plan_discovery_step_excluding("excluded-source", 1, &["tiktok".into()])
        .unwrap();
    assert!(matches!(step, DiscoveryStep::Search { platform, .. } if platform=="youtube"));
    let claim = owner
        .claim_discovery_search("excluded-source", 1, "youtube", "dedicated")
        .unwrap();
    assert_eq!(claim.query, "wireless mouse");
}
