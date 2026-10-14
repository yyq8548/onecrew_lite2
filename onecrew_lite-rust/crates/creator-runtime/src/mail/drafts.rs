use super::templates::validate_template_key;
use crate::{
    CreatorChangeScope, EmailDirection, EmailDraft, EmailDraftGetInput, EmailDraftMutationInput,
    EmailDraftState, ListEmailDraftsInput, SaveEmailDraftInput,
};

impl crate::CreatorRuntime {
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_email_drafts_async(
        &self,
        input: ListEmailDraftsInput,
    ) -> Result<crate::Page<crate::EmailDraftSummary>, crate::CreatorRepositoryCallError<String>>
    {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_email_drafts(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_email_drafts_async`] to own admission and dispatch.
    pub fn list_email_drafts(
        &self,
        input: ListEmailDraftsInput,
    ) -> Result<crate::Page<crate::EmailDraftSummary>, String> {
        self.repository.list_email_drafts(
            input.mailbox_ref_id.as_ref(),
            input.state,
            crate::MAIL_PAGE_LIMITS.effective(input.limit),
            input.cursor.as_deref(),
        )
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn get_email_draft_async(
        &self,
        input: EmailDraftGetInput,
    ) -> Result<EmailDraft, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_email_draft(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::get_email_draft_async`] to own admission and dispatch.
    pub fn get_email_draft(&self, input: EmailDraftGetInput) -> Result<EmailDraft, String> {
        self.repository.get_email_draft(&input.draft_id)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn save_email_draft_async(
        &self,
        input: SaveEmailDraftInput,
    ) -> Result<EmailDraft, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.save_email_draft(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::save_email_draft_async`] to own admission and dispatch.
    pub fn save_email_draft(&self, mut input: SaveEmailDraftInput) -> Result<EmailDraft, String> {
        self.require_mailbox_ref(&input.mailbox_ref_id)?;
        if let Some(template_id) = input.template_id.as_ref() {
            self.repository.get_email_template(template_id)?;
        }
        for key in input.template_variables.keys() {
            validate_template_key(key)?;
        }
        match (&input.creator_id, &input.contact_route_id) {
            (Some(creator_id), Some(contact_route_id)) => {
                let creator = self.repository.get_creator(creator_id)?;
                if creator.creator.archived_at.is_some() {
                    return Err("archived Creator cannot own an email draft".into());
                }
                let route = creator
                    .contact_routes
                    .iter()
                    .find(|record| record.route.id == *contact_route_id)
                    .ok_or_else(|| "email route does not belong to this Creator".to_string())?;
                crate::email::ensure_email_route(route)?;
                let recipient =
                    crate::email::normalize_email_address(&route.locator.normalized_value)?;
                if !input.to.trim().is_empty()
                    && crate::email::normalize_email_address(&input.to)? != recipient
                {
                    return Err(
                        "draft recipient is not the selected saved email contact route".into(),
                    );
                }
                input.to = recipient;
                if let Some(email_id) = input.reply_to_email_id.as_ref() {
                    let message = self.repository.get_email_message(email_id)?;
                    let thread = self
                        .repository
                        .get_email_thread_record(&message.thread_id)?;
                    if thread.mailbox_ref_id != input.mailbox_ref_id
                        || thread.creator_id.as_ref() != Some(creator_id)
                        || thread.contact_route_id.as_ref() != Some(contact_route_id)
                    {
                        return Err(
                            "draft reply email is outside its Creator, route, or mailbox".into(),
                        );
                    }
                }
            }
            (None, None) => {
                if let Some(email_id) = input.reply_to_email_id.as_ref() {
                    let message = self.repository.get_email_message(email_id)?;
                    let thread = self
                        .repository
                        .get_email_thread_record(&message.thread_id)?;
                    if thread.mailbox_ref_id != input.mailbox_ref_id
                        || thread.creator_id.is_some()
                        || thread.contact_route_id.is_some()
                    {
                        return Err(
                            "manual draft reply must belong to an unowned thread in the same mailbox"
                                .into(),
                        );
                    }
                    if message.direction != EmailDirection::Inbound {
                        return Err("manual draft reply target must be an inbound email".into());
                    }
                    if message
                        .internet_message_id
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                    {
                        return Err("manual draft reply email has no Internet Message-ID".into());
                    }
                    let recipient = crate::email::normalize_email_address(
                        message.reply_to.first().unwrap_or(&message.from),
                    )?;
                    if !input.to.trim().is_empty()
                        && crate::email::normalize_email_address(&input.to)? != recipient
                    {
                        return Err("manual draft recipient does not match its reply email".into());
                    }
                    input.to = recipient;
                } else if !input.to.trim().is_empty() {
                    input.to = crate::email::normalize_email_address(&input.to)?;
                }
            }
            _ => {
                return Err(
                    "email draft ownership requires both Creator and contact route or neither"
                        .into(),
                );
            }
        }
        let draft = self.repository.save_email_draft(input)?;
        self.publish_changes([
            CreatorChangeScope::EmailDrafts,
            CreatorChangeScope::EmailFolders,
        ]);
        Ok(draft)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn trash_email_draft_async(
        &self,
        input: EmailDraftMutationInput,
    ) -> Result<EmailDraft, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.trash_email_draft(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::trash_email_draft_async`] to own admission and dispatch.
    pub fn trash_email_draft(&self, input: EmailDraftMutationInput) -> Result<EmailDraft, String> {
        let draft = self
            .repository
            .set_email_draft_state(input, EmailDraftState::Trashed)?;
        self.publish_changes([
            CreatorChangeScope::EmailDrafts,
            CreatorChangeScope::EmailFolders,
        ]);
        Ok(draft)
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn restore_email_draft_async(
        &self,
        input: EmailDraftMutationInput,
    ) -> Result<EmailDraft, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.restore_email_draft(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::restore_email_draft_async`] to own admission and dispatch.
    pub fn restore_email_draft(
        &self,
        input: EmailDraftMutationInput,
    ) -> Result<EmailDraft, String> {
        let draft = self
            .repository
            .set_email_draft_state(input, EmailDraftState::Draft)?;
        self.publish_changes([
            CreatorChangeScope::EmailDrafts,
            CreatorChangeScope::EmailFolders,
        ]);
        Ok(draft)
    }
}
