use crate::*;
use serde_json::{Value, json};
const CAMPAIGN: &str = "facebook-native-identity";

#[tokio::test]
async fn export_discovery_facebook_identity_targets_node_oracle() {
    let Some(output) = std::env::var_os("ONECREW_FACEBOOK_IDENTITY_TARGETS_ORACLE_DIRECTORY") else { return; };
    let output = std::path::PathBuf::from(output); std::fs::create_dir_all(&output).unwrap();
    let (directory, owner, _) = setup();
    let snapshot = |name: &str| { rusqlite::Connection::open(directory.path().join("crm.db")).unwrap().execute("VACUUM INTO ?1", [output.join(name).to_str().unwrap()]).unwrap(); };
    let hit = next(&owner); snapshot("before.sqlite");
    let identity_claim = owner.claim_discovery_facebook_identity(CAMPAIGN, 1, &hit).unwrap(); snapshot("identity-claimed.sqlite");
    let identity = owner.finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput { native_identity:Some(native(&identity_claim)),claim:identity_claim.clone(),stop_code:None }).unwrap(); snapshot("identity-completed.sqlite");
    let profile_claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    let native_profile = json!({"adapter_version":"2.0.0","id":profile_claim.author_id,"display_name":"Native creator","follower_count":null,"follower_text":"1.3M followers","profile_url":profile_claim.author_url,"observed_profile_url":identity_claim.source_url.trim_end_matches('/'),"contact_links":[],"evidence_source":"native_profile_header_response"});
    owner.finish_discovery_profile(FinishDiscoveryProfileInput { native_profile:Some(native_profile),claim:profile_claim,stop_code:None }).unwrap(); snapshot("profile-completed.sqlite");
    let targets_claim = owner.claim_discovery_facebook_targets_async(CAMPAIGN.into(),1,hit.clone()).await.unwrap(); snapshot("targets-claimed.sqlite");
    let urls = vec!["https://www.facebook.com/reel/1075265928641482".into(),"https://www.facebook.com/reel/2542739356215319".into()];
    let targets = owner.finish_discovery_facebook_targets_async(targets_claim.clone(),Some(urls),None).await.unwrap(); snapshot("targets-completed.sqlite");
    std::fs::write(output.join("oracle.json"),serde_json::to_vec_pretty(&json!({"campaign":CAMPAIGN,"hit":hit,"identity_claim":identity_claim,"identity":identity,"targets_claim":targets_claim,"targets":targets})).unwrap()).unwrap();
}

#[tokio::test]
async fn export_discovery_facebook_content_node_oracle() {
    let Some(output) = std::env::var_os("ONECREW_FACEBOOK_CONTENT_ORACLE_DIRECTORY") else { return; };
    let output = std::path::PathBuf::from(output); std::fs::create_dir_all(&output).unwrap();
    let (directory, owner, hit) = originals_ready().await;
    let snapshot = |suffix: &str| { let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap(); db.execute("VACUUM INTO ?1", [output.join(format!("{suffix}.sqlite")).to_str().unwrap()]).unwrap(); };
    snapshot("before"); let first = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap(); snapshot("claimed");
    owner.finish_discovery_content(FinishDiscoveryContentInput { claim: first, native_content: None, stop_code: Some("opencli_command_failed".into()) }).unwrap(); snapshot("failed");
    let second = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap(); snapshot("second-claimed");
    owner.finish_discovery_content(FinishDiscoveryContentInput { native_content: Some(json!({"post_id":"2542739356215319","author_id":second.author_id,"source_url":second.source_url,"text":"What's the Worst Tech Linus Still Uses?","text_truncated":false})),claim:second,stop_code:None }).unwrap(); snapshot("after");
}

fn setup() -> (tempfile::TempDir, CreatorRuntime, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: CAMPAIGN.into(),
            brief: "Keyboard authors".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"min_followers":0}),
            target_count: 2,
            max_calls: Some(10),
            scopes: vec![DiscoveryPlatformScope {
                platform: "facebook".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["keyboard".into()],
            }],
        })
        .unwrap();
    assert!(
        owner
            .claim_discovery_facebook_identity(CAMPAIGN, 1, "missing")
            .is_err()
    );
    owner.approve_discovery_campaign(CAMPAIGN, 1).unwrap();
    let claim = owner
        .claim_discovery_search(CAMPAIGN, 1, "facebook", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: ["NativeCreator", "SecondCreator"]
                .iter()
                .map(|name| DiscoverySearchHitInput {
                    source_url: format!("https://www.facebook.com/{name}"),
                    title: "Public profile".into(),
                    author_label: name.to_string(),
                    author_id: None,
                    author_url: None,
                })
                .collect(),
        })
        .unwrap();
    let ids = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: CAMPAIGN.into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    (dir, owner, ids)
}
fn native(claim: &DiscoveryFacebookIdentityClaim) -> Value {
    json!({"adapter_version":"1.0.0","source_url":claim.source_url,"id":"100044572203205","display_name":"Native creator","profile_url":claim.source_url,"evidence_source":"native_profile_header_response"})
}
fn next(owner: &CreatorRuntime) -> String {
    let DiscoveryStep::ReadFacebookIdentity { search_hit_id } =
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap()
    else {
        panic!("unbound saved Facebook source requires native identity first")
    };
    search_hit_id
}

