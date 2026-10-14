use onecrew_creator_runtime::{
    ContactRouteCollision, CreatorSavePreparedRow, IdentityResolution, SourceKind,
};

mod copy;

use crate::{
    CreatorSaveBatchReview, CreatorSaveBatchRow, CreatorSaveCollision, CreatorSaveCollisionStatus,
    CreatorSavePreview, CreatorSaveRoute, NoticeTone, PresentationField, PresentationProjection,
    PresentationSection, ResultPresentationKind, ResultStoreError,
    interaction::{
        MAX_PRESENTATION_PREVIEW_BYTES, MAX_PRESENTATION_PREVIEW_ROUTES,
        MAX_PRESENTATION_TEXT_BYTES,
    },
};

/// Bound one projected value exactly the way the result store's validator will:
/// trim first, then truncate on a character boundary with a visible marker.
/// A provider's whitespace-only bio or oversized description has to cost that
/// one field, not the whole card and every provider read behind it.
pub(super) fn bounded(value: &str) -> Option<String> {
    const MARKER: &str = "… (truncated)";
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.len() <= MAX_PRESENTATION_TEXT_BYTES {
        return Some(value.to_owned());
    }
    let mut end = MAX_PRESENTATION_TEXT_BYTES - MARKER.len();
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    Some(format!("{}{MARKER}", &value[..end]))
}

pub(super) fn outcome_text(
    locale: crate::PresentationLocale,
    rows: &[crate::CreatorSaveBatchOutcome],
) -> (String, String) {
    let counts = copy::CreatorSaveCounts::from_rows(rows);
    (
        copy::creator_save_title(locale).to_owned(),
        copy::creator_save_batch_message(locale, &counts),
    )
}

/// Project the prepared rows and report which Creators had contact routes left
/// out to keep their preview presentable.
pub(super) fn review(
    rows: &[CreatorSavePreparedRow],
    intent: onecrew_creator_runtime::CreatorSaveIntent,
    summary: onecrew_creator_runtime::CreatorSaveBatchSummary,
    failures: &[crate::CreatorSavePreparedFailure],
    expires_at: u64,
) -> (CreatorSaveBatchReview, Vec<String>) {
    let mut trimmed = Vec::new();
    let rows = rows
        .iter()
        .map(|row| {
            let (identity_status, identity_creator_ids) = match &row.identity_resolution {
                IdentityResolution::New => ("new", Vec::new()),
                IdentityResolution::ExactExistingCreator { creator_id } => (
                    "exact_existing_creator",
                    vec![creator_id.as_str().to_owned()],
                ),
                IdentityResolution::PossibleMatch { creator_ids } => (
                    "possible_match",
                    creator_ids
                        .iter()
                        .map(|id| id.as_str().to_owned())
                        .collect(),
                ),
                IdentityResolution::ConflictingMatches { creator_ids } => (
                    "conflicting_matches",
                    creator_ids
                        .iter()
                        .map(|id| id.as_str().to_owned())
                        .collect(),
                ),
            };
            let (preview, dropped) =
                preview(row, identity_status, identity_creator_ids, expires_at);
            if dropped {
                trimmed
                    .push(bounded(&row.account.display_name).unwrap_or_else(|| row.row_id.clone()));
            }
            CreatorSaveBatchRow {
                row_id: row.row_id.clone(),
                preview: Box::new(preview),
            }
        })
        .collect();
    (
        CreatorSaveBatchReview {
            rows,
            failures: failures.to_vec(),
            intent,
            summary,
        },
        trimmed,
    )
}

/// Build one row's preview within the bounds its validator enforces, dropping
/// optional routes rather than letting a single oversized row reject the whole
/// review. A required route is never dropped: confirmation consumes it.
/// LinkHub-derived routes are dropped first because they are the only routes
/// that require another route to be confirmed with them, so removing them can
/// never orphan a route that stays.
fn preview(
    row: &CreatorSavePreparedRow,
    identity_status: &str,
    identity_creator_ids: Vec<String>,
    expires_at: u64,
) -> (CreatorSavePreview, bool) {
    let mut order: Vec<usize> = (0..row.route_options.len()).collect();
    order.sort_by_key(|index| {
        let route = &row.route_options[*index];
        if route.required {
            0
        } else if route.sources.contains(&SourceKind::LinkHub) {
            2
        } else {
            1
        }
    });
    let mut dropped = order.len() > MAX_PRESENTATION_PREVIEW_ROUTES;
    order.truncate(MAX_PRESENTATION_PREVIEW_ROUTES);
    loop {
        let current = build_preview(
            row,
            identity_status,
            &identity_creator_ids,
            &order,
            expires_at,
        );
        let within = serde_json::to_vec(&current)
            .is_ok_and(|bytes| bytes.len() <= MAX_PRESENTATION_PREVIEW_BYTES);
        if within
            || order
                .last()
                .is_none_or(|index| row.route_options[*index].required)
        {
            return (current, dropped);
        }
        order.pop();
        dropped = true;
    }
}

