use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{
    DiscoveryLibraryBasis, DiscoverySearchHit,
    discovery_library::{
        LibraryCandidateSource, LibrarySourceRow, MAX_LIBRARY_HITS, MAX_LIBRARY_SOURCE_BYTES,
    },
};

// No name/URL matching. Every author comes from the exact hit's completed,
// current-revision native receipt or the permanent video completion binding.
const SOURCE_ROWS: &str = r#"
WITH bindings AS (
 SELECT h.id AS hit_id,b.native_author_id,b.candidate_id,
   json_object('video_hit_id',b.search_hit_id,'native_author_id',b.native_author_id,
     'candidate_id',b.candidate_id,'criteria_revision',b.criteria_revision) AS identity_receipt
 FROM discovery_search_hits h
 JOIN discovery_video_completions done ON done.search_hit_id=h.id
 JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id
 WHERE h.discovery_campaign_id=?1 AND h.platform='youtube'
   AND b.discovery_campaign_id=h.discovery_campaign_id AND b.criteria_revision=?2
 UNION ALL
 SELECT h.id,b.native_author_id,b.candidate_id,
   json_object('web_search_hit_id',h.id,'origin_platform',h.platform,'target_platform',web.target_platform,
     'video_id',web.video_id,'native_author_id',b.native_author_id,'candidate_id',b.candidate_id,
     'criteria_revision',b.criteria_revision,'call_number',web.call_number) AS identity_receipt
 FROM discovery_search_hits h
 JOIN discovery_web_source_reads web ON web.search_hit_id=h.id AND web.discovery_campaign_id=h.discovery_campaign_id
   AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
   AND web.expected_revision=?2 AND web.target_platform IN ('youtube','twitter','douyin') AND web.status='completed' AND web.stop_code IS NULL
   AND json_extract(web.native_material_json,CASE web.target_platform WHEN 'twitter' THEN '$.identity.account_id' WHEN 'douyin' THEN '$.content.author_id' ELSE '$.channelId' END)=web.native_author_id
   AND json_extract(web.native_material_json,CASE web.target_platform WHEN 'twitter' THEN '$.identity.post_id' WHEN 'douyin' THEN '$.content.post_id' ELSE '$.videoId' END)=web.video_id
 JOIN discovery_video_completions done ON done.search_hit_id=h.id
 JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=h.discovery_campaign_id
   AND b.criteria_revision=?2 AND b.native_author_id=web.native_author_id
 WHERE h.discovery_campaign_id=?1 AND h.platform IN ('google','yahoo')
 UNION ALL
 SELECT h.id,
   CASE p.platform WHEN 'twitter' THEN m.account_id WHEN 'linkedin' THEN json_extract(p.native_profile_json,'$.account_id') ELSE p.author_id END,NULL,
   p.native_profile_json
 FROM discovery_search_hits h
 JOIN discovery_search_authors a ON a.search_hit_id=h.id
 JOIN discovery_profile_reads p ON p.search_hit_id=h.id
 LEFT JOIN discovery_profile_identity_matches m ON m.search_hit_id=h.id
 LEFT JOIN discovery_identity_reads i ON i.search_hit_id=m.identity_search_hit_id
 WHERE h.discovery_campaign_id=?1 AND p.discovery_campaign_id=h.discovery_campaign_id
   AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.expected_revision=?2
   AND p.status='completed' AND p.author_id=a.author_id
   AND json_extract(p.native_profile_json,CASE WHEN p.platform='zhihu' THEN '$.author_url' ELSE '$.profile_url' END)=a.author_url
   AND (
    (p.platform='bilibili' AND json_extract(p.native_profile_json,'$.mid')=a.author_id)
    OR (p.platform IN ('tiktok','reddit','facebook','instagram','weibo') AND json_extract(p.native_profile_json,'$.id')=a.author_id)
    OR (p.platform IN ('zhihu','hackernews','pinterest','pixiv') AND json_extract(p.native_profile_json,'$.author_id')=a.author_id)
    OR (p.platform='douyin' AND json_extract(p.native_profile_json,'$.author_id')=a.author_id
      AND EXISTS(SELECT 1 FROM discovery_douyin_author_reads joint WHERE joint.search_hit_id=h.id
        AND joint.discovery_campaign_id=h.discovery_campaign_id AND joint.expected_revision=?2
        AND joint.platform=h.platform AND joint.profile_ref=h.profile_ref AND joint.source_url=h.source_url
        AND joint.status='completed' AND joint.stop_code IS NULL AND joint.duplicate_of IS NULL
        AND joint.author_id=a.author_id AND joint.profile_url=a.author_url AND joint.call_number=p.call_number))
    OR (p.platform='bluesky' AND json_extract(p.native_profile_json,'$.did')=a.author_id)
    OR (p.platform='linkedin' AND json_extract(p.native_profile_json,'$.author_handle')=a.author_id
      AND json_extract(p.native_profile_json,'$.evidence_source')='visible_profile_header'
      AND h.source_url=a.author_url AND json_extract(p.native_profile_json,'$.account_id') IS NOT NULL)
    OR (p.platform='twitter' AND json_extract(p.native_profile_json,'$.handle')=a.author_id
      AND json_extract(p.native_profile_json,'$.identity_kind')='native_id'
      AND json_extract(p.native_profile_json,'$.account_id')=m.account_id
      AND i.discovery_campaign_id=h.discovery_campaign_id AND i.platform=h.platform
      AND i.profile_ref=h.profile_ref AND i.expected_revision=?2 AND i.status='completed'
      AND i.author_handle=a.author_id AND i.source_url=h.source_url
      AND i.account_id=m.account_id AND i.call_number<p.call_number)
   )
)
SELECT h.id,h.platform,h.profile_ref,h.search_keyword,h.source_url,h.title,
 CASE WHEN h.platform IN ('google','yahoo') AND c.id IS NOT NULL THEN c.display_name ELSE h.author_label END,
 h.captured_at,COALESCE(a.author_id,CASE WHEN h.platform IN ('google','yahoo') THEN c.platform_account_id END),
 COALESCE(a.author_url,CASE WHEN h.platform IN ('google','yahoo') THEN c.profile_url END),
 COALESCE(a.observed_at,CASE WHEN h.platform IN ('google','yahoo') THEN c.created_at END),
 c.id,c.platform_account_id,c.verdict,c.evidence_json,b.identity_receipt,manual.approved_at,c.platform
