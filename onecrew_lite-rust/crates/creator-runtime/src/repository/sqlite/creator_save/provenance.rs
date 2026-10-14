//! Versioned observation-source identity and complete historical provenance.
use super::fingerprint;
use crate::*;
use rusqlite::{Connection, OptionalExtension, params};

pub(in super::super) fn source_metadata(
    provenance: &CreatorSaveProvenance,
) -> Result<serde_json::Value, String> {
    match provenance {
        CreatorSaveProvenance::Discovery {
            candidate_id,
            origin,
            ..
        } => Ok(origin_metadata(origin, candidate_id)),
        _ => Ok(serde_json::json!({ "creator_save_v1": provenance })),
    }
}

pub(super) fn source_ref(
    account: &PlatformAccountCandidate,
    provenance: &CreatorSaveProvenance,
) -> Result<String, String> {
    match provenance {
        CreatorSaveProvenance::Discovery {
            candidate_id,
            origin,
            ..
        } => Ok(format!(
            "candidate:{}:{}",
            origin.discovery_id().as_str(),
            candidate_id
        )),
        _ => Ok(format!(
            "creator-save:v1:{}",
            fingerprint(&(
                account.platform,
                &account.external_id,
                account.target_kind,
                provenance
            ))?
        )),
    }
}

/// Preserve selection evidence separately from the immutable historical
/// discovery source. The existing locator-observation relation exposes this
/// context through normal Creator readback without changing old source bytes.
pub(super) fn discovery_selection_source(
    account: &PlatformAccountCandidate,
    provenance: &CreatorSaveProvenance,
) -> Result<Option<ObservationSourceInput>, String> {
    let CreatorSaveProvenance::Discovery {
        origin,
        selection_context: Some(context),
        ..
    } = provenance
    else {
        return Ok(None);
    };
    Ok(Some(ObservationSourceInput {
        kind: SourceKind::InitialPlatformAccount,
        platform: Some(account.platform),
        source_url: context.source_url.clone(),
        external_ref: Some(format!(
            "creator-discovery-selection:v1:{}",
            fingerprint(&(
                account.platform,
                &account.external_id,
                account.target_kind,
                provenance
            ))?
        )),
        account_ref_id: Some(origin.account_ref_id().clone()),
        metadata: serde_json::json!({"creator_discovery_selection_v1": provenance}),
    }))
}

pub(in super::super) fn contact_source_ref(
    account: &PlatformAccountCandidate,
    provenance: &CreatorSaveProvenance,
    observation: &DiscoveredContactRoute,
) -> Result<String, String> {
    match provenance {
        CreatorSaveProvenance::Discovery {
            candidate_id,
            origin,
            ..
        } => Ok(format!(
            "contact:{}:{}:{}:{}",
            origin.discovery_id().as_str(),
            observation.source.as_str(),
            observation.source_url.as_deref().unwrap_or("inline"),
            candidate_id
        )),
        _ => Ok(format!(
            "contact:{}:{}:{}",
            source_ref(account, provenance)?,
            observation.source.as_str(),
            observation.source_url.as_deref().unwrap_or("inline")
        )),
    }
}

