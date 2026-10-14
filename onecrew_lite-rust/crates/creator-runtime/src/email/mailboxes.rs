use crate::{
    EmailAuthConnectionDeletionClaim, EmailAuthConnectionId, MailboxAuthBinding, MailboxDetail,
    MailboxIdentityMapping, MailboxIdentityMappingInput, MailboxIssue, MailboxProvider,
    MailboxRecord, MailboxRefInput,
};

impl crate::CreatorRuntime {
    pub async fn reconcile_gmail_mailboxes(
        &self,
        projections: Vec<(MailboxRefInput, MailboxIdentityMappingInput)>,
    ) -> Result<(), String> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            for old in owner
                .list_mailboxes()?
                .into_iter()
                .filter(|m| m.provider == MailboxProvider::Gmail)
            {
                owner.sync_mailbox_ref(MailboxRefInput {
                    id: old.mailbox_ref_id,
                    provider: "gmail".into(),
                    label: old.label,
                    email_address: old.email_address,
                    display_name: old.display_name,
                    capabilities: old.capabilities,
                    available: false,
                    last_error: Some("gmail_authorization_required".into()),
                })?;
            }
            for (mailbox, mapping) in projections {
                if mailbox.provider != "gmail"
                    || mapping.provider_kind != crate::EmailProviderKind::Remote
                {
                    return Err("gmail_projection_invalid".to_owned());
                }
                owner.sync_mailbox_ref(mailbox)?;
                owner.upsert_mailbox_identity_mapping(mapping)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())
    }
    pub fn sync_mailbox_ref(&self, input: MailboxRefInput) -> Result<MailboxDetail, String> {
        let mailbox = self.repository.upsert_mailbox_ref(input)?;
        self.publish_changes([crate::CreatorChangeScope::EmailFolders]);
        Ok(mailbox)
    }

    /// Reconcile the Creator mailbox projection with one complete vault snapshot.
    /// Mailboxes absent from the snapshot are retained as unavailable so their
    /// historical email graph and pending local work remain readable.
    pub fn reconcile_mailbox_refs(
        &self,
        inputs: Vec<MailboxRefInput>,
    ) -> Result<Vec<MailboxDetail>, String> {
        let mailboxes = self.repository.reconcile_mailbox_refs(inputs)?;
        self.publish_changes([crate::CreatorChangeScope::EmailFolders]);
        Ok(mailboxes)
    }

    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_mailboxes_async(
        &self,
    ) -> Result<Vec<MailboxDetail>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_mailboxes())
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_mailboxes_async`] to own admission and dispatch.
    pub fn list_mailboxes(&self) -> Result<Vec<MailboxDetail>, String> {
        self.repository.list_mailbox_refs()
    }

    pub fn upsert_mailbox_identity_mapping(
        &self,
        input: MailboxIdentityMappingInput,
    ) -> Result<MailboxIdentityMapping, String> {
        let mapping = self.repository.upsert_mailbox_identity_mapping(input)?;
        self.publish_changes([crate::CreatorChangeScope::EmailFolders]);
        Ok(mapping)
    }

    /// Resolve the current mailbox binding through bounded repository admission.
    /// The returned value owns its data; no repository lease escapes the read.
    ///
    /// # Errors
    /// Reports unavailable admission, a missing mapping, or a failed read.
    pub async fn resolve_mailbox_auth_binding(
        &self,
        mailbox_ref_id: &crate::MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        let repository = self.repository.clone();
        let mailbox_ref_id = mailbox_ref_id.clone();
        self.repository_read_segment(move || {
            repository.resolve_mailbox_auth_binding(&mailbox_ref_id)
        })
        .await
        .map_err(|error| error.to_string())
    }

    /// Resolve a binding inside an existing synchronous blocking workflow.
    /// Async provider workflows use [`Self::resolve_mailbox_auth_binding`].
    ///
    /// # Errors
    /// Reports a missing mapping or storage failure. The caller must already
    /// own the blocking execution context for the complete synchronous use case.
    pub fn resolve_mailbox_auth_binding_blocking(
        &self,
        mailbox_ref_id: &crate::MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        self.repository.resolve_mailbox_auth_binding(mailbox_ref_id)
    }

    pub fn begin_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<EmailAuthConnectionDeletionClaim, String> {
        self.repository
            .begin_email_auth_connection_deletion(auth_connection_id)
    }

    pub fn finish_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<(), String> {
        self.repository
            .finish_email_auth_connection_deletion(auth_connection_id)
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn mailbox_detail(record: MailboxRecord) -> Result<MailboxDetail, String> {
    let provider = match record.provider.as_str() {
        "resend" => MailboxProvider::Resend,
        "gmail" => MailboxProvider::Gmail,
        _ => return Err("mailbox provider is unsupported".into()),
    };
    let mut issues = Vec::new();
    if !record.available {
        issues.push(MailboxIssue::Unavailable);
    }
    if record.last_error.is_some() {
        issues.push(MailboxIssue::RecordedFailure);
    }
    Ok(MailboxDetail {
        mailbox_ref_id: record.id,
        provider,
        label: record.label,
        email_address: record.email_address,
        display_name: record.display_name,
        capabilities: record.capabilities,
        available: record.available,
        issues,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}
