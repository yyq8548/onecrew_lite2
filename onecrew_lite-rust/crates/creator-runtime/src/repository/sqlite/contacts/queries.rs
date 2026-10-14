use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;

use crate::{
    Confidence, ContactLocator, ContactLocatorId, ContactLocatorInput,
    ContactRouteObservationRecord, CreatorContactRoute, CreatorContactRouteAction,
    CreatorContactRouteId, CreatorContactRouteRecord, CreatorId, LocatorObservation,
    LocatorObservationId, ObservationSource, ObservationSourceId, Platform, PlatformAccountId,
    SourceKind,
};

use crate::repository::sqlite::creators::to_sql_conversion_error;

pub(in super::super) fn creator_is_active_in(
    connection: &Connection,
    creator_id: &CreatorId,
) -> Result<Option<bool>, String> {
    connection
        .query_row(
            "SELECT creator.archived_at IS NULL AND relationship.stage != 'archived'
             FROM creators creator
             JOIN creator_relationships relationship ON relationship.creator_id = creator.id
             WHERE creator.id = ?1",
            [creator_id.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub(in super::super) fn has_root_initial_platform_observation(
    record: &CreatorContactRouteRecord,
) -> bool {
    record.observations.iter().any(|value| {
        value.source.kind == SourceKind::InitialPlatformAccount
            && value
                .source
                .metadata
                .pointer("/source/kind")
                .and_then(serde_json::Value::as_str)
                != Some("creator_contact_route")
    })
}

fn is_strict_manual_route(record: &CreatorContactRouteRecord) -> bool {
    matches!(
        record.observations.as_slice(),
        [value]
            if value.source.kind == SourceKind::ManualEntry
                && value.observation.creator_id.as_ref() == Some(&record.route.creator_id)
                && value.observation.platform_account_id.is_none()
                && value.source.external_ref.is_none()
                && value.source.account_ref_id.is_none()
                && value.source.metadata == serde_json::json!({})
    )
}

pub(super) fn is_strict_manual_route_in(
    connection: &Connection,
    record: &CreatorContactRouteRecord,
) -> Result<bool, String> {
    if !is_strict_manual_route(record) {
        return Ok(false);
    }
    let source_id = &record.observations[0].source.id;
    connection
        .query_row(
            "SELECT COUNT(*) = 1 FROM locator_observations WHERE source_id = ?1",
            [source_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn route_allowed_actions(
    connection: &Connection,
    creator_active: bool,
    record: &CreatorContactRouteRecord,
) -> Result<Vec<CreatorContactRouteAction>, String> {
    if !creator_active {
        return Ok(Vec::new());
    }
    let mut actions = Vec::with_capacity(2);
    if is_strict_manual_route_in(connection, record)? {
        actions.push(CreatorContactRouteAction::Update);
    }
    let is_root_initial = has_root_initial_platform_observation(record);
    if !is_root_initial {
        actions.push(CreatorContactRouteAction::Delete);
    }
    Ok(actions)
}

pub(in super::super) fn load_creator_contact_routes(
    connection: &Connection,
    creator_id: &CreatorId,
) -> Result<Vec<CreatorContactRouteRecord>, String> {
    let creator_active = creator_is_active_in(connection, creator_id)?.unwrap_or(false);
    let mut statement = connection
        .prepare(
            "SELECT route.id, route.creator_id, route.locator_id,
                    route.confidence, route.created_at, route.updated_at,
                    locator.channel, locator.kind, locator.normalized_value,
                    locator.display_value, locator.created_at, locator.updated_at
             FROM creator_contact_routes route
             JOIN contact_locators locator ON locator.id = route.locator_id
             WHERE route.creator_id = ?1
             ORDER BY route.created_at ASC, route.rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([creator_id.as_str()], map_contact_route_record)
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(route, locator)| {
            let observations = load_route_observations(connection, creator_id, &locator.id)?;
            let mut record = CreatorContactRouteRecord {
                route,
                locator,
                observations,
                allowed_actions: Vec::new(),
            };
            record.allowed_actions = route_allowed_actions(connection, creator_active, &record)?;
            Ok(record)
        })
        .collect()
}

pub(in super::super) fn find_contact_route_owners_in(
    connection: &Connection,
    locator: &ContactLocatorInput,
) -> Result<Vec<CreatorContactRouteRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT route.id, route.creator_id, route.locator_id,
                    route.confidence, route.created_at, route.updated_at,
                    locator.channel, locator.kind, locator.normalized_value,
                    locator.display_value, locator.created_at, locator.updated_at
             FROM contact_locators locator
             JOIN creator_contact_routes route ON route.locator_id = locator.id
             WHERE locator.channel = ?1 AND locator.kind = ?2
               AND locator.normalized_value = ?3
             ORDER BY route.created_at ASC, route.rowid ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                locator.channel.as_str(),
                locator.kind.as_str(),
                locator.normalized_value.trim()
            ],
            map_contact_route_record,
        )
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(route, locator)| {
            let observations = load_route_observations(connection, &route.creator_id, &locator.id)?;
            let creator_active =
                creator_is_active_in(connection, &route.creator_id)?.unwrap_or(false);
            let mut record = CreatorContactRouteRecord {
                route,
                locator,
                observations,
                allowed_actions: Vec::new(),
            };
            record.allowed_actions = route_allowed_actions(connection, creator_active, &record)?;
            Ok(record)
        })
        .collect()
}

/// SQLite has shipped with both a 999 and a 32766 host-parameter limit,
/// depending on the build. Keep the tuple lookup below the smaller portable
/// limit rather than relying on a connection-specific compile option.
const CONTACT_ROUTE_OWNER_LOOKUP_PARAMS_PER_ROW: usize = 3;
const CONTACT_ROUTE_OWNER_LOOKUP_CHUNK_SIZE: usize =
    999 / CONTACT_ROUTE_OWNER_LOOKUP_PARAMS_PER_ROW;

pub(in super::super) fn contact_route_owner_lookup_sql(chunk_len: usize) -> String {
    let placeholders = std::iter::repeat_n("(?, ?, ?)", chunk_len)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "SELECT DISTINCT locator.channel, locator.kind, locator.normalized_value
         FROM contact_locators locator
         JOIN creator_contact_routes route ON route.locator_id = locator.id
         WHERE (locator.channel, locator.kind, locator.normalized_value)
               IN ({placeholders})"
    )
}

pub(in super::super) fn find_contact_route_owner_keys_in(
    connection: &Connection,
    locators: &[ContactLocatorInput],
) -> Result<HashSet<String>, String> {
    let mut owners = HashSet::new();
    for chunk in locators.chunks(CONTACT_ROUTE_OWNER_LOOKUP_CHUNK_SIZE) {
        let sql = contact_route_owner_lookup_sql(chunk.len());
        let values = chunk
            .iter()
            .flat_map(|locator| {
                [
                    locator.channel.as_str().to_owned(),
                    locator.kind.as_str().to_owned(),
                    locator.normalized_value.trim().to_owned(),
                ]
            })
            .collect::<Vec<_>>();
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(values.iter()), |row| {
                let channel: String = row.get(0)?;
                let kind: String = row.get(1)?;
                let normalized_value: String = row.get(2)?;
                Ok(format!("{channel}:{kind}:{normalized_value}"))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            owners.insert(row.map_err(|error| error.to_string())?);
        }
    }
    Ok(owners)
}

pub(in super::super) fn load_creator_contact_route_by_id(
    connection: &Connection,
    creator_id: &CreatorId,
    route_id: &CreatorContactRouteId,
) -> Result<CreatorContactRouteRecord, String> {
    load_creator_contact_routes(connection, creator_id)?
        .into_iter()
        .find(|record| record.route.id == *route_id)
        .ok_or_else(|| "contact route not found for Creator".to_string())
}

fn map_contact_route_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(CreatorContactRoute, ContactLocator)> {
    let confidence: String = row.get(3)?;
    let channel: String = row.get(6)?;
    let kind: String = row.get(7)?;
    let locator_id = ContactLocatorId::new(row.get::<_, String>(2)?);
    Ok((
        CreatorContactRoute {
            id: CreatorContactRouteId::new(row.get::<_, String>(0)?),
            creator_id: CreatorId::new(row.get::<_, String>(1)?),
            locator_id: locator_id.clone(),
            confidence: Confidence::parse(&confidence).map_err(to_sql_conversion_error)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        },
        ContactLocator {
            id: locator_id,
            channel: crate::ContactChannel::parse(&channel).map_err(to_sql_conversion_error)?,
            kind: crate::LocatorKind::parse(&kind).map_err(to_sql_conversion_error)?,
            normalized_value: row.get(8)?,
            display_value: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        },
    ))
}

fn load_route_observations(
    connection: &Connection,
    creator_id: &CreatorId,
    locator_id: &ContactLocatorId,
) -> Result<Vec<ContactRouteObservationRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT observation.id, observation.locator_id, observation.source_id,
                    observation.creator_id, observation.platform_account_id,
                    observation.observed_value, observation.observed_at,
                    source.kind, source.platform, source.source_url, source.external_ref,
                    source.account_ref_id, source.metadata_json, source.observed_at
             FROM locator_observations observation
             JOIN observation_sources source ON source.id = observation.source_id
             WHERE observation.locator_id = ?1
               AND (observation.creator_id = ?2 OR observation.platform_account_id IN (
                 SELECT platform_account_id FROM creator_account_links WHERE creator_id = ?2
               ))
             ORDER BY observation.observed_at DESC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![locator_id.as_str(), creator_id.as_str()], |row| {
            let source_kind: String = row.get(7)?;
            let source_platform = row
                .get::<_, Option<String>>(8)?
                .map(|value| Platform::parse(&value).map_err(to_sql_conversion_error))
                .transpose()?;
            let metadata: String = row.get(12)?;
            Ok(ContactRouteObservationRecord {
                observation: LocatorObservation {
                    id: LocatorObservationId::new(row.get::<_, String>(0)?),
                    locator_id: ContactLocatorId::new(row.get::<_, String>(1)?),
                    source_id: ObservationSourceId::new(row.get::<_, String>(2)?),
                    creator_id: row.get::<_, Option<String>>(3)?.map(CreatorId::new),
                    platform_account_id: row
                        .get::<_, Option<String>>(4)?
                        .map(PlatformAccountId::new),
                    observed_value: row.get(5)?,
                    observed_at: row.get(6)?,
                },
                source: ObservationSource {
                    id: ObservationSourceId::new(row.get::<_, String>(2)?),
                    kind: crate::SourceKind::parse(&source_kind)
                        .map_err(to_sql_conversion_error)?,
                    platform: source_platform,
                    source_url: row.get(9)?,
                    external_ref: row.get(10)?,
                    account_ref_id: row
                        .get::<_, Option<String>>(11)?
                        .map(crate::AccountRefId::new),
                    metadata: serde_json::from_str(&metadata)
                        .map_err(|error| to_sql_conversion_error(error.to_string()))?,
                    observed_at: row.get(13)?,
                },
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}