async fn originals_ready() -> (tempfile::TempDir, CreatorRuntime, String) {
    let (dir, owner, _) = setup();
    let hit = next(&owner);
    let identity = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    owner
        .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
            native_identity: Some(native(&identity)),
            claim: identity.clone(),
            stop_code: None,
        })
        .unwrap();
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput{
        native_profile:Some(json!({"adapter_version":"2.0.0","id":claim.author_id,"display_name":"Native creator","follower_count":null,
            "follower_text":"1.3M followers","profile_url":claim.author_url,"observed_profile_url":identity.source_url.trim_end_matches('/'),
            "contact_links":[],"evidence_source":"native_profile_header_response"})),claim,stop_code:None}).unwrap();
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContentTargets {
            search_hit_id: hit.clone()
        }
    );
    let claim = owner
        .claim_discovery_facebook_targets_async(CAMPAIGN.into(), 1, hit.clone())
        .await
        .unwrap();
    owner
        .finish_discovery_facebook_targets_async(
            claim,
            Some(vec![
                "https://www.facebook.com/reel/1075265928641482".into(),
                "https://www.facebook.com/reel/2542739356215319".into(),
            ]),
            None,
        )
        .await
        .unwrap();
    (dir, owner, hit)
}

#[tokio::test]
async fn invalid_original_stops_author_expansion_and_keeps_other_authors_available() {
    let (_dir, owner, hit) = originals_ready().await;
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert!(matches!(owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadFacebookIdentity { search_hit_id } if search_hit_id != hit));
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        5
    );
    assert_eq!(
        owner
            .get_discovery_content(CAMPAIGN, &hit)
            .unwrap()
            .stop_code
            .as_deref(),
        Some("opencli_invalid_output")
    );
}

