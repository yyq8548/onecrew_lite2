use super::*;
use crate::{
    CreateDiscoveryCampaignInput, DiscoveryPlatformScope, DiscoverySearchHitInput, DiscoveryStep,
    FinishDiscoverySearchInput, FinishDiscoveryVideoInput, ListDiscoverySearchHitsInput,
    UpsertDiscoveryCandidateInput,
};
use serde_json::json;

fn setup(directory: &std::path::Path) -> (CreatorRuntime, String) {
    let owner = CreatorRuntime::open(directory, crate::CreatorGateways::default()).unwrap();
    owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
        id:"transcript".into(),brief:"AI tool creators".into(),opencli_version:"1.8.8".into(),
        criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
        scopes:vec![DiscoveryPlatformScope { platform:"youtube".into(),profile_ref:"dedicated".into(),keywords:vec!["AI tools".into()] }],
        target_count:30,max_calls: Some(10),
    }).unwrap();
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", "missing")
            .is_err()
    );
    owner.approve_discovery_campaign("transcript", 1).unwrap();
    let search = owner
        .claim_discovery_search("transcript", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                title: "AI tool demo".into(),
                author_label: "Creator".into(),
                author_id: None,
                author_url: None,
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "transcript".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit.id)
            .is_err()
    );
    let video = owner
        .claim_discovery_video("transcript", 1, "dedicated", &hit.id)
        .unwrap();
    owner.finish_discovery_video(FinishDiscoveryVideoInput { claim:video,stop_code:None,candidate:Some(UpsertDiscoveryCandidateInput {
        campaign_id:"transcript".into(),expected_revision:1,platform:"youtube".into(),source_profile_ref:"dedicated".into(),
        platform_account_id:"UC1234567890".into(),display_name:"Creator".into(),profile_url:"https://www.youtube.com/channel/UC1234567890".into(),
        evidence:json!({"source_urls":[hit.source_url],"subscribers_raw":"12.4K subscribers","subscribers_source_url":hit.source_url,"captured_at":100}),
    }) }).unwrap();
    (owner, hit.id)
}

fn completed(claim: DiscoveryTranscriptClaim) -> FinishDiscoveryTranscriptInput {
    FinishDiscoveryTranscriptInput {
        claim,
        segments: vec![DiscoveryTranscriptSegment {
            timestamp: "0:10".into(),
            speaker: "".into(),
            text: "Here I open the AI tool and type a prompt.".into(),
        }],
        truncated: false,
        stop_code: None,
    }
}

