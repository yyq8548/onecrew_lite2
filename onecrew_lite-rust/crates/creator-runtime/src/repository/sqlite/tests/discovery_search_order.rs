use super::*;
use crate::{
    CreateDiscoveryCampaignInput, DiscoveryPlatformScope, DiscoverySearchHitInput,
    FinishDiscoverySearchInput, repository::DiscoveryCampaignRepository,
};

fn ordered_fixture(platform: &str) -> (tempfile::TempDir, SqliteRepository) {
    let (dir, repository) = repository();
    repository
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "ordered".into(),
            brief: "Mouse reviewers".into(),
            opencli_version: "1.8.8".into(),
            criteria: serde_json::json!({"category":"mouse"}),
            scopes: vec![DiscoveryPlatformScope {
                platform: platform.into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["mouse review".into(), "gaming mouse".into()],
            }],
            target_count: 1,
            max_calls: Some(3),
        })
        .unwrap();
    repository.approve_discovery_campaign("ordered", 1).unwrap();
    let claim = repository
        .claim_discovery_search("ordered", 1, platform, "dedicated")
        .unwrap();
    repository
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: ["first", "second"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    let id = (1001 + index).to_string();
                    let author_url = match platform {
                        "linkedin" => format!("https://www.linkedin.com/in/{id}/"),
                        "pinterest" => format!("https://www.pinterest.com/{name}/"),
                        "facebook" => format!("https://www.facebook.com/profile.php?id={id}"),
                        "twitter" => format!("https://x.com/{name}"),
                        "reddit" => format!("https://www.reddit.com/user/{name}/"),
                        "tiktok" => format!("https://www.tiktok.com/@{name}"),
                        "bilibili" => format!("https://space.bilibili.com/{id}"),
                        "weibo" => format!("https://weibo.com/u/{id}"),
                        "youtube" => format!("https://www.youtube.com/channel/UC{id}"),
                        "instagram" => format!("https://www.instagram.com/{name}/"),
                        _ => unreachable!("unsupported test platform"),
                    };
                    DiscoverySearchHitInput {
                        source_url: if platform == "youtube" {
                            format!("https://www.youtube.com/watch?v=video{id}")
                        } else {
                            author_url.clone()
                        },
                        title: name.into(),
                        author_label: name.into(),
                        author_id: Some(id),
                        author_url: Some(author_url),
                    }
                })
                .collect(),
        })
        .unwrap();
    {
        // Deliberately reverse lexical IDs in this disposable fixture. Dispatch
        // must follow the saved provider order, independent of random UUIDs.
        let db = repository.lock().unwrap();
        db.execute_batch("BEGIN; PRAGMA defer_foreign_keys=ON;")
            .unwrap();
        for (name, new_id) in [("first", "zz-first"), ("second", "aa-second")] {
            let old: String = db
                .query_row(
                    "SELECT id FROM discovery_search_hits WHERE author_label=?1",
                    [name],
                    |r| r.get(0),
                )
                .unwrap();
            db.execute(
                "UPDATE discovery_search_authors SET search_hit_id=?1 WHERE search_hit_id=?2",
                [new_id, old.as_str()],
            )
            .unwrap();
            db.execute(
                "UPDATE discovery_search_positions SET search_hit_id=?1 WHERE search_hit_id=?2",
                [new_id, old.as_str()],
            )
            .unwrap();
            db.execute(
                "UPDATE discovery_search_hits SET id=?1 WHERE id=?2",
                [new_id, old.as_str()],
            )
            .unwrap();
        }
        db.execute_batch("COMMIT").unwrap();
    }
    drop(repository);
    let repository = SqliteRepository::open(dir.path().join("crm.db")).unwrap();
    (dir, repository)
}

#[test]
fn search_order_survives_ids_and_restart() {
    for platform in [
        "instagram",
        "linkedin",
        "pinterest",
        "twitter",
        "reddit",
        "facebook",
        "bilibili",
        "tiktok",
        "weibo",
    ] {
        let (_dir, repository) = ordered_fixture(platform);
        assert_eq!(
            repository
                .next_discovery_profile_hit("ordered", &[])
                .unwrap()
                .as_deref(),
            Some("zz-first"),
            "{platform}"
        );
        assert_eq!(
            super::super::discovery_profiles::next_profile_hit_excluding(
                &repository,
                "ordered",
                &[],
                &["zz-first".into()]
            )
            .unwrap()
            .as_deref(),
            Some("aa-second"),
            "{platform}"
        );
        assert!(
            repository
                .next_discovery_profile_hit("ordered", &[platform.into()])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            repository
                .get_discovery_campaign("ordered")
                .unwrap()
                .used_calls,
            1
        );
    }
}

#[test]
fn search_order_video_queue_uses_saved_order() {
    let (_dir, repository) = ordered_fixture("youtube");
    assert_eq!(
        repository
            .next_discovery_video_hit("ordered")
            .unwrap()
            .unwrap()
            .id,
        "zz-first"
    );
}

#[test]
fn search_order_duplicate_query_preserves_first_observation() {
    let (_dir, repository) = ordered_fixture("instagram");
    let positions = || {
        let db = repository.lock().unwrap();
        db.prepare(
            "SELECT search_hit_id,position FROM discovery_search_positions ORDER BY position",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let before = positions();
    let claim = repository
        .claim_discovery_search("ordered", 1, "instagram", "dedicated")
        .unwrap();
    repository
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: ["second", "first", "third"]
                .into_iter()
                .map(|name| DiscoverySearchHitInput {
                    source_url: format!("https://www.instagram.com/{name}/"),
                    title: name.into(),
                    author_label: name.into(),
                    author_id: None,
                    author_url: None,
                })
                .collect(),
        })
        .unwrap();
    let after = positions();
    assert_eq!(&after[..2], &before);
    assert_eq!(after.len(), 3);
    assert!(after[2].1 > after[1].1);
    assert_eq!(
        repository
            .get_discovery_campaign("ordered")
            .unwrap()
            .used_calls,
        2
    );
}
