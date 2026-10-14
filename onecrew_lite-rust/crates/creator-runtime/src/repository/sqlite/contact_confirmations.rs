//! Save-time persistence of confirmed contact routes: the locator, its
//! Creator link, every observation's provenance, and the link-hub dependency
//! edges between a confirmed hub route and the routes found on it.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{OptionalExtension, Transaction, params};

use crate::{
    ContactChannel, ContactLocatorInput, ContactRouteConfirmation,
    CreatorContactRouteDependencyKind, CreatorContactRouteId, CreatorId, LocatorKind,
    ObservationSourceInput, PlatformAccountId, SourceKind,
};

use super::contacts;

pub(super) fn persist_canonical_contact_confirmations_in(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    platform_account_id: Option<&PlatformAccountId>,
    account: &crate::PlatformAccountCandidate,
    provenance: &crate::CreatorSaveProvenance,
    confirmations: &[ContactRouteConfirmation],
) -> Result<BTreeMap<String, CreatorContactRouteId>, String> {
    let mut confirmed_keys = BTreeSet::new();
    let mut persisted_routes = BTreeMap::new();
    for confirmation in confirmations {
        let locator = validate_confirmation(confirmation)?;
        let locator_key = crate::contact_routes::locator_key(&locator);
        if !confirmed_keys.insert(locator_key.clone()) {
            return Err("the same contact locator was confirmed more than once".into());
        }

        let current_owners = contacts::find_contact_route_owners_in(transaction, &locator)?;
        let current_owner_ids = sorted_owner_ids(
            current_owners
                .iter()
                .map(|record| record.route.creator_id.as_str()),
        );
        let preview_owner_ids = sorted_owner_ids(
            confirmation
                .preview
                .collision
                .creator_ids()
                .iter()
                .map(CreatorId::as_str),
        );
        let current_external_owners = current_owner_ids
            .iter()
            .filter(|owner_id| owner_id.as_str() != creator_id.as_str())
            .cloned()
            .collect::<Vec<_>>();
        let preview_external_owners = preview_owner_ids
            .iter()
            .filter(|owner_id| owner_id.as_str() != creator_id.as_str())
            .cloned()
            .collect::<Vec<_>>();
        if current_external_owners != preview_external_owners {
            return Err("contact route collision preview is stale; discover again".into());
        }
        if !current_external_owners.is_empty() {
            return Err("contact route already belongs to another Creator".into());
        }

        let locator = contacts::upsert_contact_locator_in(transaction, &locator)?;
        let route = contacts::link_creator_contact_route_in(
            transaction,
            creator_id,
            &locator.id,
            confirmation.confidence,
        )?;
        persisted_routes.insert(locator_key, route.id);
        for observation in &confirmation.preview.observations {
            let source = contacts::upsert_observation_source_in(
                transaction,
                &ObservationSourceInput {
                    kind: observation.source,
                    platform: Some(account.platform),
                    source_url: observation.source_url.clone(),
                    external_ref: Some(super::creator_save::contact_source_ref(
                        account,
                        provenance,
                        observation,
                    )?),
                    account_ref_id: provenance.account_ref().cloned(),
                    metadata: super::creator_save::source_metadata(provenance)?,
                },
            )?;
            contacts::append_locator_observation_in(
                transaction,
                &locator.id,
                &source.id,
                Some(creator_id),
                platform_account_id,
                &observation.value,
            )?;
        }
    }

    persist_link_hub_dependencies(transaction, creator_id, confirmations, &persisted_routes)?;
    Ok(persisted_routes)
}

