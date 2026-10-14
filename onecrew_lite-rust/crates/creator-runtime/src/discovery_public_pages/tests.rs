use super::*;
use crate::{
    CreateDiscoveryCampaignInput, DiscoveryPlatformScope, DiscoverySearchHitInput,
    FinishDiscoverySearchInput, FinishDiscoveryVideoInput, ListDiscoveryCandidatesInput,
    ListDiscoverySearchHitsInput, UpsertDiscoveryCandidateInput,
};
use serde_json::json;

const LINK: &str = "https://example.com/tool";

#[test]
fn contact_navigation_skips_product_links_but_keeps_author_sites() {
    for link in [
        "https://amzn.to/example",
        "https://geni.us/example",
        "https://www.amazon.com/shop/creator",
        "https://brand.example/products/item",
        "https://shop.wuquestudio.com/collections/nama-1",
    ] {
        assert!(is_product_link(link));
    }
    for link in [
        "https://creator.example/contact",
        "https://shop.creator.example/contact",
        "https://artist.example/collections/portraits",
        "https://bit.ly/creator",
        "https://linktr.ee/creator",
        "https://www.youtube.com/channel/UC1234567890",
    ] {
        assert!(!is_product_link(link));
    }
}

#[test]
fn contact_navigation_requires_observed_same_origin_contact_links() {
    assert_eq!(
        contact_page_links(
            "https://creator.example/home",
            "[Contact](/contact-us) https://creator.example/business https://other.example/contact https://creator.example/news"
        ),
        vec![
            "https://creator.example/contact-us",
            "https://creator.example/business"
        ]
    );
    assert!(contact_page_links("https://creator.example/home", "https://user@creator.example/contact https://creator.example:8443/contact https://creator.example/contact...").is_empty());
    assert!(
        contact_page_links("https://creator.example/home", "No observed address here").is_empty()
    );
}

fn setup(path: &std::path::Path, maximum: u32) -> (CreatorRuntime, DiscoveryPublicPageInput) {
    setup_with_criteria(
        path,
        maximum,
        json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
    )
}

fn setup_with_criteria(
    path: &std::path::Path,
    maximum: u32,
    criteria: serde_json::Value,
) -> (CreatorRuntime, DiscoveryPublicPageInput) {
    let owner = CreatorRuntime::open(path, crate::CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "pages".into(),
            brief: "AI tool creators".into(),
            opencli_version: "1.8.8".into(),
            criteria,
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["AI tools".into()],
            }],
            target_count: 30,
            max_calls: Some(maximum),
        })
        .unwrap();
    owner.approve_discovery_campaign("pages", 1).unwrap();
    let search = owner
        .claim_discovery_search("pages", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim: search,
            status: "completed".into(),
            stop_code: None,
            search_hits: vec![DiscoverySearchHitInput {
                source_url: "https://www.youtube.com/watch?v=LR04bU_yV5k".into(),
                title: "AI tools".into(),
                author_label: "Creator".into(),
                author_id: None,
                author_url: None,
            }],
        })
        .unwrap();
    let hit = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "pages".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    let video = owner
        .claim_discovery_video("pages", 1, "dedicated", &hit.id)
        .unwrap();
    owner.finish_discovery_video(FinishDiscoveryVideoInput {claim:video,stop_code:None,candidate:Some(UpsertDiscoveryCandidateInput {
        campaign_id:"pages".into(),expected_revision:1,platform:"youtube".into(),source_profile_ref:"dedicated".into(),
        platform_account_id:"UC1234567890".into(),display_name:"Creator".into(),profile_url:"https://www.youtube.com/channel/UC1234567890".into(),
        evidence:json!({"source_urls":[hit.source_url],"subscribers_raw":"12.4K subscribers","subscribers_source_url":hit.source_url,"captured_at":100,
            "video_description":format!("Join https://discord.gg/2abyjxr4v6 . The tool I use: ({LINK})."),"public_links":["https://discord.gg/2abyjxr4v6",LINK,"https://unobserved.example.com/"]}),
    })}).unwrap();
    let candidate = owner
        .list_discovery_candidates(ListDiscoveryCandidatesInput {
            campaign_id: "pages".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap()
        .remove(0);
    (
        owner,
        DiscoveryPublicPageInput {
            campaign_id: "pages".into(),
            expected_revision: 1,
            profile_ref: "dedicated".into(),
            candidate_id: candidate.id,
            requested_url: LINK.into(),
        },
    )
}

#[test]
fn page_planning_respects_customer_scope_rejection_and_remaining_budget() {
    for (maximum, criteria) in [
        (
            2,
            json!({"min_followers":10000,"requires_public_tool_url":true}),
        ),
        (
            10,
            json!({"min_followers":10000,"requires_public_tool_url":false}),
        ),
        (
            10,
            json!({"min_followers":100000,"requires_public_tool_url":true}),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (owner, _) = setup_with_criteria(directory.path(), maximum, criteria);
        assert!(
            owner
                .repository
                .next_discovery_public_page("pages", &[])
                .unwrap()
                .is_none()
        );
        assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 2);
    }
}

#[test]
fn residence_campaign_can_read_an_observed_page_without_requiring_a_tool_link() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup_with_criteria(
        directory.path(),
        10,
        json!({"min_followers":10000,"requires_ai_tool_demo":false,
            "requires_public_tool_url":false,"region":"中国大陆以外"}),
    );
    assert_eq!(
        owner
            .repository
            .next_discovery_public_page("pages", &[])
            .unwrap(),
        Some(input.clone())
    );
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 2);
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    assert_eq!(claim.call_number, 3);
    let read = owner
        .finish_discovery_public_page(completed(claim))
        .unwrap();
    assert_eq!(read.status, "completed");
    assert_eq!(read.requested_url, LINK);
    assert_eq!(
        owner.count_discovery_candidates("pages").unwrap().qualified,
        0
    );
    assert!(owner.claim_discovery_public_page(input).is_err());
}

