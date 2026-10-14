//! Read-only identity, source, route and dependency preparation.
use super::super::SqliteRepository;
use super::persistence as facts;
use super::provenance::validate_source;
use crate::creators::save::PreparedSaveRow;
use crate::*;
use rusqlite::{Transaction, TransactionBehavior};
use std::collections::{BTreeMap, BTreeSet};

fn observations(input: &CanonicalCreatorSaveInput) -> Result<Vec<DiscoveredContactRoute>, String> {
    use crate::contact_routes::extraction::{
        CreatorContactProfileField as Field, CreatorContactProfileLink as Link,
        discover_creator_contacts,
    };
    let account = &input.account;
    account
        .profile_url
        .as_deref()
        .ok_or("Creator profile URL is required")?;
    let (channel, kind, value) = match account.platform {
        Platform::X => (
            ContactChannel::X,
            LocatorKind::Handle,
            account.handle.clone(),
        ),
        Platform::Instagram => (
            ContactChannel::Instagram,
            LocatorKind::Handle,
            account.handle.clone(),
        ),
        Platform::Telegram => (
            ContactChannel::Telegram,
            LocatorKind::Handle,
            account.handle.clone(),
        ),
        Platform::TikTok => (
            ContactChannel::TikTok,
            LocatorKind::Handle,
            account.handle.clone(),
        ),
        Platform::YouTube => (
            ContactChannel::YouTube,
            LocatorKind::Url,
            account.profile_url.clone(),
        ),
    };
    let mut result = vec![DiscoveredContactRoute {
        channel,
        kind,
        value: value.ok_or("initial platform contact route is required")?,
        source: SourceKind::InitialPlatformAccount,
        source_url: account.profile_url.clone(),
    }];
    let mut fields = Vec::new();
    let mut links = Vec::new();
    for evidence in &input.contact_evidence {
        match evidence {
            CreatorSaveContactEvidence::Observation {
                channel,
                kind,
                value,
                source,
                source_url,
            } => {
                if !matches!(
                    source,
                    SourceKind::ProfileBio
                        | SourceKind::ProfileWebsite
                        | SourceKind::ProfileLocation
                        | SourceKind::VideoDescription
                        | SourceKind::ChannelDescription
                        | SourceKind::LinkHub
                        | SourceKind::ManualBusinessEmail
                ) {
                    return Err("unsupported public contact observation source".into());
                }
                if (crate::contact_routes::extraction::email_only_source(account.platform, *source)
                    && *channel != ContactChannel::Email)
                    || crate::contact_routes::extraction::is_image_name(value)
                    || (matches!(kind, LocatorKind::Url | LocatorKind::InviteLink)
                        && crate::contact_routes::extraction::classify_contact_route(value)
                            .is_none())
                {
                    continue;
                }
                result.push(DiscoveredContactRoute {
                    channel: *channel,
                    kind: *kind,
                    value: value.clone(),
                    source: *source,
                    source_url: source_url.clone(),
                });
            }
            CreatorSaveContactEvidence::ProfileField {
                source,
                source_url,
                text,
            } => {
                if !matches!(
                    source,
                    SourceKind::ProfileBio
                        | SourceKind::ProfileLocation
                        | SourceKind::ChannelDescription
                        | SourceKind::VideoDescription
                        | SourceKind::ProfileWebsite
                ) {
                    return Err("unsupported public profile evidence source".into());
                }
                let field = Field {
                    source: *source,
                    source_url: source_url.as_deref(),
                    text,
                };
                if !fields.contains(&field) {
                    fields.push(field);
                }
            }
            CreatorSaveContactEvidence::ProfileLink {
                source,
                source_url,
                display_text,
                target_url,
            } => {
                if !matches!(
                    source,
                    SourceKind::ProfileBio
                        | SourceKind::ProfileWebsite
                        | SourceKind::ChannelDescription
                ) {
                    return Err("unsupported public profile link source".into());
                }
                let link = Link {
                    source: *source,
                    source_url: source_url.as_deref(),
                    display_text,
                    target_url,
                };
                if !links.contains(&link) {
                    links.push(link);
                }
            }
            CreatorSaveContactEvidence::HeldHub {
                source_url,
                text,
                links,
            } => {
                let hub_links: Vec<_> = links
                    .iter()
                    .map(|link| Link {
                        source: SourceKind::LinkHub,
                        source_url: Some(source_url),
                        display_text: &link.display_text,
                        target_url: &link.target_url,
                    })
                    .collect();
                result.extend(
                    discover_creator_contacts(
                        account.platform,
                        &[Field {
                            source: SourceKind::LinkHub,
                            source_url: Some(source_url),
                            text,
                        }],
                        &hub_links,
                    )
                    .into_iter()
                    .map(crate::contact_routes::discovered_observation),
                );
            }
        }
    }
    result.extend(
        discover_creator_contacts(account.platform, &fields, &links)
            .into_iter()
            .map(crate::contact_routes::discovered_observation),
    );
    Ok(result)
}

