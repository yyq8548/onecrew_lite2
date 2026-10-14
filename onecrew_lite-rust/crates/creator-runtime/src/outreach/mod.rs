mod context;
mod conversations;
pub(crate) mod intents;
pub(crate) mod ledger;
mod shared;

use crate::{ContactRouteLastContactAccount, ContactRouteLastContactAccountInput};

impl crate::CreatorRuntime {
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn contact_route_last_contact_account_async(
        &self,
        input: ContactRouteLastContactAccountInput,
    ) -> Result<Option<ContactRouteLastContactAccount>, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_read_segment(move || owner.contact_route_last_contact_account(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::contact_route_last_contact_account_async`] to own admission and dispatch.
    pub fn contact_route_last_contact_account(
        &self,
        input: ContactRouteLastContactAccountInput,
    ) -> Result<Option<ContactRouteLastContactAccount>, String> {
        self.repository.get_creator(&input.creator_id)?;
        self.repository
            .contact_route_last_contact_account(&input.creator_id, &input.contact_route_id)
    }
}

#[cfg(test)]
mod tests;
