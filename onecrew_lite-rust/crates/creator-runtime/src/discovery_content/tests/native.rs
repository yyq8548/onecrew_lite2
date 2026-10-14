use super::*;
use rusqlite::Connection;

#[test]
fn missing_profile_binding_cannot_reserve_content_budget() {
    let (dir, owner, hit) = setup_x(5);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("DELETE FROM discovery_profile_identity_matches", [])
        .unwrap();
    assert!(
        owner
            .claim_discovery_content("x-content", 1, &hit.id)
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-content")
            .unwrap()
            .used_calls,
        3
    );
    assert!(!matches!(
        owner.plan_discovery_step("x-content", 1).unwrap(),
        DiscoveryStep::ReadContent { .. }
    ));
    assert!(owner.get_discovery_content("x-content", &hit.id).is_err());
}

#[test]
fn content_reservation_and_native_binding_roll_back_together() {
    let (dir, owner, hit) = setup_x(5);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_native_binding BEFORE INSERT ON discovery_content_identity_bindings
        BEGIN SELECT RAISE(ABORT,'injected binding failure'); END;",
    )
    .unwrap();
    assert!(
        owner
            .claim_discovery_content("x-content", 1, &hit.id)
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-content")
            .unwrap()
            .used_calls,
        3
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-content")
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
    assert!(owner.get_discovery_content("x-content", &hit.id).is_err());
    db.execute_batch("DROP TRIGGER reject_native_binding;")
        .unwrap();
    let claim = owner
        .claim_discovery_content("x-content", 1, &hit.id)
        .unwrap();
    db.execute(
        "UPDATE discovery_content_identity_bindings SET account_id='111'",
        [],
    )
    .unwrap();
    assert!(owner.finish_discovery_content(x_success(claim)).is_err());
    assert_eq!(
        owner
            .get_discovery_content("x-content", &hit.id)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("x-content")
            .unwrap()
            .platform_runs[0]
            .status,
        "searching"
    );
}

#[test]
fn native_content_restart_reuses_only_its_original_binding_and_new_budget() {
    for legacy in [false, true] {
        let (dir, owner, hit) = setup_x(6);
        let old = owner
            .claim_discovery_content("x-content", 1, &hit.id)
            .unwrap();
        drop(owner);
        if legacy {
            let db = Connection::open(dir.path().join("crm.db")).unwrap();
            db.execute("DELETE FROM discovery_content_identity_bindings", [])
                .unwrap();
        }
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        // Opening the repository is read-only; the host owns interrupted-read recovery.
        assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
        if legacy {
            assert!(!matches!(
                owner.plan_discovery_step("x-content", 1).unwrap(),
                DiscoveryStep::ReadContent { .. }
            ));
            assert!(
                owner
                    .claim_discovery_content("x-content", 1, &hit.id)
                    .is_err()
            );
            assert_eq!(
                owner
                    .get_discovery_campaign("x-content")
                    .unwrap()
                    .used_calls,
                4
            );
        } else {
            let next = owner
                .claim_discovery_content("x-content", 1, &hit.id)
                .unwrap();
            assert_eq!(next.call_number, 5);
            assert!(owner.finish_discovery_content(x_success(old)).is_err());
            let receipt = owner.finish_discovery_content(x_success(next)).unwrap();
            drop(owner);
            let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
            assert_eq!(
                owner.get_discovery_content("x-content", &hit.id).unwrap(),
                receipt
            );
            assert_eq!(
                owner
                    .count_discovery_candidates("x-content")
                    .unwrap()
                    .qualified,
                0
            );
        }
    }
}

#[test]
fn v49_upgrade_keeps_legacy_content_bytes_and_never_backfills_identity() {
    let (dir, owner, hit) = setup_x(5);
    let claim = owner
        .claim_discovery_content("x-content", 1, &hit.id)
        .unwrap();
    owner.finish_discovery_content(x_success(claim)).unwrap();
    let path = dir.path().join("crm.db");
    drop(owner);
    let db = Connection::open(&path).unwrap();
    crate::repository::SqliteRepository::strip_reddit_reference(&db).unwrap();
    let legacy = r#"{"author_id":"creator_1","profile_url":"https://x.com/creator_1","sampled_posts":1,"excluded_retweets":1,"original_posts":[],"identity_kind":"mutable_handle"}"#;
    db.execute(
        "UPDATE discovery_content_reads SET native_content_json=?1",
        [legacy],
    )
    .unwrap();
    db.execute_batch("DROP TABLE discovery_manual_approvals; DROP TABLE discovery_language_samples; DROP TABLE discovery_library_identity_versions; DROP TABLE discovery_identity_reviews; DROP TABLE discovery_target_increases; DROP TABLE discovery_budget_increases; DROP TABLE discovery_video_read_failures; DROP TABLE discovery_evidence_reviews; DROP TABLE discovery_video_identity_bindings; DROP TABLE discovery_library_source_versions; DROP TABLE discovery_content_identity_bindings; PRAGMA user_version=48;")
        .unwrap();
    let candidate: String = db
        .query_row("SELECT evidence_json FROM discovery_candidates", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(CreatorRuntime::open(dir.path(), CreatorGateways::default()).is_err());
    let upgraded = CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(upgraded.impact, CreatorDatabaseUpgradeImpact::default());
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        48
    );
    for conn in [&db, &backup] {
        assert_eq!(
            conn.query_row(
                "SELECT native_content_json FROM discovery_content_reads",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            legacy
        );
        assert_eq!(
            conn.query_row("SELECT evidence_json FROM discovery_candidates", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            candidate
        );
        assert_eq!(
            conn.query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            conn.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_content_identity_bindings",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_content_identity_bindings")
            .is_err()
    );
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_content("x-content", &hit.id)
            .unwrap()
            .native_content
            .unwrap(),
        serde_json::from_str::<serde_json::Value>(legacy).unwrap()
    );
    assert!(
        reopened
            .claim_discovery_content("x-content", 1, &hit.id)
            .is_err()
    );
}

#[test]
fn native_content_provider_stops_preserve_binding_without_retry_or_qualification() {
    for (code, expected) in [
        ("opencli_auth_required", "unauthorized"),
        ("opencli_risk_control", "risk_control_stop"),
    ] {
        let (dir, owner, hit) = setup_x(6);
        let claim = owner
            .claim_discovery_content("x-content", 1, &hit.id)
            .unwrap();
        let receipt = owner
            .finish_discovery_content(FinishDiscoveryContentInput {
                claim,
                native_content: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        assert_eq!(receipt.status, "failed");
        assert_eq!(
            owner
                .get_discovery_campaign("x-content")
                .unwrap()
                .platform_runs[0]
                .status,
            expected
        );
        assert!(
            owner
                .claim_discovery_content("x-content", 1, &hit.id)
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_campaign("x-content")
                .unwrap()
                .used_calls,
            4
        );
        assert_eq!(
            owner
                .count_discovery_candidates("x-content")
                .unwrap()
                .qualified,
            0
        );
        drop(owner);
        let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            owner.get_discovery_content("x-content", &hit.id).unwrap(),
            receipt
        );
    }
}