#[tokio::test]
async fn scoring_one_author_allows_other_identity_without_changing_material() {
    let (_dir, owner, hit) = originals_ready().await;
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    owner.finish_discovery_content(FinishDiscoveryContentInput {
        native_content: Some(json!({"post_id":"1075265928641482","author_id":claim.author_id,
          "source_url":claim.source_url,"text":"Keyboard testing in English","text_truncated":false})),
        claim, stop_code: None,
    }).unwrap();
    let before = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    let deferred = vec!["facebook".into(), "google".into(), "yahoo".into()];
    let step = owner
        .plan_discovery_step_scoring_async(
            CAMPAIGN.into(),
            1,
            vec![],
            deferred.clone(),
            vec![hit.clone()],
        )
        .await
        .unwrap();
    let DiscoveryStep::ReadFacebookIdentity {
        search_hit_id: other,
    } = step
    else {
        panic!("Other saved author must remain available");
    };
    assert_ne!(other, hit);
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        5
    );
    let claim = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &other)
        .unwrap();
    let mut identity = native(&claim);
    identity["id"] = json!("100099999999999");
    owner
        .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
            claim,
            native_identity: Some(identity),
            stop_code: None,
        })
        .unwrap();
    assert_eq!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap()
            .material_hash,
        before.material_hash
    );
    let step = owner
        .plan_discovery_step_scoring_async(
            CAMPAIGN.into(),
            1,
            vec![],
            deferred.clone(),
            vec![hit.clone()],
        )
        .await
        .unwrap();
    assert_eq!(
        step,
        DiscoveryStep::ReadProfile {
            search_hit_id: other.clone()
        }
    );
    // Discovery of an alias of the scoring author must hold its profile too.
    let db = rusqlite::Connection::open(_dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_search_authors SET author_id='100044572203205' WHERE search_hit_id=?1",
        [&other],
    )
    .unwrap();
    assert_eq!(
        owner
            .plan_discovery_step_scoring_async(
                CAMPAIGN.into(),
                1,
                vec![],
                deferred.clone(),
                vec![hit.clone()]
            )
            .await
            .unwrap(),
        DiscoveryStep::Busy
    );
    assert_eq!(
        owner
            .plan_discovery_step_scoring_async(
                CAMPAIGN.into(),
                1,
                vec!["facebook".into()],
                deferred.clone(),
                vec![hit.clone()]
            )
            .await
            .unwrap(),
        DiscoveryStep::Busy
    );
    db.execute(
        "UPDATE discovery_search_authors SET author_id='100099999999999' WHERE search_hit_id=?1",
        [&other],
    )
    .unwrap();
    drop(db);
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &other).unwrap();
    owner.finish_discovery_profile(FinishDiscoveryProfileInput {
        native_profile: Some(json!({"adapter_version":"2.0.0", "id":claim.author_id,
            "display_name":"Other creator", "follower_count":12000, "follower_text":"12,000 followers",
            "profile_url":claim.author_url, "observed_profile_url":claim.source_url.trim_end_matches('/'),
            "contact_links":[], "evidence_source":"native_profile_header_response"})),
        claim, stop_code: None,
    }).unwrap();
    assert_eq!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap()
            .material_hash,
        before.material_hash
    );
    let targets = owner
        .claim_discovery_facebook_targets_async(CAMPAIGN.into(), 1, other.clone())
        .await
        .unwrap();
    owner
        .finish_discovery_facebook_targets_async(
            targets,
            Some(vec![
                "https://www.facebook.com/reel/2542739356215319".into(),
            ]),
            None,
        )
        .await
        .unwrap();
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &other).unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            native_content: Some(
                json!({"post_id":"2542739356215319", "author_id":claim.author_id,
            "source_url":claim.source_url, "text":"My keyboard review", "text_truncated":false}),
            ),
            claim,
            stop_code: None,
        })
        .unwrap();
    let second = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &other)
        .unwrap();
    assert_eq!(
        owner
            .plan_discovery_step_scoring_async(
                CAMPAIGN.into(),
                1,
                vec![],
                deferred,
                vec![hit.clone(), other.clone()]
            )
            .await
            .unwrap(),
        DiscoveryStep::Busy
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        9
    );
    assert_eq!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
            .unwrap()
            .material_hash,
        before.material_hash
    );
    assert_eq!(
        owner
            .prepare_discovery_review_material(CAMPAIGN, 1, &other)
            .unwrap()
            .material_hash,
        second.material_hash
    );
}

#[tokio::test]
async fn failed_native_original_advances_saved_source_once_and_preserves_receipts() {
    let (dir, owner, hit) = originals_ready().await;
    let first = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert!(first.source_url.ends_with("1075265928641482"));
    let failed = FinishDiscoveryContentInput {
        claim: first,
        native_content: None,
        stop_code: Some("opencli_command_failed".into()),
    };
    let receipt = owner.finish_discovery_content(failed.clone()).unwrap();
    assert_eq!(
        owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
        DiscoveryStep::ReadContent {
            search_hit_id: hit.clone()
        }
    );
    let second = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert!(second.source_url.ends_with("2542739356215319"));
    assert_eq!(second.call_number, 6);
    let mut modified = failed.clone();
    modified.stop_code = Some("opencli_timeout".into());
    assert!(owner.finish_discovery_content(modified).is_err());
    assert_eq!(owner.finish_discovery_content(failed).unwrap(), receipt);
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        6
    );
    let outcome = FinishDiscoveryContentInput {
        native_content: Some(
            json!({"post_id":"2542739356215319","author_id":second.author_id,
        "source_url":second.source_url,"text":"What's the Worst Tech Linus Still Uses?","text_truncated":false}),
        ),
        claim: second,
        stop_code: None,
    };
    let success = owner.finish_discovery_content(outcome.clone()).unwrap();
    assert_eq!(owner.finish_discovery_content(outcome).unwrap(), success);
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(material.native_content.as_ref().unwrap(), &success);
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_facebook_original_attempts",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        2
    );
    assert!(db.execute("UPDATE discovery_facebook_original_attempts SET stop_code='opencli_timeout' WHERE status='failed'",[]).is_err());
    drop(db);
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.get_discovery_content(CAMPAIGN, &hit).unwrap(),
        success
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        6
    );
}

#[tokio::test]
async fn interrupted_native_original_is_not_replayed_and_next_saved_source_remains() {
    let (_dir, owner, hit) = originals_ready().await;
    let first = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
    assert_eq!(
        owner
            .get_discovery_content(CAMPAIGN, &hit)
            .unwrap()
            .stop_code
            .as_deref(),
        Some("interrupted_content_read")
    );
    let second = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    assert_ne!(first.source_url, second.source_url);
    assert_eq!(second.call_number, 6);
}

