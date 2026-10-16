use super::*;
use crate::CreatorGateways;
use rusqlite::Connection;

const LINK: &str = "https://example.com/tool";

#[test]
fn requirements_bind_each_finding_to_real_saved_author_material() {
    // 2026-10-15 通过正式 MCP 读取的 Pinterest 原生资料，未模拟平台响应。
    let mut material: DiscoveryReviewMaterial = serde_json::from_str(include_str!(
        "pinterest-native-20261015.json"
    )).unwrap();
    material.criteria["requirements"] = json!([{
        "description":"公开简介提到 Makeup", "excerpt":"公开简介提到 Makeup"
    }]);
    let finding = DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::Requirement(0),
        outcome: DiscoveryReviewOutcome::Supported,
        rationale: "原生简介明确出现该词。".into(),
        citations: vec![DiscoveryReviewQuote::NativeProfile {
            profile_url: material.native_profile.as_ref().unwrap().profile_url.clone(),
            quote: "Artist & Makeup Lover".into(),
        }],
        tool: None, country_code: None, match_level: None,
    };
    let mut request = SubmitDiscoveryReviewInput {
        campaign_id: material.campaign_id.clone(), expected_revision: material.criteria_revision,
        search_hit_id: material.hit.id.clone(), material_hash: material.material_hash.clone(),
        request_id: "requirement-citation-check".into(),
        reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "Codex".into() },
        findings: vec![finding],
    };
    let mut evidence = json!({});
    apply_requirements(&material, &request, &mut evidence, "review").unwrap();
    assert_eq!(evidence["reviewed_requirements"][0]["native_author_id"], material.native_author_id);
    assert_eq!(evidence["reviewed_requirements"][0]["requirement"], material.criteria["requirements"][0]);
    request.findings[0].criterion = DiscoveryReviewCriterion::Requirement(1);
    assert_eq!(apply_requirements(&material, &request, &mut evidence, "review").unwrap_err(), "review_requirements_mismatch");
    request.findings[0].criterion = DiscoveryReviewCriterion::Requirement(0);
    request.findings[0].citations = vec![DiscoveryReviewQuote::NativeContent { quote: "未读取的文本不能作为证据".into() }];
    assert_eq!(apply_requirements(&material, &request, &mut evidence, "review").unwrap_err(), "review_requirement_citation_invalid");
    request.findings.clear();
    assert_eq!(apply_requirements(&material, &request, &mut evidence, "review").unwrap_err(), "review_requirements_incomplete");
}

#[test]
fn scored_youtube_author_can_read_own_channel_bio_before_username_search() {
    for through_website in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (owner, hit) = super::super::tests::setup_with_description(
            dir.path(),
            json!({"min_followers":10000}),
            "UC1234567890",
            10,
            None,
        );
        super::super::tests::add_receipts(&owner, &hit);
        let candidate_id = owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap()
            .candidate_id;
        let page = crate::DiscoveryPublicPageInput {
            campaign_id: "review".into(),
            expected_revision: 1,
            profile_ref: "dedicated".into(),
            candidate_id,
            requested_url: "https://www.youtube.com/channel/UC1234567890".into(),
        };
        // A native channel ID alone does not authorize this contact read.
        assert!(owner.claim_discovery_public_page(page.clone()).is_err());
        let mut request = input(&owner, &hit);
        request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
        request.findings = vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Saved transcript supports the requested topic.".into(),
            citations: vec![DiscoveryReviewQuote::Transcript {
                segment_index: 0,
                quote: "I type a prompt.".into(),
            }],
            tool: None,
            country_code: None,
            match_level: Some(DiscoveryMatchLevel::Good),
        }];
        owner.submit_discovery_review("operator", request).unwrap();
        assert!(matches!(owner.plan_discovery_step("review", 1).unwrap(),
        crate::DiscoveryStep::ReadPublicPage { requested_url, .. } if requested_url == page.requested_url));
        let mut wrong = page.clone();
        wrong.requested_url = "https://www.youtube.com/channel/UC_OTHER_CHANNEL".into();
        assert!(owner.claim_discovery_public_page(wrong).is_err());
        let before = owner.get_discovery_campaign("review").unwrap().used_calls;
        let claim = owner.claim_discovery_public_page(page.clone()).unwrap();
        assert_eq!(claim.call_number, before + 1);
        owner
            .finish_discovery_public_page(crate::FinishDiscoveryPublicPageInput {
                hidden_email_available: false,
                claim,
                document_url: Some(page.requested_url.clone()),
                title: "Creator".into(),
                markdown_excerpt: if through_website {
                    "Official website: https://creator.example/contact".into()
                } else {
                    "Business: studio@example.org https://t.me/creator_bot".into()
                },
                truncated: true,
                stop_code: None,
            })
            .unwrap();
        assert!(owner.claim_discovery_public_page(page).is_err());
        if through_website {
            let crate::DiscoveryStep::ReadPublicPage {
                candidate_id,
                profile_ref,
                requested_url,
            } = owner.plan_discovery_step("review", 1).unwrap()
            else {
                panic!("native channel website must precede username lookup");
            };
            assert_eq!(requested_url, "https://creator.example/contact");
            let claim = owner
                .claim_discovery_public_page(crate::DiscoveryPublicPageInput {
                    campaign_id: "review".into(),
                    expected_revision: 1,
                    candidate_id,
                    profile_ref,
                    requested_url: requested_url.clone(),
                })
                .unwrap();
            owner
                .finish_discovery_public_page(crate::FinishDiscoveryPublicPageInput {
                    hidden_email_available: false,
                    claim,
                    document_url: Some(requested_url),
                    title: "Contact".into(),
                    markdown_excerpt: "Business: studio@example.org https://t.me/creator_bot"
                        .into(),
                    truncated: false,
                    stop_code: None,
                })
                .unwrap();
        }
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            1
        );
        let publication = owner
            .prepare_discovery_library_publication("review", 1)
            .unwrap();
        assert_eq!(publication.material.sources.rows[0].contacts.len(), 2);
    }
}

