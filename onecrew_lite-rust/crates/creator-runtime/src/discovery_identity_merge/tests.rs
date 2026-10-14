use super::*;
use crate::{
    CreateDiscoveryCampaignInput, CreatorGateways, DiscoveryPlatformScope, DiscoverySearchHitInput,
    DiscoveryTranscriptSegment, FinishDiscoverySearchInput, FinishDiscoveryTranscriptInput,
    FinishDiscoveryVideoInput, ListDiscoverySearchHitsInput, UpsertDiscoveryCandidateInput,
};
use serde_json::json;

pub(crate) fn identity_review_input(
    owner: &CreatorRuntime,
    pair: DiscoveryIdentityMergeMaterialInput,
) -> SubmitDiscoveryIdentityReviewInput {
    super::submission::tests::input(owner, pair)
}

pub(crate) fn setup(
    path: &std::path::Path,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    setup_relations(path, None, ["20K subscribers"; 2], false)
}

pub(crate) fn setup_with_owned_links(
    path: &std::path::Path,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    setup_relations(
        path,
        Some("My other account, also owned by me:"),
        ["20K subscribers"; 2],
        false,
    )
}

pub(crate) fn setup_with_other_links(
    path: &std::path::Path,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    setup_relations(
        path,
        Some("My collaborator, a different person:"),
        ["20K subscribers"; 2],
        false,
    )
}

pub(crate) fn setup_with_mixed_followers(
    path: &std::path::Path,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    setup_relations(
        path,
        Some("My other account, also owned by me:"),
        ["20K subscribers", "592 subscribers"],
        true,
    )
}

pub(crate) fn setup_shampoo_owned_links(
    path: &std::path::Path,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    setup_relations(
        path,
        Some("My other account, also owned by me:"),
        ["20K subscribers"; 2],
        true,
    )
}

fn setup_relations(
    path: &std::path::Path,
    relation: Option<&str>,
    subscribers: [&str; 2],
    shampoo_only: bool,
) -> (CreatorRuntime, DiscoveryIdentityMergeMaterialInput) {
    let owner = CreatorRuntime::open(path, CreatorGateways::default()).unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "merge-review".into(),
            brief: "Shampoo creators".into(),
            opencli_version: "1.8.8".into(),
            criteria: if shampoo_only {
                json!({"category":"shampoo","min_subscribers":10000,"language":"","region":"",
                    "requires_ai_tool_demo":false,"requires_public_tool_url":false})
            } else {
                json!({"category":"shampoo","min_subscribers":10000})
            },
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "dedicated".into(),
                keywords: vec!["shampoo".into()],
            }],
            target_count: 200,
            max_calls: Some(4),
        })
        .unwrap();
    owner.approve_discovery_campaign("merge-review", 1).unwrap();
    let claim = owner
        .claim_discovery_search("merge-review", 1, "youtube", "dedicated")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: ["LR04bU_yV5k", "WR04bU_yV5k"]
                .map(|video| DiscoverySearchHitInput {
                    source_url: format!("https://www.youtube.com/watch?v={video}"),
                    title: "Shampoo".into(),
                    author_label: "Same Name".into(),
                    author_id: None,
                    author_url: None,
                })
                .to_vec(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "merge-review".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    for (index, hit) in hits.iter().enumerate() {
        let claim = owner
            .claim_discovery_video("merge-review", 1, "dedicated", &hit.id)
            .unwrap();
        let account = format!("UC123456789{index}");
        owner.finish_discovery_video(FinishDiscoveryVideoInput {claim,stop_code:None,
            candidate:Some(UpsertDiscoveryCandidateInput {
                campaign_id:"merge-review".into(),expected_revision:1,platform:"youtube".into(),source_profile_ref:"dedicated".into(),
                platform_account_id:account.clone(),display_name:"Same Name".into(),profile_url:format!("https://www.youtube.com/channel/{account}"),
                evidence:json!({"source_urls":[hit.source_url],"subscribers_raw":subscribers[index],"subscribers_source_url":hit.source_url,
                    "captured_at":100,"video_description":relation.map(|text| format!("My shampoo review. {text} https://www.youtube.com/channel/UC123456789{}",1-index)).unwrap_or_else(||"My shampoo review. Sponsor: https://example.com/brand".into()),"public_links":["https://example.com/brand"]}),
            }),
        }).unwrap();
    }
    (
        owner,
        DiscoveryIdentityMergeMaterialInput {
            campaign_id: "merge-review".into(),
            expected_revision: 1,
            left_search_hit_id: hits[0].id.clone(),
            right_search_hit_id: hits[1].id.clone(),
        },
    )
}