FROM discovery_search_hits h
LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id
LEFT JOIN bindings b ON b.hit_id=h.id
LEFT JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
 AND ((h.platform IN ('google','yahoo') AND c.platform IN ('youtube','twitter','douyin') AND c.id=b.candidate_id)
   OR (c.platform=h.platform AND c.source_platform=h.platform AND c.source_profile_ref=h.profile_ref))
 AND c.criteria_revision=?2 AND c.platform_account_id=b.native_author_id
 AND (b.candidate_id IS NULL OR c.id=b.candidate_id)
LEFT JOIN discovery_manual_approvals manual ON manual.discovery_campaign_id=c.discovery_campaign_id
 AND manual.criteria_revision=c.criteria_revision AND manual.candidate_id=c.id
WHERE h.discovery_campaign_id=?1
ORDER BY h.id LIMIT 10001
"#;

#[cfg(test)]
#[test]
#[ignore = "explicit read-only local receipt acceptance; no provider requests"]
fn saved_contact_receipts_project_to_recommendations() {
    let db = Connection::open_with_flags(
        std::env::var("ONECREW_ACCEPTANCE_DB").expect("read-only database path"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let campaign = std::env::var("ONECREW_ACCEPTANCE_CAMPAIGN").unwrap();
    let basis = load_basis(&db, &campaign, 1).unwrap();
    let snapshot = crate::discovery_library::build_snapshot(&basis).unwrap();
    for hit in std::env::var("ONECREW_ACCEPTANCE_HITS").unwrap().split(',') {
        let row = snapshot
            .rows
            .iter()
            .find(|r| r.hit.id == hit)
            .expect("saved source");
        assert_eq!(row.verdict, "qualified", "source {hit}: {:?}", row.reasons);
        assert!(!row.contacts.is_empty(), "source {hit}: missing contact");
        assert!(
            row.contacts
                .iter()
                .all(|c| Some(&c.native_author_id) == row.native_author_id.as_ref())
        );
    }
    if let Ok(hits) = std::env::var("ONECREW_ACCEPTANCE_REJECTED_HITS") {
        for hit in hits.split(',') {
            let row = snapshot.rows.iter().find(|r| r.hit.id == hit).unwrap();
            assert_eq!(
                row.verdict, "rejected",
                "contact routes must not bypass scoring"
            );
        }
    }
}

pub(super) fn load_basis(
    connection: &Connection,
    campaign: &str,
    revision: u32,
) -> Result<DiscoveryLibraryBasis, String> {
    if campaign.is_empty() || campaign.len() > 128 || revision == 0 || revision > i32::MAX as u32 {
        return Err("library_source_input_invalid".into());
    }
    let (criteria_json, scopes_json, approved_at): (String,String,i64) = connection.query_row(
        "SELECT r.criteria_json,r.scope_json,r.approved_at FROM discovery_campaigns c
         JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision
         WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND r.approved_at IS NOT NULL",
        params![campaign,revision], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).map_err(|_| "library_source_campaign_not_approved")?;
    let scopes_json = serde_json::to_string(
        &super::discovery_query_rounds::effective_scopes(connection, campaign, revision, &scopes_json)?
    ).map_err(|e| e.to_string())?;
    let mut statement = connection.prepare(SOURCE_ROWS).map_err(|e| e.to_string())?;
    let mut contact_cache: std::collections::HashMap<
        (String, String),
        Vec<crate::DiscoveryContact>,
    > = std::collections::HashMap::new();
    let records = statement
        .query_map(params![campaign, revision], |r| {
            let candidate_id: Option<String> = r.get(11)?;
            Ok(LibrarySourceRow {
                hit: DiscoverySearchHit {
                    id: r.get(0)?,
                    campaign_id: campaign.into(),
                    platform: r.get(1)?,
                    profile_ref: r.get(2)?,
                    search_keyword: r.get(3)?,
                    source_url: r.get(4)?,
                    title: r.get(5)?,
                    author_label: r.get(6)?,
                    captured_at: r.get(7)?,
                    author_id: r.get(8)?,
                    author_url: r.get(9)?,
                    author_observed_at: r.get(10)?,
                },
                candidate: match candidate_id {
                    Some(id) => Some(LibraryCandidateSource {
                        followers: super::discovery_review::load_contact_basis(
                            connection,
                            campaign,
                            revision,
                            &r.get::<_, String>(0)?,
                        )
                        .and_then(crate::discovery_review::build_material)
                        .ok()
                        .filter(|material| {
                            material.candidate_id == id
                                && material.native_author_id
                                    == r.get::<_, String>(12).unwrap_or_default()
                        })
                        .and_then(|material| {
                            material
                                .native_followers
                                .map(|n| n.to_string())
                                .or_else(|| {
                                    material
                                        .native_follower_estimate
                                        .map(|estimate| format!("~{}", estimate.display))
                                })
                        }).or_else(|| {
                            let platform: String = r.get(17).ok()?;
                            if platform != "youtube" { return None; }
                            let evidence: serde_json::Value = serde_json::from_str(&r.get::<_, String>(14).ok()?).ok()?;
                            let source: String = r.get(4).ok()?;
                            if evidence["subscribers_source_url"].as_str() != Some(source.as_str()) { return None; }
                            crate::discovery_qualification::subscriber_display(evidence["subscribers_raw"].as_str()?)
                        }),
                        native_platform: {
                            let native: String = r.get(17)?;
                            (native != r.get::<_, String>(1)?).then_some(native)
                        },
                        contacts: {
                            let author: String = r.get(12)?;
                            let key = (id.clone(), author.clone());
                            if let Some(routes) = contact_cache.get(&key) {
                                routes.clone()
                            } else {
                                let routes = super::discovery_contacts::recommendation_routes(
                                    connection,
                                    campaign,
                                    revision,
                                    &id,
                                    &author,
                                    &r.get::<_, String>(0)?,
                                )
                                .map_err(|e| {
                                    rusqlite::Error::ToSqlConversionFailure(Box::new(
                                        std::io::Error::other(e),
                                    ))
                                })?;
                                contact_cache.insert(key, routes.clone());
                                routes
                            }
                        },
                        id,
                        native_author_id: r.get(12)?,
                        verdict: r.get(13)?,
                        evidence_json: r.get(14)?,
                        identity_receipt: r.get(15)?,
                        manual_approved_at: r.get(16)?,
                    }),
                    None => None,
                },
            })
        })
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut bytes = criteria_json.len() + scopes_json.len();
    for record in records {
        let record = record.map_err(|e| e.to_string())?;
        let hit = &record.hit;
        bytes += hit.id.len()
            + hit.campaign_id.len()
            + hit.platform.len()
            + hit.profile_ref.len()
            + hit.source_url.len()
            + hit.title.len()
            + hit.author_label.len()
            + hit.search_keyword.len()
            + hit.author_id.as_ref().map_or(0, String::len)
            + hit.author_url.as_ref().map_or(0, String::len);
        if let Some(candidate) = &record.candidate {
            bytes += candidate.id.len()
                + candidate.native_platform.as_ref().map_or(0, String::len)
                + candidate.native_author_id.len()
                + candidate.verdict.len()
                + candidate.evidence_json.len()
                + candidate.identity_receipt.len();
            if !candidate.contacts.is_empty() {
                bytes += serde_json::to_vec(&candidate.contacts)
                    .map_err(|e| e.to_string())?
                    .len();
            }
        }
        if rows.len() == MAX_LIBRARY_HITS || bytes > MAX_LIBRARY_SOURCE_BYTES {
            return Err("library_source_snapshot_too_large".into());
        }
        rows.push(record);
    }
    Ok(DiscoveryLibraryBasis {
        contact_required: connection.query_row("SELECT policy='score_and_contact_v1' FROM discovery_recommendation_policies WHERE discovery_campaign_id=?1 AND criteria_revision=?2", params![campaign,revision], |r| r.get(0)).map_err(|e| e.to_string())?,
        campaign_id: campaign.into(),
        revision,
        criteria_json,
        scopes_json,
        approved_at,
        rows,
    })
}

pub(super) fn read_basis(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
) -> Result<DiscoveryLibraryBasis, String> {
    let mut connection = repository.read_connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let basis = load_basis(&transaction, campaign, revision)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(basis)
}

pub(super) fn reserve_version(
    repository: &SqliteRepository,
    basis: &DiscoveryLibraryBasis,
    hash: &str,
) -> Result<u64, String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("library_source_hash_invalid".into());
    }
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if load_basis(&transaction, &basis.campaign_id, basis.revision)? != *basis {
        return Err("library_source_changed".into());
    }
    let version = reserve_source_version(&transaction, basis, hash)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(version)
}