fn build_preview(
    row: &CreatorSavePreparedRow,
    identity_status: &str,
    identity_creator_ids: &[String],
    order: &[usize],
    expires_at: u64,
) -> CreatorSavePreview {
    // Retained routes keep preparation order; only which routes survive changed.
    let mut retained = order.to_vec();
    retained.sort_unstable();
    CreatorSavePreview {
        display_name: bounded(&row.account.display_name),
        platform: row.account.platform,
        handle: row.account.handle.clone(),
        profile_url: row.account.profile_url.clone(),
        source_url: selection_source_url(&row.provenance),
        avatar_url: row.profile.avatar_url.clone(),
        identity_status: identity_status.into(),
        identity_creator_ids: identity_creator_ids.to_vec(),
        intent: row.intent,
        status: row.status,
        reason: row.reason,
        notes: row.notes.as_deref().and_then(bounded),
        score: row.quality.score,
        assessment: row.quality.assessment.as_deref().and_then(bounded),
        follower_counts: row
            .quality
            .follower_counts
            .iter()
            .map(|(platform, count)| (platform.as_str().to_owned(), *count))
            .collect(),
        routes: retained
            .into_iter()
            .map(|index| {
                let route = &row.route_options[index];
                CreatorSaveRoute {
                    route_id: route.route_id.clone(),
                    channel: route.channel.as_str().to_owned(),
                    kind: route.kind.as_str().to_owned(),
                    display_value: route.display_value.clone(),
                    normalized_value: route.normalized_value.clone(),
                    sources: route
                        .sources
                        .iter()
                        .map(|source| source.as_str().to_owned())
                        .collect(),
                    required: route.required,
                    collision: CreatorSaveCollision {
                        status: match route.collision {
                            ContactRouteCollision::None => CreatorSaveCollisionStatus::None,
                            ContactRouteCollision::Possible { .. } => {
                                CreatorSaveCollisionStatus::Possible
                            }
                            ContactRouteCollision::Conflict { .. } => {
                                CreatorSaveCollisionStatus::Conflict
                            }
                        },
                        creator_ids: route
                            .collision
                            .creator_ids()
                            .iter()
                            .map(|id| id.as_str().to_owned())
                            .collect(),
                    },
                }
            })
            .collect(),
        expires_at,
    }
}

fn selection_source_url(
    provenance: &onecrew_creator_runtime::CreatorSaveProvenance,
) -> Option<String> {
    use onecrew_creator_runtime::CreatorSaveProvenance;
    match provenance {
        CreatorSaveProvenance::DirectSelection { source_url, .. }
        | CreatorSaveProvenance::ProviderRead { source_url, .. } => source_url.clone(),
        CreatorSaveProvenance::Discovery {
            selection_context, ..
        } => selection_context
            .as_ref()
            .and_then(|context| context.source_url.clone()),
        CreatorSaveProvenance::AdministratorSelection { .. } => None,
    }
}

pub(super) fn prepared(
    review: &CreatorSaveBatchReview,
    warnings: &[String],
) -> Result<PresentationProjection, ResultStoreError> {
    Ok(PresentationProjection {
        kind: ResultPresentationKind::new("creator_save_batch_confirmation")?,
        title: "Review Creator save".into(),
        summary: Some(format!(
            "Review {} freshly read selections, choose contact routes, and confirm to save.",
            review.rows.len()
        )),
        sections: warnings
            .iter()
            .map(|warning| PresentationSection::Notice {
                heading: Some("Contact verification".into()),
                body: warning.clone(),
                tone: NoticeTone::Warning,
            })
            .collect(),
    })
}

pub(super) fn details(rows: &[CreatorSavePreparedRow]) -> Vec<PresentationSection> {
    rows.iter()
        .map(|row| {
            let mut fields = Vec::new();
            for (label, value) in [
                ("Description", row.profile.description.as_deref()),
                ("Location", row.profile.location.as_deref()),
            ] {
                if let Some(value) = value.and_then(bounded) {
                    fields.push(PresentationField {
                        label: label.into(),
                        value,
                    });
                }
            }
            if let Some(value) = bounded(&row.profile.website_urls.join("\n")) {
                fields.push(PresentationField {
                    label: "Websites".into(),
                    value,
                });
            }
            if let Some(tags) = &row.tags
                && let Some(value) = bounded(&if tags.is_empty() {
                    "Clear tags".to_owned()
                } else {
                    tags.join(", ")
                })
            {
                fields.push(PresentationField {
                    label: "Tags".into(),
                    value,
                });
            }
            // A Fields section must carry at least one field, so this row keeps
            // a source line even when the source text itself is unusable.
            fields.push(PresentationField {
                label: "Selection source".into(),
                value: bounded(&selection_source(row))
                    .unwrap_or_else(|| "Unnamed selection source".into()),
            });
            PresentationSection::Fields {
                heading: bounded(&row.account.display_name),
                fields,
            }
        })
        .collect()
}

fn selection_source(row: &CreatorSavePreparedRow) -> String {
    match &row.provenance {
        onecrew_creator_runtime::CreatorSaveProvenance::Discovery { .. } => {
            "Original discovery selection".into()
        }
        onecrew_creator_runtime::CreatorSaveProvenance::DirectSelection { source_ref, .. } => {
            source_ref.clone()
        }
        onecrew_creator_runtime::CreatorSaveProvenance::ProviderRead {
            operation_id,
            source_ref,
            ..
        } => format!("{operation_id}: {source_ref}"),
        onecrew_creator_runtime::CreatorSaveProvenance::AdministratorSelection {
            community,
            ..
        } => format!("{} ({})", community.title, community.external_id),
    }
}