pub(in super::super) fn prepare_entries(
    repository: &SqliteRepository,
    inputs: Vec<CanonicalCreatorSaveInput>,
    intent: CreatorSaveIntent,
) -> Result<PreparedCreatorSaveBatch, CreatorSavePreparationError> {
    use CreatorSavePreparationErrorKind as Failure;
    let batch_error = |kind| CreatorSavePreparationError { row_id: None, kind };
    if inputs.is_empty() || inputs.len() > MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS {
        return Err(batch_error(Failure::InvalidFields));
    }
    if serde_json::to_vec(&inputs)
        .map_err(|_| batch_error(Failure::InvalidFields))?
        .len()
        > MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES
    {
        return Err(batch_error(Failure::InvalidFields));
    }
    let mut connection = repository
        .read_connection()
        .map_err(|_| batch_error(Failure::Storage))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|_| batch_error(Failure::Storage))?;
    let mut row_ids = BTreeSet::new();
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for input in inputs {
        let row_id = input.row_id.clone();
        let failure_context = (
            input.account.platform,
            input.account.display_name.clone(),
            input.account.handle.clone(),
            input.account.profile_url.clone(),
        );
        // Row identity is a batch precondition rather than one row's data
        // problem: a per-row failure has to name the row it belongs to, and a
        // duplicate identifier makes every later route selection ambiguous.
        if input.row_id.trim().is_empty()
            || input.row_id.len() > 256
            || !row_ids.insert(input.row_id.clone())
        {
            return Err(CreatorSavePreparationError {
                row_id: Some(row_id),
                kind: Failure::InvalidFields,
            });
        }
        match prepare_row(&transaction, input, intent) {
            Ok(row) => rows.push(row),
            // An unavailable repository is not this Creator's fault. Degrading
            // it per row would report fifty bad Creators when one shared
            // precondition did not hold.
            Err(Failure::Storage) => {
                return Err(CreatorSavePreparationError {
                    row_id: Some(row_id),
                    kind: Failure::Storage,
                });
            }
            Err(kind) => failures.push(CreatorSavePreparationRowFailure {
                row_id,
                kind,
                platform: failure_context.0,
                display_name: failure_context.1,
                handle: failure_context.2,
                profile_url: failure_context.3,
            }),
        }
    }
    // Nothing prepared means there is no review to confirm. Report the first
    // rejection as this batch's failure instead of minting an empty card.
    if rows.is_empty()
        && let Some(failure) = failures.first()
    {
        return Err(CreatorSavePreparationError {
            row_id: Some(failure.row_id.clone()),
            kind: failure.kind,
        });
    }
    transaction
        .commit()
        .map_err(|_| batch_error(Failure::Storage))?;
    rows = mark_batch_duplicates(&rows, intent);
    Ok(PreparedCreatorSaveBatch {
        rows,
        failures,
        intent,
    })
}