#[test]
fn scored_author_contact_page_is_audited_once_and_updates_recommendation_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, hit) = super::super::tests::setup_with_description(
        dir.path(),
        json!({"min_followers":10000}),
        "UC1234567890",
        10,
        None,
    );
    super::super::tests::add_receipts(&owner, &hit);
    const CONTACT_URL: &str = "https://creator.example/contact";
    // Fixture adds a native observed website before scoring, never a guessed link.
    Connection::open(dir.path().join("crm.db")).unwrap().execute(
        "UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.video_description',?1,'$.public_links',json(?2))",
        rusqlite::params![format!("The tool: {LINK} Website: {CONTACT_URL}"), json!([LINK,CONTACT_URL]).to_string()],
    ).unwrap();
    let mut request = input(&owner, &hit);
    request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
    request.findings = vec![DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Supported,
        rationale: "Saved original transcript supports the requested content.".into(),
        citations: vec![DiscoveryReviewQuote::Transcript {
            segment_index: 0,
            quote: "I type a prompt.".into(),
        }],
        tool: None,
        country_code: None,
        match_level: Some(DiscoveryMatchLevel::Good),
    }];
    let receipt = owner.submit_discovery_review("operator", request).unwrap();
    assert_eq!(
        owner
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        0
    );
    let channel_url = "https://www.youtube.com/channel/UC1234567890";
    assert!(matches!(
        owner.plan_discovery_step("review", 1).unwrap(),
        crate::DiscoveryStep::ReadPublicPage { requested_url, .. } if requested_url == channel_url
    ));
    let channel_claim = owner
        .claim_discovery_public_page(crate::DiscoveryPublicPageInput {
            campaign_id: "review".into(),
            expected_revision: 1,
            profile_ref: "dedicated".into(),
            candidate_id: receipt.candidate_id.clone(),
            requested_url: channel_url.into(),
        })
        .unwrap();
    owner
        .finish_discovery_public_page(crate::FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim: channel_claim,
            document_url: Some(channel_url.into()),
            title: "Creator channel".into(),
            markdown_excerpt: format!("Website: {CONTACT_URL}"),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert!(
        matches!(owner.plan_discovery_step("review", 1).unwrap(), crate::DiscoveryStep::ReadPublicPage { requested_url, .. } if requested_url == CONTACT_URL)
    );
    let input = crate::DiscoveryPublicPageInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        profile_ref: "dedicated".into(),
        candidate_id: receipt.candidate_id.clone(),
        requested_url: CONTACT_URL.into(),
    };
    let claim = owner.claim_discovery_public_page(input.clone()).unwrap();
    let read = owner
        .finish_discovery_public_page(crate::FinishDiscoveryPublicPageInput {
            hidden_email_available: false,
            claim,
            document_url: Some(CONTACT_URL.into()),
            title: "Contact".into(),
            markdown_excerpt: "Business inquiries: studio@example.org".into(),
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    assert!(owner.claim_discovery_public_page(input).is_err());
    assert_eq!(
        owner
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        1
    );
    let publication = owner
        .prepare_discovery_library_publication("review", 1)
        .unwrap();
    let contact = &publication.material.sources.rows[0].contacts[0];
    assert_eq!(contact.normalized_value, "studio@example.org");
    assert_eq!(contact.source_url, CONTACT_URL);
    assert_eq!(contact.captured_at, read.captured_at);
    assert_eq!(
        contact.attribution,
        crate::DiscoveryContactAttribution::Unconfirmed
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        reopened
            .prepare_discovery_library_publication("review", 1)
            .unwrap(),
        publication
    );
}

#[test]
fn recommendations_require_passing_score_and_observed_contacts_after_restart() {
    let late_contact = format!(
        "{}Business: hello@example.org https://t.me/creator_bot",
        "Original description. ".repeat(250)
    );
    for contact in [
        None,
        Some("Business: hello@example.org https://t.me/creator_bot"),
        Some(late_contact.as_str()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (owner, hit) = super::super::tests::setup_with_description(
            dir.path(),
            json!({"min_followers":10000}),
            "UC1234567890",
            4,
            contact,
        );
        super::super::tests::add_receipts(&owner, &hit);
        let mut request = input(&owner, &hit);
        request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
        request.findings = vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Synthetic saved original transcript supports the requested content; follower requirement meets the saved native observation.".into(),
            citations: vec![DiscoveryReviewQuote::Transcript { segment_index: 0, quote: "I type a prompt.".into() }],
            tool: None, country_code: None, match_level: Some(DiscoveryMatchLevel::Good),
        }];
        let receipt = owner
            .submit_discovery_review("operator", request.clone())
            .unwrap();
        assert_eq!(receipt.verdict, "qualified"); // Score alone is retained in the backend.
        assert_eq!(
            owner.submit_discovery_review("operator", request).unwrap(),
            receipt
        );
        let expected = u32::from(contact.is_some());
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            expected
        );
        let publication = owner
            .prepare_discovery_library_publication("review", 1)
            .unwrap();
        let row = &publication.material.sources.rows[0];
        assert_eq!(
            row.verdict,
            if contact.is_some() {
                "qualified"
            } else {
                "pending"
            }
        );
        assert_eq!(row.contacts.len(), if contact.is_some() { 2 } else { 0 });
        assert!(
            row.contacts
                .iter()
                .all(|c| c.native_author_id == receipt.native_author_id
                    && c.source_url == "https://www.youtube.com/watch?v=LR04bU_yV5k"
                    && c.captured_at == 100)
        );
        drop(owner);
        let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
        assert_eq!(
            reopened
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            expected
        );
        assert_eq!(
            reopened
                .prepare_discovery_library_publication("review", 1)
                .unwrap(),
            publication
        );
    }
}