pub(super) fn validate_source(
    connection: &Connection,
    input: &CanonicalCreatorSaveInput,
) -> Result<(), CreatorSavePreparationError> {
    let invalid_source = || CreatorSavePreparationError {
        row_id: Some(input.row_id.clone()),
        kind: CreatorSavePreparationErrorKind::InvalidSource,
    };
    let storage = || CreatorSavePreparationError {
        row_id: Some(input.row_id.clone()),
        kind: CreatorSavePreparationErrorKind::Storage,
    };
    if let Some(account_ref) = input.provenance.account_ref() {
        let platform = connection
            .query_row(
                "SELECT platform FROM account_refs WHERE id = ?1",
                [account_ref.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| storage())?
            .ok_or_else(invalid_source)?;
        if platform != input.account.platform.as_str() {
            return Err(invalid_source());
        }
    } else if input.account.platform != Platform::YouTube {
        return Err(invalid_source());
    }
    match &input.provenance {
        CreatorSaveProvenance::Discovery {
            candidate_id,
            origin,
            selection_context,
        } => {
            if candidate_id.trim().is_empty() || origin.discovery_id().as_str().trim().is_empty() {
                return Err(invalid_source());
            }
            if selection_context.as_ref().is_some_and(|context| {
                context.operation_id.trim().is_empty() || context.source_ref.trim().is_empty()
            }) {
                return Err(invalid_source());
            }
        }
        CreatorSaveProvenance::DirectSelection { source_ref, .. }
        | CreatorSaveProvenance::ProviderRead { source_ref, .. } => {
            if source_ref.trim().is_empty() {
                return Err(invalid_source());
            }
            if let CreatorSaveProvenance::ProviderRead { operation_id, .. } = &input.provenance
                && operation_id.trim().is_empty()
            {
                return Err(invalid_source());
            }
        }
        CreatorSaveProvenance::AdministratorSelection { source_ref, .. }
            if source_ref.trim().is_empty() =>
        {
            return Err(invalid_source());
        }
        _ => {}
    }
    if let Some((community, source)) = input.provenance.administrator() {
        if input.account.platform != Platform::Telegram
            || input.account.target_kind != Some(TargetKind::User)
        {
            return Err(invalid_source());
        }
        if community.external_id.trim().is_empty()
            || community.title.trim().is_empty()
            || !matches!(community.target_kind.as_str(), "channel" | "supergroup")
        {
            return Err(invalid_source());
        }
        if let CommunityAdministratorSource::CreatorContactRoute {
            source_creator_id,
            source_contact_route_id,
            operator_account_ref_id,
            source_resolved_target_id,
        } = source
        {
            let route_id = source_contact_route_id
                .as_ref()
                .ok_or_else(invalid_source)?;
            let valid = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM creator_contact_routes route
                 JOIN creators creator ON creator.id = route.creator_id
                 JOIN locator_resolutions resolution ON resolution.locator_id = route.locator_id AND resolution.account_ref_id = ?3 AND resolution.state = 'active'
                 JOIN resolved_targets target ON target.id = resolution.resolved_target_id AND target.account_ref_id = ?3
                 WHERE route.id = ?1 AND route.creator_id = ?2 AND creator.archived_at IS NULL AND target.id = ?4 AND target.platform = 'telegram' AND target.target_kind = ?5 AND target.external_target_id = ?6)",
                params![route_id.as_str(), source_creator_id.as_str(), operator_account_ref_id.as_str(), source_resolved_target_id.as_str(), community.target_kind.as_str(), community.external_id], |row| row.get::<_, bool>(0)).map_err(|_| storage())?;
            if !valid {
                return Err(invalid_source());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_identity(
    input: &CanonicalCreatorSaveInput,
    identity: &IdentityResolution,
) -> Result<(), String> {
    if matches!(
        identity,
        IdentityResolution::PossibleMatch { .. } | IdentityResolution::ConflictingMatches { .. }
    ) {
        return Err("resolve ambiguous Creator identity before saving".into());
    }
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

pub(super) fn origin_metadata(
    origin: &CreatorCandidateOrigin,
    candidate_id: &str,
) -> serde_json::Value {
    match origin {
        CreatorCandidateOrigin::PlatformSearch {
            discovery_id,
            account_ref_id,
            query,
            search_kind,
        } => serde_json::json!({
            "candidate_id": candidate_id,
            "discovery_id": discovery_id,
            "account_ref_id": account_ref_id,
            "query": query,
            "search_kind": search_kind,
        }),
        CreatorCandidateOrigin::CommunityAdministrator {
            discovery_id,
            role,
            community,
            source,
        } => serde_json::json!({
            "candidate_id": candidate_id,
            "discovery_id": discovery_id,
            "role": role,
            "community": community,
            "source": source,
        }),
    }
}