/// Prepare one row against the shared read transaction. Each rejection names
/// only its safe category; the caller decides which categories end the batch.
fn prepare_row(
    transaction: &Transaction<'_>,
    mut input: CanonicalCreatorSaveInput,
    intent: CreatorSaveIntent,
) -> Result<PreparedSaveRow, CreatorSavePreparationErrorKind> {
    use CreatorSavePreparationErrorKind as Failure;
    if input.account.platform == Platform::Telegram
        && let Err(reason) = crate::require_telegram_administrator_eligibility(&input.account)
    {
        return Err(Failure::TelegramEligibility(reason));
    }
    facts::validate_candidate(&input.account).map_err(|_| Failure::InvalidFields)?;
    if input.profile_observed_at == 0 || input.profile_observed_at > i64::MAX as u64 {
        return Err(Failure::InvalidFields);
    }
    if input.account.display_name.trim().is_empty() {
        return Err(Failure::InvalidFields);
    }
    if input.account.platform == Platform::TikTok
        && input
            .account
            .external_id
            .as_deref()
            .is_none_or(|id| !id.starts_with("tto:") || id.len() <= 4)
    {
        return Err(Failure::Identity);
    }
    input.tags = input
        .tags
        .map(crate::creators::normalize_creator_tags)
        .transpose()
        .map_err(Failure::Tags)?;
    input.quality = input
        .quality
        .normalized()
        .map_err(|_| Failure::InvalidFields)?;
    if let Some(count) = input.profile.follower_count {
        input
            .quality
            .follower_counts
            .insert(input.account.platform, count);
    }
    if input
        .notes
        .as_deref()
        .is_some_and(|notes| notes.trim().is_empty())
    {
        return Err(Failure::InvalidFields);
    }
    validate_source(transaction, &input).map_err(|error| error.kind)?;
    let identity = super::super::identity::resolve_creator_identity(transaction, &input.account)
        .map_err(|_| Failure::Storage)?;
    validate_identity_for_prepare(&input, &identity).map_err(|_| Failure::Identity)?;
    if let IdentityResolution::ExactExistingCreator { creator_id } = &identity {
        facts::ensure_active_creator(transaction, creator_id).map_err(|_| Failure::Identity)?;
    }
    let mut grouped = BTreeMap::<String, ContactRoutePreview>::new();
    for observation in observations(&input).map_err(|_| Failure::ContactNormalization)? {
        let locator = crate::contact_routes::normalize_discovered_contact_route(&observation)
            .map_err(|_| Failure::ContactNormalization)?;
        let key = crate::contact_routes::locator_key(&locator);
        let entry = grouped.entry(key).or_insert(ContactRoutePreview {
            locator,
            observations: Vec::new(),
            collision: ContactRouteCollision::None,
        });
        if !entry.observations.iter().any(|current| {
            current.source == observation.source
                && current.source_url == observation.source_url
                && current.value == observation.value
        }) {
            entry.observations.push(observation);
        }
    }
    let mut routes = Vec::new();
    let mut options = Vec::new();
    for (index, mut preview) in grouped.into_values().enumerate() {
        let mut creator_ids =
            super::super::contacts::find_contact_route_owners_in(transaction, &preview.locator)
                .map_err(|_| Failure::Storage)?
                .into_iter()
                .map(|record| record.route.creator_id)
                .collect::<Vec<_>>();
        creator_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        creator_ids.dedup();
        if let IdentityResolution::ExactExistingCreator { creator_id } = &identity {
            creator_ids.retain(|owner_id| owner_id != creator_id);
        }
        preview.collision = match creator_ids.len() {
            0 => ContactRouteCollision::None,
            1 => ContactRouteCollision::Possible { creator_ids },
            _ => ContactRouteCollision::Conflict { creator_ids },
        };
        let route_id = format!("route-{index}");
        options.push(CreatorCandidateSaveRouteOption {
            route_id: route_id.clone(),
            channel: preview.locator.channel,
            kind: preview.locator.kind,
            normalized_value: preview.locator.normalized_value.clone(),
            display_value: preview.locator.display_value.clone(),
            sources: preview
                .observations
                .iter()
                .map(|observation| observation.source)
                .collect(),
            collision: preview.collision.clone(),
            required: preview
                .observations
                .iter()
                .any(|observation| observation.source == SourceKind::InitialPlatformAccount),
        });
        routes.push((route_id, preview));
    }
    let has_required_collision = options
        .iter()
        .any(|route| route.required && !matches!(route.collision, ContactRouteCollision::None));
    let (status, reason) = prepared_status(intent, &identity, has_required_collision);
    let mut required_parent_routes = BTreeMap::<String, Vec<String>>::new();
    for (route_id, route) in &routes {
        for observation in &route.observations {
            if observation.source != SourceKind::LinkHub {
                continue;
            }
            let parent_url = crate::contact_routes::normalize_url(
                observation
                    .source_url
                    .as_deref()
                    .ok_or(Failure::ContactNormalization)?,
                true,
            )
            .map_err(|_| Failure::ContactNormalization)?
            .normalized_value;
            if let IdentityResolution::ExactExistingCreator { creator_id } = &identity
                && super::super::contact_confirmations::saved_parent_route(
                    transaction,
                    creator_id,
                    &parent_url,
                )
                .map_err(|_| Failure::Storage)?
                .is_some()
            {
                continue;
            }
            let parent_id = routes
                .iter()
                .find(|(_, parent)| {
                    parent.locator.kind == LocatorKind::Url
                        && matches!(
                            parent.locator.channel,
                            ContactChannel::LinkHub | ContactChannel::Website
                        )
                        && parent.locator.normalized_value == parent_url
                })
                .map(|(id, _)| id)
                .ok_or(Failure::ContactNormalization)?;
            if parent_id == route_id {
                return Err(Failure::ContactNormalization);
            }
            required_parent_routes
                .entry(route_id.clone())
                .or_default()
                .push(parent_id.clone());
        }
    }
    let preview = CreatorSavePreparedRow {
        row_id: input.row_id.clone(),
        intent,
        status,
        reason,
        account: input.account.clone(),
        profile: input.profile.clone(),
        profile_observed_at: input.profile_observed_at,
        provenance: input.provenance.clone(),
        notes: input.notes.clone(),
        tags: input.tags.clone(),
        quality: input.quality.clone(),
        identity_resolution: identity,
        route_options: options,
    };
    Ok(PreparedSaveRow {
        input,
        preview,
        routes,
        required_parent_routes,
    })
}