#[tokio::test]
async fn native_original_risk_stops_platform_without_advancing_source() {
    let (_dir, owner, hit) = originals_ready().await;
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            claim,
            native_content: None,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    assert!(owner.claim_discovery_content(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        5
    );
}

#[test]
fn native_vanity_profile_requires_identity_and_preserves_observed_contact_routes() {
    let (dir, owner, _) = setup();
    let hit = next(&owner);
    let identity = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    owner
        .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
            native_identity: Some(native(&identity)),
            claim: identity.clone(),
            stop_code: None,
        })
        .unwrap();
    let claim = owner.claim_discovery_profile(CAMPAIGN, 1, &hit).unwrap();
    let base = identity.source_url.trim_end_matches('/');
    let value = json!({"adapter_version":"2.0.0","id":claim.author_id,"display_name":"Native creator","follower_count":null,
        "follower_text":"1.3M followers","profile_url":claim.author_url,"observed_profile_url":base,
        "contact_links":[format!("{base}/directory_contact_info"),format!("{base}/directory_links")],"evidence_source":"native_profile_header_response"});
    let mut wrong = value.clone();
    wrong["observed_profile_url"] = json!("https://www.facebook.com/Other");
    assert!(
        owner
            .finish_discovery_profile(crate::FinishDiscoveryProfileInput {
                claim: claim.clone(),
                native_profile: Some(wrong),
                stop_code: None
            })
            .is_err()
    );
    // Prove the durable boundary also refuses an otherwise valid v2 write when
    // its source identity is absent, independently of the Rust decoder.
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    db.execute(
        "DELETE FROM discovery_facebook_identity_reads WHERE search_hit_id=?1",
        [&hit],
    )
    .unwrap();
    assert!(db.execute("UPDATE discovery_profile_reads SET status='completed',native_profile_json=?2 WHERE search_hit_id=?1",
        rusqlite::params![hit,value.to_string()]).is_err());
    db.execute_batch("ROLLBACK").unwrap();
    let input = crate::FinishDiscoveryProfileInput {
        claim: claim.clone(),
        native_profile: Some(value),
        stop_code: None,
    };
    let receipt = owner.finish_discovery_profile(input.clone()).unwrap();
    let mut changed = input.clone();
    changed.native_profile.as_mut().unwrap()["display_name"] = json!("Changed creator");
    assert!(owner.finish_discovery_profile(changed).is_err());
    assert_eq!(owner.finish_discovery_profile(input).unwrap(), receipt);
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    drop(db);
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.get_discovery_profile(CAMPAIGN, &hit).unwrap(),
        receipt
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        3
    );
}

#[test]
fn vanity_profile_without_identity_receipt_cannot_reserve_provider_budget() {
    let (dir, owner, _) = setup();
    let hit = next(&owner);
    let claim = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    owner
        .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
            native_identity: Some(native(&claim)),
            claim,
            stop_code: None,
        })
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "DELETE FROM discovery_facebook_identity_reads WHERE search_hit_id=?1",
        [&hit],
    )
    .unwrap();
    assert!(owner.claim_discovery_profile(CAMPAIGN, 1, &hit).is_err());
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_profile_reads", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn identity_receipt_binds_budget_source_revision_and_native_id_then_survives_reopen() {
    let (dir, owner, _) = setup();
    let hit = next(&owner);
    assert!(
        owner
            .claim_discovery_facebook_identity(CAMPAIGN, 2, &hit)
            .is_err()
    );
    let claim = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(claim.call_number, 2);
    assert_eq!(claim.profile_ref, "dedicated");
    assert!(
        owner
            .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
            .is_err()
    );
    let input = FinishDiscoveryFacebookIdentityInput {
        native_identity: Some(native(&claim)),
        claim: claim.clone(),
        stop_code: None,
    };
    let read = owner
        .finish_discovery_facebook_identity(input.clone())
        .unwrap();
    assert_eq!(
        owner.finish_discovery_facebook_identity(input).unwrap(),
        read
    );
    let saved = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: CAMPAIGN.into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .into_iter()
        .find(|h| h.id == hit)
        .unwrap();
    assert_eq!(saved.source_url, claim.source_url);
    assert_eq!(saved.author_id.as_deref(), Some("100044572203205"));
    assert_eq!(
        saved.author_url.as_deref(),
        Some("https://www.facebook.com/profile.php?id=100044572203205")
    );
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        0
    );
    assert!(
        matches!(owner.plan_discovery_step(CAMPAIGN,1).unwrap(),DiscoveryStep::ReadProfile{search_hit_id} if search_hit_id==hit)
    );
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner
            .get_discovery_facebook_identity(CAMPAIGN, &hit)
            .unwrap(),
        read
    );
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
    assert!(
        owner
            .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
            .is_err()
    );
}