/// Resolve a LinkHub/Website parent route this Creator already owns.
pub(super) fn saved_parent_route(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    parent_url: &str,
) -> Result<Option<CreatorContactRouteId>, String> {
    transaction
        .query_row(
            "SELECT route.id
             FROM creator_contact_routes route
             JOIN contact_locators locator ON locator.id = route.locator_id
             WHERE route.creator_id = ?1
               AND locator.kind = 'url'
               AND locator.channel IN ('link_hub', 'website')
               AND locator.normalized_value = ?2",
            params![creator_id.as_str(), parent_url],
            |row| row.get::<_, String>(0).map(CreatorContactRouteId::new),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn persist_link_hub_dependencies(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    confirmations: &[ContactRouteConfirmation],
    persisted_routes: &BTreeMap<String, CreatorContactRouteId>,
) -> Result<(), String> {
    let mut parent_routes = BTreeMap::<String, CreatorContactRouteId>::new();
    for confirmation in confirmations {
        let locator = &confirmation.preview.locator;
        if matches!(
            locator.channel,
            ContactChannel::LinkHub | ContactChannel::Website
        ) && locator.kind == LocatorKind::Url
        {
            let route_id = persisted_routes
                .get(&crate::contact_routes::locator_key(locator))
                .ok_or_else(|| {
                    "persisted contact route is missing from confirmation set".to_string()
                })?;
            parent_routes.insert(locator.normalized_value.clone(), route_id.clone());
        }
    }

    let mut edges = BTreeSet::new();
    for confirmation in confirmations {
        let child_route_id = persisted_routes
            .get(&crate::contact_routes::locator_key(
                &confirmation.preview.locator,
            ))
            .ok_or_else(|| {
                "persisted contact route is missing from confirmation set".to_string()
            })?;
        for observation in &confirmation.preview.observations {
            if observation.source != SourceKind::LinkHub {
                continue;
            }
            let source_url = observation.source_url.as_deref().ok_or_else(|| {
                "LinkHub-derived contact route is missing its parent URL".to_string()
            })?;
            let parent_url =
                crate::contact_routes::normalize_url(source_url, true)?.normalized_value;
            let parent_route_id = match parent_routes.get(&parent_url) {
                Some(parent_route_id) => Some(parent_route_id.clone()),
                // An incrementally confirmed route may derive from a LinkHub
                // route this Creator saved earlier; resolve the parent from
                // persisted state before failing.
                None => saved_parent_route(transaction, creator_id, &parent_url)?,
            };
            let parent_route_id = parent_route_id.ok_or_else(|| {
                "LinkHub-derived contact route requires its parent LinkHub route to be saved"
                    .to_string()
            })?;
            if &parent_route_id == child_route_id {
                return Err("LinkHub contact route cannot derive itself".into());
            }
            if edges.insert((
                parent_route_id.as_str().to_owned(),
                child_route_id.as_str().to_owned(),
            )) {
                contacts::link_creator_contact_route_dependency_in(
                    transaction,
                    &parent_route_id,
                    child_route_id,
                    CreatorContactRouteDependencyKind::LinkHub,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_confirmation(
    confirmation: &ContactRouteConfirmation,
) -> Result<ContactLocatorInput, String> {
    if confirmation.preview.observations.is_empty() {
        return Err("contact route confirmation has no observations".into());
    }
    let expected = &confirmation.preview.locator;
    let normalized = confirmation
        .preview
        .observations
        .iter()
        .map(crate::contact_routes::normalize_discovered_contact_route)
        .collect::<Result<Vec<_>, _>>()?;
    if normalized
        .iter()
        .any(|locator| !same_locator(locator, expected))
    {
        return Err("contact route observations do not match the preview locator".into());
    }
    let canonical = normalized
        .into_iter()
        .next()
        .expect("non-empty observations were validated");
    if !same_locator(&canonical, expected) {
        return Err("contact route preview is not canonically normalized".into());
    }
    Ok(canonical)
}

fn same_locator(left: &ContactLocatorInput, right: &ContactLocatorInput) -> bool {
    left.channel == right.channel
        && left.kind == right.kind
        && left
            .normalized_value
            .trim()
            .eq_ignore_ascii_case(right.normalized_value.trim())
}

fn sorted_owner_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut ids = ids.map(str::to_owned).collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}