fn completed(claim: DiscoveryPublicPageClaim) -> FinishDiscoveryPublicPageInput {
    FinishDiscoveryPublicPageInput {
        hidden_email_available: false,
        claim,
        document_url: Some("https://example.com/tool".into()),
        title: "Tool".into(),
        markdown_excerpt: "An observed AI tool page.".into(),
        truncated: false,
        stop_code: None,
    }
}

#[test]
fn public_page_requires_permanent_author_binding_before_budget_and_completion() {
    for corruption in [
        "DELETE FROM discovery_video_identity_bindings",
        "UPDATE discovery_video_identity_bindings SET native_author_id='UCotherchannel'",
        "UPDATE discovery_candidates SET platform_account_id='UCchangedcandidate'",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (owner, input) = setup(directory.path(), 10);
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        db.execute(corruption, []).unwrap();
        assert!(
            owner
                .repository
                .next_discovery_public_page("pages", &[])
                .unwrap()
                .is_none()
        );
        assert!(owner.claim_discovery_public_page(input).is_err());
        assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 2);
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM discovery_public_page_reads",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    for corruption in [
        "DELETE FROM discovery_video_identity_bindings",
        "UPDATE discovery_video_identity_bindings SET native_author_id='UCotherchannel'",
        "UPDATE discovery_candidates SET platform_account_id='UCchangedcandidate'",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (owner, input) = setup(directory.path(), 10);
        let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
        let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
        db.execute(corruption, []).unwrap();
        assert!(
            owner
                .finish_discovery_public_page(completed(claim))
                .is_err()
        );
        let status: String = db
            .query_row(
                "SELECT status FROM discovery_public_page_reads",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "reading");
        assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 3);
    }
}

#[test]
fn planner_proposes_observed_unattempted_page_without_spending_or_qualifying() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup(directory.path(), 10);
    let crate::DiscoveryStep::ReadTranscript {
        profile_ref,
        search_hit_id,
    } = owner.plan_discovery_step("pages", 1).unwrap()
    else {
        panic!("transcript precedes public page evidence");
    };
    let claim = owner
        .claim_discovery_transcript("pages", 1, &profile_ref, &search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(crate::FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let expected = crate::DiscoveryStep::ReadPublicPage {
        profile_ref: input.profile_ref.clone(),
        candidate_id: input.candidate_id.clone(),
        requested_url: LINK.into(),
    };
    assert_eq!(owner.plan_discovery_step("pages", 1).unwrap(), expected);
    assert_eq!(owner.plan_discovery_step("pages", 1).unwrap(), expected);
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 3);
    assert!(owner.plan_discovery_step("pages", 2).is_err());
    let page = owner.claim_discovery_public_page(input).unwrap();
    assert_eq!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::Busy
    );
    owner.finish_discovery_public_page(completed(page)).unwrap();
    assert_eq!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::NeedsEvidence
    );
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 4);
    assert_eq!(
        owner.count_discovery_candidates("pages").unwrap().qualified,
        0
    );
    drop(owner);
    let reopened =
        CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::NeedsEvidence
    );
}

