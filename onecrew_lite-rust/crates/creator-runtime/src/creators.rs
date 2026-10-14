pub(crate) mod save;

use crate::{
    Creator, CreatorChangeScope, CreatorDeleteResult, CreatorId, CreatorQualityInput,
    CreatorRecord, CreatorRelationship, CreatorStage, DeleteCreatorInput, ManualCreatorCreateInput,
    ManualCreatorCreateResult, UpdateCreatorInput, UpdateCreatorStageInput,
};
use crate::{CreatorListPage, CreatorRepositoryCallError, ListCreatorsInput};
use sha2::Digest;
use std::sync::Arc;

pub const MAX_CREATOR_TAGS: usize = 3;
pub const MAX_CREATOR_TAG_CHARS: usize = 12;

/// A violation of the canonical Creator tag constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorTagsError {
    TooMany,
    ControlCharacters,
    InvalidLength,
    Duplicate,
}

impl std::fmt::Display for CreatorTagsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany => write!(
                formatter,
                "Creator supports at most {MAX_CREATOR_TAGS} tags"
            ),
            Self::ControlCharacters => {
                formatter.write_str("Creator tags cannot contain control characters")
            }
            Self::InvalidLength => write!(
                formatter,
                "Creator tags must contain 1 to {MAX_CREATOR_TAG_CHARS} characters"
            ),
            Self::Duplicate => formatter.write_str("Creator tags must be unique"),
        }
    }
}

impl std::error::Error for CreatorTagsError {}

/// Validate tags using their trimmed, case-sensitive values and Unicode character lengths.
///
/// # Errors
/// Returns the violated tag constraint without retaining tag content.
pub fn validate_creator_tags(tags: &[String]) -> Result<(), CreatorTagsError> {
    if tags.len() > MAX_CREATOR_TAGS {
        return Err(CreatorTagsError::TooMany);
    }
    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        if tag.chars().any(char::is_control) {
            return Err(CreatorTagsError::ControlCharacters);
        }
        let tag = tag.trim();
        if tag.is_empty() || tag.chars().count() > MAX_CREATOR_TAG_CHARS {
            return Err(CreatorTagsError::InvalidLength);
        }
        if normalized.contains(&tag) {
            return Err(CreatorTagsError::Duplicate);
        }
        normalized.push(tag);
    }
    Ok(())
}

/// Shared tag requirements for field documentation and live tool guidance.
pub fn creator_tags_description() -> String {
    format!(
        "Creator tags: at most {MAX_CREATOR_TAGS} per Creator. Trim surrounding whitespace; each tag must contain 1 to {MAX_CREATOR_TAG_CHARS} Unicode characters after trimming. Control characters are forbidden. Tags must be unique after trimming (case-sensitive)."
    )
}

/// JSON Schema for an optional canonical Creator tag list.
pub fn creator_tags_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"description":creator_tags_description(), "type":["array", "null"], "maxItems":MAX_CREATOR_TAGS, "uniqueItems":true,
        "items":{"type":"string", "minLength":1, "maxLength":MAX_CREATOR_TAG_CHARS}})
}

/// Normalize the canonical Creator tag list without truncating or changing case.
pub(crate) fn normalize_creator_tags(tags: Vec<String>) -> Result<Vec<String>, CreatorTagsError> {
    validate_creator_tags(&tags)?;
    Ok(tags.into_iter().map(|tag| tag.trim().to_owned()).collect())
}