#[test]
fn native_source_pair_is_read_only_restart_stable_and_order_independent() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = setup(dir.path());
    let counts = owner.count_discovery_candidates("merge-review").unwrap();
    let campaign = owner.get_discovery_campaign("merge-review").unwrap();
    let pair = owner
        .prepare_discovery_identity_merge_material(input.clone())
        .unwrap();
    let reverse = owner
        .prepare_discovery_identity_merge_material(DiscoveryIdentityMergeMaterialInput {
            left_search_hit_id: input.right_search_hit_id.clone(),
            right_search_hit_id: input.left_search_hit_id.clone(),
            ..input.clone()
        })
        .unwrap();
    assert_eq!(pair.material_hash, reverse.material_hash);
    assert_ne!(
        pair.accounts[0].native_author_id,
        pair.accounts[1].native_author_id
    );
    // Same names and shared sponsor links neither merge accounts nor qualify anyone.
    assert_eq!(
        owner.count_discovery_candidates("merge-review").unwrap(),
        counts
    );
    assert_eq!(
        owner.get_discovery_campaign("merge-review").unwrap(),
        campaign
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .prepare_discovery_identity_merge_material(input)
            .unwrap()
            .material_hash,
        pair.material_hash
    );
}

#[test]
fn new_source_evidence_invalidates_the_identity_review_hash() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = setup(dir.path());
    let before = owner
        .prepare_discovery_identity_merge_material(input.clone())
        .unwrap();
    let claim = owner
        .claim_discovery_transcript("merge-review", 1, "dedicated", &input.left_search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "Another account is my collaborator, not me.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let after = owner
        .prepare_discovery_identity_merge_material(input)
        .unwrap();
    assert_ne!(after.material_hash, before.material_hash);
}

#[test]
fn missing_same_or_wrong_revision_sources_fail_without_spending_budget() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = setup(dir.path());
    let campaign = owner.get_discovery_campaign("merge-review").unwrap();
    for invalid in [
        DiscoveryIdentityMergeMaterialInput {
            right_search_hit_id: input.left_search_hit_id.clone(),
            ..input.clone()
        },
        DiscoveryIdentityMergeMaterialInput {
            right_search_hit_id: "missing".into(),
            ..input.clone()
        },
        DiscoveryIdentityMergeMaterialInput {
            expected_revision: 2,
            ..input.clone()
        },
        DiscoveryIdentityMergeMaterialInput {
            campaign_id: "other-campaign".into(),
            ..input.clone()
        },
    ] {
        assert!(
            owner
                .prepare_discovery_identity_merge_material(invalid)
                .is_err()
        );
    }
    assert_eq!(
        owner.get_discovery_campaign("merge-review").unwrap(),
        campaign
    );
}

#[test]
fn duplicate_native_account_or_changed_frozen_criteria_cannot_form_a_pair() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = setup(dir.path());
    let left = owner
        .prepare_discovery_review_material("merge-review", 1, &input.left_search_hit_id)
        .unwrap();
    let right = owner
        .prepare_discovery_review_material("merge-review", 1, &input.right_search_hit_id)
        .unwrap();
    let mut duplicate = right.clone();
    duplicate.native_author_id = left.native_author_id.clone();
    assert_eq!(
        prepare_pair(&input, left.clone(), duplicate).unwrap_err(),
        "identity_merge_same_account"
    );
    let mut changed = right;
    changed.criteria["category"] = json!("shoes");
    assert_eq!(
        prepare_pair(&input, left, changed).unwrap_err(),
        "identity_merge_source_changed"
    );
}

#[tokio::test]
async fn asynchronous_owner_returns_the_same_native_material() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = setup(dir.path());
    let synchronous = owner
        .prepare_discovery_identity_merge_material(input.clone())
        .unwrap();
    let asynchronous = owner
        .prepare_discovery_identity_merge_material_async(input)
        .await
        .unwrap();
    assert_eq!(synchronous.material_hash, asynchronous.material_hash);
}

#[test]
fn callers_cannot_supply_native_facts_or_an_identity_verdict() {
    for field in [
        "accounts",
        "native_author_id",
        "verdict",
        "operator_account_id",
    ] {
        let mut input = json!({"campaign_id":"campaign","expected_revision":1,"left_search_hit_id":"a","right_search_hit_id":"b"});
        input[field] = json!("forged");
        assert!(serde_json::from_value::<DiscoveryIdentityMergeMaterialInput>(input).is_err());
    }
}
