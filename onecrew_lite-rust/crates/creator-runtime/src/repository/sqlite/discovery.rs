use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::{
    CreateDiscoveryCampaignInput, DiscoveryCampaign, DiscoveryCandidate, DiscoveryCandidateCounts,
    DiscoveryPlatformRun, DiscoveryPlatformScope, DiscoverySearchClaim, DiscoverySearchHit,
    DiscoveryVideoClaim, FinishDiscoverySearchInput, FinishDiscoveryVideoInput,
    IncreaseDiscoveryBudgetInput, IncreaseDiscoveryTargetInput, ListDiscoveryCandidatesInput,
    ListDiscoverySearchHitsInput, UpsertDiscoveryCandidateInput,
    discovery_qualification::{assess, assessed_evidence, merge_pending_evidence},
    repository::DiscoveryCampaignRepository,
};

use super::{SqliteRepository, now};

fn merge_with_audited_review_proofs(
    transaction: &rusqlite::Connection,
    current: serde_json::Value,
    previous_json: &str,
    candidate_id: &str,
    campaign_id: &str,
    revision: u32,
    native_author_id: &str,
) -> Result<serde_json::Value, String> {
    let mut previous: serde_json::Value =
        serde_json::from_str(previous_json).map_err(|error| error.to_string())?;
    super::discovery_review::strip_unaudited_proofs(
        transaction,
        &mut previous,
        candidate_id,
        campaign_id,
        revision,
        native_author_id,
    )?;
    let mut merged = merge_pending_evidence(current, &previous);
    let object = merged
        .as_object_mut()
        .ok_or("Candidate evidence is invalid")?;
    for key in [
        "reviewed_apparel_content",
        "reviewed_category_content",
        "reviewed_language",
        "reviewed_match",
        "reviewed_residence",
    ] {
        let Some(proof) = previous.get(key) else {
            continue;
        };
        let source = proof["source_url"]
            .as_str()
            .ok_or("Reviewed source is invalid")?;
        let sources = object
            .get_mut("source_urls")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or("Candidate source list is invalid")?;
        if !sources.iter().any(|value| value.as_str() == Some(source)) {
            sources.insert(1.min(sources.len()), serde_json::json!(source));
            sources.truncate(20);
        }
        object.insert(key.into(), proof.clone());
    }
    Ok(merged)
}