fn reserve_source_version(
    transaction: &Connection,
    basis: &DiscoveryLibraryBasis,
    hash: &str,
) -> Result<u64, String> {
    let prior: Option<(u64,String,u32)> = transaction.query_row(
        "SELECT source_revision,snapshot_hash,criteria_revision FROM discovery_library_source_versions WHERE discovery_campaign_id=?1",
        [&basis.campaign_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(|e| e.to_string())?;
    if let Some((version, old_hash, revision)) = &prior
        && old_hash == hash
        && *revision == basis.revision
    {
        return Ok(*version);
    }
    let version = prior.map_or(1, |(version, _, _)| version + 1);
    if version > 9_007_199_254_740_991 {
        return Err("library_source_version_exhausted".into());
    }
    transaction.execute(
        "INSERT INTO discovery_library_source_versions VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(discovery_campaign_id) DO UPDATE SET criteria_revision=excluded.criteria_revision,
         source_revision=excluded.source_revision,snapshot_hash=excluded.snapshot_hash,updated_at=excluded.updated_at",
        params![basis.campaign_id,basis.revision,version,hash,now()],
    ).map_err(|e| e.to_string())?;
    Ok(version)
}

pub(super) fn reserve_publication(
    repository: &SqliteRepository,
    basis: &crate::DiscoveryIdentityGroupsBasis,
    source_hash: &str,
    identity_hash: &str,
) -> Result<crate::DiscoveryLibraryVersions, String> {
    if [source_hash, identity_hash].iter().any(|hash| {
        hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        return Err("library_publication_hash_invalid".into());
    }
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let library = &basis.library;
    if super::discovery_identity_groups::read_snapshot(&tx, &library.campaign_id, library.revision)?
        != *basis
    {
        return Err("library_publication_changed".into());
    }
    let source_revision = reserve_source_version(&tx, library, source_hash)?;
    let prior: Option<(u64, u32, u64, String, String)> = tx
        .query_row(
            "SELECT identity_revision,criteria_revision,source_revision,source_hash,identity_hash
         FROM discovery_library_identity_versions WHERE discovery_campaign_id=?1",
            [&library.campaign_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let unchanged = prior.as_ref().is_some_and(|(_, criteria, source, sh, ih)| {
        *criteria == library.revision
            && *source == source_revision
            && sh == source_hash
            && ih == identity_hash
    });
    let identity_revision = match &prior {
        Some((version, ..)) if unchanged => *version,
        Some((version, ..)) => version
            .checked_add(1)
            .ok_or("library_identity_version_exhausted")?,
        None => 1,
    };
    if identity_revision > 9_007_199_254_740_991 {
        return Err("library_identity_version_exhausted".into());
    }
    if !unchanged {
        tx.execute(
            "INSERT INTO discovery_library_identity_versions VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(discovery_campaign_id) DO UPDATE SET criteria_revision=excluded.criteria_revision,
             source_revision=excluded.source_revision,source_hash=excluded.source_hash,
             identity_revision=excluded.identity_revision,identity_hash=excluded.identity_hash,updated_at=excluded.updated_at",
            params![library.campaign_id,library.revision,source_revision,source_hash,identity_revision,identity_hash,now()],
        ).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(crate::DiscoveryLibraryVersions {
        source_revision,
        identity_revision,
    })
}
