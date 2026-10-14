//! Reply triage: what an agent made of an inbound message.
//!
//! A scheduled watch reads a mailbox or a creator's DMs and has to put its
//! reading somewhere a human can act on. Prose in a run report cannot be
//! sorted, tagged, or dismissed; this can.

use crate::models::{
    AcknowledgeReplyTriageInput, CreatorChangeScope, ListReplyTriageInput, Page,
    RecordReplyTriageInput, ReplyTriage, ReplyTriageView,
};

impl crate::CreatorRuntime {
    /// Record one verdict about one inbound message. Judging the same
    /// message again replaces the reading — a message has one current
    /// meaning, not a history of guesses — and clears any acknowledgement,
    /// because what the human dismissed is not what is being said now.
    pub fn record_reply_triage(
        &self,
        input: RecordReplyTriageInput,
    ) -> Result<ReplyTriage, String> {
        let triage = self.repository.record_reply_triage(input)?;
        self.publish_changes([CreatorChangeScope::ReplyTriage]);
        Ok(triage)
    }

    /// Dismiss one verdict. Idempotent: acknowledging twice is the same as
    /// once, so a double click is not an error.
    pub fn acknowledge_reply_triage(
        &self,
        input: AcknowledgeReplyTriageInput,
    ) -> Result<ReplyTriage, String> {
        let triage = self.repository.acknowledge_reply_triage(input)?;
        self.publish_changes([CreatorChangeScope::ReplyTriage]);
        Ok(triage)
    }

    pub fn list_reply_triage(
        &self,
        input: ListReplyTriageInput,
    ) -> Result<Page<ReplyTriageView>, String> {
        self.repository.list_reply_triage(input)
    }
}
