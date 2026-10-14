use crate::{
    CreatorChangeScope, EMAIL_THREAD_MESSAGE_PAGE_LIMITS, EmailFolderSummary, EmailMessageGetInput,
    EmailMessageView, EmailThreadGetInput, EmailThreadMutationInput, EmailThreadSummary,
    EmailThreadView, ListEmailFoldersInput, ListEmailThreadsInput, MAIL_PAGE_LIMITS, MailboxDetail,
    MailboxGetError, MailboxGetInput, MailboxGetOutcome, MailboxRefId, Page,
    UpdateEmailThreadDispositionInput,
};

impl crate::CreatorRuntime {
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_email_threads_async(
        &self,
        input: ListEmailThreadsInput,
    ) -> Result<Page<EmailThreadSummary>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_email_threads(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_email_threads_async`] to own admission and dispatch.
    pub fn list_email_threads(
        &self,
        input: ListEmailThreadsInput,
    ) -> Result<Page<EmailThreadSummary>, String> {
        let limit = MAIL_PAGE_LIMITS.effective(input.limit);
        let mut page = self.repository.list_email_threads(
            input.mailbox_ref_id.as_ref(),
            input.folder,
            input.sort,
            limit,
            input.cursor.as_deref(),
        )?;
        for summary in &mut page.items {
            summary.creator_display_name = summary
                .thread
                .creator_id
                .as_ref()
                .map(|creator_id| {
                    self.repository
                        .get_creator(creator_id)
                        .map(|record| record.creator.display_name)
                })
                .transpose()?;
        }
        Ok(page)
    }

    pub fn get_mailbox(
        &self,
        input: MailboxGetInput,
    ) -> Result<MailboxGetOutcome, MailboxGetError> {
        match self
            .repository
            .find_mailbox_ref(&input.mailbox_ref_id)
            .map_err(|_| MailboxGetError::LookupUnavailable)?
        {
            Some(detail) => Ok(MailboxGetOutcome::Found { detail }),
            None => Ok(MailboxGetOutcome::NotFound {
                mailbox_ref_id: input.mailbox_ref_id,
            }),
        }
    }

    pub(crate) fn require_mailbox_ref(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxDetail, String> {
        self.repository
            .find_mailbox_ref(mailbox_ref_id)?
            .ok_or_else(|| "mailbox not found".into())
    }

    pub fn get_email_message(
        &self,
        input: EmailMessageGetInput,
    ) -> Result<EmailMessageView, String> {
        self.repository
            .get_email_message(&input.email_id)
            .map(crate::EmailMessage::view)
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_email_folders_async(
        &self,
        input: ListEmailFoldersInput,
    ) -> Result<Vec<EmailFolderSummary>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_email_folders(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_email_folders_async`] to own admission and dispatch.
    pub fn list_email_folders(
        &self,
        input: ListEmailFoldersInput,
    ) -> Result<Vec<EmailFolderSummary>, String> {
        self.repository
            .list_email_folder_counts(input.mailbox_ref_id.as_ref())
    }

    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn update_email_thread_disposition_async(
        &self,
        input: UpdateEmailThreadDispositionInput,
    ) -> Result<EmailThreadView, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.update_email_thread_disposition(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::update_email_thread_disposition_async`] to own admission and dispatch.
    pub fn update_email_thread_disposition(
        &self,
        input: UpdateEmailThreadDispositionInput,
    ) -> Result<EmailThreadView, String> {
        match input.action {
            crate::EmailThreadDispositionAction::Archive => {
                self.repository.archive_email_thread(&input.thread_id)?;
            }
            crate::EmailThreadDispositionAction::Unarchive => {
                self.repository.unarchive_email_thread(&input.thread_id)?;
            }
            crate::EmailThreadDispositionAction::Restore => {
                self.repository.restore_email_thread(&input.thread_id)?;
            }
            crate::EmailThreadDispositionAction::Delete => {
                self.repository.trash_email_thread(&input.thread_id)?;
            }
        }
        self.publish_changes([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    pub fn archive_email_thread(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, String> {
        self.repository.archive_email_thread(&input.thread_id)?;
        self.publish_changes([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    pub fn unarchive_email_thread(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, String> {
        self.repository.unarchive_email_thread(&input.thread_id)?;
        self.publish_changes([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    pub fn trash_email_thread(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, String> {
        self.repository.trash_email_thread(&input.thread_id)?;
        self.publish_changes([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    /// Clear the thread's inbound unread marks and return the updated thread.
    /// This is local attention state: it performs no provider write, so it needs
    /// no confirmation or idempotency key, and it leaves the thread's position
    /// in the list untouched.
    /// Run this use case under bounded write admission before blocking dispatch.
    pub async fn mark_email_thread_read_async(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || owner.mark_email_thread_read(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::mark_email_thread_read_async`] to own admission and dispatch.
    pub fn mark_email_thread_read(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, String> {
        self.repository.mark_email_thread_read(&input.thread_id)?;
        self.publish_changes([CreatorChangeScope::EmailThreads]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    pub fn restore_email_thread(
        &self,
        input: EmailThreadMutationInput,
    ) -> Result<EmailThreadView, String> {
        self.repository.restore_email_thread(&input.thread_id)?;
        self.publish_changes([
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]);
        self.get_email_thread(EmailThreadGetInput {
            thread_id: input.thread_id,
            limit: None,
            cursor: None,
        })
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn get_email_thread_async(
        &self,
        input: EmailThreadGetInput,
    ) -> Result<EmailThreadView, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_email_thread(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::get_email_thread_async`] to own admission and dispatch.
    pub fn get_email_thread(&self, input: EmailThreadGetInput) -> Result<EmailThreadView, String> {
        let limit = EMAIL_THREAD_MESSAGE_PAGE_LIMITS.effective(input.limit);
        let thread = self.repository.get_email_thread_record(&input.thread_id)?;
        let messages: Page<EmailMessageView> = self.repository.get_email_thread_messages(
            &input.thread_id,
            limit,
            input.cursor.as_deref(),
        )?;
        let mailbox = self.require_mailbox_ref(&thread.mailbox_ref_id)?;
        let creator_display_name = thread
            .creator_id
            .as_ref()
            .map(|creator_id| {
                self.repository
                    .get_creator(creator_id)
                    .map(|record| record.creator.display_name)
            })
            .transpose()?;
        Ok(EmailThreadView {
            mailbox,
            thread,
            messages: messages.items,
            next_message_cursor: messages.next_cursor,
            creator_display_name,
        })
    }
}