pub(super) fn next_search_keyword(
    connection: &rusqlite::Connection,
    campaign_id: &str,
    scope: &DiscoveryPlatformScope,
) -> Result<Option<String>, String> {
    let failed_query = connection
        .query_row(
            "SELECT status,stop_code,active_query,calls_used FROM discovery_platform_runs
             WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
            params![campaign_id, scope.platform, scope.profile_ref],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, u32>(3)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let start = if failed_query.0 == "technical_stop"
        && crate::discovery::can_advance_search_after_stop(failed_query.1.as_deref())
    {
        failed_query
            .2
            .as_ref()
            .and_then(|query| scope.keywords.iter().position(|keyword| keyword == query))
            .map_or(failed_query.3 as usize, |index| index + 1)
    } else {
        0
    };
    let mut statement = connection
        .prepare(
            "SELECT EXISTS(SELECT 1 FROM discovery_search_completions
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND search_keyword=?4)",
        )
        .map_err(|error| error.to_string())?;
    for keyword in scope.keywords.iter().skip(start) {
        let completed: bool = statement
            .query_row(
                params![campaign_id, scope.platform, scope.profile_ref, keyword],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !completed {
            return Ok(Some(keyword.clone()));
        }
    }
    Ok(None)
}

impl DiscoveryCampaignRepository for SqliteRepository {
    fn append_discovery_queries(
        &self,
        account: &str,
        input: &crate::AppendDiscoveryQueries,
    ) -> Result<(), String> {
        super::discovery_query_rounds::append(self, account, input)
    }
    fn discovery_read_stops(
        &self,
        campaign: &str,
    ) -> Result<Vec<crate::DiscoveryReadStop>, String> {
        let db = self.read_connection()?;
        let mut result = super::discovery_read_health::stops(&db, campaign, "profile")?;
        result.extend(super::discovery_read_health::stops(
            &db, campaign, "content",
        )?);
        Ok(result)
    }
    fn discovery_library_table_name(
        &self,
        campaign_id: &str,
    ) -> Result<crate::DiscoveryLibraryTableName, String> {
        let connection = self.read_connection()?;
        // Creation order (including drafts) is stable even for simultaneous
        // plans. Full local dates distinguish years; repeated syncs don't count.
        let (brief, date, ordinal): (String, String, i64) = connection.query_row(
            "SELECT r.brief_json, strftime('%m%d',c.created_at,'unixepoch','localtime'),
             (SELECT COUNT(*) FROM discovery_campaigns p WHERE p.rowid<=c.rowid
               AND date(p.created_at,'unixepoch','localtime')=date(c.created_at,'unixepoch','localtime'))
             FROM discovery_campaigns c JOIN discovery_campaign_revisions r
               ON r.discovery_campaign_id=c.id AND r.revision=1 WHERE c.id=?1",
            [campaign_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).map_err(|error| error.to_string())?;
        let brief: String = serde_json::from_str(&brief).map_err(|error| error.to_string())?;
        let target = brief
            .lines()
            .take_while(|line| {
                ![
                    "Creator location:",
                    "Content language:",
                    "Content category:",
                    "Preferred search keywords:",
                    "Search only these entries:",
                    "Search all supported entries;",
                ]
                .iter()
                .any(|prefix| line.starts_with(prefix))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let target: String = target
            .trim()
            .chars()
            .filter(|character| !character.is_control())
            .take(120)
            .collect();
        if target.is_empty() {
            return Err("discovery_target_name_missing".into());
        }
        let product = crate::discovery_library::product_name(&brief);
        if product.is_empty() {
            return Err("discovery_target_name_missing".into());
        }
        let (criteria, count): (String, u32) = connection.query_row(
            "SELECT r.criteria_json,c.target_count FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=?1",
            [campaign_id], |r| Ok((r.get(0)?,r.get(1)?))
        ).map_err(|e|e.to_string())?;
        let criteria: serde_json::Value =
            serde_json::from_str(&criteria).map_err(|e| e.to_string())?;
        let subject = criteria["table_subject"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&product);
        let subject: String = subject
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(40)
            .collect();
        Ok(crate::DiscoveryLibraryTableName {
            name: format!("{subject}{count}个"),
            previous_auto_name: format!("{product}_{date}_{ordinal}"),
        })
    }

    fn recent_discovery_campaigns(&self) -> Result<Vec<crate::DiscoveryCampaignSummary>, String> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT c.id,r.brief_json,c.phase,c.target_count,c.used_calls,c.max_calls,c.updated_at
             FROM discovery_campaigns c JOIN discovery_campaign_revisions r
               ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision
             ORDER BY c.updated_at DESC,c.created_at DESC,c.id DESC LIMIT 5"
        ).map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let (id, brief, phase, target_count, used_calls, max_calls, updated_at) =
                row.map_err(|error| error.to_string())?;
            Ok(crate::DiscoveryCampaignSummary {
                id,
                brief: serde_json::from_str(&brief).map_err(|error| error.to_string())?,
                phase,
                target_count,
                used_calls,
                max_calls,
                updated_at,
            })
        })
        .collect()
    }

    fn read_discovery_identity_groups_basis(
        &self,
        campaign: &str,
        revision: u32,
    ) -> Result<crate::DiscoveryIdentityGroupsBasis, String> {
        super::discovery_identity_groups::read(self, campaign, revision)
    }
    fn read_discovery_identity_revocation(
        &self,
        input: &crate::RevokeDiscoveryIdentityReviewInput,
    ) -> Result<crate::DiscoveryIdentityRevocationBasis, String> {
        super::discovery_identity_review::read_revocation(self, input)
    }

    fn commit_discovery_identity_revocation(
        &self,
        prepared: &crate::PreparedDiscoveryIdentityRevocation,
    ) -> Result<crate::StoredDiscoveryIdentityReview, String> {
        super::discovery_identity_review::commit_revocation(self, prepared)
    }
    fn read_discovery_identity_submission(
        &self,
        pair: &crate::DiscoveryIdentityMergeMaterialInput,
        decision_id: &str,
    ) -> Result<crate::DiscoveryIdentitySubmissionBasis, String> {
        super::discovery_identity_review::read_submission(self, pair, decision_id)
    }

    fn find_discovery_identity_review(
        &self,
        campaign_id: &str,
        decision_id: &str,
    ) -> Result<Option<crate::StoredDiscoveryIdentityReview>, String> {
        super::discovery_identity_review::find(self, campaign_id, decision_id)
    }

    fn commit_discovery_identity_review(
        &self,
        prepared: &crate::PreparedDiscoveryIdentityReview,
    ) -> Result<crate::StoredDiscoveryIdentityReview, String> {
        super::discovery_identity_review::commit(self, prepared)
    }
    fn read_discovery_review_submission(
        &self,
        account: &str,
        campaign: &str,
        request: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryReviewSubmissionBasis, String> {
        super::discovery_review::read_submission(self, account, campaign, request, revision, hit)
    }

    fn find_discovery_review(
        &self,
        account: &str,
        campaign: &str,
        request: &str,
    ) -> Result<Option<crate::StoredDiscoveryReview>, String> {
        super::discovery_review::find(self, account, campaign, request)
    }

    fn commit_discovery_review(
        &self,
        prepared: &crate::PreparedDiscoveryReview,
    ) -> Result<crate::StoredDiscoveryReview, String> {
        super::discovery_review::commit(self, prepared)
    }

    fn read_discovery_review_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        search_hit_id: &str,
    ) -> Result<crate::DiscoveryReviewBasis, String> {
        super::discovery_review::read_basis(self, campaign_id, expected_revision, search_hit_id)
    }

    fn next_discovery_independent_read(
        &self,
        campaign: &str,
        revision: u32,
        scoring_hit: &str,
        scoring_hits: &[String],
    ) -> Result<Option<crate::DiscoveryStep>, String> {
        let basis = super::discovery_review::read_basis(self, campaign, revision, scoring_hit)?;
        let platform = basis.hit.platform.as_str();
        // Composite web sources keep conservative deferral. Native work is selected
        // only after excluding the scoring author and all known aliases.
        if !matches!(
            platform,
            "twitter"
                | "bilibili"
                | "facebook"
                | "reddit"
                | "instagram"
                | "weibo"
                | "pinterest"
                | "linkedin"
                | "tiktok"
                | "youtube"
        ) {
            return Ok(None);
        }
        let db = self.read_connection()?;
        let mut query = db.prepare("SELECT h.id FROM discovery_search_hits h
          LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id
          LEFT JOIN discovery_search_authors held ON held.search_hit_id=?2
          LEFT JOIN discovery_identity_reads i ON i.search_hit_id=h.id
          WHERE h.discovery_campaign_id=?1 AND (h.platform!=?3 OR h.id=?2
            OR a.author_id=held.author_id
            OR a.author_id=?4 OR i.account_id=?4
            OR EXISTS(SELECT 1 FROM discovery_video_identity_bindings b WHERE b.search_hit_id=h.id AND b.native_author_id=?4)
            OR h.source_url IN (SELECT source_url FROM discovery_search_hits WHERE id=?2))")
            .map_err(|e|e.to_string())?;
        let mut held = Vec::new();
        for held_hit in scoring_hits {
            let held_basis =
                super::discovery_review::read_basis(self, campaign, revision, held_hit)?;
            if matches!(held_basis.hit.platform.as_str(), "google" | "yahoo") {
                return Ok(None);
            }
            let excluded = query
                .query_map(
                    rusqlite::params![campaign, held_hit, platform, held_basis.native_author_id],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            held.extend(excluded);
        }
        held.sort();
        held.dedup();
        drop(query);
        drop(db);
        if platform == "youtube" {
            if let Some(hit) =
                super::discovery_transcripts::next_transcript_hit_excluding(self, campaign, &held)?
            {
                return Ok(Some(crate::DiscoveryStep::ReadTranscript {
                    search_hit_id: hit.id,
                    profile_ref: hit.profile_ref,
                }));
            }
            let db = self.read_connection()?;
            return Ok(super::discovery_language::next_hit_excluding(
                &db, campaign, platform, &held,
            )?
            .map(|search_hit_id| crate::DiscoveryStep::ReadContent { search_hit_id }));
        }
        if let Some(search_hit_id) =
            super::discovery_content::next_content_hit_excluding(self, campaign, platform, &held)?
        {
            return Ok(Some(crate::DiscoveryStep::ReadContent { search_hit_id }));
        }
        let excluded = Vec::new();
        if let Some(search_hit_id) =
            super::discovery_profiles::next_profile_hit_excluding(self, campaign, &excluded, &held)?
        {
            return Ok(Some(
                if super::discovery_identity::profile_needs_identity(
                    self,
                    campaign,
                    &search_hit_id,
                )? {
                    crate::DiscoveryStep::ReadIdentity { search_hit_id }
                } else {
                    crate::DiscoveryStep::ReadProfile { search_hit_id }
                },
            ));
        }
        if platform == "facebook"
            && let Some(search_hit_id) =
                super::discovery_facebook_identity::next_hit(self, campaign, &[])?
            && !held.contains(&search_hit_id)
        {
            return Ok(Some(crate::DiscoveryStep::ReadFacebookIdentity {
                search_hit_id,
            }));
        }
        Ok(None)
    }

    fn read_discovery_library_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<crate::DiscoveryLibraryBasis, String> {
        super::discovery_library::read_basis(self, campaign_id, expected_revision)
    }

    fn read_discovery_identity_merge_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        left_search_hit_id: &str,
        right_search_hit_id: &str,
    ) -> Result<[crate::DiscoveryReviewBasis; 2], String> {
        super::discovery_review::read_pair_basis(
            self,
            campaign_id,
            expected_revision,
            left_search_hit_id,
            right_search_hit_id,
        )
    }

    fn reserve_discovery_library_version(
        &self,
        basis: &crate::DiscoveryLibraryBasis,
        snapshot_hash: &str,
    ) -> Result<u64, String> {
        super::discovery_library::reserve_version(self, basis, snapshot_hash)
    }

    fn reserve_discovery_library_publication(
        &self,
        basis: &crate::DiscoveryIdentityGroupsBasis,
        source_hash: &str,
        identity_hash: &str,
    ) -> Result<crate::DiscoveryLibraryVersions, String> {
        super::discovery_library::reserve_publication(self, basis, source_hash, identity_hash)
    }

    fn next_discovery_contact_lookup(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<crate::DiscoveryContactLookup>, String> {
        super::discovery_contact_lookup::next(self, campaign, excluded_platforms)
    }
    fn claim_discovery_contact_lookup(
        &self,
        input: crate::DiscoveryContactLookup,
    ) -> Result<crate::DiscoveryContactLookup, String> {
        super::discovery_contact_lookup::claim(self, input)
    }
    fn finish_discovery_contact_lookup(
        &self,
        input: crate::DiscoveryContactLookup,
    ) -> Result<crate::DiscoveryContactLookup, String> {
        super::discovery_contact_lookup::finish(self, input)
    }

    fn next_discovery_public_page(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<crate::DiscoveryPublicPageInput>, String> {
        super::discovery_public_pages::next_public_page(self, campaign, excluded_platforms)
    }

    fn discovery_profile_needs_identity(&self, campaign: &str, hit: &str) -> Result<bool, String> {
        super::discovery_identity::profile_needs_identity(self, campaign, hit)
    }

    fn claim_discovery_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryIdentityClaim, String> {
        super::discovery_identity::claim_identity_read(self, campaign, revision, hit)
    }

    fn finish_discovery_identity(
        &self,
        input: crate::FinishDiscoveryIdentityInput,
    ) -> Result<crate::DiscoveryIdentityRead, String> {
        super::discovery_identity::finish_identity_read(self, input)
    }

    fn get_discovery_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryIdentityRead, String> {
        super::discovery_identity::get_identity_read(self, campaign, hit)
    }

    fn next_discovery_content_hit(
        &self,
        campaign: &str,
        platform: &str,
    ) -> Result<Option<String>, String> {
        super::discovery_content::next_content_hit(self, campaign, platform)
    }

    fn next_discovery_facebook_target_hit(&self, campaign: &str) -> Result<Option<String>, String> {
        let connection = self.read_connection()?;
        super::discovery_facebook::next_hit(&connection, campaign)
    }

    fn claim_discovery_facebook_targets(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileClaim, String> {
        super::discovery_facebook::claim(self, campaign, revision, hit)
    }

    fn finish_discovery_facebook_targets(
        &self,
        claim: crate::DiscoveryProfileClaim,
        urls: Option<Vec<String>>,
        stop: Option<String>,
    ) -> Result<crate::DiscoveryFacebookTargetsRead, String> {
        super::discovery_facebook::finish(self, claim, urls, stop)
    }

    fn claim_discovery_language(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<Option<crate::DiscoveryLanguageClaim>, String> {
        super::discovery_language::claim(self, campaign, revision, hit)
    }

    fn finish_discovery_language(
        &self,
        claim: crate::DiscoveryLanguageClaim,
        sample: Option<crate::DiscoveryLanguageSample>,
        stop: Option<String>,
    ) -> Result<(), String> {
        super::discovery_language::finish(self, claim, sample, stop)
    }

    fn claim_discovery_content(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryContentClaim, String> {
        super::discovery_content::claim_content_read(self, campaign, revision, hit)
    }

    fn finish_discovery_content(
        &self,
        input: crate::FinishDiscoveryContentInput,
    ) -> Result<crate::DiscoveryContentRead, String> {
        super::discovery_content::finish_content_read(self, input)
    }

    fn get_discovery_content(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryContentRead, String> {
        super::discovery_content::get_content_read(self, campaign, hit)
    }

    fn next_discovery_source_author_hit(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<String>, String> {
        super::discovery_douyin::next_hit(self, campaign, excluded_platforms)
    }
    fn next_discovery_facebook_identity_hit(
        &self,
        campaign: &str,
        excluded: &[String],
    ) -> Result<Option<String>, String> {
        super::discovery_facebook_identity::next_hit(self, campaign, excluded)
    }
    fn claim_discovery_facebook_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryFacebookIdentityClaim, String> {
        super::discovery_facebook_identity::claim(self, campaign, revision, hit)
    }
    fn finish_discovery_facebook_identity(
        &self,
        input: crate::FinishDiscoveryFacebookIdentityInput,
    ) -> Result<crate::DiscoveryFacebookIdentityRead, String> {
        super::discovery_facebook_identity::finish(self, input)
    }
    fn get_discovery_facebook_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryFacebookIdentityRead, String> {
        super::discovery_facebook_identity::get(self, campaign, hit)
    }
    fn claim_discovery_source_author(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoverySourceAuthorClaim, String> {
        super::discovery_douyin::claim(self, campaign, revision, hit)
    }
    fn finish_discovery_source_author(
        &self,
        input: crate::FinishDiscoverySourceAuthorInput,
    ) -> Result<crate::DiscoverySourceAuthorRead, String> {
        super::discovery_douyin::finish(self, input)
    }
    fn get_discovery_source_author(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoverySourceAuthorRead, String> {
        super::discovery_douyin::get(self, campaign, hit)
    }

    fn next_discovery_web_source_hit(
        &self,
        campaign: &str,
        excluded: &[String],
    ) -> Result<Option<String>, String> {
        super::discovery_web_sources::next_hit(self, campaign, excluded)
    }
    fn claim_discovery_web_source(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryWebSourceClaim, String> {
        super::discovery_web_sources::claim(self, campaign, revision, hit)
    }
    fn finish_discovery_web_source(
        &self,
        input: crate::FinishDiscoveryWebSourceInput,
    ) -> Result<crate::DiscoveryWebSourceRead, String> {
        super::discovery_web_sources::finish(self, input)
    }
    fn get_discovery_web_source(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryWebSourceRead, String> {
        super::discovery_web_sources::get(self, campaign, hit)
    }

    fn next_discovery_profile_hit(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<String>, String> {
        super::discovery_profiles::next_profile_hit(self, campaign, excluded_platforms)
    }

    fn claim_discovery_profile(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileClaim, String> {
        super::discovery_profiles::claim_profile_read(self, campaign, revision, hit)
    }

    fn finish_discovery_profile(
        &self,
        input: crate::FinishDiscoveryProfileInput,
    ) -> Result<crate::DiscoveryProfileRead, String> {
        super::discovery_profiles::finish_profile_read(self, input)
    }

    fn get_discovery_profile(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileRead, String> {
        super::discovery_profiles::get_profile_read(self, campaign, hit)
    }

    fn claim_discovery_public_page(
        &self,
        input: crate::DiscoveryPublicPageInput,
    ) -> Result<crate::DiscoveryPublicPageClaim, String> {
        super::discovery_public_pages::claim_public_page(self, input)
    }

    fn finish_discovery_public_page(
        &self,
        input: crate::FinishDiscoveryPublicPageInput,
    ) -> Result<crate::DiscoveryPublicPageRead, String> {
        super::discovery_public_pages::finish_public_page(self, input)
    }

    fn get_discovery_public_page(
        &self,
        campaign: &str,
        candidate: &str,
        url: &str,
    ) -> Result<crate::DiscoveryPublicPageRead, String> {
        super::discovery_public_pages::get_public_page(self, campaign, candidate, url)
    }

    fn next_discovery_transcript_hit(
        &self,
        campaign: &str,
    ) -> Result<Option<DiscoverySearchHit>, String> {
        super::discovery_transcripts::next_transcript_hit(self, campaign)
    }

    fn claim_discovery_transcript(
        &self,
        campaign: &str,
        revision: u32,
        profile: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryTranscriptClaim, String> {
        super::discovery_transcripts::claim_transcript_read(self, campaign, revision, profile, hit)
    }

    fn finish_discovery_transcript(
        &self,
        input: crate::FinishDiscoveryTranscriptInput,
    ) -> Result<crate::DiscoveryTranscriptRead, String> {
        super::discovery_transcripts::finish_transcript_read(self, input)
    }

    fn get_discovery_transcript(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryTranscriptRead, String> {
        super::discovery_transcripts::get_transcript_read(self, campaign, hit)
    }

    fn next_discovery_search_keyword(
        &self,
        campaign_id: &str,
        scope: &DiscoveryPlatformScope,
    ) -> Result<Option<String>, String> {
        let connection = self.read_connection()?;
        next_search_keyword(&connection, campaign_id, scope)
    }

    fn next_discovery_video_hit(
        &self,
        campaign_id: &str,
    ) -> Result<Option<DiscoverySearchHit>, String> {
        let connection = self.read_connection()?;
        connection
            .query_row(
                "SELECT h.id,h.discovery_campaign_id,h.platform,h.profile_ref,h.search_keyword,
                    h.source_url,h.title,h.author_label,h.captured_at,p.author_id,p.author_url,p.observed_at
             FROM discovery_search_hits h
             LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id
             JOIN discovery_platform_runs r
               ON r.discovery_campaign_id=h.discovery_campaign_id
              AND r.platform=h.platform AND r.profile_ref=h.profile_ref
             WHERE h.discovery_campaign_id=?1 AND h.platform='youtube'
               AND (r.status='completed' OR
                    (r.status='technical_stop' AND r.stop_code='interrupted_video_read'
                     AND r.active_query LIKE 'video:' || h.id || ':%'))
               AND NOT EXISTS(SELECT 1 FROM discovery_video_completions v
                   WHERE v.search_hit_id=h.id)
               AND NOT EXISTS(SELECT 1 FROM discovery_video_read_failures f
                   WHERE f.search_hit_id=h.id)
               AND NOT EXISTS(SELECT 1 FROM discovery_candidates known
                   WHERE known.discovery_campaign_id=h.discovery_campaign_id
                     AND known.criteria_revision=(SELECT current_revision FROM discovery_campaigns WHERE id=h.discovery_campaign_id)
                     AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref
                     AND r.status='completed' AND known.platform_account_id=p.author_id AND known.verdict='qualified'
                     AND json_extract(known.evidence_json,'$.reviewed_match.verified')=1
                     AND json_extract(known.evidence_json,'$.reviewed_match.native_author_id')=p.author_id
                     AND json_extract(known.evidence_json,'$.reviewed_match.level') IN ('good','excellent')
                     AND json_extract(known.evidence_json,'$.reviewed_match.outcome')='supported')
               AND NOT EXISTS(SELECT 1 FROM discovery_candidates c,
                   json_each(c.evidence_json, '$.source_urls') s
                   WHERE c.discovery_campaign_id=h.discovery_campaign_id
                     AND c.platform=h.platform AND s.value=h.source_url)
             ORDER BY CASE WHEN r.status='technical_stop' THEN 0 ELSE 1 END,
               EXISTS(SELECT 1 FROM discovery_candidates known
                   WHERE known.discovery_campaign_id=h.discovery_campaign_id
                     AND known.platform=h.platform AND known.source_profile_ref=h.profile_ref
                     AND known.display_name=h.author_label),h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1",
                [campaign_id],
                |row| {
                    Ok(DiscoverySearchHit {
                        id: row.get(0)?,
                        campaign_id: row.get(1)?,
                        platform: row.get(2)?,
                        profile_ref: row.get(3)?,
                        search_keyword: row.get(4)?,
                        source_url: row.get(5)?,
                        title: row.get(6)?,
                        author_label: row.get(7)?,
                        captured_at: row.get(8)?,
                        author_id: row.get(9)?,
                        author_url: row.get(10)?,
                        author_observed_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    fn claim_discovery_video(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        profile_ref: &str,
        search_hit_id: &str,
    ) -> Result<DiscoveryVideoClaim, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let (used_calls, max_calls) = transaction
            .query_row(
                "SELECT used_calls,max_calls FROM discovery_campaigns
             WHERE id=?1 AND phase='active' AND current_revision=?2",
                params![campaign_id, expected_revision],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, Option<u32>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("Campaign is not active at the approved revision")?;
        if max_calls.is_some_and(|max| used_calls >= max) {
            return Err("Campaign read budget is exhausted".into());
        }
        let source_url: String = transaction
            .query_row(
                "SELECT source_url FROM discovery_search_hits
             WHERE id=?1 AND discovery_campaign_id=?2 AND platform='youtube' AND profile_ref=?3",
                params![search_hit_id, campaign_id, profile_ref],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("Search hit is outside the approved YouTube profile")?;
        let url = url::Url::parse(&source_url).map_err(|_| "Search hit URL is invalid")?;
        let valid_video_id = |value: &str| {
            value.len() == 11
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        };
        let valid_target = if url.path() == "/watch" {
            url.query_pairs()
                .any(|(key, value)| key == "v" && valid_video_id(&value))
        } else {
            url.path_segments()
                .and_then(|mut segments| {
                    let kind = segments.next()?;
                    let id = segments.next()?;
                    Some(matches!(kind, "shorts" | "live") && valid_video_id(id))
                })
                .unwrap_or(false)
        };
        if url.scheme() != "https"
            || !matches!(url.host_str(), Some("youtube.com" | "www.youtube.com"))
            || !valid_target
        {
            return Err("Search hit is not a YouTube video".into());
        }
        let active_prefix = format!("video:{search_hit_id}:");
        let (status, stop_code, active_query): (String, Option<String>, Option<String>) =
            transaction
                .query_row(
                    "SELECT status,stop_code,active_query FROM discovery_platform_runs
             WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2",
                    params![campaign_id, profile_ref],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|error| error.to_string())?;
        let retry = status == "technical_stop"
            && stop_code.as_deref() == Some("interrupted_video_read")
            && active_query
                .as_deref()
                .is_some_and(|value| value.starts_with(&active_prefix));
        let explicit_technical_retry = status == "technical_stop"
            && active_query.is_none()
            && matches!(
                stop_code.as_deref(),
                Some(
                    "opencli_timeout"
                        | "opencli_command_failed"
                        | "opencli_invalid_output"
                        | "opencli_daemon_unavailable"
                        | "opencli_extension_unavailable"
                        | "opencli_profile_unavailable"
                )
            );
        if status != "completed" && !retry && !explicit_technical_retry {
            return Err("YouTube platform has not completed first search or is stopped".into());
        }
        let busy: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND status='searching')",
            [campaign_id], |row| row.get(0),
        ).map_err(|error| error.to_string())?;
        if busy {
            return Err("A Campaign read is active".into());
        }
        if !retry {
            let already_saved: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM discovery_video_completions WHERE search_hit_id=?3)
                 OR EXISTS(SELECT 1 FROM discovery_candidates c, json_each(c.evidence_json, '$.source_urls') s
                 WHERE c.discovery_campaign_id=?1 AND c.platform='youtube' AND s.value=?2)",
                params![campaign_id, source_url, search_hit_id], |row| row.get(0),
            ).map_err(|error| error.to_string())?;
            if already_saved {
                return Err("Search hit already has a saved candidate".into());
            }
        }
        let next_call = used_calls
            .checked_add(1)
            .ok_or("Campaign call counter overflow")?;
        let active = format!("{active_prefix}{next_call}");
        let timestamp = now();
        transaction
            .execute(
                "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
                params![campaign_id, next_call, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.execute(
            "UPDATE discovery_platform_runs SET status='searching',active_query=?3,
             stop_code=NULL,updated_at=?4 WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2",
            params![campaign_id, profile_ref, active, timestamp],
        ).map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(DiscoveryVideoClaim {
            campaign_id: campaign_id.into(),
            expected_revision,
            profile_ref: profile_ref.into(),
            search_hit_id: search_hit_id.into(),
            source_url,
            call_number: next_call,
        })
    }

    fn finish_discovery_video(
        &self,
        input: FinishDiscoveryVideoInput,
    ) -> Result<DiscoveryCampaign, String> {
        if input.candidate.is_some() == input.stop_code.is_some() {
            return Err("Video read needs exactly one candidate or stop code".into());
        }
        if input.stop_code.as_deref().is_some_and(|code| {
            code.is_empty() || code.len() > 128 || code.chars().any(char::is_control)
        }) {
            return Err("Video stop code is invalid".into());
        }
        let claim = &input.claim;
        if let Some(candidate) = &input.candidate {
            crate::discovery::validate_discovery_candidate(candidate)?;
            if candidate.campaign_id != claim.campaign_id
                || candidate.expected_revision != claim.expected_revision
                || candidate.platform != "youtube"
                || candidate.source_profile_ref != claim.profile_ref
                || !candidate
                    .evidence
                    .get("source_urls")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|sources| {
                        sources
                            .iter()
                            .any(|source| source.as_str() == Some(&claim.source_url))
                    })
            {
                return Err("Video candidate does not match reserved search hit".into());
            }
        }
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        // A malformed or unavailable individual video should not stop every
        // other hit from this profile. Transport, login and risk stops still
        // belong to the whole platform. The failure remains retryable by an
        // explicit claim and never counts as completed evidence.
        let source_local_failure = matches!(
            input.stop_code.as_deref(),
            Some(
                "opencli_command_failed"
                    | "opencli_invalid_output"
                    | "opencli_output_too_large"
                    | "opencli_timeout"
            )
        );
        let affected = transaction.execute(
            "UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
             WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2
               AND status='searching' AND active_query=?3
               AND EXISTS(SELECT 1 FROM discovery_search_hits hit WHERE hit.id=?9
                 AND hit.discovery_campaign_id=?1 AND hit.platform='youtube'
                 AND hit.profile_ref=?2 AND hit.source_url=?10)
               AND EXISTS(SELECT 1 FROM discovery_campaigns c WHERE c.id=?1 AND c.phase='active'
                 AND c.current_revision=?7 AND c.used_calls>=?8)",
            params![claim.campaign_id, claim.profile_ref, format!("video:{}:{}", claim.search_hit_id, claim.call_number),
                if input.candidate.is_some() || source_local_failure { "completed" }
                    else if input.stop_code.as_deref() == Some("opencli_risk_control") { "risk_control_stop" }
                    else { "technical_stop" },
                if source_local_failure { None } else { input.stop_code.as_deref() },
                timestamp, claim.expected_revision, claim.call_number,
                claim.search_hit_id, claim.source_url],
        ).map_err(|error| error.to_string())?;
        if affected != 1 {
            return Err("Video claim is stale or Campaign authorization changed".into());
        }
        if let Some(candidate) = input.candidate {
            transaction
                .execute(
                    "DELETE FROM discovery_video_read_failures WHERE search_hit_id=?1",
                    [&claim.search_hit_id],
                )
                .map_err(|error| error.to_string())?;
            let criteria_json: String = transaction
                .query_row(
                    "SELECT criteria_json FROM discovery_campaign_revisions
                 WHERE discovery_campaign_id=?1 AND revision=?2 AND approved_at IS NOT NULL",
                    params![claim.campaign_id, claim.expected_revision],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let criteria =
                serde_json::from_str(&criteria_json).map_err(|error| error.to_string())?;
            let previous: Option<(String, String, String)> = transaction
                .query_row(
                    "SELECT id,verdict,evidence_json FROM discovery_candidates
                 WHERE discovery_campaign_id=?1 AND platform='youtube' AND platform_account_id=?2",
                    params![claim.campaign_id, candidate.platform_account_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let mut merged = match &previous {
                Some((candidate_id, _, evidence)) => merge_with_audited_review_proofs(
                    &transaction,
                    candidate.evidence,
                    evidence,
                    candidate_id,
                    &claim.campaign_id,
                    claim.expected_revision,
                    &candidate.platform_account_id,
                )?,
                None => candidate.evidence,
            };
            let assessment = assess(&criteria, &merged, &candidate.platform_account_id);
            let preserve_rejection = previous
                .as_ref()
                .is_some_and(|(_, previous_verdict, _)| previous_verdict == "rejected");
            if let Some((_, _, previous_json)) = previous.as_ref().filter(|_| preserve_rejection) {
                let old: serde_json::Value =
                    serde_json::from_str(previous_json).map_err(|error| error.to_string())?;
                merged["assessment"] = old["assessment"].clone();
            }
            // Qualified is a derived verdict: if a later video leaves no
            // audited proof, the candidate must return to pending/rejected.
            let verdict = if preserve_rejection {
                "rejected"
            } else {
                assessment.verdict
            };
            let evidence = if preserve_rejection {
                merged
            } else {
                assessed_evidence(merged, &assessment)
            };
            if evidence.to_string().len() > 16_384 {
                return Err("Assessed discovery evidence exceeds the size limit".into());
            }
            transaction.execute(
                "INSERT INTO discovery_candidates
                 (id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,
                  platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
                 VALUES(?1,?2,?3,'youtube',?4,?5,?6,?7,?8,?9,?10,?10,'youtube')
                 ON CONFLICT(discovery_campaign_id,platform,platform_account_id)
                 DO UPDATE SET source_platform=excluded.source_platform,display_name=excluded.display_name,profile_url=excluded.profile_url,
                   evidence_json=excluded.evidence_json,verdict=excluded.verdict,updated_at=excluded.updated_at
                 WHERE discovery_candidates.criteria_revision=excluded.criteria_revision",
                params![uuid::Uuid::new_v4().to_string(), claim.campaign_id, claim.expected_revision,
                    claim.profile_ref, candidate.platform_account_id, candidate.display_name,
                    candidate.profile_url, verdict, evidence.to_string(), timestamp],
            ).map_err(|error| error.to_string())?;
            transaction.execute(
                "INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?1,?2)",
                params![claim.search_hit_id, timestamp],
            ).map_err(|error| error.to_string())?;
            let bound = transaction.execute(
                "INSERT INTO discovery_video_identity_bindings
                 (search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at)
                 SELECT ?1,discovery_campaign_id,criteria_revision,id,platform_account_id,?2
                 FROM discovery_candidates WHERE discovery_campaign_id=?3 AND platform='youtube'
                   AND platform_account_id=?4 AND criteria_revision=?5 AND source_profile_ref=?6",
                params![claim.search_hit_id, timestamp, claim.campaign_id,
                    candidate.platform_account_id, claim.expected_revision, claim.profile_ref],
            ).map_err(|error| error.to_string())?;
            if bound != 1 {
                return Err(
                    "Video candidate identity or revision no longer matches the claim".into(),
                );
            }
        } else if source_local_failure {
            transaction.execute(
                "INSERT INTO discovery_video_read_failures(search_hit_id,call_number,stop_code,updated_at)
                 VALUES(?1,?2,?3,?4)
                 ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,
                   stop_code=excluded.stop_code,updated_at=excluded.updated_at",
                params![claim.search_hit_id, claim.call_number, input.stop_code, timestamp],
            ).map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(&claim.campaign_id)
    }

    fn count_discovery_candidates(
        &self,
        campaign_id: &str,
    ) -> Result<DiscoveryCandidateCounts, String> {
        self.count_discovery_candidates_excluding(campaign_id, &[])
    }

    fn count_discovery_candidates_excluding(
        &self,
        campaign_id: &str,
        excluded_platforms: &[String],
    ) -> Result<DiscoveryCandidateCounts, String> {
        let mut connection = self.read_connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let (revision, phase, policy): (u32, String, String) = tx.query_row(
            "SELECT c.current_revision,c.phase,p.policy FROM discovery_campaigns c
             JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.id AND p.criteria_revision=c.current_revision WHERE c.id=?1",
            [campaign_id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).optional().map_err(|error| error.to_string())?.ok_or("Campaign not found")?;
        let public_page_errors: u32 = tx.query_row(
            "SELECT COUNT(*) FROM discovery_public_page_reads r JOIN discovery_candidates c ON c.id=r.candidate_id
             WHERE c.discovery_campaign_id=?1 AND c.criteria_revision=?2 AND r.status='failed'
             AND NOT EXISTS(SELECT 1 FROM json_each(?3) e WHERE e.value=c.platform)",
            params![campaign_id, revision, serde_json::to_string(excluded_platforms).map_err(|e|e.to_string())?], |r| r.get(0)
        ).map_err(|e|e.to_string())?;
        if policy == "score_and_contact_v1" {
            if phase != "active" {
                return Ok(DiscoveryCandidateCounts {
                    public_page_errors,
                    qualified: 0,
                    pending: 0,
                    rejected: 0,
                });
            }
            // Reuse audited evidence, contact binding and identity grouping used by Library.
            // This prevents raw candidate labels and duplicate platform accounts filling the target.
            let basis =
                super::discovery_identity_groups::read_snapshot(&tx, campaign_id, revision)?;
            tx.commit().map_err(|error| error.to_string())?;
            drop(connection);
            let snapshot = crate::discovery_identity_merge::groups::build(basis)?;
            return Ok(DiscoveryCandidateCounts {
                public_page_errors,
                qualified: snapshot
                    .groups
                    .iter()
                    .filter(|group| {
                        group.members.iter().any(|member| {
                            member.verdict == "qualified"
                                && !excluded_platforms.contains(&member.platform)
                        })
                    })
                    .count() as u32,
                pending: snapshot
                    .groups
                    .iter()
                    .flat_map(|group| &group.members)
                    .filter(|member| {
                        member.verdict == "pending"
                            && !excluded_platforms.contains(&member.platform)
                    })
                    .count() as u32,
                rejected: snapshot
                    .groups
                    .iter()
                    .flat_map(|group| &group.members)
                    .filter(|member| {
                        member.verdict == "rejected"
                            && !excluded_platforms.contains(&member.platform)
                    })
                    .count() as u32,
            });
        }
        tx.commit().map_err(|error| error.to_string())?;
        connection
            .query_row(
                "WITH candidates AS (
                 SELECT d.id,d.verdict,
                    d.verdict='qualified' OR m.candidate_id IS NOT NULL AS recommended
             FROM discovery_campaigns c LEFT JOIN discovery_candidates d
               ON d.discovery_campaign_id=c.id
             JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.id
               AND p.criteria_revision=c.current_revision
             LEFT JOIN discovery_manual_approvals m ON m.discovery_campaign_id=c.id
               AND m.criteria_revision=c.current_revision AND m.criteria_revision=d.criteria_revision
               AND m.candidate_id=d.id
             WHERE c.id=?1 AND (d.id IS NULL OR d.criteria_revision=c.current_revision)
               AND NOT EXISTS(SELECT 1 FROM json_each(?2) excluded WHERE excluded.value=d.platform))
             SELECT COUNT(*) FILTER (WHERE recommended),
                    COUNT(*) FILTER (WHERE id IS NOT NULL AND verdict!='rejected' AND NOT recommended),
                    COUNT(*) FILTER (WHERE verdict='rejected' AND NOT recommended)
             FROM candidates",
                params![campaign_id, serde_json::to_string(excluded_platforms).map_err(|error| error.to_string())?],
                |row| {
                    Ok(DiscoveryCandidateCounts {
                        public_page_errors,
                        qualified: row.get(0)?,
                        pending: row.get(1)?,
                        rejected: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Campaign not found".into())
    }

    fn claim_discovery_search(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        platform: &str,
        profile_ref: &str,
    ) -> Result<DiscoverySearchClaim, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let (max_calls, used_calls, scope_json) = transaction
            .query_row(
                "SELECT c.max_calls,c.used_calls,r.scope_json
                 FROM discovery_campaigns c JOIN discovery_campaign_revisions r
                   ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision
                 WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?2
                   AND r.approved_at IS NOT NULL",
                params![campaign_id, expected_revision],
                |row| {
                    Ok((
                        row.get::<_, Option<u32>>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Campaign is not active at the approved revision".to_string())?;
        if max_calls.is_some_and(|max| used_calls >= max) {
            return Err("Campaign search budget is exhausted".into());
        }
        let scopes = super::discovery_query_rounds::effective_scopes(
            &transaction,
            campaign_id,
            expected_revision,
            &scope_json,
        )?;
        let scope = scopes
            .iter()
            .find(|scope| scope.platform == platform && scope.profile_ref == profile_ref)
            .ok_or_else(|| "Search is outside the approved Campaign scope".to_string())?;
        let (status, calls_used, stop_code, active_query) = transaction
            .query_row(
                "SELECT status,calls_used,stop_code,active_query FROM discovery_platform_runs
                 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
                params![campaign_id, platform, profile_ref],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
        let retry_interrupted =
            status == "technical_stop" && stop_code.as_deref() == Some("interrupted_read");
        let advance_after_failed_keyword = status == "technical_stop"
            && crate::discovery::can_advance_search_after_stop(stop_code.as_deref());
        if status != "unsearched"
            && status != "completed"
            && !retry_interrupted
            && !advance_after_failed_keyword
        {
            return Err("Platform search is already running or stopped".into());
        }
        // First-round ordering belongs to the planner, which knows excluded
        // sources. A temporarily unavailable source must not block a distinct
        // approved keyword here; revision, scope, budget and claim guards remain.
        let query = if retry_interrupted {
            let query = active_query.ok_or("Interrupted search has no saved query")?;
            if !scope.keywords.contains(&query) {
                return Err("Interrupted search query is outside approved scope".into());
            }
            query
        } else {
            next_search_keyword(&transaction, campaign_id, scope)?
                .ok_or_else(|| "Approved scope has no remaining search keywords".to_string())?
        };
        let next_call = calls_used
            .checked_add(1)
            .ok_or("Platform call counter overflow")?;
        let timestamp = now();
        transaction
            .execute(
                "UPDATE discovery_platform_runs SET status='searching',calls_used=?4,
                 stop_code=NULL,active_query=?6,updated_at=?5 WHERE discovery_campaign_id=?1
                 AND platform=?2 AND profile_ref=?3",
                params![
                    campaign_id,
                    platform,
                    profile_ref,
                    next_call,
                    timestamp,
                    query
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=?2 WHERE id=?1",
                params![campaign_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(DiscoverySearchClaim {
            campaign_id: campaign_id.to_owned(),
            expected_revision,
            platform: platform.to_owned(),
            profile_ref: profile_ref.to_owned(),
            call_number: next_call,
            query,
        })
    }

    fn finish_discovery_search(
        &self,
        input: FinishDiscoverySearchInput,
    ) -> Result<DiscoveryCampaign, String> {
        if !matches!(
            input.status.as_str(),
            "completed" | "technical_stop" | "risk_control_stop" | "unauthorized"
        ) || (input.status == "completed" && input.stop_code.is_some())
            || (input.status != "completed"
                && input
                    .stop_code
                    .as_deref()
                    .is_none_or(|code| code.is_empty() || code.len() > 128))
            || input.search_hits.len() > 50
            || (input.status != "completed" && !input.search_hits.is_empty())
        {
            return Err("Search completion status or stop code is invalid".into());
        }
        let mut seen_urls = std::collections::HashSet::new();
        for hit in &input.search_hits {
            if input.claim.platform == "douyin"
                && crate::discovery_douyin::post_id(&hit.source_url).is_none()
            {
                return Err("Douyin search requires its canonical matched video".into());
            }
            match (&hit.author_id, &hit.author_url) {
                (None, None) => {}
                (Some(id), Some(source)) => {
                    if input.claim.platform == "douyin" {
                        return Err(
                            "Douyin native author must be resolved from its matched video receipt"
                                .into(),
                        );
                    }
                    let url = url::Url::parse(source).map_err(|_| "Author source is invalid")?;
                    if input.claim.platform == "linkedin"
                        && (!crate::discovery_linkedin::valid_handle(id)
                            || source != &format!("https://www.linkedin.com/in/{id}/")
                            || &hit.source_url != source)
                    {
                        return Err(
                            "LinkedIn author requires an exact observed profile locator".into()
                        );
                    }
                    let facebook_profile = input.claim.platform == "facebook"
                        && (1..=20).contains(&id.len())
                        && !id.starts_with('0')
                        && id.bytes().all(|b| b.is_ascii_digit())
                        && source == &format!("https://www.facebook.com/profile.php?id={id}");
                    let hackernews_profile = input.claim.platform == "hackernews"
                        && crate::discovery_hackernews::valid_username(id)
                        && source == &format!("https://news.ycombinator.com/user?id={id}");
                    if input.claim.platform == "pixiv"
                        && (!crate::discovery_pixiv::valid_id(id)
                            || source != &format!("https://www.pixiv.net/users/{id}")
                            || crate::discovery_pixiv::artwork_id(&hit.source_url).is_none())
                    {
                        return Err("invalid Pixiv native author binding".into());
                    }
                    if input.claim.platform == "pinterest" {
                        let username = source
                            .strip_prefix("https://www.pinterest.com/")
                            .and_then(|path| path.strip_suffix('/'))
                            .unwrap_or_default();
                        if !crate::discovery_pinterest::valid_id(id)
                            || !crate::discovery_pinterest::valid_username(username)
                            || (&hit.source_url != source
                                && crate::discovery_pinterest::pin_id(&hit.source_url).is_none())
                        {
                            return Err("Pinterest author requires an observed native ID, profile and valid search source".into());
                        }
                    }
                    if input.claim.platform == "hackernews" && !hackernews_profile {
                        return Err("Hacker News author must have an exact observed username profile source".into());
                    }
                    if input.claim.platform == "facebook" && !facebook_profile {
                        return Err(
                            "Facebook author must have an exact observed numeric profile source"
                                .into(),
                        );
                    }
                    if id.trim().is_empty()
                        || id.trim() != id
                        || id.len() > 100
                        || id.chars().any(char::is_control)
                        || source.len() > 2048
                        || url.scheme() != "https"
                        || url.host_str().is_none()
                        || source.chars().any(char::is_control)
                        || url.port().is_some()
                        || !url.username().is_empty()
                        || url.password().is_some()
                        || (url.query().is_some() && !facebook_profile && !hackernews_profile)
                        || url.fragment().is_some()
                    {
                        return Err("Author identity or source is invalid".into());
                    }
                }
                _ => return Err("Author identity and source must be observed together".into()),
            }
            let url = url::Url::parse(&hit.source_url)
                .map_err(|_| "Search hit URL is invalid".to_string())?;
            if url.scheme() != "https"
                || hit.source_url.len() > 2048
                || hit.title.trim().is_empty()
                || hit.title.chars().count() > 500
                || hit.author_label.chars().count() > 200
                || hit.title.chars().any(char::is_control)
                || hit.author_label.chars().any(char::is_control)
                || !seen_urls.insert(&hit.source_url)
            {
                return Err("Search hit source or summary is invalid".into());
            }
        }
        let claim = &input.claim;
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        let preserve_failed_query = input.status == "technical_stop"
            && crate::discovery::can_advance_search_after_stop(input.stop_code.as_deref());
        let affected = transaction
            .execute(
                "UPDATE discovery_platform_runs SET status=?5,stop_code=?6,
                 active_query=CASE WHEN ?10 THEN ?9 ELSE NULL END,updated_at=?7
                 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3
                   AND calls_used=?4 AND status='searching' AND active_query=?9
                   AND EXISTS(SELECT 1 FROM discovery_campaigns c
                     WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?8)",
                params![
                    claim.campaign_id,
                    claim.platform,
                    claim.profile_ref,
                    claim.call_number,
                    input.status,
                    input.stop_code,
                    timestamp,
                    claim.expected_revision,
                    claim.query,
                    preserve_failed_query,
                ],
            )
            .map_err(|error| error.to_string())?;
        if affected != 1 {
            return Err("Search claim is stale or Campaign authorization changed".into());
        }
        if input.status == "completed" {
            transaction
                .execute(
                    "INSERT INTO discovery_search_completions
                 (discovery_campaign_id,platform,profile_ref,search_keyword,completed_at)
                 VALUES(?1,?2,?3,?4,?5)",
                    params![
                        claim.campaign_id,
                        claim.platform,
                        claim.profile_ref,
                        claim.query,
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        for hit in &input.search_hits {
            let hit_id = uuid::Uuid::new_v4().to_string();
            let inserted = transaction
                .execute(
                    "INSERT INTO discovery_search_hits
                     (id,discovery_campaign_id,platform,profile_ref,search_keyword,
                      source_url,title,author_label,captured_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                     ON CONFLICT(discovery_campaign_id,platform,profile_ref,source_url)
                     DO NOTHING",
                    params![
                        hit_id,
                        claim.campaign_id,
                        claim.platform,
                        claim.profile_ref,
                        claim.query,
                        hit.source_url,
                        hit.title,
                        hit.author_label,
                        timestamp,
                    ],
                )
                .map_err(|error| error.to_string())?;
            if inserted == 1 {
                transaction
                    .execute(
                        "INSERT INTO discovery_search_positions(search_hit_id) VALUES(?1)",
                        [&hit_id],
                    )
                    .map_err(|e| e.to_string())?;
            }
            if let (Some(author_id), Some(author_url)) = (&hit.author_id, &hit.author_url) {
                let hit_id: String = transaction
                    .query_row(
                        "SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=?1
                     AND platform=?2 AND profile_ref=?3 AND source_url=?4",
                        params![
                            claim.campaign_id,
                            claim.platform,
                            claim.profile_ref,
                            hit.source_url
                        ],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let existing: Option<(String, String)> = transaction.query_row(
                    "SELECT author_id,author_url FROM discovery_search_authors WHERE search_hit_id=?1",
                    [&hit_id], |row| Ok((row.get(0)?, row.get(1)?)),
                ).optional().map_err(|e| e.to_string())?;
                if let Some((id, url)) = existing {
                    if id != *author_id || url != *author_url {
                        return Err(
                            "Search source author conflicts with the recorded observation".into(),
                        );
                    }
                } else {
                    transaction.execute(
                        "INSERT INTO discovery_search_authors(search_hit_id,author_id,author_url,observed_at)
                         VALUES(?1,?2,?3,?4)",
                        params![hit_id, author_id, author_url, timestamp],
                    ).map_err(|e| e.to_string())?;
                }
            }
        }
        transaction
            .execute(
                "UPDATE discovery_campaigns SET updated_at=?2 WHERE id=?1",
                params![claim.campaign_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(&claim.campaign_id)
    }

    fn list_discovery_search_hits(
        &self,
        input: ListDiscoverySearchHitsInput,
    ) -> Result<Vec<DiscoverySearchHit>, String> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT h.id,h.discovery_campaign_id,h.platform,h.profile_ref,h.search_keyword,
                        h.source_url,h.title,h.author_label,h.captured_at,p.author_id,p.author_url,p.observed_at
                 FROM discovery_search_hits h LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id
                 WHERE h.discovery_campaign_id=?1 AND h.id>?2
                 ORDER BY h.id LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        statement
            .query_map(
                params![
                    input.campaign_id,
                    input.after_id.unwrap_or_default(),
                    input.limit
                ],
                |row| {
                    Ok(DiscoverySearchHit {
                        id: row.get(0)?,
                        campaign_id: row.get(1)?,
                        platform: row.get(2)?,
                        profile_ref: row.get(3)?,
                        search_keyword: row.get(4)?,
                        source_url: row.get(5)?,
                        title: row.get(6)?,
                        author_label: row.get(7)?,
                        captured_at: row.get(8)?,
                        author_id: row.get(9)?,
                        author_url: row.get(10)?,
                        author_observed_at: row.get(11)?,
                    })
                },
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    fn interrupt_discovery_searches(&self) -> Result<u32, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        // Preserve one-attempt receipts on restart; no provider request is replayed.
        transaction.execute("UPDATE discovery_contact_lookups SET receipt_json=json_set(receipt_json,'$.status','failed','$.stop_code','interrupted_contact_lookup','$.links',json('[]'))
          WHERE json_extract(receipt_json,'$.status')='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
            WHERE r.discovery_campaign_id=discovery_contact_lookups.discovery_campaign_id AND r.status='searching'
              AND r.profile_ref=json_extract(receipt_json,'$.profile_ref')
              AND r.active_query='contact:' || discovery_contact_lookups.candidate_id || ':' || json_extract(receipt_json,'$.call_number'))",[]).map_err(|e|e.to_string())?;
        let contacts_interrupted=transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND EXISTS(SELECT 1 FROM discovery_contact_lookups l
           WHERE l.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id
             AND json_extract(l.receipt_json,'$.profile_ref')=discovery_platform_runs.profile_ref
             AND json_extract(l.receipt_json,'$.stop_code')='interrupted_contact_lookup'
             AND active_query='contact:' || l.candidate_id || ':' || json_extract(l.receipt_json,'$.call_number'))",[now()]).map_err(|e|e.to_string())?;
        // Dead contact-page reads are failed attempts, never automatic retries.
        transaction.execute("UPDATE discovery_public_page_reads SET status='failed',stop_code='interrupted_public_page_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_candidates c
            JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.discovery_campaign_id AND r.platform=c.platform AND r.profile_ref=c.source_profile_ref
            WHERE c.id=discovery_public_page_reads.candidate_id AND c.verdict='qualified' AND r.status='searching'
              AND r.active_query='page:' || c.id || ':' || discovery_public_page_reads.call_number)", [now()]).map_err(|e|e.to_string())?;
        let pages_interrupted = transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND EXISTS(SELECT 1 FROM discovery_public_page_reads p JOIN discovery_candidates c ON c.id=p.candidate_id
            WHERE c.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND c.platform=discovery_platform_runs.platform
              AND c.source_profile_ref=discovery_platform_runs.profile_ref AND c.verdict='qualified'
              AND p.status='failed' AND p.stop_code='interrupted_public_page_read'
              AND active_query='page:' || p.candidate_id || ':' || p.call_number)", [now()]).map_err(|e|e.to_string())?;
        transaction.execute("UPDATE discovery_facebook_content_targets SET status='failed',stop_code='interrupted_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
            WHERE r.discovery_campaign_id=discovery_facebook_content_targets.discovery_campaign_id AND r.platform='facebook'
              AND r.profile_ref=discovery_facebook_content_targets.profile_ref AND r.status='searching'
              AND r.active_query='targets:' || discovery_facebook_content_targets.search_hit_id || ':' || discovery_facebook_content_targets.call_number)",
          [now()]).map_err(|e|e.to_string())?;
        transaction.execute(
            "UPDATE discovery_identity_reads SET status='failed',stop_code='interrupted_identity_read',updated_at=?1
             WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
               WHERE r.discovery_campaign_id=discovery_identity_reads.discovery_campaign_id
                 AND r.platform='twitter' AND r.profile_ref=discovery_identity_reads.profile_ref
                 AND r.status='searching'
                 AND r.active_query='identity:' || discovery_identity_reads.search_hit_id || ':' || discovery_identity_reads.call_number)",
            [now()]).map_err(|e| e.to_string())?;
        // An interrupted source-author operation has no completed identity.
        transaction.execute("UPDATE discovery_facebook_identity_reads SET status='failed',stop_code='interrupted_facebook_identity_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
            WHERE r.discovery_campaign_id=discovery_facebook_identity_reads.discovery_campaign_id AND r.platform='facebook'
              AND r.profile_ref=discovery_facebook_identity_reads.profile_ref AND r.status='searching'
              AND r.active_query='facebook_identity:' || discovery_facebook_identity_reads.search_hit_id || ':' || discovery_facebook_identity_reads.call_number)", [now()]).map_err(|e|e.to_string())?;
        let facebook_interrupted=transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND platform='facebook' AND EXISTS(SELECT 1 FROM discovery_facebook_identity_reads p
            WHERE p.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND p.profile_ref=discovery_platform_runs.profile_ref
              AND p.status='failed' AND p.stop_code='interrupted_facebook_identity_read'
              AND active_query='facebook_identity:' || p.search_hit_id || ':' || p.call_number)", [now()]).map_err(|e|e.to_string())?;
        // Preserve its paid attempt and advance to another saved video.
        transaction.execute("UPDATE discovery_douyin_author_reads SET status='failed',stop_code='interrupted_source_author_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
            WHERE r.discovery_campaign_id=discovery_douyin_author_reads.discovery_campaign_id AND r.platform='douyin'
              AND r.profile_ref=discovery_douyin_author_reads.profile_ref AND r.status='searching'
              AND r.active_query='source_author:' || discovery_douyin_author_reads.search_hit_id || ':' || discovery_douyin_author_reads.call_number)", [now()]).map_err(|e|e.to_string())?;
        let authors_interrupted=transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND platform='douyin' AND EXISTS(SELECT 1 FROM discovery_douyin_author_reads p
            WHERE p.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND p.profile_ref=discovery_platform_runs.profile_ref
              AND p.status='failed' AND p.stop_code='interrupted_source_author_read'
              AND active_query='source_author:' || p.search_hit_id || ':' || p.call_number)", [now()]).map_err(|e|e.to_string())?;
        transaction.execute("UPDATE discovery_web_source_reads SET status='failed',stop_code='interrupted_web_source_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_platform_runs r
            WHERE r.discovery_campaign_id=discovery_web_source_reads.discovery_campaign_id AND r.platform=discovery_web_source_reads.origin_platform
              AND r.profile_ref=discovery_web_source_reads.profile_ref AND r.status='searching'
              AND r.active_query='web_source:' || discovery_web_source_reads.search_hit_id || ':' || discovery_web_source_reads.call_number)",[now()]).map_err(|e|e.to_string())?;
        let web_interrupted=transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND EXISTS(SELECT 1 FROM discovery_web_source_reads p
            WHERE p.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND p.origin_platform=discovery_platform_runs.platform
              AND p.profile_ref=discovery_platform_runs.profile_ref AND p.status='failed' AND p.stop_code='interrupted_web_source_read'
              AND active_query='web_source:' || p.search_hit_id || ':' || p.call_number)",[now()]).map_err(|e|e.to_string())?;
        // A native original request is paid once. After restart, preserve the
        // failed source and continue the next already saved original.
        transaction.execute("UPDATE discovery_content_reads SET status='failed',native_content_json=NULL,stop_code='interrupted_content_read',updated_at=?1
          WHERE status='reading' AND platform='facebook' AND EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f
            JOIN discovery_platform_runs r ON r.discovery_campaign_id=f.discovery_campaign_id AND r.platform='facebook' AND r.profile_ref=f.profile_ref
            WHERE f.search_hit_id=discovery_content_reads.search_hit_id AND f.call_number=discovery_content_reads.call_number
              AND f.source_url=discovery_content_reads.source_url AND f.status='reading' AND r.status='searching'
              AND r.active_query='content:' || f.search_hit_id || ':' || f.call_number)",[now()]).map_err(|e|e.to_string())?;
        transaction.execute("UPDATE discovery_facebook_original_attempts SET status='failed',stop_code='interrupted_content_read',updated_at=?1
          WHERE status='reading' AND EXISTS(SELECT 1 FROM discovery_content_reads c
            WHERE c.search_hit_id=discovery_facebook_original_attempts.search_hit_id AND c.call_number=discovery_facebook_original_attempts.call_number
              AND c.source_url=discovery_facebook_original_attempts.source_url AND c.status='failed' AND c.stop_code='interrupted_content_read')",[now()]).map_err(|e|e.to_string())?;
        let originals_interrupted=transaction.execute("UPDATE discovery_platform_runs SET status='completed',stop_code=NULL,active_query=NULL,updated_at=?1
          WHERE status='searching' AND platform='facebook' AND EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f
            WHERE f.discovery_campaign_id=discovery_platform_runs.discovery_campaign_id AND f.profile_ref=discovery_platform_runs.profile_ref
              AND f.status='failed' AND f.stop_code='interrupted_content_read'
              AND active_query='content:' || f.search_hit_id || ':' || f.call_number)",[now()]).map_err(|e|e.to_string())?;
        let affected = transaction
            .execute(
                "UPDATE discovery_platform_runs SET status='technical_stop',
                 stop_code=CASE WHEN active_query LIKE 'video:%' THEN 'interrupted_video_read'
                    WHEN active_query LIKE 'transcript:%' THEN 'interrupted_transcript_read'
                    WHEN active_query LIKE 'profile:%' THEN 'interrupted_profile_read'
                    WHEN active_query LIKE 'content:%' THEN 'interrupted_content_read'
                    WHEN active_query LIKE 'identity:%' THEN 'interrupted_identity_read'
                    WHEN active_query LIKE 'page:%' THEN 'interrupted_public_page_read'
                   ELSE 'interrupted_read' END,updated_at=?1 WHERE status='searching'",
                [now()],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|e| e.to_string())?;
        u32::try_from(
            affected
                + contacts_interrupted
                + pages_interrupted
                + authors_interrupted
                + web_interrupted
                + facebook_interrupted
                + originals_interrupted,
        )
        .map_err(|error| error.to_string())
    }

    fn create_discovery_campaign(
        &self,
        input: CreateDiscoveryCampaignInput,
    ) -> Result<DiscoveryCampaign, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO discovery_campaigns
                 (id,phase,current_revision,target_count,max_calls,used_calls,created_at,updated_at)
                 VALUES(?1,'draft',1,?2,?3,0,?4,?4)",
                params![input.id, input.target_count, input.max_calls, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO discovery_campaign_revisions
                 (discovery_campaign_id,revision,brief_json,opencli_version,criteria_json,scope_json,approved_at)
                 VALUES(?1,1,?2,?3,?4,?5,NULL)",
                params![
                    input.id,
                    serde_json::to_string(&input.brief).map_err(|error| error.to_string())?,
                    input.opencli_version,
                    input.criteria.to_string(),
                    serde_json::to_string(&input.scopes).map_err(|error| error.to_string())?,
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO discovery_recommendation_policies VALUES(?1,1,'score_and_contact_v1')",
                [&input.id],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(&input.id)
    }

    fn approve_discovery_campaign(
        &self,
        id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryCampaign, String> {
        if expected_revision == 0 {
            return Err("Campaign revision is required".into());
        }
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        let affected = transaction
            .execute(
                "UPDATE discovery_campaigns SET phase='active', updated_at=?3
                 WHERE id=?1 AND phase='draft' AND current_revision=?2",
                params![id, expected_revision, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if affected != 1 {
            return Err("Campaign is missing, already approved, or revision changed".into());
        }
        let scope_json = transaction
            .query_row(
                "SELECT scope_json FROM discovery_campaign_revisions
                 WHERE discovery_campaign_id=?1 AND revision=?2 AND approved_at IS NULL",
                params![id, expected_revision],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?;
        let scopes: Vec<DiscoveryPlatformScope> =
            serde_json::from_str(&scope_json).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE discovery_campaign_revisions SET approved_at=?3
                 WHERE discovery_campaign_id=?1 AND revision=?2 AND approved_at IS NULL",
                params![id, expected_revision, timestamp],
            )
            .map_err(|error| error.to_string())?;
        for scope in scopes {
            transaction
                .execute(
                    "INSERT INTO discovery_platform_runs
                     (discovery_campaign_id,platform,profile_ref,status,cursor,calls_used,stop_code,updated_at)
                     VALUES(?1,?2,?3,'unsearched',NULL,0,NULL,?4)",
                    params![id, scope.platform, scope.profile_ref, timestamp],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(id)
    }

    fn increase_discovery_budget(
        &self,
        account: &str,
        input: IncreaseDiscoveryBudgetInput,
    ) -> Result<DiscoveryCampaign, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let prior: Option<(String, String, u32, u32, u32)> = transaction
            .query_row(
                "SELECT operator_account_id,discovery_campaign_id,criteria_revision,
                        previous_max_calls,new_max_calls
                 FROM discovery_budget_increases WHERE id=?1",
                [&input.decision_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some((old_account, old_campaign, old_revision, old_max, new_max)) = prior {
            if old_account != account
                || old_campaign != input.campaign_id
                || old_revision != input.expected_revision
                || old_max != input.expected_max_calls
                || new_max != input.new_max_calls
            {
                return Err("budget_increase_idempotency_conflict".into());
            }
            transaction.commit().map_err(|error| error.to_string())?;
            drop(connection);
            return self.get_discovery_campaign(&input.campaign_id);
        }
        let (phase, revision, old_max, used_calls, active_reads): (
            String,
            u32,
            Option<u32>,
            u32,
            u32,
        ) = transaction
            .query_row(
                "SELECT c.phase,c.current_revision,c.max_calls,c.used_calls,
                    (SELECT COUNT(*) FROM discovery_platform_runs r
                     WHERE r.discovery_campaign_id=c.id AND r.status='searching')
                 FROM discovery_campaigns c WHERE c.id=?1",
                [&input.campaign_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(|_| "budget_increase_campaign_unavailable".to_owned())?;
        let old_max = old_max.ok_or("budget_increase_requires_finite_approval")?;
        if phase != "active"
            || revision != input.expected_revision
            || old_max != input.expected_max_calls
            || used_calls > old_max
            || active_reads != 0
        {
            return Err("budget_increase_campaign_changed".into());
        }
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO discovery_budget_increases
             (id,operator_account_id,discovery_campaign_id,criteria_revision,
              previous_max_calls,new_max_calls,used_calls_at_decision,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    input.decision_id,
                    account,
                    input.campaign_id,
                    revision,
                    old_max,
                    input.new_max_calls,
                    used_calls,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE discovery_campaigns SET max_calls=?2,updated_at=?3 WHERE id=?1",
                params![input.campaign_id, input.new_max_calls, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(&input.campaign_id)
    }

    fn increase_discovery_target(
        &self,
        account: &str,
        input: IncreaseDiscoveryTargetInput,
    ) -> Result<DiscoveryCampaign, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let prior: Option<(String, String, u32, u32, u32)> = transaction
            .query_row(
                "SELECT operator_account_id,discovery_campaign_id,criteria_revision,
                        previous_target_count,new_target_count
                 FROM discovery_target_increases WHERE id=?1",
                [&input.decision_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some((old_account, old_campaign, old_revision, old_max, new_max)) = prior {
            if old_account != account
                || old_campaign != input.campaign_id
                || old_revision != input.expected_revision
                || old_max != input.expected_target_count
                || new_max != input.new_target_count
            {
                return Err("target_increase_idempotency_conflict".into());
            }
            transaction.commit().map_err(|error| error.to_string())?;
            drop(connection);
            return self.get_discovery_campaign(&input.campaign_id);
        }
        let (phase, revision, old_max, used_calls, active_reads): (String, u32, u32, u32, u32) =
            transaction
                .query_row(
                    "SELECT c.phase,c.current_revision,c.target_count,c.used_calls,
                    (SELECT COUNT(*) FROM discovery_platform_runs r
                     WHERE r.discovery_campaign_id=c.id AND r.status='searching')
                 FROM discovery_campaigns c WHERE c.id=?1",
                    [&input.campaign_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .map_err(|_| "target_increase_campaign_unavailable".to_owned())?;
        if phase != "active"
            || revision != input.expected_revision
            || old_max != input.expected_target_count
            || active_reads != 0
        {
            return Err("target_increase_campaign_changed".into());
        }
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO discovery_target_increases
             (id,operator_account_id,discovery_campaign_id,criteria_revision,
              previous_target_count,new_target_count,used_calls_at_decision,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    input.decision_id,
                    account,
                    input.campaign_id,
                    revision,
                    old_max,
                    input.new_target_count,
                    used_calls,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE discovery_campaigns SET target_count=?2,updated_at=?3 WHERE id=?1",
                params![input.campaign_id, input.new_target_count, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        drop(connection);
        self.get_discovery_campaign(&input.campaign_id)
    }

    fn get_discovery_campaign(&self, id: &str) -> Result<DiscoveryCampaign, String> {
        let mut connection = self.read_connection()?;
        let snapshot = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let record = snapshot
            .query_row(
                "SELECT c.id,c.phase,c.current_revision,r.brief_json,r.criteria_json,
                        r.scope_json,c.target_count,c.max_calls,c.used_calls,r.approved_at,
                        c.created_at,c.updated_at,r.opencli_version
                 FROM discovery_campaigns c JOIN discovery_campaign_revisions r
                   ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision
                 WHERE c.id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, Option<u32>>(7)?,
                        row.get::<_, u32>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, i64>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, String>(12)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Campaign not found".to_string())?;
        let (
            id,
            phase,
            revision,
            brief_json,
            criteria_json,
            scope_json,
            target_count,
            max_calls,
            used_calls,
            approved_at,
            created_at,
            updated_at,
            opencli_version,
        ) = record;
        let mut statement = snapshot
            .prepare(
                "SELECT platform,profile_ref,status,cursor,calls_used,stop_code,updated_at
                 FROM discovery_platform_runs WHERE discovery_campaign_id=?1
                 ORDER BY platform,profile_ref",
            )
            .map_err(|error| error.to_string())?;
        let platform_runs = statement
            .query_map([&id], |row| {
                Ok(DiscoveryPlatformRun {
                    platform: row.get(0)?,
                    profile_ref: row.get(1)?,
                    status: row.get(2)?,
                    cursor: row.get(3)?,
                    calls_used: row.get(4)?,
                    stop_code: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        let scopes =
            super::discovery_query_rounds::effective_scopes(&snapshot, &id, revision, &scope_json)?;
        snapshot.commit().map_err(|error| error.to_string())?;
        Ok(DiscoveryCampaign {
            id,
            phase,
            revision,
            brief: serde_json::from_str(&brief_json).map_err(|error| error.to_string())?,
            opencli_version,
            criteria: serde_json::from_str(&criteria_json).map_err(|error| error.to_string())?,
            scopes,
            platform_runs,
            target_count,
            max_calls,
            used_calls,
            approved_at,
            created_at,
            updated_at,
        })
    }

    fn upsert_discovery_candidate(
        &self,
        input: UpsertDiscoveryCandidateInput,
    ) -> Result<DiscoveryCandidate, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let criteria_json = transaction
            .query_row(
                "SELECT revision.criteria_json FROM discovery_campaigns c
                 JOIN discovery_campaign_revisions revision ON revision.discovery_campaign_id=c.id
                    AND revision.revision=c.current_revision
                 JOIN discovery_platform_runs run ON run.discovery_campaign_id=c.id
                 WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?2
                   AND run.platform=?3 AND run.profile_ref=?4",
                params![
                    input.campaign_id,
                    input.expected_revision,
                    input.platform,
                    input.source_profile_ref
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "Candidate is outside the approved Campaign scope or revision".to_string()
            })?;
        let criteria = serde_json::from_str(&criteria_json).map_err(|error| error.to_string())?;
        let previous: Option<(String, String)> = transaction
            .query_row(
                "SELECT id,evidence_json FROM discovery_candidates WHERE discovery_campaign_id=?1
             AND platform=?2 AND platform_account_id=?3 AND verdict='pending'",
                params![input.campaign_id, input.platform, input.platform_account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let merged = if let Some((candidate_id, previous)) = previous {
            merge_with_audited_review_proofs(
                &transaction,
                input.evidence.clone(),
                &previous,
                &candidate_id,
                &input.campaign_id,
                input.expected_revision,
                &input.platform_account_id,
            )?
        } else {
            input.evidence.clone()
        };
        let assessment = assess(&criteria, &merged, &input.platform_account_id);
        let evidence = assessed_evidence(merged, &assessment);
        if evidence.to_string().len() > 16_384 {
            return Err("Assessed discovery evidence exceeds the size limit".into());
        }
        let timestamp = now();
        let affected = transaction
            .execute(
                "INSERT INTO discovery_candidates
                 (id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,
                  platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?4)
                 ON CONFLICT(discovery_campaign_id,platform,platform_account_id)
                 DO UPDATE SET source_platform=excluded.source_platform,source_profile_ref=excluded.source_profile_ref,
                   display_name=excluded.display_name,profile_url=excluded.profile_url,
                   verdict=excluded.verdict,evidence_json=excluded.evidence_json,updated_at=excluded.updated_at
                 WHERE discovery_candidates.verdict='pending'
                   AND discovery_candidates.criteria_revision=excluded.criteria_revision",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    input.campaign_id,
                    input.expected_revision,
                    input.platform,
                    input.source_profile_ref,
                    input.platform_account_id,
                    input.display_name,
                    input.profile_url,
                    assessment.verdict,
                    evidence.to_string(),
                    timestamp,
                ],
            )
            .map_err(|error| error.to_string())?;
        if affected != 1 {
            return Err("Candidate is already decided or needs criteria reassessment".into());
        }
        let raw = transaction
            .query_row(
                "SELECT id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,
                        platform_account_id,display_name,profile_url,verdict,evidence_json,
                        created_at,updated_at
                 FROM discovery_candidates
                 WHERE discovery_campaign_id=?1 AND platform=?2 AND platform_account_id=?3",
                params![input.campaign_id, input.platform, input.platform_account_id],
                read_candidate_row,
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        candidate_from_raw(raw)
    }

    fn approve_discovery_candidate(
        &self,
        input: crate::ApproveDiscoveryCandidateInput,
    ) -> Result<crate::DiscoveryManualApproval, String> {
        super::discovery_manual_approval::approve(self, input)
    }

    fn discovery_recommendation_revision(&self, campaign: &str) -> Result<String, String> {
        // Scheduling hint only: publication still uses the audited projection.
        // No full table snapshot and no evidence text needs to leave the repository.
        let connection = self.read_connection()?;
        let mut query = connection.prepare(
            "SELECT c.id,json_array(c.display_name,c.profile_url,c.platform,c.evidence_json),o.kind,o.normalized_value,o.source_url
             FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
             JOIN discovery_contact_observations o ON o.candidate_id=c.id
               AND o.criteria_revision=c.criteria_revision
             WHERE d.id=?1 AND c.criteria_revision=d.current_revision AND c.verdict='qualified'
             ORDER BY c.id,o.kind,o.normalized_value,o.source_url"
        ).map_err(|e| e.to_string())?;
        let rows = query
            .query_map(params![campaign], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        serde_json::to_string(&rows).map_err(|e| e.to_string())
    }

    fn discovery_candidate_is_qualified(
        &self,
        campaign: &str,
        candidate: &str,
    ) -> Result<bool, String> {
        self.read_connection()?
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_campaigns d
             ON d.id=c.discovery_campaign_id WHERE c.id=?1 AND d.id=?2
             AND c.criteria_revision=d.current_revision AND c.verdict='qualified')",
                params![candidate, campaign],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())
    }

    fn list_discovery_candidates(
        &self,
        input: ListDiscoveryCandidatesInput,
    ) -> Result<Vec<DiscoveryCandidate>, String> {
        if input.campaign_id.is_empty() || !(1..=100).contains(&input.limit) {
            return Err("Candidate page requires a Campaign and limit 1-100".into());
        }
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare(
                "WITH ranked AS (SELECT *, CASE
                    WHEN verdict='pending' THEN 4
                    ELSE CASE json_extract(evidence_json,'$.assessment.match_rating.level')
                      WHEN 'excellent' THEN 0 WHEN 'good' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END
                    END AS match_rank FROM discovery_candidates WHERE discovery_campaign_id=?1)
                 SELECT id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,
                        platform_account_id,display_name,profile_url,verdict,
                        CASE WHEN EXISTS (SELECT 1 FROM discovery_manual_approvals m
                          WHERE m.discovery_campaign_id=ranked.discovery_campaign_id
                            AND m.criteria_revision=ranked.criteria_revision AND m.candidate_id=ranked.id)
                        THEN json_set(evidence_json,'$.manual_approval',json_object('approved_at',
                          (SELECT approved_at FROM discovery_manual_approvals m
                           WHERE m.discovery_campaign_id=ranked.discovery_campaign_id
                             AND m.criteria_revision=ranked.criteria_revision AND m.candidate_id=ranked.id)))
                        ELSE json_remove(evidence_json,'$.manual_approval') END,
                        created_at,updated_at
                 FROM ranked
                 WHERE ?2='' OR (match_rank,id)>(SELECT match_rank,id FROM ranked WHERE id=?2)
                 ORDER BY match_rank,id LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![
                    input.campaign_id,
                    input.after_id.unwrap_or_default(),
                    input.limit
                ],
                read_candidate_row,
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows.into_iter().map(|raw| {
            let mut candidate = candidate_from_raw(raw)?;
            // Display-only owner projection. Never write these derived fields
            // into saved evidence or use them as reviewer-provided proof.
            if let Some(evidence) = candidate.evidence.as_object_mut() {
                evidence.remove("contact_status");
                evidence.remove("associated_social_accounts");
                evidence.remove("manual_profile_contact");
                evidence.remove("manual_wechat_accounts");
            }
            if matches!(candidate.platform.as_str(), "linkedin" | "reddit") {
                let current: bool = connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM discovery_campaigns WHERE id=?1 AND current_revision=?2)",
                    params![candidate.campaign_id, candidate.criteria_revision], |row| row.get(0),
                ).map_err(|e| e.to_string())?;
                if current && super::discovery_contacts::eligible_author(&connection,
                    &candidate.campaign_id, candidate.criteria_revision, &candidate.id)?.as_deref()
                    == Some(candidate.platform_account_id.as_str()) {
                    candidate.evidence["manual_profile_contact"] = serde_json::json!(true);
                }
            }
            let wechat = super::discovery_contacts::manual_wechat(&connection, &candidate)?;
            if !wechat.is_empty() { candidate.evidence["manual_wechat_accounts"] = serde_json::json!(wechat); }
            let accounts = super::discovery_contacts::social_accounts(&connection, &candidate.campaign_id,
                candidate.criteria_revision, &candidate.id, &candidate.platform_account_id)?;
            if !accounts.is_empty() {
                candidate.evidence["associated_social_accounts"] = serde_json::to_value(accounts)
                    .map_err(|e| e.to_string())?;
            }
            let hidden = super::discovery_contacts::has_hidden_email(&connection,
                &candidate.campaign_id, candidate.criteria_revision, &candidate.id,
                &candidate.platform_account_id)?;
            if hidden && super::discovery_contacts::eligible_author(&connection,
                &candidate.campaign_id, candidate.criteria_revision, &candidate.id)?.is_some()
                && super::discovery_contacts::list(&connection, &candidate.campaign_id,
                    candidate.criteria_revision, &candidate.id, &candidate.platform_account_id)?.is_empty() {
                candidate.evidence["contact_status"] = serde_json::json!({
                    "has_contact": false, "manual_email_required": true
                });
            }
            Ok(candidate)
        }).collect()
    }
}

type RawCandidate = (
    String,
    String,
    u32,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
);

fn read_candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawCandidate> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
        row.get(11)?,
    ))
}

fn candidate_from_raw(raw: RawCandidate) -> Result<DiscoveryCandidate, String> {
    Ok(DiscoveryCandidate {
        id: raw.0,
        campaign_id: raw.1,
        criteria_revision: raw.2,
        platform: raw.3,
        source_profile_ref: raw.4,
        platform_account_id: raw.5,
        display_name: raw.6,
        profile_url: raw.7,
        verdict: raw.8,
        evidence: serde_json::from_str(&raw.9).map_err(|error| error.to_string())?,
        created_at: raw.10,
        updated_at: raw.11,
    })
}