#[test]
fn initial_match_ranking_is_global_before_cursor_pagination() {
    let (dir, owner, _) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    for (id, level) in [
        ("a-low", Some("low")),
        ("b-good", Some("good")),
        ("z-excellent", Some("excellent")),
        ("c-unrated", None),
    ] {
        let rating = level.map(|level| json!({"level":level,"reason":"Synthetic rank fixture","source_url":"https://www.youtube.com/watch?v=LR04bU_yV5k"}));
        db.execute("INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
            VALUES(?1,'review',1,'youtube','dedicated',?1,?1,'https://www.youtube.com/channel/UC1234567890',?2,?3,100,100,'youtube')",
            rusqlite::params![id, if level == Some("low") {"rejected"} else if level.is_some() {"qualified"} else {"pending"},
                json!({"source_urls":["https://www.youtube.com/watch?v=LR04bU_yV5k"],"assessment":{"reasons":[],"subscribers_normalized":null,"match_rating":rating}}).to_string()]).unwrap();
    }
    let mut after = None;
    for expected in ["z-excellent", "b-good", "a-low"] {
        let page = owner
            .list_discovery_candidates(crate::ListDiscoveryCandidatesInput {
                campaign_id: "review".into(),
                after_id: after,
                limit: 1,
            })
            .unwrap();
        assert_eq!(page[0].id, expected);
        after = Some(page[0].id.clone());
    }
    let remaining = owner
        .list_discovery_candidates(crate::ListDiscoveryCandidatesInput {
            campaign_id: "review".into(),
            after_id: after,
            limit: 100,
        })
        .unwrap();
    assert!(
        remaining
            .iter()
            .all(|candidate| candidate.evidence["assessment"]["match_rating"].is_null())
    );
}

#[test]
fn initial_matching_replaces_the_legacy_checklist_and_survives_readback() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    let mut request = input(&owner, &hit);
    request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
    request.findings = vec![DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit,
        outcome: DiscoveryReviewOutcome::Supported,
        rationale:
            "Saved original text describes direct AI use. Initial assessment of this sample only."
                .into(),
        citations: vec![DiscoveryReviewQuote::Transcript {
            segment_index: 0,
            quote: "I type a prompt.".into(),
        }],
        tool: None,
        country_code: None,
        match_level: Some(DiscoveryMatchLevel::Good),
    }];
    let receipt = owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert_eq!(receipt.verdict, "qualified");
    assert_eq!(receipt.policy_version, "initial-match-evaluation-v1");
    assert!(receipt.reasons.is_empty());
    assert_eq!(
        owner
            .submit_discovery_review("operator", request.clone())
            .unwrap(),
        receipt
    );
    assert_eq!(audit_count(&dir), 1);
    // The owner must validate the persisted score against its audit, not strip
    // it merely because it is the new policy instead of the legacy proof list.
    let (_, _, preserve) = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(owner.prepare_initial_match_material_async("review".into(), 1, hit.clone()))
        .unwrap();
    assert!(preserve);
    let score: String = db.query_row("SELECT json_extract(evidence_json,'$.assessment.match_rating.level') FROM discovery_candidates WHERE id=?1", [&receipt.candidate_id], |r| r.get(0)).unwrap();
    assert_eq!(score, "good");
    // An explicit reassessment replaces level and reason together. Uncertainty
    // has a null score and a saved explanation, never an invented low grade.
    request.request_id = "initial-unrated".into();
    request.material_hash = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap()
        .material_hash;
    request.findings[0].outcome = DiscoveryReviewOutcome::Insufficient;
    request.findings[0].match_level = None;
    request.findings[0].citations.clear();
    request.findings[0].rationale =
        "Necessary content evidence is unavailable in this saved sample.".into();
    assert_eq!(
        owner
            .submit_discovery_review("operator", request)
            .unwrap()
            .verdict,
        "pending"
    );
    let (material, _, preserve) = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(owner.prepare_initial_match_material_async("review".into(), 1, hit))
        .unwrap();
    assert!(!preserve);
    assert!(!material.candidate_evidence_hash.is_empty());
    let raw: String = db
        .query_row(
            "SELECT evidence_json FROM discovery_candidates WHERE id=?1",
            [&receipt.candidate_id],
            |r| r.get(0),
        )
        .unwrap();
    let saved: Value = serde_json::from_str(&raw).unwrap();
    assert!(saved["assessment"]["match_rating"].is_null());
    assert_eq!(saved["reviewed_match"]["language_only_pending"], false);
    assert_eq!(
        saved["reviewed_match"]["reason"],
        "Necessary content evidence is unavailable in this saved sample."
    );
}

#[test]
fn initial_matching_required_failure_is_low_and_quotes_cannot_be_fabricated() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'", [r#"{"min_followers":20000,"language":"Chinese"}"#]).unwrap();
    let mut request = input(&owner, &hit);
    request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
    request.findings = vec![DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::MatchFit, outcome: DiscoveryReviewOutcome::Supported,
        rationale: "Observed 12400 followers are below the requested 20000 minimum. Initial assessment rejects this necessary condition.".into(),
        citations: vec![], tool: None, country_code: None, match_level: Some(DiscoveryMatchLevel::Good),
    }];
    assert_eq!(
        owner
            .submit_discovery_review("operator", request.clone())
            .unwrap_err(),
        "initial_match_required_failure_must_be_low"
    );
    assert_eq!(audit_count(&dir), 0);
    request.findings[0].match_level = Some(DiscoveryMatchLevel::Low);
    let receipt = owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert_eq!(receipt.verdict, "rejected");
    let level: String = db.query_row("SELECT json_extract(evidence_json,'$.assessment.match_rating.level') FROM discovery_candidates WHERE id=?1", [&receipt.candidate_id], |r| r.get(0)).unwrap();
    assert_eq!(level, "low");
    request.request_id = "fabricated-initial".into();
    request.material_hash = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap()
        .material_hash;
    request.findings[0].citations = vec![DiscoveryReviewQuote::Description {
        quote: "Invented author text".into(),
    }];
    assert_eq!(
        owner
            .submit_discovery_review("operator", request)
            .unwrap_err(),
        "initial_match_finding_invalid"
    );
    assert_eq!(audit_count(&dir), 1);
}