#[test]
fn public_page_reservation_requires_exact_observed_source_and_budget() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup(directory.path(), 10);
    for field in [
        "campaign",
        "revision",
        "profile",
        "candidate",
        "url",
        "retained",
        "community",
    ] {
        let mut wrong = input.clone();
        match field {
            "campaign" => wrong.campaign_id = "other".into(),
            "revision" => wrong.expected_revision = 2,
            "profile" => wrong.profile_ref = "other".into(),
            "candidate" => wrong.candidate_id = "missing".into(),
            "retained" => wrong.requested_url = "https://unobserved.example.com/".into(),
            "community" => wrong.requested_url = "https://discord.gg/2abyjxr4v6".into(),
            _ => wrong.requested_url = "https://example.com/tool-extra".into(),
        }
        assert!(owner.claim_discovery_public_page(wrong).is_err(), "{field}");
    }
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 2);
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    assert_eq!(claim.call_number, 3);
    assert!(owner.claim_discovery_public_page(input.clone()).is_err());
    let mut wrong = completed(claim.clone());
    wrong.claim.source_url = "https://www.youtube.com/watch?v=00000000000".into();
    assert!(owner.finish_discovery_public_page(wrong).is_err());
    let mut too_large = completed(claim.clone());
    too_large.markdown_excerpt = "x".repeat(20_001);
    assert!(owner.finish_discovery_public_page(too_large).is_err());
    let read = owner
        .finish_discovery_public_page(completed(claim))
        .unwrap();
    assert_eq!(read.status, "completed");
    assert!(read.source_excerpt.contains(LINK));
    assert!(owner.claim_discovery_public_page(input.clone()).is_err());
    assert_eq!(
        owner.count_discovery_candidates("pages").unwrap().qualified,
        0
    );
    drop(owner);
    let reopened =
        CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .get_discovery_public_page("pages", &input.candidate_id, LINK)
            .unwrap(),
        read
    );
    assert!(
        reopened
            .get_discovery_public_page("other", &input.candidate_id, LINK)
            .is_err()
    );
    assert_eq!(
        reopened.get_discovery_campaign("pages").unwrap().used_calls,
        3
    );
    let other = tempfile::tempdir().unwrap();
    let (exhausted, target) = setup(other.path(), 2);
    assert!(exhausted.claim_discovery_public_page(target).is_err());
    assert_eq!(
        exhausted
            .get_discovery_campaign("pages")
            .unwrap()
            .used_calls,
        2
    );
}

#[test]
fn public_page_interruption_recharges_and_failed_reads_do_not_retry() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup(directory.path(), 10);
    let old = owner.claim_discovery_public_page(input.clone()).unwrap();
    drop(owner);
    let owner = CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    owner.interrupt_discovery_searches().unwrap();
    assert_eq!(
        owner.get_discovery_campaign("pages").unwrap().platform_runs[0]
            .stop_code
            .as_deref(),
        Some("interrupted_public_page_read")
    );
    assert_eq!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::ReadPublicPage {
            profile_ref: input.profile_ref.clone(),
            candidate_id: input.candidate_id.clone(),
            requested_url: LINK.into(),
        }
    );
    let fresh = owner.claim_discovery_public_page(input.clone()).unwrap();
    assert_eq!(fresh.call_number, 4);
    assert!(owner.finish_discovery_public_page(completed(old)).is_err());
    let failed = owner
        .finish_discovery_public_page(FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim: fresh,
            document_url: None,
            title: String::new(),
            markdown_excerpt: String::new(),
            truncated: false,
            stop_code: Some("opencli_risk_control".into()),
        })
        .unwrap();
    assert_eq!(failed.status, "failed");
    assert!(owner.claim_discovery_public_page(input.clone()).is_err());
    assert_eq!(
        owner.get_discovery_campaign("pages").unwrap().platform_runs[0].status,
        "risk_control_stop"
    );
    assert_eq!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::NeedsEvidence
    );
    drop(owner);
    let owner = CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        owner
            .get_discovery_public_page("pages", &input.candidate_id, LINK)
            .unwrap(),
        failed
    );
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 4);
    assert_eq!(
        owner.count_discovery_candidates("pages").unwrap().qualified,
        0
    );
}

