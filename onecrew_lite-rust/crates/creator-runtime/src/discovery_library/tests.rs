use super::*;
use crate::*;
use rusqlite::{Connection, params};

fn setup() -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id: "library-proof".into(), brief: "AI creators".into(),opencli_version:"1.8.8".into(),
        criteria:serde_json::json!({"min_followers":10000,"requires_ai_tool_demo":false,"requires_public_tool_url":false}),
        scopes:vec![DiscoveryPlatformScope {platform:"youtube".into(),profile_ref:"dedicated".into(),keywords:vec!["AI tools".into()]}],
        target_count:30,max_calls: Some(5),
    }).unwrap();
    owner
        .approve_discovery_campaign("library-proof", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("library-proof", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                title: "AI tools review".into(),
                author_label: "Same display name".into(),
                author_id: None,
                author_url: None,
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "library-proof".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    (dir, owner, hit.id)
}

fn read_video(owner: &CreatorRuntime, hit: &str) {
    let claim = owner
        .claim_discovery_video("library-proof", 1, "dedicated", hit)
        .unwrap();
    owner.finish_discovery_video(FinishDiscoveryVideoInput {
        candidate:Some(UpsertDiscoveryCandidateInput {
            campaign_id:claim.campaign_id.clone(),expected_revision:1,platform:"youtube".into(),
            source_profile_ref:"dedicated".into(),platform_account_id:"UC1234567890".into(),
            display_name:"Same display name".into(),profile_url:"https://www.youtube.com/channel/UC1234567890".into(),
            evidence:serde_json::json!({"source_urls":[claim.source_url],"subscribers_source_url":claim.source_url,
                "subscribers_raw":"20000 subscribers","captured_at":1}),
        }),claim,stop_code:None,
    }).unwrap();
}

#[test]
fn youtube_followers_survive_canonical_video_url_in_library_snapshot() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    let canonical = "https://www.youtube.com/watch?v=LR04bU_yV5k";
    let search_url = format!("{canonical}&pp=search");
    db.execute("UPDATE discovery_search_hits SET source_url=?1 WHERE id=?2", params![search_url, hit]).unwrap();
    let claim = owner.claim_discovery_video("library-proof", 1, "dedicated", &hit).unwrap();
    owner.finish_discovery_video(FinishDiscoveryVideoInput {
        candidate: Some(UpsertDiscoveryCandidateInput {
            campaign_id: claim.campaign_id.clone(), expected_revision: 1,
            platform: "youtube".into(), source_profile_ref: "dedicated".into(),
            platform_account_id: "UC1234567890".into(), display_name: "Maker".into(),
            profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
            evidence: serde_json::json!({"source_urls":[search_url, canonical],
                "subscribers_source_url":canonical,"subscribers_raw":"12.4K subscribers","captured_at":1}),
        }), claim, stop_code: None,
    }).unwrap();
    let snapshot = owner.prepare_discovery_library_snapshot("library-proof", 1).unwrap();
    assert_eq!(snapshot.rows[0].followers.as_deref(), Some("~12400"));
    // An unrelated video's evidence must not leak a count into this channel.
    db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.subscribers_source_url','https://www.youtube.com/watch?v=abcdefghijk')", []).unwrap();
    let unknown = owner.prepare_discovery_library_snapshot("library-proof", 1).unwrap();
    assert_eq!(unknown.rows[0].followers, None);
    assert!(unknown.source_revision > snapshot.source_revision);
    db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.subscribers_source_url',?1)", [canonical]).unwrap();
    let restored = owner.prepare_discovery_library_snapshot("library-proof", 1).unwrap();
    assert_eq!(restored.rows[0].followers.as_deref(), Some("~12400"));
    assert!(restored.source_revision > unknown.source_revision);
}

#[test]
fn youtube_followers_are_shared_only_by_completed_videos_of_the_same_channel() {
    let (dir, owner, hit) = setup();
    read_video(&owner, &hit);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("INSERT INTO discovery_search_hits VALUES ('second','library-proof','youtube','dedicated','AI tools','https://www.youtube.com/watch?v=abcdefghijk','Review','Same display name',1)", []).unwrap();
    read_video(&owner, "second");
    let snapshot = owner.prepare_discovery_library_snapshot("library-proof", 1).unwrap();
    assert_eq!(snapshot.rows.len(), 2);
    assert!(snapshot.rows.iter().all(|row| row.followers.as_deref() == Some("20000")));
}

