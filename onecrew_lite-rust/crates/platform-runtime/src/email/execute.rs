use std::sync::Arc;

use onecrew_creator_runtime::EmailSendFailure;

use super::models::{EmailCallOutput, EmailSendIntent, PreparedEmailAction};
use crate::{LocalPlatform, SubmittedOwnerGrant};

impl LocalPlatform {
    /// Consume the single-use grant and run the exact send the owner reviewed.
    ///
    /// The Creator runtime still owns the claim, the provider call, readback,
    /// and recording. This hands it only the intent that was sealed before the
    /// review, so a resumed call cannot substitute different arguments.
    ///
    /// # Errors
    /// Returns the Creator owner's typed send failure unchanged, including
    /// whether the message reached the provider.
    pub async fn execute_email_submitted(
        self: &Arc<Self>,
        grant: SubmittedOwnerGrant,
    ) -> Result<EmailCallOutput, EmailSendFailure> {
        // A refused grant means the store never yielded this authority, so the
        // send never started.
        let action = grant
            .into_email()
            .map_err(|error| EmailSendFailure::not_sent(error.to_string()))?;
        let PreparedEmailAction { intent, .. } = action;
        let creators = Arc::clone(&self.creators);
        match intent {
            EmailSendIntent::Direct(input) => creators
                .send_email(*input)
                .await
                .map(|result| EmailCallOutput::Direct(Box::new(result))),
            EmailSendIntent::Manual(input) => creators
                .send_manual_email(*input)
                .await
                .map(|result| EmailCallOutput::Manual(Box::new(result))),
            EmailSendIntent::Draft(input) => creators
                .send_email_draft(input)
                .await
                .map(|result| EmailCallOutput::Draft(Box::new(result))),
            EmailSendIntent::Batch(input) => creators
                .send_email_many(*input)
                .await
                .map(|result| EmailCallOutput::Batch(Box::new(result))),
            EmailSendIntent::Reconcile(input) => creators
                .reconcile_email_send_many(*input)
                .await
                .map(|result| EmailCallOutput::Batch(Box::new(result))),
        }
    }
}
