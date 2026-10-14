use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::{
    CommunityAdministratorSource, Confidence, CreatorAccountLinkId, CreatorCandidateOrigin,
    CreatorId, CreatorOriginId, LinkState, ObservationSourceId, PlatformAccountCandidate,
    PlatformAccountId, PlatformAccountObservationId,
};

use super::super::{json, now};

pub(super) fn ensure_active_creator(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
) -> Result<(), String> {
    let active = transaction
        .query_row(
            "SELECT creator.archived_at IS NULL AND relationship.stage != 'archived'
             FROM creators creator
             JOIN creator_relationships relationship ON relationship.creator_id = creator.id
             WHERE creator.id = ?1",
            [creator_id.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "existing Creator no longer exists".to_string())?;
    if !active {
        return Err(
            "archived Creator cannot accept discovery updates; delete it or use an active Creator"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn validate_candidate(candidate: &PlatformAccountCandidate) -> Result<(), String> {
    if candidate
        .external_id
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
        || candidate.target_kind.is_none()
    {
        return Err("a stable external_id and target_kind are required to save a Creator".into());
    }
    if candidate.target_kind == Some(crate::TargetKind::Bot) {
        return Err("Bots cannot be saved as Creators".into());
    }
    Ok(())
}

pub(super) fn insert_creator(
    transaction: &Transaction<'_>,
    display_name: &str,
    notes: Option<&str>,
    quality: &crate::CreatorQualityInput,
    tags: Option<Vec<String>>,
) -> Result<CreatorId, String> {
    let id = CreatorId::new(Uuid::new_v4().to_string());
    let timestamp = now();
    let follower_counts_json =
        serde_json::to_string(&quality.follower_counts).map_err(|error| error.to_string())?;
    let tags = crate::creators::normalize_creator_tags(tags.unwrap_or_default())
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO creators
               (id, display_name, notes, score, assessment, follower_counts_json,
                created_at, updated_at, archived_at, tags_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, NULL, ?8)",
            params![
                id.as_str(),
                display_name,
                notes,
                quality.score,
                quality.assessment,
                follower_counts_json,
                timestamp,
                serde_json::to_string(&tags).map_err(|error| error.to_string())?
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO creator_relationships
               (creator_id, stage, last_touch_at, created_at, updated_at)
             VALUES (?1, 'prospect', NULL, ?2, ?2)",
            params![id.as_str(), timestamp],
        )
        .map_err(|error| error.to_string())?;
    Ok(id)
}

pub(super) fn upsert_account(
    transaction: &Transaction<'_>,
    candidate: &PlatformAccountCandidate,
) -> Result<PlatformAccountId, String> {
    let external_id = candidate.external_id.as_deref().expect("validated").trim();
    let target_kind = candidate.target_kind.expect("validated");
    let timestamp = now();
    let existing = transaction
        .query_row(
            "SELECT id FROM platform_accounts
             WHERE platform = ?1 AND target_kind = ?2 AND external_id = ?3",
            params![
                candidate.platform.as_str(),
                target_kind.as_str(),
                external_id
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let id = PlatformAccountId::new(existing.unwrap_or_else(|| Uuid::new_v4().to_string()));
    transaction
        .execute(
            "INSERT INTO platform_accounts
               (id, platform, target_kind, external_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(platform, target_kind, external_id)
             DO UPDATE SET updated_at = excluded.updated_at",
            params![
                id.as_str(),
                candidate.platform.as_str(),
                target_kind.as_str(),
                external_id,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(id)
}

pub(super) fn link_account(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    account_id: &PlatformAccountId,
) -> Result<(), String> {
    let timestamp = now();
    let existing = transaction
        .query_row(
            "SELECT id FROM creator_account_links
             WHERE creator_id = ?1 AND platform_account_id = ?2",
            params![creator_id.as_str(), account_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let id = CreatorAccountLinkId::new(existing.unwrap_or_else(|| Uuid::new_v4().to_string()));
    transaction
        .execute(
            "INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
             ON CONFLICT(creator_id, platform_account_id) DO UPDATE SET
               state = excluded.state, confidence = excluded.confidence,
               updated_at = excluded.updated_at",
            params![
                id.as_str(),
                creator_id.as_str(),
                account_id.as_str(),
                LinkState::Confirmed.as_str(),
                Confidence::Exact.as_str(),
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn append_observation(
    transaction: &Transaction<'_>,
    account_id: &PlatformAccountId,
    source_id: &ObservationSourceId,
    candidate: &PlatformAccountCandidate,
    public_data: &serde_json::Value,
    observed_at: u64,
) -> Result<(), String> {
    let public_json = json(public_data)?;
    let exists = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM platform_account_observations
               WHERE platform_account_id = ?1 AND source_id = ?2
                 AND handle IS ?3 AND display_name IS ?4
                 AND profile_url IS ?5 AND public_json = ?6
                 AND EXISTS (SELECT 1 FROM platform_account_observations latest
                   WHERE latest.id = (SELECT current.id FROM platform_account_observations current
                     WHERE current.platform_account_id = ?1
                     ORDER BY current.observed_at DESC, current.rowid DESC LIMIT 1)
                   AND (latest.observed_at > ?7 OR
                     (latest.handle IS ?3 AND latest.display_name IS ?4
                      AND latest.profile_url IS ?5 AND latest.public_json = ?6)))
             )",
            params![
                account_id.as_str(),
                source_id.as_str(),
                candidate.handle.as_deref(),
                candidate.display_name.trim(),
                candidate.profile_url.as_deref(),
                public_json,
                observed_at
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| error.to_string())?;
    if exists {
        return Ok(());
    }
    let id = PlatformAccountObservationId::new(Uuid::new_v4().to_string());
    transaction
        .execute(
            "INSERT INTO platform_account_observations
               (id, platform_account_id, source_id, handle, display_name, profile_url,
                public_json, observed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.as_str(),
                account_id.as_str(),
                source_id.as_str(),
                candidate.handle.as_deref(),
                candidate.display_name.trim(),
                candidate.profile_url.as_deref(),
                public_json,
                observed_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(super) fn append_origin(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    source_id: &ObservationSourceId,
    origin: &CreatorCandidateOrigin,
) -> Result<(), String> {
    let id = CreatorOriginId::new(Uuid::new_v4().to_string());
    match origin {
        CreatorCandidateOrigin::PlatformSearch {
            discovery_id,
            account_ref_id,
            ..
        } => transaction.execute(
            "INSERT INTO creator_origins
                   (id, kind, creator_id, source_id, discovery_id, source_creator_id,
                    source_contact_route_id, operator_account_ref_id,
                    source_resolved_target_id, discovered_at)
                 VALUES (?1, 'platform_search', ?2, ?3, ?4, NULL, NULL, ?5, NULL, ?6)
                 ON CONFLICT DO NOTHING",
            params![
                id.as_str(),
                creator_id.as_str(),
                source_id.as_str(),
                discovery_id.as_str(),
                account_ref_id.as_str(),
                now(),
            ],
        ),
        CreatorCandidateOrigin::CommunityAdministrator {
            discovery_id,
            role,
            community,
            source:
                CommunityAdministratorSource::CreatorContactRoute {
                    source_creator_id,
                    source_contact_route_id,
                    operator_account_ref_id,
                    source_resolved_target_id,
                },
        } => {
            let source_contact_route_id = source_contact_route_id.as_ref().ok_or_else(|| {
                "community administrator save requires a live source contact route".to_string()
            })?;
            transaction.execute(
                "INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, source_creator_id,
                source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                administrator_role, community_external_id, community_title, community_handle,
                community_target_kind, discovered_at)
             VALUES (?1, 'community_administrator', ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                     ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT DO NOTHING",
                params![
                    id.as_str(),
                    creator_id.as_str(),
                    source_id.as_str(),
                    discovery_id.as_str(),
                    source_creator_id.as_str(),
                    source_contact_route_id.as_str(),
                    operator_account_ref_id.as_str(),
                    source_resolved_target_id.as_str(),
                    role.as_str(),
                    community.external_id,
                    community.title,
                    community.handle,
                    community.target_kind.as_str(),
                    now(),
                ],
            )
        }
        CreatorCandidateOrigin::CommunityAdministrator {
            discovery_id,
            role,
            community,
            source:
                CommunityAdministratorSource::OperatorAccount {
                    operator_account_ref_id,
                },
        } => transaction.execute(
            "INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, source_creator_id,
                source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                administrator_role, community_external_id, community_title, community_handle,
                community_target_kind, discovered_at)
             VALUES (?1, 'community_administrator', ?2, ?3, ?4, NULL, NULL, ?5, NULL,
                     ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT DO NOTHING",
            params![
                id.as_str(),
                creator_id.as_str(),
                source_id.as_str(),
                discovery_id.as_str(),
                operator_account_ref_id.as_str(),
                role.as_str(),
                community.external_id,
                community.title,
                community.handle,
                community.target_kind.as_str(),
                now()
            ],
        ),
    }
    .map_err(|error| error.to_string())?;
    Ok(())
}