#[test]
fn shortened_or_prefix_links_cannot_acquire_provenance() {
    assert!(!description_observes_link(
        "https://example.com/tool...",
        LINK
    ));
    assert!(!description_observes_link(
        "https://example.com/tool…",
        LINK
    ));
    assert!(!description_observes_link(
        "https://example.com/tool-extra",
        LINK
    ));
    assert!(description_observes_link(
        "Use (https://example.com/tool).",
        LINK
    ));
}

#[test]
fn invitation_filter_is_url_scoped_not_a_substring_or_tool_certification() {
    for link in [
        "https://discord.gg/code",
        "https://www.discord.gg/code",
        "https://discord.com/invite/code",
        "https://discordapp.com/invite/code",
    ] {
        assert!(is_community_invite_link(link), "{link}");
    }
    for link in [
        "https://example.com/tool",
        "https://discord.gg.example.com/tool",
        "https://discord.com/developers/applications",
        "https://example.com/?ref=discord.gg/code",
    ] {
        assert!(!is_community_invite_link(link), "{link}");
    }
}

#[test]
fn invalid_provider_page_is_settled_without_busy_or_replay() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup(directory.path(), 10);
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    let mut response = completed(claim);
    response.markdown_excerpt.clear();
    let read = owner.settle_discovery_public_page(response).unwrap();
    assert_eq!(read.status, "failed");
    assert_eq!(read.stop_code.as_deref(), Some("opencli_invalid_output"));
    assert_ne!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::Busy
    );
    assert!(owner.claim_discovery_public_page(input).is_err());
    assert_eq!(owner.get_discovery_campaign("pages").unwrap().used_calls, 3);
}

#[test]
fn qualified_contact_page_restart_releases_busy_without_replay() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, input) = setup(directory.path(), 10);
    // Stand in for an already audited candidate; recovery must never create
    // recommendation evidence or dispatch a new provider call.
    let db = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    db.execute(
        "UPDATE discovery_candidates SET verdict='qualified' WHERE id=?1",
        [&input.candidate_id],
    )
    .unwrap();
    drop(db);
    drop(owner);
    let reopened =
        CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 1);
    let campaign = reopened.get_discovery_campaign("pages").unwrap();
    assert_eq!(campaign.used_calls, 3);
    assert_eq!(campaign.platform_runs[0].status, "completed");
    assert!(
        reopened
            .finish_discovery_public_page(completed(claim))
            .is_err()
    );
    let read = reopened
        .get_discovery_public_page("pages", &input.candidate_id, LINK)
        .unwrap();
    assert_eq!(read.status, "failed");
    assert!(reopened.claim_discovery_public_page(input).is_err());
    assert_eq!(reopened.interrupt_discovery_searches().unwrap(), 0);
    assert_eq!(
        reopened.get_discovery_campaign("pages").unwrap().used_calls,
        3
    );
}

#[test]
fn excluded_platform_skips_saved_transcript_and_public_pages_without_erasing_history() {
    let directory = tempfile::tempdir().unwrap();
    let (owner, _) = setup(directory.path(), 10);
    let before = owner.get_discovery_campaign("pages").unwrap();
    let counts = owner.count_discovery_candidates("pages").unwrap();
    assert!(
        owner
            .repository
            .next_discovery_public_page("pages", &[])
            .unwrap()
            .is_some()
    );
    assert!(
        owner
            .repository
            .next_discovery_public_page("pages", &["youtube".into()])
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        owner.plan_discovery_step("pages", 1).unwrap(),
        crate::DiscoveryStep::ReadTranscript { .. }
    ));
    assert!(matches!(
        owner
            .plan_discovery_step_excluding("pages", 1, &["youtube".into()])
            .unwrap(),
        crate::DiscoveryStep::SearchExhausted
    ));
    assert_eq!(owner.get_discovery_campaign("pages").unwrap(), before);
    assert_eq!(owner.count_discovery_candidates("pages").unwrap(), counts);
    drop(owner);
    let owner = CreatorRuntime::open(directory.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(owner.count_discovery_candidates("pages").unwrap(), counts);
    assert!(
        owner
            .repository
            .next_discovery_public_page("pages", &[])
            .unwrap()
            .is_some()
    );
}