fn setup() -> (tempfile::TempDir, CreatorRuntime, String) {
    let dir = tempfile::tempdir().unwrap();
    let (owner, hit) = super::super::tests::setup(dir.path());
    super::super::tests::add_receipts(&owner, &hit);
    (dir, owner, hit)
}

fn input(owner: &CreatorRuntime, hit: &str) -> SubmitDiscoveryReviewInput {
    SubmitDiscoveryReviewInput {
        campaign_id: "review".into(), expected_revision: 1, search_hit_id: hit.into(),
        material_hash: owner.prepare_discovery_review_material("review", 1, hit).unwrap().material_hash,
        request_id: "request".into(), reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "synthetic test reviewer".into() },
        findings: vec![
            DiscoveryReviewFinding { criterion: DiscoveryReviewCriterion::OwnAiToolDemo, outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Synthetic fixture judgment, not live qualification or proof of viewed video frames.".into(),
                citations: vec![DiscoveryReviewQuote::Transcript { segment_index: 0, quote: "I type a prompt.".into() }], tool: None, match_level: None, country_code: None },
            DiscoveryReviewFinding { criterion: DiscoveryReviewCriterion::PublicToolUrl, outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Synthetic page identifies the observed tool.".into(),
                citations: vec![DiscoveryReviewQuote::PublicPage { requested_url: LINK.into(), quote: "AI editing software.".into() }],
                tool: Some(DiscoveryReviewedTool { name: "Example tool".into(), requested_url: LINK.into() }), match_level: None, country_code: None },
        ],
    }
}

fn audit_count(dir: &tempfile::TempDir) -> i64 {
    Connection::open(dir.path().join("crm.db"))
        .unwrap()
        .query_row("SELECT count(*) FROM discovery_evidence_reviews", [], |r| {
            r.get(0)
        })
        .unwrap()
}

