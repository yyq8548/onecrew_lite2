use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    Confidence, ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    ContactLocator, ContactLocatorId, ContactLocatorInput, CreatorContactRoute,
    CreatorContactRouteDependencyKind, CreatorContactRouteId, CreatorContactRouteRecord, CreatorId,
    DiscoveredContactRoute, LocatorKind, LocatorObservation, ManualCreatorContactRouteAddInput,
    ManualCreatorContactRouteAddResult, ManualCreatorContactRouteAddStatus,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateResult,
    ManualCreatorContactRouteUpdateStatus, ObservationSource, ObservationSourceId,
    ObservationSourceInput, Platform, PlatformAccountId, PrepareCreatorContactRouteDeleteInput,
    PrepareCreatorContactRouteDeleteResult, SourceKind, repository::ContactRepository,
};

use super::{
    super::{SqliteRepository, json, now},
    graph::{
        append_locator_observation_in, link_creator_contact_route_dependency_in,
        link_creator_contact_route_in, upsert_contact_locator_in, upsert_observation_source_in,
    },
    queries::{
        creator_is_active_in, find_contact_route_owner_keys_in, find_contact_route_owners_in,
        is_strict_manual_route_in, load_creator_contact_route_by_id, load_creator_contact_routes,
    },
};

impl ContactRepository for SqliteRepository {
    #[cfg(test)]
    fn insert_historical_shared_contact_route_for_test(
        &self,
        creator_id: &CreatorId,
        locator_id: &ContactLocatorId,
        confidence: Confidence,
    ) -> Result<CreatorContactRoute, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        transaction
            .execute_batch(
                "DROP TRIGGER prevent_shared_creator_contact_locator_insert;
                 DROP TRIGGER prevent_shared_creator_contact_locator_update;
                 DROP INDEX creator_contact_routes_one_per_locator;",
            )
            .map_err(|error| error.to_string())?;
        let route =
            link_creator_contact_route_in(&transaction, creator_id, locator_id, confidence)?;
        transaction
            .execute_batch(super::super::schema::NO_SHARED_CONTACT_ROUTE_TRIGGERS_SQL)
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(route)
    }

    fn add_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteAddInput,
        locator: ContactLocatorInput,
    ) -> Result<ManualCreatorContactRouteAddResult, String> {
        let value = input.value.trim();
        if value.is_empty() {
            return Err("contact route value is required".into());
        }
        let source_url = input
            .source_url
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("contact route source_url cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let canonical =
            crate::contact_routes::normalize_discovered_contact_route(&DiscoveredContactRoute {
                channel: input.channel,
                kind: LocatorKind::Handle,
                value: value.to_owned(),
                source: SourceKind::ManualEntry,
                source_url: source_url.clone(),
            })?;
        if canonical.channel != locator.channel
            || canonical.kind != locator.kind
            || canonical.normalized_value != locator.normalized_value
            || canonical.display_value != locator.display_value
        {
            return Err("manual contact route was not canonically normalized".into());
        }

        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let active = transaction
            .query_row(
                "SELECT creator.archived_at IS NULL AND relationship.stage != 'archived'
                 FROM creators creator
                 JOIN creator_relationships relationship ON relationship.creator_id = creator.id
                 WHERE creator.id = ?1",
                [input.creator_id.as_str()],
                |row| row.get::<_, bool>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "creator not found".to_string())?;
        if !active {
            return Err(
                "archived Creator cannot be used for outreach; delete it or use an active Creator"
                    .into(),
            );
        }

        let owners = find_contact_route_owners_in(&transaction, &canonical)?;
        if owners
            .iter()
            .any(|owner| owner.route.creator_id != input.creator_id)
        {
            return Err("contact route already belongs to another Creator".into());
        }
        if let Some(existing) = owners
            .into_iter()
            .find(|owner| owner.route.creator_id == input.creator_id)
        {
            return Ok(manual_contact_result(
                ManualCreatorContactRouteAddStatus::AlreadyExists,
                existing.route,
                existing.locator,
            ));
        }

        let source = upsert_observation_source_in(
            &transaction,
            &ObservationSourceInput {
                kind: SourceKind::ManualEntry,
                platform: platform_for_contact_channel(input.channel),
                source_url,
                external_ref: None,
                account_ref_id: None,
                metadata: serde_json::json!({}),
            },
        )?;
        let locator = upsert_contact_locator_in(&transaction, &canonical)?;
        let route = link_creator_contact_route_in(
            &transaction,
            &input.creator_id,
            &locator.id,
            Confidence::Asserted,
        )?;
        append_locator_observation_in(
            &transaction,
            &locator.id,
            &source.id,
            Some(&input.creator_id),
            None,
            value,
        )?;
        record_contact_route_audit_in(
            &transaction,
            "creator_contact_route_added",
            &input.creator_id,
            &route,
            &locator.id,
            serde_json::json!({ "route_id": route.id.as_str() }),
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(manual_contact_result(
            ManualCreatorContactRouteAddStatus::Added,
            route,
            locator,
        ))
    }

    fn update_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteUpdateInput,
        locator: ContactLocatorInput,
    ) -> Result<ManualCreatorContactRouteUpdateResult, String> {
        let value = input.value.trim();
        if value.is_empty() {
            return Err("contact route value is required".into());
        }
        let source_url = input
            .source_url
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("contact route source_url cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let input = ManualCreatorContactRouteUpdateInput {
            creator_id: input.creator_id,
            contact_route_id: input.contact_route_id,
            channel: input.channel,
            value: value.to_owned(),
            source_url,
        };
        let value = input.value.as_str();
        let canonical =
            canonical_manual_locator(input.channel, value, input.source_url.clone(), &locator)?;

        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        require_active_creator_in(&transaction, &input.creator_id)?;
        let existing = load_creator_contact_routes(&transaction, &input.creator_id)?
            .into_iter()
            .find(|record| record.route.id == input.contact_route_id)
            .ok_or_else(|| "contact route not found for Creator".to_string())?;
        if !is_strict_manual_route_in(&transaction, &existing)? {
            return Err("only an active manually added contact route can be updated".into());
        }

        let observation = existing
            .observations
            .first()
            .ok_or_else(|| "manual contact route provenance is missing".to_string())?;
        let same_canonical = existing.locator.channel == canonical.channel
            && existing.locator.kind == canonical.kind
            && existing.locator.normalized_value == canonical.normalized_value;
        if same_canonical {
            let unchanged = existing.locator.display_value == canonical.display_value
                && existing.route.confidence == Confidence::Asserted
                && observation.observation.observed_value == value
                && observation.source.source_url == input.source_url;
            if unchanged {
                return Ok(ManualCreatorContactRouteUpdateResult {
                    status: ManualCreatorContactRouteUpdateStatus::Unchanged,
                    creator_id: input.creator_id,
                    route_id: existing.route.id.clone(),
                    route: existing,
                });
            }

            let timestamp = now();
            transaction
                .execute(
                    "UPDATE contact_locators
                     SET display_value = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        canonical.display_value,
                        timestamp,
                        existing.locator.id.as_str()
                    ],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE observation_sources
                     SET platform = ?1, source_url = ?2, observed_at = ?3 WHERE id = ?4",
                    params![
                        platform_for_contact_channel(input.channel).map(Platform::as_str),
                        input.source_url.as_deref(),
                        timestamp,
                        observation.source.id.as_str()
                    ],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE locator_observations
                     SET observed_value = ?1, observed_at = ?2 WHERE id = ?3",
                    params![value, timestamp, observation.observation.id.as_str()],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE creator_contact_routes
                     SET confidence = 'asserted', updated_at = ?1 WHERE id = ?2",
                    params![timestamp, existing.route.id.as_str()],
                )
                .map_err(|error| error.to_string())?;
            record_contact_route_audit_in(
                &transaction,
                "creator_contact_route_updated",
                &input.creator_id,
                &existing.route,
                &existing.locator.id,
                serde_json::json!({ "route_id": existing.route.id.as_str() }),
            )?;
            transaction.commit().map_err(|error| error.to_string())?;
            let route = load_creator_contact_route_by_id(
                &connection,
                &input.creator_id,
                &existing.route.id,
            )?;
            return Ok(ManualCreatorContactRouteUpdateResult {
                status: ManualCreatorContactRouteUpdateStatus::Updated,
                creator_id: input.creator_id,
                route_id: route.route.id.clone(),
                route,
            });
        }
        Ok(ManualCreatorContactRouteUpdateResult {
            status: ManualCreatorContactRouteUpdateStatus::ReplacementRequired,
            creator_id: input.creator_id,
            route_id: existing.route.id.clone(),
            route: existing,
        })
    }

    fn prepare_creator_contact_route_delete(
        &self,
        input: PrepareCreatorContactRouteDeleteInput,
    ) -> Result<PrepareCreatorContactRouteDeleteResult, String> {
        super::super::contact_route_deletion::prepare(self, input)
    }

    fn confirm_creator_contact_route_delete(
        &self,
        input: ConfirmCreatorContactRouteDeleteInput,
    ) -> Result<(ConfirmCreatorContactRouteDeleteResult, bool), String> {
        super::super::contact_route_deletion::confirm(self, input)
    }

    fn create_observation_source(
        &self,
        input: ObservationSourceInput,
    ) -> Result<ObservationSource, String> {
        let connection = self.lock()?;
        upsert_observation_source_in(&connection, &input)
    }

    fn upsert_contact_locator(&self, input: ContactLocatorInput) -> Result<ContactLocator, String> {
        let connection = self.lock()?;
        upsert_contact_locator_in(&connection, &input)
    }

    fn link_creator_contact_route(
        &self,
        creator_id: &CreatorId,
        locator_id: &ContactLocatorId,
        confidence: Confidence,
    ) -> Result<CreatorContactRoute, String> {
        let connection = self.lock()?;
        link_creator_contact_route_in(&connection, creator_id, locator_id, confidence)
    }

    fn link_creator_contact_route_dependency(
        &self,
        parent_contact_route_id: &CreatorContactRouteId,
        child_contact_route_id: &CreatorContactRouteId,
        kind: CreatorContactRouteDependencyKind,
    ) -> Result<(), String> {
        let connection = self.lock()?;
        link_creator_contact_route_dependency_in(
            &connection,
            parent_contact_route_id,
            child_contact_route_id,
            kind,
        )
    }

    fn append_locator_observation(
        &self,
        locator_id: &ContactLocatorId,
        source_id: &ObservationSourceId,
        creator_id: Option<&CreatorId>,
        platform_account_id: Option<&PlatformAccountId>,
        observed_value: &str,
    ) -> Result<LocatorObservation, String> {
        let connection = self.lock()?;
        append_locator_observation_in(
            &connection,
            locator_id,
            source_id,
            creator_id,
            platform_account_id,
            observed_value,
        )
    }

    fn creator_contact_routes(
        &self,
        creator_id: &CreatorId,
    ) -> Result<Vec<CreatorContactRouteRecord>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let routes = load_creator_contact_routes(&transaction, creator_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(routes)
    }

    fn find_contact_route_owners(
        &self,
        locator: &ContactLocatorInput,
    ) -> Result<Vec<CreatorContactRouteRecord>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let owners = find_contact_route_owners_in(&transaction, locator)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(owners)
    }

    fn find_contact_route_owner_keys(
        &self,
        locators: &[ContactLocatorInput],
    ) -> Result<Vec<String>, String> {
        if locators.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let owners = find_contact_route_owner_keys_in(&transaction, locators)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(owners.into_iter().collect())
    }
}

