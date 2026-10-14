use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{
    Confidence, ContactLocator, ContactLocatorId, ContactLocatorInput, CreatorContactRoute,
    CreatorContactRouteDependencyKind, CreatorContactRouteId, CreatorId, LocatorObservation,
    LocatorObservationId, ObservationSource, ObservationSourceId, ObservationSourceInput, Platform,
    PlatformAccountId,
};

use super::super::{json, now};
use crate::repository::sqlite::creators::to_sql_conversion_error;

pub(in super::super) fn upsert_observation_source_in(
    connection: &Connection,
    input: &ObservationSourceInput,
) -> Result<ObservationSource, String> {
    if let Some(external_ref) = input
        .external_ref
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && let Some(source) = load_observation_source_by_external_ref(connection, external_ref)?
    {
        return Ok(source);
    }

    let id = ObservationSourceId::new(Uuid::new_v4().to_string());
    let observed_at = now();
    connection
        .execute(
            "INSERT INTO observation_sources
               (id, kind, platform, source_url, external_ref, account_ref_id,
                metadata_json, observed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.as_str(),
                input.kind.as_str(),
                input.platform.map(Platform::as_str),
                input.source_url.as_deref(),
                input.external_ref.as_deref(),
                input.account_ref_id.as_ref().map(|id| id.as_str()),
                json(&input.metadata)?,
                observed_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(ObservationSource {
        id,
        kind: input.kind,
        platform: input.platform,
        source_url: input.source_url.clone(),
        external_ref: input.external_ref.clone(),
        account_ref_id: input.account_ref_id.clone(),
        metadata: input.metadata.clone(),
        observed_at,
    })
}

pub(in super::super) fn upsert_contact_locator_in(
    connection: &Connection,
    input: &ContactLocatorInput,
) -> Result<ContactLocator, String> {
    let normalized_value = input.normalized_value.trim();
    let display_value = input.display_value.trim();
    if normalized_value.is_empty() || display_value.is_empty() {
        return Err("contact locator values are required".into());
    }
    let timestamp = now();
    let existing = connection
        .query_row(
            "SELECT id, created_at FROM contact_locators
             WHERE channel = ?1 AND kind = ?2 AND normalized_value = ?3",
            params![
                input.channel.as_str(),
                input.kind.as_str(),
                normalized_value
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (id, created_at) = existing
        .map(|(id, created_at)| (ContactLocatorId::new(id), created_at))
        .unwrap_or_else(|| (ContactLocatorId::new(Uuid::new_v4().to_string()), timestamp));
    connection
        .execute(
            "INSERT INTO contact_locators
               (id, channel, kind, normalized_value, display_value, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(channel, kind, normalized_value) DO UPDATE SET
               display_value = excluded.display_value, updated_at = excluded.updated_at",
            params![
                id.as_str(),
                input.channel.as_str(),
                input.kind.as_str(),
                normalized_value,
                display_value,
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(ContactLocator {
        id,
        channel: input.channel,
        kind: input.kind,
        normalized_value: normalized_value.to_owned(),
        display_value: display_value.to_owned(),
        created_at,
        updated_at: timestamp,
    })
}

pub(in super::super) fn link_creator_contact_route_in(
    connection: &Connection,
    creator_id: &CreatorId,
    locator_id: &ContactLocatorId,
    confidence: Confidence,
) -> Result<CreatorContactRoute, String> {
    let timestamp = now();
    let existing = connection
        .query_row(
            "SELECT id, created_at FROM creator_contact_routes
             WHERE creator_id = ?1 AND locator_id = ?2",
            params![creator_id.as_str(), locator_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (id, created_at) = existing
        .map(|(id, created_at)| (CreatorContactRouteId::new(id), created_at))
        .unwrap_or_else(|| {
            (
                CreatorContactRouteId::new(Uuid::new_v4().to_string()),
                timestamp,
            )
        });
    connection
        .execute(
            "INSERT INTO creator_contact_routes
               (id, creator_id, locator_id, confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(creator_id, locator_id) DO UPDATE SET
               confidence = excluded.confidence, updated_at = excluded.updated_at",
            params![
                id.as_str(),
                creator_id.as_str(),
                locator_id.as_str(),
                confidence.as_str(),
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(CreatorContactRoute {
        id,
        creator_id: creator_id.clone(),
        locator_id: locator_id.clone(),
        confidence,
        created_at,
        updated_at: timestamp,
    })
}

pub(in super::super) fn link_creator_contact_route_dependency_in(
    connection: &Connection,
    parent_contact_route_id: &CreatorContactRouteId,
    child_contact_route_id: &CreatorContactRouteId,
    kind: CreatorContactRouteDependencyKind,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO creator_contact_route_dependencies
               (parent_contact_route_id, child_contact_route_id, kind, created_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(parent_contact_route_id, child_contact_route_id) DO UPDATE SET
               kind = excluded.kind",
            params![
                parent_contact_route_id.as_str(),
                child_contact_route_id.as_str(),
                match kind {
                    CreatorContactRouteDependencyKind::LinkHub => "link_hub",
                    CreatorContactRouteDependencyKind::CommunityAdministrator => {
                        "community_administrator"
                    }
                },
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(in super::super) fn append_locator_observation_in(
    connection: &Connection,
    locator_id: &ContactLocatorId,
    source_id: &ObservationSourceId,
    creator_id: Option<&CreatorId>,
    platform_account_id: Option<&PlatformAccountId>,
    observed_value: &str,
) -> Result<LocatorObservation, String> {
    if creator_id.is_none() && platform_account_id.is_none() {
        return Err("locator observation requires a creator or platform account".into());
    }
    let observed_value = observed_value.trim();
    if observed_value.is_empty() {
        return Err("locator observation value is required".into());
    }
    let existing = connection
        .query_row(
            "SELECT id, observed_at FROM locator_observations
             WHERE locator_id = ?1 AND source_id = ?2
               AND creator_id IS ?3 AND platform_account_id IS ?4
               AND observed_value = ?5 LIMIT 1",
            params![
                locator_id.as_str(),
                source_id.as_str(),
                creator_id.map(CreatorId::as_str),
                platform_account_id.map(PlatformAccountId::as_str),
                observed_value
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((id, observed_at)) = existing {
        return Ok(LocatorObservation {
            id: LocatorObservationId::new(id),
            locator_id: locator_id.clone(),
            source_id: source_id.clone(),
            creator_id: creator_id.cloned(),
            platform_account_id: platform_account_id.cloned(),
            observed_value: observed_value.to_owned(),
            observed_at,
        });
    }

    let id = LocatorObservationId::new(Uuid::new_v4().to_string());
    let observed_at = now();
    connection
        .execute(
            "INSERT INTO locator_observations
               (id, locator_id, source_id, creator_id, platform_account_id,
                observed_value, observed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.as_str(),
                locator_id.as_str(),
                source_id.as_str(),
                creator_id.map(CreatorId::as_str),
                platform_account_id.map(PlatformAccountId::as_str),
                observed_value,
                observed_at
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(LocatorObservation {
        id,
        locator_id: locator_id.clone(),
        source_id: source_id.clone(),
        creator_id: creator_id.cloned(),
        platform_account_id: platform_account_id.cloned(),
        observed_value: observed_value.to_owned(),
        observed_at,
    })
}

fn load_observation_source_by_external_ref(
    connection: &Connection,
    external_ref: &str,
) -> Result<Option<ObservationSource>, String> {
    connection
        .query_row(
            "SELECT id, kind, platform, source_url, external_ref, account_ref_id,
                    metadata_json, observed_at
             FROM observation_sources WHERE external_ref = ?1 LIMIT 1",
            [external_ref],
            |row| {
                let kind: String = row.get(1)?;
                let platform = row
                    .get::<_, Option<String>>(2)?
                    .map(|value| Platform::parse(&value).map_err(to_sql_conversion_error))
                    .transpose()?;
                let metadata: String = row.get(6)?;
                Ok(ObservationSource {
                    id: ObservationSourceId::new(row.get::<_, String>(0)?),
                    kind: crate::SourceKind::parse(&kind).map_err(to_sql_conversion_error)?,
                    platform,
                    source_url: row.get(3)?,
                    external_ref: row.get(4)?,
                    account_ref_id: row
                        .get::<_, Option<String>>(5)?
                        .map(crate::AccountRefId::new),
                    metadata: serde_json::from_str(&metadata)
                        .map_err(|error| to_sql_conversion_error(error.to_string()))?,
                    observed_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}