#[test]
fn language_review_qualifies_and_match_never_replaces_a_hard_condition() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'",
        [r#"{"min_subscribers":8000,"language":"English","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#]).unwrap();
    let finding = |criterion, outcome, level| DiscoveryReviewFinding {
        criterion,
        outcome,
        rationale: "Author's saved English transcript describes hands-on product use.".into(),
        citations: vec![DiscoveryReviewQuote::Transcript {
            segment_index: 0,
            quote: "I type a prompt.".into(),
        }],
        tool: None,
        country_code: None,
        match_level: level,
    };
    let mut request = input(&owner, &hit);
    request.findings = vec![
        finding(
            DiscoveryReviewCriterion::ContentLanguage,
            DiscoveryReviewOutcome::Insufficient,
            None,
        ),
        finding(
            DiscoveryReviewCriterion::MatchFit,
            DiscoveryReviewOutcome::Supported,
            Some(DiscoveryMatchLevel::Excellent),
        ),
    ];
    let pending = owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert_eq!(pending.verdict, "pending");
    assert!(pending.reasons.iter().any(|r| r == "language_unverified"));
    request.request_id = "language-supported".into();
    request.material_hash = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap()
        .material_hash;
    request.findings[0].outcome = DiscoveryReviewOutcome::Supported;
    let qualified = owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert_eq!(qualified.verdict, "qualified");
    let read_evidence = || -> Value {
        let raw: String = db
            .query_row(
                "SELECT evidence_json FROM discovery_candidates WHERE id=?1",
                [&qualified.candidate_id],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_str(&raw).unwrap()
    };
    assert_eq!(
        read_evidence()["assessment"]["match_rating"]["level"],
        "excellent"
    );
    request
        .findings
        .retain(|finding| finding.criterion != DiscoveryReviewCriterion::MatchFit);
    request.request_id = "retain-match".into();
    request.material_hash = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap()
        .material_hash;
    owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert_eq!(
        read_evidence()["assessment"]["match_rating"]["level"],
        "excellent"
    );
    db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.reviewed_match.level','low') WHERE id=?1",[&qualified.candidate_id]).unwrap();
    request.request_id = "strip-forged-match".into();
    request.material_hash = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap()
        .material_hash;
    owner
        .submit_discovery_review("operator", request.clone())
        .unwrap();
    assert!(read_evidence()["assessment"]["match_rating"].is_null());
    // Loading the next material validates retained language and match proofs
    // against the immutable audit, rather than trusting booleans.
    let basis = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    request.material_hash = basis.material_hash;
    request.request_id = "language-conflict".into();
    request.findings[0].outcome = DiscoveryReviewOutcome::Contradicted;
    let rejected = owner.submit_discovery_review("operator", request).unwrap();
    assert_eq!(rejected.verdict, "rejected");
    assert!(rejected.reasons.iter().any(|r| r == "language_mismatch"));
}

#[test]
fn spoken_language_rejects_description_and_fabricated_quote() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'",
        [r#"{"language":"spoken English","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#]).unwrap();
    let mut request = input(&owner, &hit);
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    request.findings = vec![DiscoveryReviewFinding {
        criterion: DiscoveryReviewCriterion::ContentLanguage,
        outcome: DiscoveryReviewOutcome::Supported,
        rationale: "A description cannot establish the speaker's language.".into(),
        citations: vec![DiscoveryReviewQuote::Description {
            quote: material.video_description.unwrap(),
        }],
        tool: None,
        country_code: None,
        match_level: None,
    }];
    assert!(
        owner
            .submit_discovery_review("operator", request.clone())
            .unwrap_err()
            .contains("review_transcript_required")
    );
    request.findings[0].citations = vec![DiscoveryReviewQuote::Transcript {
        segment_index: 0,
        quote: "invented speech".into(),
    }];
    assert!(owner.submit_discovery_review("operator", request).is_err());
    assert_eq!(audit_count(&dir), 0);
}

#[test]
fn apparel_review_requires_author_content_and_corroborated_overseas_residence() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'",
        [r#"{"min_followers":10000,"region":"中国大陆以外","category":"服装","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#]).unwrap();
    db.execute("UPDATE discovery_transcript_reads SET segments_json=?1 WHERE search_hit_id=?2",
        rusqlite::params![r#"[{"timestamp":"0:10","speaker":"","text":"I live in Toronto, Canada, and run an apparel brand selling clothing internationally."}]"#,hit]).unwrap();
    db.execute("UPDATE discovery_public_page_reads SET markdown_excerpt='Our founder lives in Toronto, Canada and operates an apparel export business.' WHERE search_hit_id=?1", [&hit]).unwrap();
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let valid = SubmitDiscoveryReviewInput {
        campaign_id:"review".into(), expected_revision:1, search_hit_id:hit.clone(),
        material_hash:material.material_hash, request_id:"apparel-review".into(),
        reviewer: DiscoveryReviewer { kind:DiscoveryReviewerKind::Human, label:"fixture reviewer".into() },
        findings:vec![
            DiscoveryReviewFinding { criterion:DiscoveryReviewCriterion::ApparelBusinessContent,
                outcome:DiscoveryReviewOutcome::Supported,
                rationale:"First-person clothing export business description.".into(),
                citations:vec![DiscoveryReviewQuote::Transcript {segment_index:0, quote:"run an apparel brand selling clothing internationally".into()}],
                tool:None, match_level: None, country_code:None },
            DiscoveryReviewFinding { criterion:DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
                outcome:DiscoveryReviewOutcome::Supported,
                rationale:"Current Toronto residence stated by the creator and corroborated by the linked page.".into(),
                citations:vec![
                    DiscoveryReviewQuote::Transcript {segment_index:0, quote:"I live in Toronto, Canada".into()},
                    DiscoveryReviewQuote::PublicPage {requested_url:LINK.into(), quote:"founder lives in Toronto, Canada".into()},
                ], tool:None, match_level: None, country_code:Some("CA".into()) },
        ],
    };
    let mut invalid = valid.clone();
    invalid.findings[1].citations.pop();
    assert_eq!(
        owner
            .submit_discovery_review("account", invalid)
            .unwrap_err(),
        "review_residence_corroboration_required"
    );
    let mut invalid = valid.clone();
    invalid.findings[1].country_code = Some("CN".into());
    assert_eq!(
        owner
            .submit_discovery_review("account", invalid)
            .unwrap_err(),
        "review_country_invalid"
    );
    let mut invalid = valid.clone();
    invalid.findings[0].citations = vec![DiscoveryReviewQuote::Description { quote: LINK.into() }];
    assert_eq!(
        owner
            .submit_discovery_review("account", invalid)
            .unwrap_err(),
        "review_first_hand_evidence_required"
    );
    let receipt = owner
        .submit_discovery_review("account", valid.clone())
        .unwrap();
    assert_eq!(receipt.policy_version, "apparel-residence-review-v1");
    assert_eq!(receipt.verdict, "qualified");
    assert!(receipt.reasons.is_empty());
    assert_eq!(
        owner.submit_discovery_review("account", valid).unwrap(),
        receipt
    );
}

#[test]
fn shoe_review_requires_cited_author_content_and_preserves_frozen_threshold() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'",
        [r#"{"min_subscribers":10000,"category":"鞋子","region":"","language":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#],
    ).unwrap();
    db.execute(
        "UPDATE discovery_transcript_reads SET segments_json=?1 WHERE search_hit_id=?2",
        rusqlite::params![r#"[{"timestamp":"0:10","speaker":"","text":"I compare the fit and sole of these running shoes on camera."}]"#, hit],
    ).unwrap();
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let valid = SubmitDiscoveryReviewInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        search_hit_id: hit.clone(),
        material_hash: material.material_hash,
        request_id: "shoe-review".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Human,
            label: "fixture reviewer".into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::CategoryContent,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale:
                "The author discusses the shoes' fit and sole in the saved video transcript.".into(),
            citations: vec![DiscoveryReviewQuote::Transcript {
                segment_index: 0,
                quote: "compare the fit and sole of these running shoes".into(),
            }],
            tool: None,
            match_level: None,
            country_code: None,
        }],
    };
    let mut uncited = valid.clone();
    uncited.findings[0].citations.clear();
    assert_eq!(
        owner
            .submit_discovery_review("account", uncited)
            .unwrap_err(),
        "review_citation_invalid"
    );
    let receipt = owner.submit_discovery_review("account", valid).unwrap();
    assert_eq!(receipt.policy_version, "category-content-review-v1");
    assert_eq!(receipt.verdict, "qualified");
    assert_eq!(receipt.subscribers, Some(12400));
    assert!(receipt.reasons.is_empty());
    let next_material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let next = SubmitDiscoveryReviewInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        search_hit_id: hit,
        material_hash: next_material.material_hash,
        request_id: "shoe-second-review".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Human,
            label: "fixture reviewer".into(),
        },
        findings: vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::CategoryContent,
            outcome: DiscoveryReviewOutcome::Insufficient,
            rationale: "No new category finding; retain the earlier audited source.".into(),
            citations: vec![],
            tool: None,
            match_level: None,
            country_code: None,
        }],
    };
    assert_eq!(
        owner
            .submit_discovery_review("account", next)
            .unwrap()
            .verdict,
        "qualified"
    );
    assert_eq!(audit_count(&dir), 2);
}