#[test]
fn transcript_requires_permanent_video_binding_before_budget_and_completion() {
    for corruption in [
        "DELETE FROM discovery_video_identity_bindings",
        "UPDATE discovery_video_identity_bindings SET native_author_id='UCotherchannel'",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (owner, hit) = setup(directory.path());
        let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        connection.execute(corruption, []).unwrap();
        assert!(
            owner
                .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_campaign("transcript")
                .unwrap()
                .used_calls,
            2
        );
        assert!(!matches!(
            owner.plan_discovery_step("transcript", 1).unwrap(),
            DiscoveryStep::ReadTranscript { .. }
        ));
        let receipts: i64 = connection
            .query_row("SELECT count(*) FROM discovery_transcript_reads", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(receipts, 0);
    }
    let directory = tempfile::tempdir().unwrap();
    let (owner, hit) = setup(directory.path());
    let claim = owner
        .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
        .unwrap();
    let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute("DELETE FROM discovery_video_identity_bindings", [])
        .unwrap();
    assert!(owner.finish_discovery_transcript(completed(claim)).is_err());
    assert_eq!(
        owner
            .get_discovery_transcript("transcript", &hit)
            .unwrap()
            .status,
        "reading"
    );
    assert_eq!(
        owner
            .get_discovery_campaign("transcript")
            .unwrap()
            .used_calls,
        3
    );
}

#[test]
fn transcript_reservation_restart_and_stale_completion_preserve_evidence_and_budget() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, hit) = setup(directory.path());
    for (campaign, revision, profile, target) in [
        ("other", 1, "dedicated", hit.as_str()),
        ("transcript", 2, "dedicated", hit.as_str()),
        ("transcript", 1, "other", hit.as_str()),
        ("transcript", 1, "dedicated", "missing"),
    ] {
        assert!(
            owner
                .claim_discovery_transcript(campaign, revision, profile, target)
                .is_err()
        );
    }
    assert_eq!(
        owner
            .get_discovery_campaign("transcript")
            .unwrap()
            .used_calls,
        2
    );
    assert_eq!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::ReadTranscript {
            profile_ref: "dedicated".into(),
            search_hit_id: hit.clone()
        }
    );
    let old = owner
        .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
        .unwrap();
    assert_eq!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::Busy
    );
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
            .is_err()
    );
    let mut forged = completed(old.clone());
    forged.claim.source_url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into();
    assert!(owner.finish_discovery_transcript(forged).is_err());
    assert_eq!(
        owner
            .get_discovery_transcript("transcript", &hit)
            .unwrap()
            .status,
        "reading"
    );
    drop(owner);
    let owner = CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(owner.interrupt_discovery_searches().unwrap(), 1);
    assert!(matches!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::ReadTranscript { .. }
    ));
    let fresh = owner
        .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
        .unwrap();
    assert_eq!(fresh.call_number, 4);
    assert!(owner.finish_discovery_transcript(completed(old)).is_err());
    let mut oversized = completed(fresh.clone());
    oversized.segments[0].text = "x".repeat(4001);
    assert!(owner.finish_discovery_transcript(oversized).is_err());
    let saved = owner.finish_discovery_transcript(completed(fresh)).unwrap();
    assert_eq!(saved.status, "completed");
    assert_eq!(saved.platform_account_id, "UC1234567890");
    assert_eq!(saved.segments[0].timestamp, "0:10");
    assert_eq!(
        owner
            .count_discovery_candidates("transcript")
            .unwrap()
            .qualified,
        0
    );
    assert_eq!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::NeedsEvidence
    );
    assert!(owner.get_discovery_transcript("other", &hit).is_err());
    drop(owner);
    let owner = CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.get_discovery_transcript("transcript", &hit).unwrap(),
        saved
    );
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
            .is_err()
    );
    assert_eq!(
        owner
            .get_discovery_campaign("transcript")
            .unwrap()
            .used_calls,
        4
    );
}

#[test]
fn transcript_unavailable_and_failures_are_terminal_without_auto_retry_or_qualification() {
    for (stop, platform_status, read_status) in [
        (None, "completed", "unavailable"),
        (Some("opencli_command_failed"), "completed", "failed"),
        (Some("opencli_auth_required"), "unauthorized", "failed"),
        (Some("opencli_risk_control"), "risk_control_stop", "failed"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (owner, hit) = setup(directory.path());
        let claim = owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
            .unwrap();
        let result = owner
            .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
                claim,
                segments: vec![],
                truncated: false,
                stop_code: stop.map(str::to_owned),
            })
            .unwrap();
        assert_eq!(result.status, read_status);
        assert_eq!(
            owner
                .get_discovery_campaign("transcript")
                .unwrap()
                .platform_runs[0]
                .status,
            platform_status
        );
        drop(owner);
        let owner =
            CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
        assert_eq!(
            owner.plan_discovery_step("transcript", 1).unwrap(),
            DiscoveryStep::NeedsEvidence
        );
        assert!(
            owner
                .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_campaign("transcript")
                .unwrap()
                .used_calls,
            3
        );
        assert_eq!(
            owner
                .count_discovery_candidates("transcript")
                .unwrap()
                .qualified,
            0
        );
    }
}

#[test]
fn transcript_requires_remaining_budget_and_pending_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, hit) = setup(directory.path());
    let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute("UPDATE discovery_campaigns SET used_calls=max_calls", [])
        .unwrap();
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
            .is_err()
    );
    assert_eq!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::BudgetExhausted
    );
    connection
        .execute("UPDATE discovery_campaigns SET used_calls=2", [])
        .unwrap();
    connection
        .execute("UPDATE discovery_candidates SET verdict='rejected'", [])
        .unwrap();
    assert!(
        owner
            .claim_discovery_transcript("transcript", 1, "dedicated", &hit)
            .is_err()
    );
    assert_eq!(
        owner.plan_discovery_step("transcript", 1).unwrap(),
        DiscoveryStep::SearchExhausted
    );
    assert_eq!(
        owner
            .get_discovery_campaign("transcript")
            .unwrap()
            .used_calls,
        2
    );
}