#[test]
fn video_reads_prioritize_new_labels_without_merging_or_discarding_namesakes() {
    let (dir, owner, hit) = setup();
    read_video(&owner, &hit);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    for (id, label, video) in [
        ("a-known", "Same display name", "abcdefghijk"),
        ("z-new", "New author", "lmnopqrstuv"),
    ] {
        db.execute("INSERT INTO discovery_search_hits VALUES (?1,'library-proof','youtube','dedicated','AI tools',?2,'Review',?3,1)", params![id, format!("https://www.youtube.com/watch?v={video}"), label]).unwrap();
    }
    assert_eq!(
        owner
            .repository
            .next_discovery_video_hit("library-proof")
            .unwrap()
            .unwrap()
            .id,
        "z-new"
    );
    db.execute("DELETE FROM discovery_search_hits WHERE id='z-new'", [])
        .unwrap();
    assert_eq!(
        owner
            .repository
            .next_discovery_video_hit("library-proof")
            .unwrap()
            .unwrap()
            .id,
        "a-known"
    );
    db.execute("INSERT INTO discovery_search_authors VALUES ('a-known','UC1234567890','https://www.youtube.com/channel/UC1234567890',1)", []).unwrap();
    db.execute("UPDATE discovery_candidates SET verdict='qualified',evidence_json=json_set(evidence_json,'$.reviewed_match',json(?1))", [r#"{"verified":true,"native_author_id":"UC1234567890","level":"good","outcome":"supported"}"#]).unwrap();
    assert!(
        owner
            .repository
            .next_discovery_video_hit("library-proof")
            .unwrap()
            .is_none()
    );
}

#[test]
fn library_snapshot_is_source_bound_versioned_idempotent_and_restart_safe() {
    let (dir, owner, hit) = setup();
    crate::discovery_contacts::legacy_approval_fixture(dir.path());
    let first = owner
        .prepare_discovery_library_snapshot("library-proof", 1)
        .unwrap();
    assert_eq!(first.source_revision, 1);
    assert_eq!(first.rows[0].verdict, "unverified");
    assert_eq!(first.rows[0].candidate_id, None);
    assert_eq!(first.rows[0].followers, None);
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap(),
        first
    );
    read_video(&owner, &hit);
    let next = owner
        .prepare_discovery_library_snapshot("library-proof", 1)
        .unwrap();
    let candidate = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "library-proof".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert_eq!(next.source_revision, 2);
    assert_eq!(
        next.rows[0].candidate_id.as_deref(),
        Some(candidate.id.as_str())
    );
    assert_eq!(
        next.rows[0].native_author_id.as_deref(),
        Some("UC1234567890")
    );
    assert_eq!(next.rows[0].verdict, candidate.verdict);
    assert_eq!(next.rows[0].verdict, "qualified");
    assert_eq!(next.rows[0].followers.as_deref(), Some("20000"));
    assert_ne!(
        first.rows[0].evidence_version,
        next.rows[0].evidence_version
    );
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_library_source_versions SET updated_at=123",
        [],
    )
    .unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap(),
        next
    );
    assert_eq!(
        db.query_row(
            "SELECT updated_at FROM discovery_library_source_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        123
    );
    assert_eq!(
        owner
            .get_discovery_campaign("library-proof")
            .unwrap()
            .used_calls,
        2
    );
}

#[test]
fn library_snapshot_never_guesses_legacy_or_cross_profile_or_revision_bindings() {
    let (dir, owner, hit) = setup();
    read_video(&owner, &hit);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("INSERT INTO discovery_platform_runs VALUES('library-proof','youtube','other','completed',NULL,NULL,0,NULL,1);").unwrap();
    db.execute(
        "UPDATE discovery_candidates SET source_profile_ref='other'",
        [],
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "unverified"
    );
    db.execute(
        "UPDATE discovery_candidates SET source_profile_ref='dedicated',criteria_revision=2",
        [],
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "unverified"
    );
    db.execute("UPDATE discovery_candidates SET criteria_revision=1", [])
        .unwrap();
    db.execute("DELETE FROM discovery_video_identity_bindings", [])
        .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap()
            .rows[0]
            .verdict,
        "unverified"
    );
    assert!(
        owner
            .prepare_discovery_library_snapshot("other-campaign", 1)
            .is_err()
    );
    assert!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 2)
            .is_err()
    );
}

#[test]
fn library_version_rechecks_source_and_rolls_back_failed_reservation() {
    let (dir, owner, _) = setup();
    let basis = owner
        .repository
        .read_discovery_library_basis("library-proof", 1)
        .unwrap();
    let snapshot = build_snapshot(&basis).unwrap();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_search_hits SET title='changed'", [])
        .unwrap();
    assert_eq!(
        owner
            .repository
            .reserve_discovery_library_version(&basis, &snapshot.snapshot_hash)
            .unwrap_err(),
        "library_source_changed"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_library_source_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("CREATE TRIGGER reject_library_version BEFORE INSERT ON discovery_library_source_versions BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
    assert!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .is_err()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_library_source_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER reject_library_version")
        .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap()
            .source_revision,
        1
    );
}

#[tokio::test]
async fn library_concurrent_preparation_reuses_one_version_without_provider_calls() {
    let (_dir, owner, _) = setup();
    let (left, right) = tokio::join!(
        owner.prepare_discovery_library_snapshot_async("library-proof".into(), 1),
        owner.prepare_discovery_library_snapshot_async("library-proof".into(), 1)
    );
    assert_eq!(left.unwrap(), right.unwrap());
    assert_eq!(
        owner
            .get_discovery_campaign("library-proof")
            .unwrap()
            .used_calls,
        1
    );
}

#[test]
fn library_snapshot_has_explicit_bounds_and_does_not_publish_partial_pages() {
    let (dir, owner, _) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch(
        "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000)
        INSERT INTO discovery_search_hits SELECT 'extra-'||x,'library-proof','youtube','dedicated',
        'AI tools','https://example.com/'||x,'title','author',1 FROM n;",
    )
    .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap_err(),
        "library_source_snapshot_too_large"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_library_source_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn library_snapshot_rejects_approval_changes_and_version_exhaustion() {
    let (dir, owner, _) = setup();
    let first = owner
        .prepare_discovery_library_snapshot("library-proof", 1)
        .unwrap();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_library_source_versions SET source_revision=?1",
        params![9_007_199_254_740_991_u64],
    )
    .unwrap();
    db.execute("UPDATE discovery_search_hits SET title='changed'", [])
        .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .unwrap_err(),
        "library_source_version_exhausted"
    );
    db.execute(
        "UPDATE discovery_campaign_revisions SET approved_at=NULL",
        [],
    )
    .unwrap();
    assert!(
        owner
            .prepare_discovery_library_snapshot("library-proof", 1)
            .is_err()
    );
    assert_eq!(first.source_revision, 1);
}