#[test]
fn separate_reviews_survive_candidate_refresh_and_combine_only_audited_proofs() {
    let (dir, owner, hit) = setup();
    crate::discovery_contacts::legacy_approval_fixture(dir.path());
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute(
        "UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id='review'",
        [r#"{"min_followers":10000,"region":"中国大陆以外","category":"服装","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#],
    ).unwrap();
    db.execute(
        "UPDATE discovery_transcript_reads SET segments_json=?1 WHERE search_hit_id=?2",
        rusqlite::params![r#"[{"timestamp":"0:10","speaker":"","text":"I live in Toronto, Canada, and run an apparel brand selling clothing internationally."}]"#, hit],
    ).unwrap();
    db.execute(
        "UPDATE discovery_public_page_reads SET markdown_excerpt='Our founder lives in Toronto, Canada.' WHERE search_hit_id=?1",
        [&hit],
    ).unwrap();
    let first = SubmitDiscoveryReviewInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        search_hit_id: hit.clone(),
        material_hash: owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap()
            .material_hash,
        request_id: "apparel-first".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "fixture reviewer".into(),
        },
        findings: vec![
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::ApparelBusinessContent,
                outcome: DiscoveryReviewOutcome::Supported,
                rationale: "The author describes their clothing business.".into(),
                citations: vec![DiscoveryReviewQuote::Transcript {
                    segment_index: 0,
                    quote: "run an apparel brand selling clothing internationally".into(),
                }],
                tool: None,
                match_level: None,
                country_code: None,
            },
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
                outcome: DiscoveryReviewOutcome::Insufficient,
                rationale: "Country claim awaits linked-page review.".into(),
                citations: vec![],
                tool: None,
                match_level: None,
                country_code: None,
            },
        ],
    };
    let first_receipt = owner.submit_discovery_review("account", first).unwrap();
    assert_eq!(first_receipt.verdict, "pending");

    owner.upsert_discovery_candidate(crate::UpsertDiscoveryCandidateInput {
        campaign_id: "review".into(), expected_revision: 1, platform: "youtube".into(),
        source_profile_ref: "dedicated".into(), platform_account_id: "UC1234567890".into(),
        display_name: "Creator".into(),
        profile_url: "https://www.youtube.com/channel/UC1234567890".into(),
        evidence: json!({"source_urls":["https://www.youtube.com/watch?v=dQw4w9WgXcQ"],
            "subscribers_raw":"12.4K subscribers", "subscribers_source_url":"https://www.youtube.com/watch?v=dQw4w9WgXcQ", "captured_at":101}),
    }).unwrap();
    let stored: String = db
        .query_row(
            "SELECT evidence_json FROM discovery_candidates WHERE discovery_campaign_id='review'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let stored: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(
        stored["reviewed_apparel_content"]["review_id"],
        first_receipt.review_id
    );

    let second = SubmitDiscoveryReviewInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        search_hit_id: hit.clone(),
        material_hash: owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap()
            .material_hash,
        request_id: "residence-second".into(),
        reviewer: DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Agent,
            label: "fixture reviewer".into(),
        },
        findings: vec![
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::ApparelBusinessContent,
                outcome: DiscoveryReviewOutcome::Insufficient,
                rationale: "No additional apparel finding in this review.".into(),
                citations: vec![],
                tool: None,
                match_level: None,
                country_code: None,
            },
            DiscoveryReviewFinding {
                criterion: DiscoveryReviewCriterion::CreatorResidenceOutsideMainland,
                outcome: DiscoveryReviewOutcome::Supported,
                rationale: "Creator statement corroborated by the linked page.".into(),
                citations: vec![
                    DiscoveryReviewQuote::Transcript {
                        segment_index: 0,
                        quote: "I live in Toronto, Canada".into(),
                    },
                    DiscoveryReviewQuote::PublicPage {
                        requested_url: LINK.into(),
                        quote: "founder lives in Toronto, Canada".into(),
                    },
                ],
                tool: None,
                match_level: None,
                country_code: Some("CA".into()),
            },
        ],
    };
    assert_eq!(
        owner
            .submit_discovery_review("account", second)
            .unwrap()
            .verdict,
        "qualified"
    );
    drop(owner);
    let reopened = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(audit_count(&dir), 2);
}

#[tokio::test]
async fn review_commit_projects_qualification_is_idempotent_and_survives_restart() {
    let (dir, owner, hit) = setup();
    crate::discovery_contacts::legacy_approval_fixture(dir.path());
    let request = input(&owner, &hit);
    let before = owner
        .prepare_discovery_library_snapshot("review", 1)
        .unwrap();
    assert_eq!(before.rows[0].verdict, "pending");
    let budget = owner.get_discovery_campaign("review").unwrap().used_calls;
    let result = owner
        .submit_discovery_review_async("account".into(), request.clone())
        .await
        .unwrap();
    assert_eq!(result.verdict, "qualified");
    assert!(result.reasons.is_empty());
    assert_eq!(
        owner
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        1
    );
    assert_eq!(
        owner
            .submit_discovery_review("account", request.clone())
            .unwrap(),
        result
    );
    assert_eq!(
        owner
            .get_discovery_review_async("account".into(), "review".into(), "request".into())
            .await
            .unwrap(),
        Some(result.clone())
    );
    assert!(
        owner
            .get_discovery_review("other-account", "review", "request")
            .unwrap()
            .is_none()
    );
    assert!(
        owner
            .get_discovery_review("account", "other-campaign", "request")
            .unwrap()
            .is_none()
    );
    let mut changed = request.clone();
    changed.findings[0].rationale.push_str(" altered");
    assert_eq!(
        owner
            .submit_discovery_review("account", changed)
            .unwrap_err(),
        "review_idempotency_conflict"
    );
    assert_eq!(audit_count(&dir), 1);
    assert_eq!(
        owner.get_discovery_campaign("review").unwrap().used_calls,
        budget
    );
    let after = owner
        .prepare_discovery_library_snapshot("review", 1)
        .unwrap();
    assert_eq!(after.rows[0].verdict, "qualified");
    assert_eq!(after.source_revision, before.source_revision + 1);
    assert_ne!(
        after.rows[0].evidence_version,
        before.rows[0].evidence_version
    );
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("review", 1)
            .unwrap(),
        after
    );
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner.submit_discovery_review("account", request).unwrap(),
        result
    );
    assert_eq!(
        owner
            .prepare_discovery_library_snapshot("review", 1)
            .unwrap(),
        after
    );
    assert_eq!(audit_count(&dir), 1);
}