fn platform_for_contact_channel(channel: crate::ContactChannel) -> Option<Platform> {
    match channel {
        crate::ContactChannel::X => Some(Platform::X),
        crate::ContactChannel::Instagram => Some(Platform::Instagram),
        crate::ContactChannel::Telegram => Some(Platform::Telegram),
        crate::ContactChannel::YouTube => Some(Platform::YouTube),
        crate::ContactChannel::TikTok => Some(Platform::TikTok),
        crate::ContactChannel::Email
        | crate::ContactChannel::Website
        | crate::ContactChannel::LinkHub | crate::ContactChannel::Wechat => None,
    }
}

fn canonical_manual_locator(
    channel: crate::ContactChannel,
    value: &str,
    source_url: Option<String>,
    supplied: &ContactLocatorInput,
) -> Result<ContactLocatorInput, String> {
    let canonical =
        crate::contact_routes::normalize_discovered_contact_route(&DiscoveredContactRoute {
            channel,
            kind: LocatorKind::Handle,
            value: value.to_owned(),
            source: SourceKind::ManualEntry,
            source_url,
        })?;
    if canonical.channel != supplied.channel
        || canonical.kind != supplied.kind
        || canonical.normalized_value != supplied.normalized_value
        || canonical.display_value != supplied.display_value
    {
        return Err("manual contact route was not canonically normalized".into());
    }
    Ok(canonical)
}

