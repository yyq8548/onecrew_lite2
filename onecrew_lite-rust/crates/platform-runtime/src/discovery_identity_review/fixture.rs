//! Isolated authenticated owner composition; no provider or customer data is used.
use crate::LocalPlatform;
use onecrew_creator_runtime::{
    CreateDiscoveryCampaignInput, DiscoveryIdentityMergeMaterialInput, DiscoveryPlatformScope,
    DiscoveryReviewer, DiscoveryReviewerKind, DiscoverySearchHitInput, FinishDiscoverySearchInput,
    FinishDiscoveryVideoInput, ListDiscoverySearchHitsInput, UpsertDiscoveryCandidateInput,
};
use onecrew_local_domain::*;
use serde_json::json;

pub fn authenticate(platform: &LocalPlatform, account: &str) -> CreatorSyncCredentialMetadata {
    let metadata = CreatorSyncCredentialMetadata {
        installation_id: uuid::Uuid::new_v4(),
        credential_id: uuid::Uuid::new_v4(),
        access_user_id: account.into(),
        access_device_id: "dev_000000000000000000000001".into(),
        source_instance_id: uuid::Uuid::new_v4(),
        access_pairing_id: "pair_000000000000000000000001".into(),
        client_platform: CreatorSyncClientPlatform::Macos,
        app_version: "0.6.28".into(),
        status: CreatorSyncCredentialStatus::Active,
        revision: 1,
        created_at: 100,
        updated_at: 101,
        last_seen_at: Some(101),
        revoked_at: None,
        protocol: CreatorSyncProtocol::ReacherCreatorSync,
        protocol_version: 1,
        capabilities: [CreatorSyncCapability::WriteChunkedQuarantine],
        accepted_bundle: CreatorSyncAcceptedBundle {
            format: "reacher.creator_bundle".into(),
            format_version: 1,
            profile: CreatorSyncBundleProfile::PortablePublic,
            max_snapshot_bytes: 32 * 1024 * 1024,
            max_chunk_bytes: 1024 * 1024,
            max_chunks: 128,
            max_namespace_staged_bytes: 100 * 1024 * 1024,
            max_access_user_staged_bytes: 400 * 1024 * 1024,
        },
    };
    platform
        .vault
        .save_creator_sync_credential(
            &metadata,
            &ServiceCredentialSecret::new(format!("{}_{}", metadata.credential_id, "A".repeat(43))),
        )
        .unwrap();
    metadata
}

pub fn saved_pair(platform: &LocalPlatform) -> DiscoveryIdentityMergeMaterialInput {
    saved_pair_with_author_ids(platform, ["UC1234567890", "UC1234567891"])
}

pub fn saved_pair_with_author_ids(
    platform: &LocalPlatform,
    authors: [&str; 2],
) -> DiscoveryIdentityMergeMaterialInput {
    let owner = platform.creators().unwrap();
    owner
        .create_discovery_campaign(CreateDiscoveryCampaignInput {
            id: "authenticated-review".into(),
            brief: "Shoe reviewers".into(),
            opencli_version: "1.8.8".into(),
            criteria: json!({"category":"shoes","min_subscribers":8000,"language":"","region":"",
            "requires_ai_tool_demo":false,"requires_public_tool_url":false}),
            scopes: vec![DiscoveryPlatformScope {
                platform: "youtube".into(),
                profile_ref: "fixture".into(),
                keywords: vec!["shoes".into()],
            }],
            target_count: 100,
            max_calls: Some(400),
        })
        .unwrap();
    owner
        .approve_discovery_campaign("authenticated-review", 1)
        .unwrap();
    let claim = owner
        .claim_discovery_search("authenticated-review", 1, "youtube", "fixture")
        .unwrap();
    owner
        .finish_discovery_search(FinishDiscoverySearchInput {
            claim,
            status: "completed".into(),
            stop_code: None,
            search_hits: ["LR04bU_yV5k", "WR04bU_yV5k"]
                .map(|video| DiscoverySearchHitInput {
                    source_url: format!("https://www.youtube.com/watch?v={video}"),
                    title: "Running shoe review".into(),
                    author_label: "Same Name".into(),
                    author_id: None,
                    author_url: None,
                })
                .to_vec(),
        })
        .unwrap();
    let hits = owner
        .list_discovery_search_hits(ListDiscoverySearchHitsInput {
            campaign_id: "authenticated-review".into(),
            after_id: None,
            limit: 10,
        })
        .unwrap();
    for (index, hit) in hits.iter().enumerate() {
        let claim = owner
            .claim_discovery_video("authenticated-review", 1, "fixture", &hit.id)
            .unwrap();
        let native = authors[index].to_owned();
        owner.finish_discovery_video(FinishDiscoveryVideoInput {
            claim, stop_code: None, candidate: Some(UpsertDiscoveryCandidateInput {
                campaign_id: "authenticated-review".into(), expected_revision: 1, platform: "youtube".into(),
                source_profile_ref: "fixture".into(), platform_account_id: native.clone(), display_name: "Same Name".into(),
                profile_url: format!("https://www.youtube.com/channel/{native}"),
                evidence: json!({"source_urls":[hit.source_url],"subscribers_raw":(["20K subscribers","592 subscribers"][index]),
                    "subscribers_source_url":hit.source_url,"captured_at":100,
                    "video_description":format!("My running shoe review. My other account, also owned by me: https://www.youtube.com/channel/{}",authors[1-index]),
                    "public_links":[]}),
            }),
        }).unwrap();
    }
    DiscoveryIdentityMergeMaterialInput {
        campaign_id: "authenticated-review".into(),
        expected_revision: 1,
        left_search_hit_id: hits[0].id.clone(),
        right_search_hit_id: hits[1].id.clone(),
    }
}

pub fn reviewer() -> DiscoveryReviewer {
    DiscoveryReviewer {
        kind: DiscoveryReviewerKind::Human,
        label: "fixture reviewer".into(),
    }
}

pub fn submission(
    pair: DiscoveryIdentityMergeMaterialInput,
    material: onecrew_creator_runtime::DiscoveryIdentityMergeMaterial,
) -> onecrew_creator_runtime::SubmitDiscoveryIdentityReviewInput {
    use onecrew_creator_runtime::{
        DiscoveryIdentityDecision, DiscoveryIdentityFinding, DiscoveryIdentityQuote,
        DiscoveryIdentityRelationship,
    };
    onecrew_creator_runtime::SubmitDiscoveryIdentityReviewInput {
        pair, material_hash: material.material_hash, decision_id: "merge".into(), reviewer: reviewer(),
        decision: DiscoveryIdentityDecision::SameAuthor,
        rationale: "Both saved author-owned descriptions explicitly identify the other bound account as their own.".into(),
        findings: material.accounts.each_ref().map(|account| DiscoveryIdentityFinding {
            source_search_hit_id: account.hit.id.clone(), relationship: DiscoveryIdentityRelationship::SelfOwnedAccount,
            citation: DiscoveryIdentityQuote::Description { quote: account.video_description.clone().unwrap() },
        }).to_vec(),
    }
}
