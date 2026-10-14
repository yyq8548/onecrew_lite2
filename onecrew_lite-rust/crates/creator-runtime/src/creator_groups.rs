use crate::{
    BulkMoveCreatorsToGroupInput, CreateCreatorGroupInput, CreatorChangeScope, CreatorGroup,
    CreatorGroupError, CreatorGroupListResult, DeleteCreatorGroupInput, DeleteCreatorGroupResult,
    ListCreatorGroupsInput, MoveCreatorsToGroupResult, SetCreatorGroupInput,
    UpdateCreatorGroupInput,
};

impl crate::CreatorRuntime {
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_creator_groups_async(
        &self,
        input: ListCreatorGroupsInput,
    ) -> Result<CreatorGroupListResult, crate::CreatorRepositoryCallError<CreatorGroupError>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_creator_groups(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_creator_groups_async`] to own admission and dispatch.
    pub fn list_creator_groups(
        &self,
        input: ListCreatorGroupsInput,
    ) -> Result<CreatorGroupListResult, CreatorGroupError> {
        self.repository.list_creator_groups(input)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn create_creator_group_async(
        &self,
        input: CreateCreatorGroupInput,
    ) -> Result<CreatorGroup, crate::CreatorRepositoryCallError<CreatorGroupError>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.create_creator_group(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::create_creator_group_async`] to own admission and dispatch.
    pub fn create_creator_group(
        &self,
        input: CreateCreatorGroupInput,
    ) -> Result<CreatorGroup, CreatorGroupError> {
        let group = self.repository.create_creator_group(input)?;
        self.publish_changes([CreatorChangeScope::CreatorGroups]);
        Ok(group)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn update_creator_group_async(
        &self,
        input: UpdateCreatorGroupInput,
    ) -> Result<CreatorGroup, crate::CreatorRepositoryCallError<CreatorGroupError>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.update_creator_group(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::update_creator_group_async`] to own admission and dispatch.
    pub fn update_creator_group(
        &self,
        input: UpdateCreatorGroupInput,
    ) -> Result<CreatorGroup, CreatorGroupError> {
        let (group, changed) = self.repository.update_creator_group(input)?;
        if changed {
            self.publish_changes([
                CreatorChangeScope::CreatorGroups,
                CreatorChangeScope::Creators,
            ]);
        }
        Ok(group)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn delete_creator_group_async(
        &self,
        input: DeleteCreatorGroupInput,
    ) -> Result<DeleteCreatorGroupResult, crate::CreatorRepositoryCallError<CreatorGroupError>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.delete_creator_group(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::delete_creator_group_async`] to own admission and dispatch.
    pub fn delete_creator_group(
        &self,
        input: DeleteCreatorGroupInput,
    ) -> Result<DeleteCreatorGroupResult, CreatorGroupError> {
        let result = self.repository.delete_creator_group(input)?;
        self.publish_changes([
            CreatorChangeScope::CreatorGroups,
            CreatorChangeScope::Creators,
        ]);
        Ok(result)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn set_creator_group_async(
        &self,
        input: SetCreatorGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, crate::CreatorRepositoryCallError<CreatorGroupError>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.set_creator_group(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::set_creator_group_async`] to own admission and dispatch.
    pub fn set_creator_group(
        &self,
        input: SetCreatorGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError> {
        let result = self.repository.set_creator_group(input)?;
        self.publish_creator_group_move(&result);
        Ok(result)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn bulk_move_creators_to_group_async(
        &self,
        input: BulkMoveCreatorsToGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, crate::CreatorRepositoryCallError<CreatorGroupError>>
    {
        let owner = self.clone();
        self.repository_write_segment(move || owner.bulk_move_creators_to_group(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::bulk_move_creators_to_group_async`] to own admission and dispatch.
    pub fn bulk_move_creators_to_group(
        &self,
        input: BulkMoveCreatorsToGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError> {
        let result = self.repository.bulk_move_creators_to_group(input)?;
        self.publish_creator_group_move(&result);
        Ok(result)
    }

    fn publish_creator_group_move(&self, result: &MoveCreatorsToGroupResult) {
        if result.moved_count > 0 {
            self.publish_changes([
                CreatorChangeScope::CreatorGroups,
                CreatorChangeScope::Creators,
            ]);
        }
    }
}

#[cfg(test)]
mod tests;