fn require_active_creator_in(
    connection: &Connection,
    creator_id: &CreatorId,
) -> Result<(), String> {
    let active = creator_is_active_in(connection, creator_id)?
        .ok_or_else(|| "creator not found".to_string())?;
    if !active {
        return Err(
            "archived Creator cannot be used for outreach; delete it or use an active Creator"
                .into(),
        );
    }
    Ok(())
}

fn record_contact_route_audit_in(
    connection: &Connection,
    kind: &str,
    creator_id: &CreatorId,
    route: &CreatorContactRoute,
    locator_id: &ContactLocatorId,
    payload: serde_json::Value,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events
               (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                contact_locator_id, contact_route_id, email_id, payload_json,
                created_at)
             VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?5, NULL, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                kind,
                creator_id.as_str(),
                locator_id.as_str(),
                route.id.as_str(),
                json(&serde_json::json!({
                    "route_id": route.id.as_str(),
                    "details": payload,
                }))?,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn manual_contact_result(
    status: ManualCreatorContactRouteAddStatus,
    route: CreatorContactRoute,
    locator: ContactLocator,
) -> ManualCreatorContactRouteAddResult {
    ManualCreatorContactRouteAddResult {
        status,
        creator_id: route.creator_id,
        route_id: route.id,
        channel: locator.channel,
        locator_kind: locator.kind,
        normalized_value: locator.normalized_value,
        display_value: locator.display_value,
        confidence: route.confidence,
        created_at: route.created_at,
    }
}