#[test]
fn review_rejects_invented_citations_missing_conditions_stale_material_and_raw_verdicts() {
    let (dir, owner, hit) = setup();
    let valid = input(&owner, &hit);
    let mut requests = Vec::new();
    let mut bad = valid.clone();
    bad.findings[1].tool.as_mut().unwrap().name = "Invented product".into();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].rationale = "x".repeat(4001);
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].citations = vec![bad.findings[0].citations[0].clone(); 9];
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].citations = vec![DiscoveryReviewQuote::Transcript {
        segment_index: 200,
        quote: "I type a prompt.".into(),
    }];
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].citations = vec![DiscoveryReviewQuote::Description {
        quote: format!("The tool: {LINK}"),
    }];
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].citations = vec![DiscoveryReviewQuote::Transcript {
        segment_index: 0,
        quote: "invented result".into(),
    }];
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[1].tool.as_mut().unwrap().requested_url = "https://example.com/unseen".into();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[1].citations.clear();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings.pop();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[1] = bad.findings[0].clone();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.findings[0].rationale = " ".into();
    requests.push(bad);
    let mut bad = valid.clone();
    bad.material_hash = "0".repeat(64);
    requests.push(bad);
    let mut bad = valid.clone();
    bad.expected_revision = 2;
    requests.push(bad);
    for bad in requests {
        assert!(owner.submit_discovery_review("account", bad).is_err());
    }
    for extra in [
        "verdict",
        "operator_account_id",
        "policy_version",
        "evidence",
        "verified",
    ] {
        let mut raw = serde_json::to_value(&valid).unwrap();
        raw[extra] = json!("untrusted");
        assert!(serde_json::from_value::<SubmitDiscoveryReviewInput>(raw).is_err());
    }
    assert_eq!(audit_count(&dir), 0);
    assert_eq!(
        owner
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn review_keeps_insufficient_and_unhandled_hard_conditions_pending() {
    let (dir, owner, hit) = setup();
    let mut request = input(&owner, &hit);
    request.findings[0].outcome = DiscoveryReviewOutcome::Insufficient;
    request.findings[0].citations.clear();
    let result = owner.submit_discovery_review("account", request).unwrap();
    assert_eq!(result.verdict, "pending");
    assert!(
        result
            .reasons
            .iter()
            .any(|s| s == "own_ai_tool_demo_unverified")
    );
    // Test-only mutation models an additional frozen hard condition, never a live Campaign edit.
    Connection::open(dir.path().join("crm.db")).unwrap().execute(
        "UPDATE discovery_campaign_revisions SET criteria_json=json_set(criteria_json,'$.language','English','$.custom_requirement','visual proof')", []).unwrap();
    let mut request = input(&owner, &hit);
    request.request_id = "second".into();
    let result = owner.submit_discovery_review("account", request).unwrap();
    assert_eq!(result.verdict, "pending");
    assert!(result.reasons.iter().any(|s| s == "language_unverified"));
    assert!(result.reasons.iter().any(|s| s == "unsupported_criterion"));
}

#[test]
fn review_never_overrides_follower_threshold_or_uses_failed_receipt_text() {
    let (dir, owner, hit) = setup();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.subscribers_raw','100 subscribers')", []).unwrap();
    let result = owner
        .submit_discovery_review("account", input(&owner, &hit))
        .unwrap();
    assert_eq!(result.verdict, "rejected");
    assert!(
        result
            .reasons
            .iter()
            .any(|reason| reason == "below_min_subscribers")
    );
    db.execute("UPDATE discovery_transcript_reads SET status='failed',segments_json='[]',truncated=0,stop_code='opencli_timeout'", []).unwrap();
    let mut request = input(&owner, &hit);
    request.request_id = "failed-evidence".into();
    assert!(owner.submit_discovery_review("account", request).is_err());
    assert_eq!(audit_count(&dir), 1);
    assert_eq!(
        owner
            .count_discovery_candidates("review")
            .unwrap()
            .qualified,
        0
    );
}

#[test]
fn review_revalidates_every_source_after_preparation_without_writing_partial_audit() {
    for mutation in [
        "UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.new_observation',1)",
        "UPDATE discovery_candidates SET platform_account_id='another-author'",
        "UPDATE discovery_campaigns SET phase='paused'",
        "UPDATE discovery_transcript_reads SET segments_json='[]'",
        "DELETE FROM discovery_public_page_reads",
        "DELETE FROM discovery_video_identity_bindings",
    ] {
        let (dir, owner, hit) = setup();
        let request = input(&owner, &hit);
        let basis = owner
            .repository
            .read_discovery_review_basis("review", 1, &hit)
            .unwrap();
        let prepared = prepare(
            "account".into(),
            request.clone(),
            request_hash("account", &request).unwrap(),
            basis,
        )
        .unwrap();
        Connection::open(dir.path().join("crm.db"))
            .unwrap()
            .execute_batch(mutation)
            .unwrap();
        assert!(
            owner.repository.commit_discovery_review(&prepared).is_err(),
            "{mutation}"
        );
        assert_eq!(audit_count(&dir), 0);
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            0
        );
    }
}