impl crate::CreatorRuntime {
    /// List Creators through bounded read admission and the read-only pool.
    ///
    /// # Errors
    ///
    /// Returns [`CreatorRepositoryCallError::Owner`] for the existing list/query
    /// failure, or [`CreatorRepositoryCallError::Dispatch`] when admission is
    /// closed/overloaded or the blocking result is lost. This operation is
    /// read-only: even `OutcomeUnknown` carries no repository write effect.
    pub async fn list_creators_async(
        &self,
        input: ListCreatorsInput,
    ) -> Result<CreatorListPage, CreatorRepositoryCallError<String>> {
        let repository = Arc::clone(&self.repository);
        self.repository_read_segment(move || repository.list_creators(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_creators_async`] to own admission and dispatch.
    pub fn list_creators(
        &self,
        input: crate::ListCreatorsInput,
    ) -> Result<CreatorListPage, String> {
        self.repository.list_creators(input)
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn get_creator_async(
        &self,
        creator_id: CreatorId,
    ) -> Result<CreatorRecord, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_creator(&creator_id))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::get_creator_async`] to own admission and dispatch.
    pub fn get_creator(&self, creator_id: &CreatorId) -> Result<CreatorRecord, String> {
        self.repository.get_creator(creator_id)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn create_manual_creator_async(
        &self,
        input: ManualCreatorCreateInput,
    ) -> Result<ManualCreatorCreateResult, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.create_manual_creator(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::create_manual_creator_async`] to own admission and dispatch.
    pub fn create_manual_creator(
        &self,
        input: ManualCreatorCreateInput,
    ) -> Result<ManualCreatorCreateResult, String> {
        validate_manual_creator_item(&input)?;
        let display_name = input.display_name.trim();
        if display_name.is_empty() {
            return Err("creator display_name is required".into());
        }
        let notes = input
            .notes
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("creator notes cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let result = self
            .repository
            .create_manual_creator(ManualCreatorCreateInput {
                tags: input.tags,
                display_name: display_name.to_owned(),
                notes,
                contact_routes: input.contact_routes,
            })?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(result)
    }

    pub fn create_manual_creator_batch(
        &self,
        input: crate::ManualCreatorBatchCreateInput,
    ) -> Result<crate::ManualCreatorBatchCreateResult, String> {
        let key = input.idempotency_key.trim();
        if key.is_empty() || key.len() > 128 {
            return Err("idempotency_key must contain 1-128 characters".into());
        }
        if input.items.len() > crate::MAX_MANUAL_CREATOR_BATCH_ITEMS {
            return Err(format!(
                "Creator batch supports at most {} items",
                crate::MAX_MANUAL_CREATOR_BATCH_ITEMS
            ));
        }
        let payload_bytes = serde_json::to_vec(&input.items)
            .map_err(|error| format!("Creator batch payload cannot be encoded: {error}"))?;
        if payload_bytes.len() > crate::MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES {
            return Err(format!(
                "Creator batch payload exceeds {} bytes",
                crate::MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES
            ));
        }
        if input.items.is_empty() {
            return Err("Creator batch must contain at least one item".into());
        }
        if input
            .items
            .iter()
            .any(|item| item.contact_routes.len() > crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR)
        {
            return Err(format!(
                "each Creator supports at most {} initial contact routes",
                crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR
            ));
        }
        let preflight = self.preflight_manual_creator_batch(&input.items)?;
        if input.validation == crate::ManualCreatorBatchValidation::Strict
            && let Some((input_index, error)) = preflight
                .iter()
                .enumerate()
                .find_map(|(input_index, error)| error.as_deref().map(|error| (input_index, error)))
        {
            return Err(format!(
                "Creator batch item {input_index} rejected: {error}"
            ));
        }
        let fingerprint = creator_batch_fingerprint(&input)?;
        let claim = self.repository.begin_action_with_fingerprint(
            key,
            "creator_batch_create",
            &fingerprint,
        )?;
        match claim {
            crate::IdempotencyClaim::Completed(value) => {
                let mut result: crate::ManualCreatorBatchCreateResult =
                    serde_json::from_value(value).map_err(|error| error.to_string())?;
                result.replayed = true;
                return Ok(result);
            }
            crate::IdempotencyClaim::Failed(value) => {
                return Err(format!("Creator batch previously failed: {value}"));
            }
            crate::IdempotencyClaim::Acquired => {}
        }
        let mut outcomes = Vec::with_capacity(input.items.len());
        let mut valid_items = Vec::new();
        for (input_index, (item, route_error)) in input.items.into_iter().zip(preflight).enumerate()
        {
            if let Some(error) = route_error {
                outcomes.push(crate::ManualCreatorBatchItemOutcome::Rejected {
                    input_index,
                    message: error,
                });
                continue;
            }
            if let Err(error) = validate_manual_creator_item(&item) {
                if input.validation == crate::ManualCreatorBatchValidation::Permissive {
                    outcomes.push(crate::ManualCreatorBatchItemOutcome::Rejected {
                        input_index,
                        message: error,
                    });
                    continue;
                }
                let failure = serde_json::json!({"message": error});
                if let Err(record) =
                    self.repository
                        .fail_action(key, "creator_batch_create", failure)
                {
                    log::warn!("recording the failed creator batch item failed: {record}");
                }
                return Err(format!(
                    "Creator batch item {input_index} rejected: {error}"
                ));
            }
            valid_items.push((input_index, item));
        }
        if !valid_items.is_empty() {
            let created = match self.repository.create_manual_creator_batch(
                valid_items.iter().map(|(_, item)| item.clone()).collect(),
            ) {
                Ok(created) => created,
                Err(error) => {
                    let failure = serde_json::json!({"message": error});
                    if let Err(record) =
                        self.repository
                            .fail_action(key, "creator_batch_create", failure)
                    {
                        log::warn!("recording the failed creator batch item failed: {record}");
                    }
                    return Err(error);
                }
            };
            debug_assert_eq!(created.len(), valid_items.len());
            for ((input_index, _), creator) in valid_items.into_iter().zip(created) {
                outcomes.push(crate::ManualCreatorBatchItemOutcome::Accepted {
                    input_index,
                    creator,
                });
            }
        }
        outcomes.sort_by_key(|outcome| match outcome {
            crate::ManualCreatorBatchItemOutcome::Accepted { input_index, .. }
            | crate::ManualCreatorBatchItemOutcome::Rejected { input_index, .. } => *input_index,
        });
        let result = crate::ManualCreatorBatchCreateResult {
            outcomes,
            replayed: false,
        };
        self.repository.complete_action(
            key,
            "creator_batch_create",
            serde_json::to_value(&result).map_err(|error| error.to_string())?,
        )?;
        if result.outcomes.iter().any(|outcome| {
            matches!(
                outcome,
                crate::ManualCreatorBatchItemOutcome::Accepted { .. }
            )
        }) {
            self.publish_changes([CreatorChangeScope::Creators]);
        }
        Ok(result)
    }

    /// Pin or unpin one active Creator on the workbench list. A pure
    /// ordering preference: no revision, no touchpoint, no updated_at.
    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn set_creator_pinned_async(
        &self,
        input: crate::SetCreatorPinnedInput,
    ) -> Result<(), crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.set_creator_pinned(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::set_creator_pinned_async`] to own admission and dispatch.
    pub fn set_creator_pinned(&self, input: crate::SetCreatorPinnedInput) -> Result<(), String> {
        self.require_active_creator(&input.creator_id)?;
        self.repository
            .set_creator_pinned(&input.creator_id, input.pinned)?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(())
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn update_creator_stage_async(
        &self,
        input: UpdateCreatorStageInput,
    ) -> Result<CreatorRelationship, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.update_creator_stage(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::update_creator_stage_async`] to own admission and dispatch.
    pub fn update_creator_stage(
        &self,
        input: UpdateCreatorStageInput,
    ) -> Result<CreatorRelationship, String> {
        if input.stage == CreatorStage::Archived {
            return Err("use onecrew_creator_archive to archive a Creator".into());
        }
        let creator = self.require_active_creator(&input.creator_id)?;
        if creator.relationship.stage == input.stage {
            return Ok(creator.relationship);
        }
        let relationship = self
            .repository
            .set_creator_stage(&input.creator_id, input.stage)?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(relationship)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn update_creator_async(
        &self,
        input: UpdateCreatorInput,
    ) -> Result<CreatorRecord, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.update_creator(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::update_creator_async`] to own admission and dispatch.
    pub fn update_creator(&self, input: UpdateCreatorInput) -> Result<CreatorRecord, String> {
        let display_name = input
            .display_name
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("creator display_name cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let notes = input
            .notes
            .as_deref()
            .map(str::trim)
            .map(|value| {
                if value.is_empty() {
                    Err("creator notes cannot be empty when provided".to_string())
                } else {
                    Ok(value.to_owned())
                }
            })
            .transpose()?;
        let tags = input
            .tags
            .clone()
            .map(normalize_creator_tags)
            .transpose()
            .map_err(|error| error.to_string())?;
        let quality = CreatorQualityInput {
            score: input.score,
            assessment: input.assessment,
            follower_counts: input.follower_counts.clone().unwrap_or_default(),
        }
        .normalized()?;
        if display_name.is_none() && notes.is_none() && quality.is_empty() && tags.is_none() {
            return Err("creator update requires at least one field to change".into());
        }
        self.require_active_creator(&input.creator_id)?;
        let record = self.repository.update_creator(UpdateCreatorInput {
            tags,
            creator_id: input.creator_id,
            display_name,
            notes,
            score: quality.score,
            assessment: quality.assessment,
            follower_counts: Some(quality.follower_counts),
        })?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(record)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn delete_creator_async(
        &self,
        input: DeleteCreatorInput,
    ) -> Result<CreatorDeleteResult, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.delete_creator(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::delete_creator_async`] to own admission and dispatch.
    pub fn delete_creator(&self, input: DeleteCreatorInput) -> Result<CreatorDeleteResult, String> {
        let result = self.repository.delete_creator(input)?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(result)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn archive_creator_async(
        &self,
        creator_id: CreatorId,
    ) -> Result<Creator, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.archive_creator(&creator_id))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::archive_creator_async`] to own admission and dispatch.
    pub fn archive_creator(&self, creator_id: &CreatorId) -> Result<Creator, String> {
        let current = self.repository.get_creator(creator_id)?;
        if current.creator.archived_at.is_some()
            && current.relationship.stage == CreatorStage::Archived
        {
            return Ok(current.creator);
        }
        let creator = self.repository.archive_creator(creator_id)?;
        self.publish_changes([CreatorChangeScope::Creators]);
        Ok(creator)
    }

    pub(crate) fn require_active_creator(
        &self,
        creator_id: &CreatorId,
    ) -> Result<CreatorRecord, String> {
        let creator = self.repository.get_creator(creator_id)?;
        if creator.creator.archived_at.is_some()
            || creator.relationship.stage == CreatorStage::Archived
        {
            return Err(
                "archived Creator cannot be used for outreach; delete it or use an active Creator"
                    .into(),
            );
        }
        Ok(creator)
    }
}

impl crate::CreatorRuntime {
    fn preflight_manual_creator_batch(
        &self,
        items: &[crate::ManualCreatorCreateInput],
    ) -> Result<Vec<Option<String>>, String> {
        let mut seen = std::collections::HashSet::new();
        let mut errors = Vec::with_capacity(items.len());
        let mut item_keys = Vec::with_capacity(items.len());
        let mut locators = Vec::new();
        for item in items {
            let mut item_error = validate_manual_creator_item(item).err();
            let mut canonical_locators = Vec::new();
            if item_error.is_none() {
                for route in &item.contact_routes {
                    let canonical = crate::contact_routes::normalize_discovered_contact_route(
                        &crate::DiscoveredContactRoute {
                            channel: route.channel,
                            kind: crate::LocatorKind::Handle,
                            value: route.value.trim().to_owned(),
                            source: crate::SourceKind::ManualEntry,
                            source_url: route.source_url.clone(),
                        },
                    );
                    let canonical = match canonical {
                        Ok(canonical) => canonical,
                        Err(error) => {
                            item_error = Some(error);
                            break;
                        }
                    };
                    let key = crate::contact_routes::locator_key(&canonical);
                    if !seen.insert(key.clone()) {
                        item_error = Some("duplicate contact route in Creator batch".into());
                        break;
                    }
                    canonical_locators.push((key, canonical));
                }
            }
            if item_error.is_none() {
                item_keys.push(
                    canonical_locators
                        .iter()
                        .map(|(key, _)| key.clone())
                        .collect(),
                );
                locators.extend(canonical_locators.into_iter().map(|(_, locator)| locator));
            } else {
                item_keys.push(Vec::new());
            }
            errors.push(item_error);
        }
        if !locators.is_empty() {
            let owners = self
                .repository
                .find_contact_route_owner_keys(&locators)?
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            for (index, keys) in item_keys.into_iter().enumerate() {
                if errors[index].is_none() && keys.iter().any(|key| owners.contains(key)) {
                    errors[index] = Some("contact route already belongs to another Creator".into());
                }
            }
        }
        Ok(errors)
    }
}

fn creator_batch_fingerprint(
    input: &crate::ManualCreatorBatchCreateInput,
) -> Result<String, String> {
    let payload =
        serde_json::to_vec(&(input.validation, &input.items)).map_err(|error| error.to_string())?;
    Ok(sha2::Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn validate_manual_creator_item(input: &crate::ManualCreatorCreateInput) -> Result<(), String> {
    if let Some(tags) = input.tags.clone() {
        normalize_creator_tags(tags).map_err(|error| error.to_string())?;
    }
    if input.display_name.trim().is_empty() {
        return Err("creator display_name is required".into());
    }
    if input
        .notes
        .as_deref()
        .is_some_and(|notes| notes.trim().is_empty())
    {
        return Err("creator notes cannot be empty when provided".into());
    }
    for route in &input.contact_routes {
        if route.value.trim().is_empty() {
            return Err("contact route value is required".into());
        }
        if route
            .source_url
            .as_deref()
            .is_some_and(|url| url.trim().is_empty())
        {
            return Err("contact route source_url cannot be empty when provided".into());
        }
    }
    if input.contact_routes.len() > crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR {
        return Err(format!(
            "Creator supports at most {} initial contact routes",
            crate::MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