#[test]
fn native_identity_rejects_wrong_source_native_id_and_private_payload_without_binding_author() {
    let (_dir, owner, _) = setup();
    let hit = next(&owner);
    let claim = owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    for changes in [
        json!({"source_url":"https://www.facebook.com/Other"}),
        json!({"id":"NativeCreator"}),
        json!({"profile_url":"https://www.facebook.com/Other"}),
        json!({"cookie":"forbidden"}),
    ] {
        let mut value = native(&claim);
        value
            .as_object_mut()
            .unwrap()
            .extend(changes.as_object().unwrap().clone());
        assert!(
            owner
                .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
                    claim: claim.clone(),
                    native_identity: Some(value),
                    stop_code: None
                })
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_facebook_identity(CAMPAIGN, &hit)
                .unwrap()
                .status,
            "reading"
        );
    }
    owner
        .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
            claim,
            native_identity: None,
            stop_code: Some("opencli_invalid_output".into()),
        })
        .unwrap();
    assert_ne!(next(&owner), hit);
    assert_eq!(
        owner
            .get_discovery_campaign(CAMPAIGN)
            .unwrap()
            .platform_runs[0]
            .status,
        "completed"
    );
}

#[test]
fn risk_and_auth_stop_the_platform_instead_of_retrying_another_saved_source() {
    for (code, status) in [
        ("opencli_risk_control", "risk_control_stop"),
        ("opencli_auth_required", "unauthorized"),
    ] {
        let (_dir, owner, _) = setup();
        let hit = next(&owner);
        let claim = owner
            .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
            .unwrap();
        owner
            .finish_discovery_facebook_identity(FinishDiscoveryFacebookIdentityInput {
                claim,
                native_identity: None,
                stop_code: Some(code.into()),
            })
            .unwrap();
        assert_eq!(
            owner
                .get_discovery_campaign(CAMPAIGN)
                .unwrap()
                .platform_runs[0]
                .status,
            status
        );
        assert!(!matches!(
            owner.plan_discovery_step(CAMPAIGN, 1).unwrap(),
            DiscoveryStep::ReadFacebookIdentity { .. }
        ));
    }
}

#[test]
fn interrupted_identity_is_failed_once_and_next_source_remains_available() {
    let (_dir, owner, _) = setup();
    let hit = next(&owner);
    owner
        .claim_discovery_facebook_identity(CAMPAIGN, 1, &hit)
        .unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    let read = owner
        .get_discovery_facebook_identity(CAMPAIGN, &hit)
        .unwrap();
    assert_eq!(read.status, "failed");
    assert_eq!(
        read.stop_code.as_deref(),
        Some("interrupted_facebook_identity_read")
    );
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 0);
    assert_ne!(next(&owner), hit);
    assert_eq!(
        owner.get_discovery_campaign(CAMPAIGN).unwrap().used_calls,
        2
    );
}

#[tokio::test]
async fn facebook_scored_author_qualifies_with_native_homepage_without_email() {
    let (_dir, owner, hit) = originals_ready().await;
    let claim = owner.claim_discovery_content(CAMPAIGN, 1, &hit).unwrap();
    let text = "I review keyboards and gaming mice.";
    owner
        .finish_discovery_content(FinishDiscoveryContentInput {
            native_content: Some(
                json!({"post_id":"1075265928641482", "author_id":claim.author_id,
            "source_url":claim.source_url, "text":text, "text_truncated":false}),
            ),
            claim,
            stop_code: None,
        })
        .unwrap();
    let material = owner
        .prepare_discovery_review_material(CAMPAIGN, 1, &hit)
        .unwrap();
    let receipt = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: CAMPAIGN.into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "facebook-homepage".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: DiscoveryReviewCriterion::MatchFit,
                    outcome: DiscoveryReviewOutcome::Supported,
                    match_level: Some(DiscoveryMatchLevel::Good),
                    tool: None,
                    country_code: None,
                    rationale: "Original keyboard reviews match.".into(),
                    citations: vec![DiscoveryReviewQuote::NativeContent { quote: text.into() }],
                }],
            },
        )
        .unwrap();
    assert_eq!(receipt.verdict, "qualified");
    assert_eq!(
        owner
            .count_discovery_candidates(CAMPAIGN)
            .unwrap()
            .qualified,
        1
    );
}