#[test]
fn review_audit_and_candidate_projection_roll_back_as_one_transaction() {
    for target in ["discovery_candidates", "discovery_evidence_reviews"] {
        let (dir, owner, hit) = setup();
        let request = input(&owner, &hit);
        let db = Connection::open(dir.path().join("crm.db")).unwrap();
        let operation = if target == "discovery_candidates" {
            "UPDATE"
        } else {
            "INSERT"
        };
        db.execute_batch(&format!("CREATE TRIGGER review_test_failure BEFORE {operation} ON {target} BEGIN SELECT RAISE(ABORT,'injected review failure'); END;")).unwrap();
        assert!(
            owner
                .submit_discovery_review("account", request.clone())
                .is_err()
        );
        assert_eq!(audit_count(&dir), 0);
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            0
        );
        assert!(
            owner
                .get_discovery_review("account", "review", "request")
                .unwrap()
                .is_none()
        );
        db.execute_batch("DROP TRIGGER review_test_failure;")
            .unwrap();
        assert_eq!(
            owner
                .submit_discovery_review("account", request)
                .unwrap()
                .verdict,
            "qualified"
        );
        assert_eq!(audit_count(&dir), 1);
    }
}

#[test]
fn concurrent_identical_reviews_return_one_original_receipt() {
    let (dir, owner, hit) = setup();
    let request = input(&owner, &hit);
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            barrier.wait();
            owner
                .submit_discovery_review("account", request.clone())
                .unwrap()
        });
        let b = scope.spawn(|| {
            barrier.wait();
            owner
                .submit_discovery_review("account", request.clone())
                .unwrap()
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    assert_eq!(a, b);
    assert_eq!(audit_count(&dir), 1);
}

#[test]
fn legacy_hidden_email_entry_preserves_availability_without_inventing_address() {
    for hidden in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let channel = "UCabcdefghijklmnopqrstuv";
        let (owner, hit) = super::super::tests::setup_with_description(
            dir.path(),
            json!({"min_followers":10000}),
            channel,
            10,
            None,
        );
        super::super::tests::add_receipts(&owner, &hit);
        let mut request = input(&owner, &hit);
        request.reviewer.label = super::super::initial_match::INITIAL_MATCH_REVIEWER.into();
        request.findings = vec![DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Supported,
            rationale: "Saved transcript supports the requested topic.".into(),
            citations: vec![DiscoveryReviewQuote::Transcript {
                segment_index: 0,
                quote: "I type a prompt.".into(),
            }],
            tool: None,
            country_code: None,
            match_level: Some(DiscoveryMatchLevel::Good),
        }];
        owner.submit_discovery_review("operator", request).unwrap();

        let candidate = owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap()
            .candidate_id;
        let url = format!("https://www.youtube.com/channel/{channel}");
        // Released candidates can have a score and receipts without a persisted
        // native-account contact, leaving the original metadata read claimable.
        let db = Connection::open(dir.path().join("crm.db")).unwrap();
        assert_eq!(
            db.execute(
                "DELETE FROM discovery_contact_observations WHERE candidate_id=?1 AND kind='social_profile' AND source_excerpt='Verified native author account'",
                [&candidate],
            )
            .unwrap(),
            1
        );
        let claim = owner
            .claim_discovery_public_page(crate::DiscoveryPublicPageInput {
                campaign_id: "review".into(),
                expected_revision: 1,
                profile_ref: "dedicated".into(),
                candidate_id: candidate.clone(),
                requested_url: url.clone(),
            })
            .unwrap();
        owner
            .finish_discovery_public_page(crate::FinishDiscoveryPublicPageInput {
                hidden_email_available: hidden,
                claim,
                document_url: Some(url.clone()),
                title: "Creator".into(),
                markdown_excerpt: "Official biography without an address".into(),
                truncated: true,
                stop_code: None,
            })
            .unwrap();
        assert_eq!(
            owner
                .count_discovery_candidates("review")
                .unwrap()
                .qualified,
            1
        );
        let values: Vec<(String, String)> = db
            .prepare("SELECT kind,normalized_value FROM discovery_contact_observations WHERE candidate_id=?1 AND kind IN ('email','hidden_email')")
            .unwrap()
            .query_map([&candidate], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        if hidden {
            assert_eq!(values, vec![("hidden_email".into(), url.clone())]);
        } else {
            assert!(values.is_empty());
        }
        let snapshot = owner
            .prepare_discovery_library_snapshot("review", 1)
            .unwrap();
        let row = snapshot
            .rows
            .iter()
            .find(|row| row.candidate_id.as_deref() == Some(candidate.as_str()))
            .unwrap();
        assert_eq!(row.verdict, "qualified");
        assert!(
            !row.contacts
                .iter()
                .any(|contact| contact.kind == crate::DiscoveryContactKind::Email)
        );
        assert!(row.contacts.iter().any(|contact| contact.kind
            == crate::DiscoveryContactKind::SocialProfile
            && contact.normalized_value == url
            && contact.source_url == url
            && contact.native_author_id == channel
            && contact.attribution == crate::DiscoveryContactAttribution::AuthorPublished));
        let hidden_contacts: Vec<_> = row
            .contacts
            .iter()
            .filter(|contact| contact.kind == crate::DiscoveryContactKind::HiddenEmail)
            .collect();
        assert_eq!(hidden_contacts.len(), usize::from(hidden));
        if let Some(contact) = hidden_contacts.first() {
            assert_eq!(contact.normalized_value, url);
            assert_eq!(contact.source_url, url);
            assert_eq!(contact.native_author_id, channel);
            assert_eq!(
                contact.attribution,
                crate::DiscoveryContactAttribution::AuthorPublished
            );
        }
        let candidates = owner
            .list_discovery_candidates(crate::ListDiscoveryCandidatesInput {
                campaign_id: "review".into(),
                after_id: None,
                limit: 100,
            })
            .unwrap();
        let displayed = candidates.iter().find(|row| row.id == candidate).unwrap();
        assert_eq!(displayed.verdict, "qualified");
        assert!(
            displayed.evidence.get("contact_status").is_none(),
            "Available native-profile and hidden-email routes do not require a disclosed mailbox"
        );
    }
}