fn validate_identity_for_prepare(
    input: &CanonicalCreatorSaveInput,
    identity: &IdentityResolution,
) -> Result<(), String> {
    if let (
        IdentityResolution::ExactExistingCreator { creator_id },
        Some((
            _,
            CommunityAdministratorSource::CreatorContactRoute {
                source_creator_id, ..
            },
        )),
    ) = (identity, input.provenance.administrator())
        && creator_id == source_creator_id
    {
        return Err("a community administrator cannot be merged into the source Creator".into());
    }
    Ok(())
}

fn prepared_status(
    intent: CreatorSaveIntent,
    identity: &IdentityResolution,
    has_required_collision: bool,
) -> (CreatorSavePreparedStatus, Option<CreatorSavePreparedReason>) {
    if has_required_collision {
        return (
            CreatorSavePreparedStatus::ContactConflict,
            Some(CreatorSavePreparedReason::RequiredContactRouteConflict),
        );
    }
    match (intent, identity) {
        (CreatorSaveIntent::AddNew, IdentityResolution::New) => {
            (CreatorSavePreparedStatus::EligibleNew, None)
        }
        (CreatorSaveIntent::AddNew, IdentityResolution::ExactExistingCreator { .. }) => (
            CreatorSavePreparedStatus::ExistingSkipped,
            Some(CreatorSavePreparedReason::ExistingCreator),
        ),
        (CreatorSaveIntent::SaveOrUpdate, IdentityResolution::New) => {
            (CreatorSavePreparedStatus::EligibleNew, None)
        }
        (CreatorSaveIntent::SaveOrUpdate, IdentityResolution::ExactExistingCreator { .. }) => {
            (CreatorSavePreparedStatus::EligibleUpdate, None)
        }
        (
            _,
            IdentityResolution::PossibleMatch { .. }
            | IdentityResolution::ConflictingMatches { .. },
        ) => (
            CreatorSavePreparedStatus::IdentityPending,
            Some(CreatorSavePreparedReason::IdentityAmbiguous),
        ),
    }
}

fn mark_batch_duplicates(
    rows: &[PreparedSaveRow],
    intent: CreatorSaveIntent,
) -> Vec<PreparedSaveRow> {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    let mut updates =
        BTreeMap::<usize, (CreatorSavePreparedStatus, CreatorSavePreparedReason)>::new();
    for index in 0..rows.len() {
        let row = &rows[index];
        let Some(external_id) = row.input.account.external_id.as_deref() else {
            continue;
        };
        let key = format!(
            "{}:{}:{}",
            row.input.account.platform.as_str(),
            row.input
                .account
                .target_kind
                .map_or("", |kind| kind.as_str()),
            external_id,
        );
        if !matches!(
            row.preview.status,
            CreatorSavePreparedStatus::EligibleNew | CreatorSavePreparedStatus::EligibleUpdate
        ) {
            continue;
        }
        let group = groups.entry(key).or_default();
        if let Some(&representative) = group.first() {
            if intent == CreatorSaveIntent::SaveOrUpdate
                && updates_conflict_with_group(rows, group, index)
            {
                for &member in group.iter().chain(std::iter::once(&index)) {
                    updates.insert(
                        member,
                        (
                            CreatorSavePreparedStatus::IdentityPending,
                            CreatorSavePreparedReason::ConflictingUpdate,
                        ),
                    );
                }
            } else if intent == CreatorSaveIntent::AddNew
                || rows[representative].preview.status == CreatorSavePreparedStatus::EligibleNew
            {
                updates.insert(
                    index,
                    (
                        CreatorSavePreparedStatus::DuplicateSkipped,
                        CreatorSavePreparedReason::DuplicateSelection,
                    ),
                );
            }
        }
        group.push(index);
    }
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let Some((status, reason)) = updates.get(&index).copied() else {
                return row.clone();
            };
            PreparedSaveRow {
                preview: CreatorSavePreparedRow {
                    status,
                    reason: Some(reason),
                    ..row.preview.clone()
                },
                ..row.clone()
            }
        })
        .collect()
}

fn updates_conflict(left: &PreparedSaveRow, right: &PreparedSaveRow) -> bool {
    serde_json::to_vec(&(&left.input.notes, &left.input.quality)).ok()
        != serde_json::to_vec(&(&right.input.notes, &right.input.quality)).ok()
}

fn updates_conflict_with_group(
    rows: &[PreparedSaveRow],
    group: &[usize],
    candidate: usize,
) -> bool {
    group
        .iter()
        .any(|&member| updates_conflict(&rows[member], &rows[candidate]))
}
