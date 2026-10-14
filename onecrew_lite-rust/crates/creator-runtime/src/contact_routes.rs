pub(crate) mod extraction;
mod normalization;
mod observations;
pub(crate) mod telegram_community;

pub(crate) use normalization::{locator_key, normalize_discovered_contact_route, normalize_url};
pub(crate) use observations::discovered_observation;

use crate::{
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    CreatorChangeScope, DiscoveredContactRoute, LocatorKind, ManualCreatorContactRouteAddInput,
    ManualCreatorContactRouteAddResult, ManualCreatorContactRouteAddStatus,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateResult,
    ManualCreatorContactRouteUpdateStatus, PrepareCreatorContactRouteDeleteInput,
    PrepareCreatorContactRouteDeleteResult, SourceKind,
};

impl crate::CreatorRuntime {
    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn add_manual_creator_contact_route_async(
        &self,
        input: ManualCreatorContactRouteAddInput,
    ) -> Result<ManualCreatorContactRouteAddResult, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.add_manual_creator_contact_route(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::add_manual_creator_contact_route_async`] to own admission and dispatch.
    pub fn add_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteAddInput,
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
        let input = ManualCreatorContactRouteAddInput {
            creator_id: input.creator_id,
            channel: input.channel,
            value: value.to_owned(),
            source_url,
        };
        let locator = normalize_discovered_contact_route(&DiscoveredContactRoute {
            channel: input.channel,
            kind: LocatorKind::Handle,
            value: input.value.clone(),
            source: SourceKind::ManualEntry,
            source_url: input.source_url.clone(),
        })?;
        let result = self
            .repository
            .add_manual_creator_contact_route(input, locator)?;
        if result.status == ManualCreatorContactRouteAddStatus::Added {
            self.publish_changes([CreatorChangeScope::Creators]);
        }
        Ok(result)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn update_manual_creator_contact_route_async(
        &self,
        input: ManualCreatorContactRouteUpdateInput,
    ) -> Result<ManualCreatorContactRouteUpdateResult, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.update_manual_creator_contact_route(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::update_manual_creator_contact_route_async`] to own admission and dispatch.
    pub fn update_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteUpdateInput,
    ) -> Result<ManualCreatorContactRouteUpdateResult, String> {
        let value = input.value.trim();
        if value.is_empty() {
            return Err("contact route value is required".into());
        }
        let source_url = normalize_optional_source_url(input.source_url.as_deref())?;
        let input = ManualCreatorContactRouteUpdateInput {
            creator_id: input.creator_id,
            contact_route_id: input.contact_route_id,
            channel: input.channel,
            value: value.to_owned(),
            source_url,
        };
        let locator = normalize_discovered_contact_route(&DiscoveredContactRoute {
            channel: input.channel,
            kind: LocatorKind::Handle,
            value: input.value.clone(),
            source: SourceKind::ManualEntry,
            source_url: input.source_url.clone(),
        })?;
        let result = self
            .repository
            .update_manual_creator_contact_route(input, locator)?;
        if result.status == ManualCreatorContactRouteUpdateStatus::Updated {
            self.publish_changes([CreatorChangeScope::Creators]);
        }
        Ok(result)
    }

    /// Persist the prepared deletion plan under bounded writer admission.
    pub async fn prepare_creator_contact_route_delete_async(
        &self,
        input: PrepareCreatorContactRouteDeleteInput,
    ) -> Result<PrepareCreatorContactRouteDeleteResult, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.prepare_creator_contact_route_delete(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::prepare_creator_contact_route_delete_async`] to own admission and dispatch.
    pub fn prepare_creator_contact_route_delete(
        &self,
        input: PrepareCreatorContactRouteDeleteInput,
    ) -> Result<PrepareCreatorContactRouteDeleteResult, String> {
        self.repository.prepare_creator_contact_route_delete(input)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn confirm_creator_contact_route_delete_async(
        &self,
        input: ConfirmCreatorContactRouteDeleteInput,
    ) -> Result<ConfirmCreatorContactRouteDeleteResult, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.confirm_creator_contact_route_delete(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::confirm_creator_contact_route_delete_async`] to own admission and dispatch.
    pub fn confirm_creator_contact_route_delete(
        &self,
        input: ConfirmCreatorContactRouteDeleteInput,
    ) -> Result<ConfirmCreatorContactRouteDeleteResult, String> {
        let (result, changed) = self
            .repository
            .confirm_creator_contact_route_delete(input)?;
        if changed {
            self.publish_changes(contact_route_delete_change_scopes(&result));
        }
        Ok(result)
    }
}

fn contact_route_delete_change_scopes(
    result: &ConfirmCreatorContactRouteDeleteResult,
) -> Vec<CreatorChangeScope> {
    let ConfirmCreatorContactRouteDeleteResult::Deleted { deleted, .. } = result else {
        return Vec::new();
    };
    let mut scopes = vec![CreatorChangeScope::Creators];
    if deleted.email_threads > 0 || deleted.email_messages > 0 || deleted.email_recipients > 0 {
        scopes.extend([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
    }
    if deleted.email_drafts > 0 {
        if !scopes.contains(&CreatorChangeScope::EmailFolders) {
            scopes.push(CreatorChangeScope::EmailFolders);
        }
        scopes.push(CreatorChangeScope::EmailDrafts);
    }
    if deleted.direct_message_conversations > 0
        || deleted.direct_messages > 0
        || deleted.outreach_touchpoints > 0
    {
        scopes.push(CreatorChangeScope::Outreach);
    }
    scopes
}

fn normalize_optional_source_url(source_url: Option<&str>) -> Result<Option<String>, String> {
    source_url
        .map(str::trim)
        .map(|value| {
            if value.is_empty() {
                Err("contact route source_url cannot be empty when provided".to_string())
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

#[cfg(test)]
mod runtime_tests;

#[cfg(test)]
mod tests {
    use super::contact_route_delete_change_scopes;
    use crate::{
        ConfirmCreatorContactRouteDeleteResult, CreatorChangeScope,
        CreatorContactRouteDeleteImpact, CreatorContactRouteDeleteOperationId,
        CreatorContactRouteId, CreatorId,
    };

    #[test]
    fn route_delete_change_scopes_cover_every_removed_projection_once() {
        let result = ConfirmCreatorContactRouteDeleteResult::Deleted {
            operation_id: CreatorContactRouteDeleteOperationId::new("operation"),
            creator_id: CreatorId::new("creator"),
            contact_route_id: CreatorContactRouteId::new("route"),
            deleted: CreatorContactRouteDeleteImpact {
                email_threads: 1,
                email_messages: 1,
                email_recipients: 1,
                email_drafts: 1,
                direct_message_conversations: 1,
                direct_messages: 1,
                outreach_touchpoints: 1,
                ..CreatorContactRouteDeleteImpact::default()
            },
            remote_not_deleted: true,
        };
        assert_eq!(
            contact_route_delete_change_scopes(&result),
            vec![
                CreatorChangeScope::Creators,
                CreatorChangeScope::EmailFolders,
                CreatorChangeScope::EmailThreads,
                CreatorChangeScope::EmailDrafts,
                CreatorChangeScope::Outreach,
            ]
        );
    }
}
