//! Web-only, tool-free initial matching over immutable Creator-owned evidence.
use crate::LocalPlatform;
use onecrew_creator_runtime::{
    DiscoveryReviewFinding, DiscoveryReviewMaterial, DiscoveryReviewMaterialInput,
    DiscoveryReviewReceipt, DiscoveryReviewer, DiscoveryReviewerKind, GetDiscoveryReviewInput,
    INITIAL_MATCH_REVIEWER, SubmitDiscoveryReviewInput,
};
use schemars::JsonSchema;
use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const DISCOVERY_SCORE_CONCURRENCY: usize = 2;

/// Bound model concurrency and prevent concurrent reviews of one native author.
pub(crate) struct ScoreGate {
    slots: tokio::sync::Semaphore,
    authors: std::sync::Mutex<std::collections::HashSet<String>>,
}
impl Default for ScoreGate {
    fn default() -> Self {
        Self {
            slots: tokio::sync::Semaphore::new(DISCOVERY_SCORE_CONCURRENCY),
            authors: Default::default(),
        }
    }
}
struct ScorePermit<'a> {
    gate: &'a ScoreGate,
    author: String,
    _permit: tokio::sync::SemaphorePermit<'a>,
}
impl Drop for ScorePermit<'_> {
    fn drop(&mut self) {
        self.gate
            .authors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.author);
    }
}
impl ScoreGate {
    fn acquire(&self, author: String) -> Result<ScorePermit<'_>, String> {
        let permit = self.slots.try_acquire().map_err(|_| "initial_match_busy")?;
        if !self
            .authors
            .lock()
            .map_err(|_| "initial_match_busy")?
            .insert(author.clone())
        {
            return Err("initial_match_busy".into());
        }
        Ok(ScorePermit {
            gate: self,
            author,
            _permit: permit,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Proposal {
    finding: DiscoveryReviewFinding,
    language_only_pending: bool,
}

const RUBRIC: &str = include_str!("discovery_match_rubric.txt");
mod prompt;
mod rules;

fn bound_comment_window(value: &mut serde_json::Value) {
    let matched_source = if value["native_content"]["status"] == "completed"
        && value["native_content"]["stop_code"].is_null()
    {
        if value["native_content"]["claim"]["platform"] == "twitter" {
            value["native_content"]["native_content"]["identity"]["source_url"]
                .as_str()
                .or_else(|| value["native_content"]["claim"]["source_url"].as_str())
        } else if matches!(
            value["native_content"]["claim"]["platform"].as_str(),
            Some("instagram" | "linkedin" | "pinterest")
        ) {
            value["native_content"]["native_content"]["source_url"].as_str()
        } else {
            value["native_content"]["claim"]["source_url"].as_str()
        }
    } else {
        value["hit"]["source_url"].as_str()
    }
    .map(str::to_owned);
    if let Some(samples) = value["language_samples"].as_array_mut() {
        for sample in samples {
            let matched = matched_source.as_deref() == sample["source_url"].as_str();
            if let Some(comments) = sample["comments"].as_array_mut() {
                // Preserve observed ordering, including duplicates. Later
                // comments must not refill rejected items in the first five.
                if matched {
                    comments.truncate(5);
                } else {
                    comments.clear();
                }
            }
        }
    }
}

/// Owner-created material; callers cannot supply their own evidence or identity.
pub struct PreparedDiscoveryMatch {
    account: String,
    input: DiscoveryReviewMaterialInput,
    material: DiscoveryReviewMaterial,
    request_id: String,
    prompt: String,
}

impl PreparedDiscoveryMatch {
    /// Stable automatic evidence key, including rubric and approval inputs.
    pub fn evidence_key(&self) -> &str {
        &self.request_id
    }
}

pub enum DiscoveryMatchPreparation {
    Skipped,
    Cached(Box<DiscoveryReviewReceipt>),
    Model(Box<PreparedDiscoveryMatch>),
}

impl LocalPlatform {
    pub async fn evaluate_discovery_match(
        &self,
        input: DiscoveryReviewMaterialInput,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        self.evaluate_match_inner(input, false).await
    }

    pub async fn reevaluate_discovery_match(
        &self,
        input: DiscoveryReviewMaterialInput,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        self.evaluate_match_inner(input, true).await
    }

    async fn evaluate_match_inner(
        &self,
        input: DiscoveryReviewMaterialInput,
        reassess: bool,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        match self.prepare_match_inner(input, reassess, |_, _| {}).await? {
            DiscoveryMatchPreparation::Skipped => Ok(None),
            DiscoveryMatchPreparation::Cached(receipt) => Ok(Some(*receipt)),
            DiscoveryMatchPreparation::Model(prepared) => {
                self.evaluate_prepared_discovery_match(*prepared, |_, _| {})
                    .await
            }
        }
    }

    pub async fn prepare_discovery_match(
        &self,
        input: DiscoveryReviewMaterialInput,
    ) -> Result<DiscoveryMatchPreparation, String> {
        self.prepare_match_inner(input, false, |_, _| {}).await
    }

    pub async fn prepare_discovery_match_observed(
        &self,
        input: DiscoveryReviewMaterialInput,
        observe: impl FnMut(&str, u64) + Send,
    ) -> Result<DiscoveryMatchPreparation, String> {
        self.prepare_match_inner(input, false, observe).await
    }

    async fn prepare_match_inner(
        &self,
        input: DiscoveryReviewMaterialInput,
        reassess: bool,
        mut observe: impl FnMut(&str, u64) + Send,
    ) -> Result<DiscoveryMatchPreparation, String> {
        let account = self.library_access_user().await?;
        let creators = self.creators()?;
        let (material, count, preserve) = match creators
            .prepare_initial_match_material_async(
                input.campaign_id.clone(),
                input.expected_revision,
                input.search_hit_id.clone(),
            )
            .await
        {
            Ok(material) => material,
            Err(onecrew_creator_runtime::CreatorRepositoryCallError::Owner(error))
                if matches!(
                    error.as_str(),
                    "review_material_author_binding_unavailable"
                        | "review_platform_unsupported"
                        | "review_source_unavailable"
                ) =>
            {
                return Ok(DiscoveryMatchPreparation::Skipped);
            }
            Err(_) => return Err("initial_match_material_unavailable".into()),
        };
        // Ordinary repeated discovery preserves a non-null initial rating. An
        // explicit manual reassessment still uses the existing audited owner.
        let mut value =
            serde_json::to_value(&material).map_err(|_| "initial_match_encoding_failed")?;
        // Review writes change the candidate hash, but not the evidence being rated.
        value.as_object_mut().unwrap().remove("material_hash");
        value
            .as_object_mut()
            .unwrap()
            .remove("candidate_evidence_hash");
        // Instagram supports profile-only initial screening. Keep its source
        // attribution intact; homepage previews are never original post text.
        if material.hit.platform != "instagram" {
            value.as_object_mut().unwrap().remove("native_profile");
        }
        value["observed_followers"] = serde_json::json!(count);
        bound_comment_window(&mut value);
        // One canonical owner-bound parent, shared by the model's instructions
        // and the retained comment window. Profile feeds are not post sources.
        value["matched_language_source_url"] = serde_json::json!(
            material
                .native_content
                .as_ref()
                .and_then(|read| {
                    if read.claim.platform == "twitter"
                        && read.status == "completed"
                        && read.stop_code.is_none()
                    {
                        read.native_content
                            .as_ref()?
                            .pointer("/identity/source_url")?
                            .as_str()
                    } else {
                        read.observed_source_url()
                    }
                })
                .unwrap_or(material.hit.source_url.as_str())
        );
        let request_id = format!(
            "initial-v1-{:x}",
            Sha256::digest(
                format!(
                    "{}:{}:{}",
                    RUBRIC,
                    value,
                    if reassess {
                        material.material_hash.as_str()
                    } else {
                        "automatic"
                    }
                )
                .as_bytes()
            )
        );
        let get = GetDiscoveryReviewInput {
            campaign_id: input.campaign_id.clone(),
            request_id: request_id.clone(),
        };
        let started = std::time::Instant::now();
        let cached = self.get_discovery_review(get.clone()).await;
        observe(
            "score.cache_lookup",
            started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        );
        if let Some(saved) = cached? {
            observe("score.cache_hit", 0);
            return Ok(DiscoveryMatchPreparation::Cached(Box::new(saved)));
        }
        if preserve && !reassess {
            return Ok(DiscoveryMatchPreparation::Skipped);
        }
        let campaign = creators
            .get_discovery_campaign_async(input.campaign_id.clone())
            .await
            .map_err(|_| "initial_match_campaign_unavailable")?;
        if let Some((observed, minimum)) = rules::below_minimum(&material.criteria, count) {
            if self.library_access_user().await? != account {
                return Err("initial_match_account_changed".into());
            }
            let started = std::time::Instant::now();
            let chinese = campaign
                .brief
                .chars()
                .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
            let rationale = if chinese {
                format!(
                    "初步评估：已观察到的原生粉丝数为 {observed}，低于要求的 {minimum}。此项必要条件已确定不符合；其他样本证据或未知信息不会抵消该门槛失败。"
                )
            } else {
                format!(
                    "Initial assessment: the observed native follower count is {observed}, below the required minimum of {minimum}. This confirmed required failure takes precedence over other sample evidence or unknown information."
                )
            };
            let result = self
                .submit_discovery_review(SubmitDiscoveryReviewInput {
                    campaign_id: input.campaign_id.clone(),
                    expected_revision: input.expected_revision,
                    search_hit_id: input.search_hit_id.clone(),
                    material_hash: material.material_hash.clone(),
                    request_id,
                    reviewer: DiscoveryReviewer {
                        kind: DiscoveryReviewerKind::Agent,
                        label: INITIAL_MATCH_REVIEWER.into(),
                    },
                    findings: vec![DiscoveryReviewFinding {
                        criterion: onecrew_creator_runtime::DiscoveryReviewCriterion::MatchFit,
                        outcome: onecrew_creator_runtime::DiscoveryReviewOutcome::Supported,
                        match_level: Some(onecrew_creator_runtime::DiscoveryMatchLevel::Low),
                        rationale,
                        citations: vec![],
                        tool: None,
                        country_code: None,
                    }],
                })
                .await;
            observe("score.rule", started.elapsed().as_millis() as u64);
            let receipt = match result {
                Ok(saved) => saved,
                Err(error) => self.get_discovery_review(get).await?.ok_or(error)?,
            };
            return Ok(DiscoveryMatchPreparation::Cached(Box::new(receipt)));
        }
        let prompt = prompt::build(&campaign.brief, &material, value);
        Ok(DiscoveryMatchPreparation::Model(Box::new(
            PreparedDiscoveryMatch {
                account,
                input,
                material,
                request_id,
                prompt,
            },
        )))
    }

    pub async fn evaluate_prepared_discovery_match(
        &self,
        prepared: PreparedDiscoveryMatch,
        mut observe: impl FnMut(&str, u64) + Send,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        let PreparedDiscoveryMatch {
            account,
            input,
            material,
            request_id,
            prompt,
        } = prepared;
        if self.library_access_user().await? != account {
            return Err("initial_match_account_changed".into());
        }
        let author = serde_json::to_string(&(
            &account,
            &input.campaign_id,
            input.expected_revision,
            material
                .native_platform
                .as_deref()
                .unwrap_or(&material.hit.platform),
            &material.native_author_id,
        ))
        .map_err(|_| "initial_match_encoding_failed")?;
        let _permit = self.discovery_score_gate.acquire(author)?;
        observe("score.model_started", 0);
        let started = std::time::Instant::now();
        let proposal =
            crate::discovery_draft::codex::structured_observed::<Proposal>(prompt, &mut observe)
                .await;
        observe(
            "score.model",
            started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        );
        let proposal = proposal.map_err(|error| format!("initial_match_{}", error.code()))?;
        if self.library_access_user().await? != account {
            return Err("initial_match_account_changed".into());
        }
        let mut findings = vec![proposal.finding];
        if proposal.language_only_pending
            && findings[0].outcome == onecrew_creator_runtime::DiscoveryReviewOutcome::Insufficient
        {
            findings.push(DiscoveryReviewFinding {
                criterion: onecrew_creator_runtime::DiscoveryReviewCriterion::ContentLanguage,
                outcome: onecrew_creator_runtime::DiscoveryReviewOutcome::Insufficient,
                match_level: None,
                tool: None,
                country_code: None,
                rationale: findings[0].rationale.clone(),
                citations: findings[0].citations.clone(),
            });
        }
        let request = SubmitDiscoveryReviewInput {
            campaign_id: input.campaign_id,
            expected_revision: input.expected_revision,
            search_hit_id: input.search_hit_id,
            material_hash: material.material_hash,
            request_id,
            reviewer: DiscoveryReviewer {
                kind: DiscoveryReviewerKind::Agent,
                label: INITIAL_MATCH_REVIEWER.into(),
            },
            findings,
        };
        let get = GetDiscoveryReviewInput {
            campaign_id: request.campaign_id.clone(),
            request_id: request.request_id.clone(),
        };
        let started = std::time::Instant::now();
        let result = match self.submit_discovery_review(request).await {
            Ok(saved) => Ok(Some(saved)),
            Err(error) => match self.get_discovery_review(get).await? {
                Some(saved) => Ok(Some(saved)),
                None => Err(error),
            },
        };
        observe(
            "score.commit",
            started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        );
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn numeric_rule_rejects_account_change_before_commit() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair};
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "original-operator");
        let pair = saved_pair(&platform);
        let input = DiscoveryReviewMaterialInput {
            campaign_id: pair.campaign_id,
            expected_revision: 1,
            search_hit_id: pair.right_search_hit_id,
        };
        let result = platform
            .prepare_discovery_match_observed(input, |phase, _| {
                if phase == "score.cache_lookup" {
                    authenticate(&platform, "changed-operator");
                }
            })
            .await;
        assert_eq!(
            result.err().as_deref(),
            Some("initial_match_account_changed")
        );
    }

    #[tokio::test]
    async fn known_below_minimum_is_recorded_without_model_admission() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair_with_author_ids};
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "fixture-operator");
        let pair = saved_pair_with_author_ids(
            &platform,
            ["UCaaaaaaaaaaaaaaaaaaaaaa", "UCbbbbbbbbbbbbbbbbbbbbbb"],
        );
        let _held = platform
            .discovery_score_gate
            .slots
            .try_acquire_many(2)
            .unwrap();
        let input = DiscoveryReviewMaterialInput {
            campaign_id: pair.campaign_id,
            expected_revision: 1,
            search_hit_id: pair.right_search_hit_id,
        };
        let prepared = platform
            .prepare_discovery_match(input.clone())
            .await
            .unwrap();
        assert!(
            matches!(prepared, DiscoveryMatchPreparation::Cached(_)),
            "a known required follower failure must not launch a model"
        );
        let saved = platform
            .evaluate_discovery_match(input)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.subscribers, Some(592));
        assert_eq!(saved.verdict, "rejected");
    }

    #[test]
    fn score_gate_allows_two_distinct_authors_and_releases_identity_on_drop() {
        let gate = ScoreGate::default();
        let first = gate.acquire("author-a".into()).unwrap();
        assert!(gate.acquire("author-a".into()).is_err());
        let second = gate.acquire("author-b".into()).unwrap();
        assert!(gate.acquire("author-c".into()).is_err());
        drop(first);
        let third = gate.acquire("author-c".into()).unwrap();
        assert!(gate.acquire("author-b".into()).is_err());
        drop(second);
        drop(third);
        assert!(gate.acquire("author-a".into()).is_ok());
    }

    #[tokio::test]
    async fn cached_material_prepares_while_model_lane_is_busy_without_model_call() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair_with_author_ids};
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "fixture-operator");
        let pair = saved_pair_with_author_ids(
            &platform,
            ["UCaaaaaaaaaaaaaaaaaaaaaa", "UCbbbbbbbbbbbbbbbbbbbbbb"],
        );
        let input = DiscoveryReviewMaterialInput {
            campaign_id: pair.campaign_id,
            expected_revision: 1,
            search_hit_id: pair.left_search_hit_id,
        };
        let DiscoveryMatchPreparation::Model(prepared) = platform
            .prepare_discovery_match(input.clone())
            .await
            .unwrap()
        else {
            panic!("New evidence requires model");
        };
        let request_id = prepared.request_id.clone();
        platform
            .submit_discovery_review(SubmitDiscoveryReviewInput {
                campaign_id: input.campaign_id.clone(),
                expected_revision: 1,
                search_hit_id: input.search_hit_id.clone(),
                material_hash: prepared.material.material_hash,
                request_id: request_id.clone(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: INITIAL_MATCH_REVIEWER.into(),
                },
                findings: vec![DiscoveryReviewFinding {
                    criterion: onecrew_creator_runtime::DiscoveryReviewCriterion::MatchFit,
                    outcome: onecrew_creator_runtime::DiscoveryReviewOutcome::Insufficient,
                    match_level: None,
                    tool: None,
                    country_code: None,
                    rationale: "Fixture evidence needs further verification".into(),
                    citations: vec![],
                }],
            })
            .await
            .unwrap();
        let _held_model = platform
            .discovery_score_gate
            .slots
            .try_acquire_many(2)
            .unwrap();
        let mut phases = Vec::new();
        let DiscoveryMatchPreparation::Cached(receipt) = platform
            .prepare_discovery_match_observed(input.clone(), |phase, _| {
                phases.push(phase.to_owned())
            })
            .await
            .unwrap()
        else {
            panic!("Unchanged material must use cache before model admission");
        };
        assert_eq!(receipt.request_id, request_id);
        assert_eq!(phases, vec!["score.cache_lookup", "score.cache_hit"]);
        assert!(
            platform
                .evaluate_discovery_match(input.clone())
                .await
                .unwrap()
                .is_some()
        );
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        db.execute("UPDATE discovery_search_hits SET title='New original shoe review evidence' WHERE id=?1", [&input.search_hit_id]).unwrap();
        let DiscoveryMatchPreparation::Model(changed) =
            platform.prepare_discovery_match(input).await.unwrap()
        else {
            panic!("New evidence must remain eligible");
        };
        assert_ne!(changed.evidence_key(), request_id);
    }

    #[tokio::test]
    async fn missing_author_material_skips_before_model_admission() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair_with_author_ids};
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "fixture-operator");
        let pair = saved_pair_with_author_ids(
            &platform,
            ["UCaaaaaaaaaaaaaaaaaaaaaa", "UCbbbbbbbbbbbbbbbbbbbbbb"],
        );
        let _held_model = platform
            .discovery_score_gate
            .slots
            .try_acquire_many(2)
            .unwrap();
        let input = DiscoveryReviewMaterialInput {
            campaign_id: pair.campaign_id,
            expected_revision: 1,
            search_hit_id: "missing".into(),
        };
        assert!(matches!(
            platform.prepare_discovery_match(input).await.unwrap(),
            DiscoveryMatchPreparation::Skipped
        ));
    }

    #[test]
    fn model_comment_projection_preserves_first_five_and_excludes_other_posts() {
        let mut value = serde_json::json!({
            "hit":{"source_url":"matched"},
            "language_samples":[
                {"source_url":"matched","text":"original", "comments":["one","one","emoji","four","five","six"]},
                {"source_url":"other","text":"other original", "comments":["a","b","c"]}
            ]
        });
        bound_comment_window(&mut value);
        assert_eq!(
            value["language_samples"][0]["comments"],
            serde_json::json!(["one", "one", "emoji", "four", "five"])
        );
        assert_eq!(
            value["language_samples"][1]["comments"],
            serde_json::json!([])
        );
        assert_eq!(value["language_samples"][1]["text"], "other original");
    }

    #[test]
    fn profile_locators_do_not_remove_linkedin_or_pinterest_native_post_comments() {
        for platform in ["linkedin", "pinterest"] {
            let mut value = serde_json::json!({"hit":{"source_url":"profile"},
                "native_content":{"status":"completed","stop_code":null,"claim":{"platform":platform,"source_url":"profile"},
                    "native_content":{"source_url":"actual-native-post"}},
                "language_samples":[{"source_url":"actual-native-post","comments":[1,2,3,4,5,6]},
                    {"source_url":"profile","comments":[1,2,3]}]});
            bound_comment_window(&mut value);
            assert_eq!(
                value["language_samples"][0]["comments"],
                serde_json::json!([1, 2, 3, 4, 5])
            );
            assert_eq!(
                value["language_samples"][1]["comments"],
                serde_json::json!([])
            );
        }
    }

    #[test]
    fn instagram_model_comments_bind_observed_post_not_requested_author_feed() {
        let mut value = serde_json::json!({
            "hit":{"source_url":"https://www.instagram.com/creator/"},
            "native_content":{"status":"completed","stop_code":null,
                "claim":{"platform":"instagram","source_url":"https://www.instagram.com/creator/"},
                "native_content":{"source_url":"https://www.instagram.com/p/Abc123/"}},
            "language_samples":[
                {"source_url":"https://www.instagram.com/p/Abc123/","comments":[1,2,3,4,5,6]},
                {"source_url":"https://www.instagram.com/creator/","comments":[1,2,3]}
            ]
        });
        bound_comment_window(&mut value);
        assert_eq!(
            value["language_samples"][0]["comments"],
            serde_json::json!([1, 2, 3, 4, 5])
        );
        assert_eq!(
            value["language_samples"][1]["comments"],
            serde_json::json!([])
        );
    }

    #[tokio::test]
    #[ignore = "live Codex evaluation over isolated audience-language fixture; no provider or customer writes"]
    async fn installed_codex_language_supplement_accepts_audience_evidence() {
        audience_language_case(
            "中文",
            "这篇内容帮助我理解了稳定币的储备和赎回机制，谢谢分享。",
            true,
        )
        .await;
    }

    #[tokio::test]
    #[ignore = "live Codex multilingual scoring and Library projection; isolated data only"]
    async fn installed_codex_audience_language_follows_requested_language() {
        for (language, comment, accepted) in [
            (
                "英语",
                "Thank you for explaining stablecoin reserves and redemption. Your explanation helped me understand how these assets work.",
                true,
            ),
            (
                "日语",
                "ステーブルコインの準備資産と償還の仕組みがよく分かりました。丁寧な説明をありがとうございます。",
                true,
            ),
            (
                "日语",
                "这篇内容帮助我理解了稳定币的储备和赎回机制，谢谢分享。",
                false,
            ),
        ] {
            audience_language_case(language, comment, accepted).await;
        }
    }

    async fn audience_language_case(language: &str, comment: &str, accepted: bool) {
        use crate::discovery_identity_test_support::{authenticate, saved_pair_with_author_ids};
        use onecrew_creator_runtime::{
            DiscoveryLanguageComment, DiscoveryLanguageSample, DiscoveryReviewCriterion,
            DiscoveryReviewOutcome,
        };
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "fixture-operator");
        let pair = saved_pair_with_author_ids(
            &platform,
            ["UCaaaaaaaaaaaaaaaaaaaaaa", "UCbbbbbbbbbbbbbbbbbbbbbb"],
        );
        let owner = platform.creators().unwrap();
        let material = owner
            .prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id)
            .unwrap();
        let author = "UCaaaaaaaaaaaaaaaaaaaaaa";
        let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
        let criteria = serde_json::json!({"min_subscribers":8000,"category":"稳定币","language":language,"region":"","requires_ai_tool_demo":false,"requires_public_tool_url":false});
        db.execute("UPDATE discovery_campaign_revisions SET brief_json=json_quote(?1),criteria_json=?2 WHERE discovery_campaign_id=?3",
            rusqlite::params![format!("寻找发布{language}稳定币内容的作者"), criteria.to_string(), pair.campaign_id]).unwrap();
        db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.video_description','USDC / USDT / DAI / 1:1 USD') WHERE id=?1",
            [&material.candidate_id]).unwrap();
        db.execute(
            "UPDATE discovery_search_hits SET title='USDC / USDT / DAI / 1:1 USD' WHERE id=?1",
            [&pair.left_search_hit_id],
        )
        .unwrap();
        let material = owner
            .prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id)
            .unwrap();
        let pending = DiscoveryReviewFinding {
            criterion: DiscoveryReviewCriterion::MatchFit,
            outcome: DiscoveryReviewOutcome::Insufficient,
            rationale: "主题和粉丝数符合；原创内容语言尚待补读。".into(),
            citations: vec![],
            tool: None,
            country_code: None,
            match_level: None,
        };
        owner
            .submit_discovery_review(
                "fixture-operator",
                SubmitDiscoveryReviewInput {
                    campaign_id: pair.campaign_id.clone(),
                    expected_revision: 1,
                    search_hit_id: pair.left_search_hit_id.clone(),
                    material_hash: material.material_hash,
                    request_id: "language-fixture".into(),
                    reviewer: DiscoveryReviewer {
                        kind: DiscoveryReviewerKind::Agent,
                        label: INITIAL_MATCH_REVIEWER.into(),
                    },
                    findings: vec![
                        pending.clone(),
                        DiscoveryReviewFinding {
                            criterion: DiscoveryReviewCriterion::ContentLanguage,
                            ..pending
                        },
                    ],
                },
            )
            .unwrap();
        for ordinal in 1..=1 {
            let claim = owner
                .claim_discovery_language_async(
                    pair.campaign_id.clone(),
                    1,
                    pair.left_search_hit_id.clone(),
                )
                .await
                .unwrap()
                .expect("language-only candidate remains eligible");
            assert_eq!(claim.ordinal, ordinal);
            let source = claim.root_url.clone();
            owner
                .finish_discovery_language_async(
                    claim,
                    Some(DiscoveryLanguageSample {
                        author_id: author.into(),
                        source_url: source.clone(),
                        text: "USDC / USDT / DAI / 1:1 USD".into(),
                        comments: [
                            comment.to_owned(),
                            match language {
                                "英语" => "The explanation of reserve assets is very clear.",
                                "日语" if accepted => {
                                    "準備資産の説明がとても分かりやすかったです。"
                                }
                                _ => "储备资产的解释很清楚，学到了很多。",
                            }
                            .to_owned(),
                            match language {
                                "英语" => "I understand the redemption mechanism now, thanks.",
                                "日语" if accepted => {
                                    "償還の仕組みを理解できました。ありがとうございます。"
                                }
                                _ => "谢谢介绍赎回机制，期待下一期视频。",
                            }
                            .to_owned(),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, text)| DiscoveryLanguageComment {
                            source_url: format!("{source}&lc=Ug{}", index + 1),
                            text,
                        })
                        .collect(),
                    }),
                    None,
                )
                .await
                .unwrap();
            let receipt = platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: pair.campaign_id.clone(),
                    expected_revision: 1,
                    search_hit_id: pair.left_search_hit_id.clone(),
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                receipt.verdict,
                if accepted { "qualified" } else { "pending" },
                "requested {language}"
            );
            let raw: String = db
                .query_row(
                    "SELECT evidence_json FROM discovery_candidates WHERE id=?1",
                    [&receipt.candidate_id],
                    |row| row.get(0),
                )
                .unwrap();
            let saved: serde_json::Value = serde_json::from_str(&raw).unwrap();
            if accepted {
                assert_eq!(saved["assessment"]["match_rating"]["level"], "good");
                assert!(
                    saved["assessment"]["match_rating"]
                        .to_string()
                        .contains("粉丝评论语言符合")
                );
            } else {
                assert!(saved["assessment"]["match_rating"].is_null());
            }
            let publication = owner
                .prepare_discovery_library_publication(&pair.campaign_id, 1)
                .unwrap();
            let row = publication
                .material
                .sources
                .rows
                .iter()
                .find(|row| row.hit.id == pair.left_search_hit_id)
                .unwrap();
            assert_eq!(
                row.verdict, "pending",
                "passing language and score without a contact must stay out of recommendations"
            );
            assert!(row.contacts.is_empty());
            assert_eq!(row.match_rating.is_some(), accepted);
            if let Some(rating) = &row.match_rating {
                assert_eq!(
                    rating.level,
                    onecrew_creator_runtime::DiscoveryMatchLevel::Good
                );
                assert!(rating.reason.contains("粉丝评论语言符合"));
            }
            eprintln!(
                "language_fixture_requested={language} sample={ordinal} verdict={} score={}",
                receipt.verdict, saved["assessment"]["match_rating"]["level"]
            );
        }
        assert_eq!(
            owner
                .claim_discovery_language_async(pair.campaign_id, 1, pair.left_search_hit_id)
                .await
                .unwrap()
                .is_none(),
            accepted,
            "accepted audience-language evidence must stop further reads"
        );
    }

    #[tokio::test]
    async fn scoring_requires_an_active_account_before_any_model_call() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        assert_eq!(
            platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: "campaign".into(),
                    expected_revision: 1,
                    search_hit_id: "hit".into(),
                })
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
    }

    #[tokio::test]
    #[ignore = "live Codex scoring of explicit synthetic comment fixtures; no provider or customer writes"]
    async fn installed_codex_comment_threshold_requires_three_of_five() {
        let cases = [
            (
                vec![
                    "这条视频解释了稳定币的原理。",
                    "终于弄懂美元锚定是怎么回事。",
                    "储备资产的说明很清楚。",
                    "Useful explanation.",
                    "Gracias por compartir.",
                ],
                true,
            ),
            (
                vec![
                    "这条视频解释了稳定币的原理。",
                    "终于弄懂美元锚定是怎么回事。",
                    "Useful explanation.",
                    "Thanks for explaining.",
                    "Gracias por compartir.",
                ],
                false,
            ),
            (
                vec![
                    "这条视频解释了稳定币的原理。",
                    "这条视频解释了稳定币的原理。",
                    "这条视频解释了稳定币的原理。",
                ],
                false,
            ),
            (
                vec![
                    "Useful explanation.",
                    "Thanks for explaining.",
                    "This helped me understand stablecoins.",
                ],
                false,
            ),
        ];
        for (comments, accepted) in cases {
            let material = serde_json::json!({
                "hit":{"platform":"twitter","source_url":"https://x.com/creator/status/100"},
                "matched_language_source_url":"https://x.com/creator/status/100",
                "criteria":{"language":"中文","category":"稳定币","min_subscribers":8000},
                "observed_followers":10000,
                "video_description":"This video explains how USDC and USDT maintain their dollar peg through reserve assets.",
                "language_samples":[{
                    "author_id":"42","source_url":"https://x.com/creator/status/100",
                    "text":"This video explains how USDC and USDT maintain their dollar peg through reserve assets.",
                    "comments":comments.iter().enumerate().map(|(i,text)|serde_json::json!({
                        "text":text,"source_url":format!("https://x.com/viewer/status/{}",200+i)
                    })).collect::<Vec<_>>()
                }]
            });
            let proposal: Proposal = crate::discovery_draft::codex::structured(format!(
                "{RUBRIC}\nUNTRUSTED_SAVED_DATA:\n{}",
                serde_json::json!({"objective":"寻找粉丝不少于8000、稳定币内容相关、语言中文的博主","material":material})
            )).await.unwrap();
            if accepted {
                assert_eq!(
                    proposal.finding.match_level,
                    Some(onecrew_creator_runtime::DiscoveryMatchLevel::Good)
                );
                assert!(!proposal.language_only_pending);
                assert!(proposal.finding.rationale.contains("粉丝评论语言符合"));
            } else {
                assert_eq!(
                    proposal.finding.outcome,
                    onecrew_creator_runtime::DiscoveryReviewOutcome::Insufficient
                );
                assert!(proposal.finding.match_level.is_none());
                assert!(proposal.language_only_pending);
            }
        }
    }

    #[tokio::test]
    #[ignore = "three bounded signed-in Codex evaluations; isolated fixture DB, no provider search or customer writes"]
    async fn installed_codex_initial_matching_round_trip() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair};
        for (name, description, expected) in [
            (
                "chinese",
                "今天解析稳定币的储备资产与赎回机制。我展示美元稳定币的链上发行流程，并比较不同稳定币的储备审计报告。",
                "qualified",
            ),
            (
                "english",
                "Today I explain stablecoin reserves and redemption. I demonstrate on-chain issuance and compare reserve audit reports for dollar stablecoins.",
                "pending",
            ),
            ("missing", "", "pending"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let platform = LocalPlatform::open(dir.path()).unwrap();
            authenticate(&platform, "fixture-operator");
            let pair = saved_pair(&platform);
            let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
            db.execute("UPDATE discovery_campaign_revisions SET brief_json=json_quote('寻找发布中文稳定币内容的作者') WHERE discovery_campaign_id=?1", [&pair.campaign_id]).unwrap();
            db.execute("UPDATE discovery_campaign_revisions SET criteria_json=?1 WHERE discovery_campaign_id=?2",
                rusqlite::params![r#"{"min_subscribers":8000,"category":"稳定币","language":"中文","region":"","requires_ai_tool_demo":false,"requires_public_tool_url":false}"#, pair.campaign_id]).unwrap();
            let material = platform
                .creators()
                .unwrap()
                .prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id)
                .unwrap();
            db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.video_description',?1) WHERE id=?2",
                rusqlite::params![description, material.candidate_id]).unwrap();
            let receipt = platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: pair.campaign_id.clone(),
                    expected_revision: 1,
                    search_hit_id: pair.left_search_hit_id.clone(),
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(receipt.verdict, expected, "case {name}");
            assert_eq!(receipt.operator_account_id, "fixture-operator");
            let raw: String = db
                .query_row(
                    "SELECT evidence_json FROM discovery_candidates WHERE id=?1",
                    [&receipt.candidate_id],
                    |r| r.get(0),
                )
                .unwrap();
            let saved: serde_json::Value = serde_json::from_str(&raw).unwrap();
            eprintln!(
                "initial_matching_case={name} verdict={} score={} reason={}",
                receipt.verdict,
                saved["assessment"]["match_rating"]["level"],
                saved["reviewed_match"]["reason"]
            );
            assert_eq!(
                saved["assessment"]["match_rating"].is_null(),
                expected == "pending"
            );
            let retry = platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: pair.campaign_id,
                    expected_revision: 1,
                    search_hit_id: pair.left_search_hit_id,
                })
                .await
                .unwrap();
            assert_eq!(retry.unwrap().review_id, receipt.review_id);
            let count: i64 = db
                .query_row("SELECT count(*) FROM discovery_evidence_reviews", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[tokio::test]
    #[ignore = "three live model evaluations over original multilingual text; isolated fixture DB, no provider or customer writes"]
    async fn installed_codex_sufficient_original_text_does_not_request_comments() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair};
        for (language, description) in [
            (
                "English",
                "Today I explain how stablecoins maintain a dollar peg. I compare reserve assets and redemption mechanisms, demonstrate on-chain issuance, and discuss what reserve audit reports can and cannot prove.",
            ),
            (
                "日本語",
                "今日はステーブルコインの準備資産と償還の仕組みを解説します。米ドルとの交換価値を維持する方法を比較し、オンチェーンでの発行手順を実演します。さらに、準備資産の監査報告書から分かることと分からないことを説明します。",
            ),
            (
                "Français",
                "Aujourd'hui, j'explique comment les stablecoins maintiennent leur valeur par rapport au dollar. Je compare les actifs de réserve et les mécanismes de remboursement, puis je montre leur émission sur la blockchain. Je présente également les limites des rapports d'audit des réserves.",
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let platform = LocalPlatform::open(dir.path()).unwrap();
            authenticate(&platform, "fixture-operator");
            let pair = saved_pair(&platform);
            let owner = platform.creators().unwrap();
            let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
            db.execute("UPDATE discovery_campaign_revisions SET brief_json=json_quote(?1),criteria_json=?2 WHERE discovery_campaign_id=?3",
                rusqlite::params![format!("寻找稳定币内容作者，内容语言为{language}"),
                    serde_json::json!({"min_subscribers":8000,"category":"稳定币","language":language,"requires_ai_tool_demo":false,"requires_public_tool_url":false}).to_string(),pair.campaign_id]).unwrap();
            let material = owner
                .prepare_discovery_review_material(&pair.campaign_id, 1, &pair.left_search_hit_id)
                .unwrap();
            db.execute("UPDATE discovery_candidates SET evidence_json=json_set(evidence_json,'$.video_description',?1) WHERE id=?2",
                rusqlite::params![description,material.candidate_id]).unwrap();
            let receipt = platform
                .evaluate_discovery_match(DiscoveryReviewMaterialInput {
                    campaign_id: pair.campaign_id.clone(),
                    expected_revision: 1,
                    search_hit_id: pair.left_search_hit_id.clone(),
                })
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                receipt.verdict, "qualified",
                "{language}: original text must qualify without comments"
            );
            assert!(
                owner
                    .claim_discovery_language_async(
                        pair.campaign_id.clone(),
                        1,
                        pair.left_search_hit_id
                    )
                    .await
                    .unwrap()
                    .is_none()
            );
            let samples: i64 = db
                .query_row("SELECT count(*) FROM discovery_language_samples", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(samples, 0, "{language}: no comments reservation or read");
            eprintln!(
                "original_language_case={language} verdict={} comments_reserved={samples}",
                receipt.verdict
            );
        }
    }
}

#[cfg(test)]
#[path = "discovery_match_live.rs"]
mod live_tests;

#[cfg(test)]
#[path = "discovery_native_acceptance.rs"]
mod native_acceptance;
