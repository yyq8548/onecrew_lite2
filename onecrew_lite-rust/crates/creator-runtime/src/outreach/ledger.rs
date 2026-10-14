//! Typed durable phases and body-free recovery facts for the three outreach writes.

use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorOutreachPhase {
    RemoteStarted,
    RemoteAccepted,
    RecordPending,
    Completed,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorOutreachReceipt {
    Relationship,
    ContentReply {
        external_reply_id: String,
    },
    DirectMessage {
        external_message_id: String,
        provider_thread_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorOutreachVerification {
    Relationship {
        following: RelationshipValue,
        can_message: RelationshipValue,
        checked_at: u64,
    },
    ContentReply {
        external_reply_id: String,
    },
    DirectMessage {
        external_message_id: String,
        provider_thread_id: String,
        sent_at: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorOutreachRecordIds {
    Relationship {
        relationship_id: AccountRelationshipId,
        can_message_id: AccountRelationshipId,
        touchpoint_id: TouchpointId,
        audit_event_id: AuditEventId,
    },
    ContentReply {
        external_reply_id: String,
        touchpoint_id: TouchpointId,
        audit_event_id: AuditEventId,
    },
    DirectMessage {
        conversation_id: ConversationId,
        message_id: MessageId,
        touchpoint_id: TouchpointId,
        audit_event_id: AuditEventId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorOutreachActionState {
    pub operation: CreatorOutreachOperation,
    pub identity: CreatorOutreachIdentity,
    pub fingerprint: String,
    pub claimed_at: u64,
    pub phase: CreatorOutreachPhase,
    pub receipt: Option<CreatorOutreachReceipt>,
    pub verification: Option<CreatorOutreachVerification>,
    pub record: Option<CreatorOutreachRecordIds>,
}

impl CreatorOutreachActionState {
    pub fn creator_recorded(&self) -> bool {
        self.phase == CreatorOutreachPhase::Completed && self.record.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct CreatorOutreachClaim {
    pub is_new: bool,
    pub state: CreatorOutreachActionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorRetiredOutreachPhase {
    Completed,
    Failed,
    RetiredActionUnresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreatorOutreachStatus {
    Current {
        state: Box<CreatorOutreachActionState>,
    },
    Retired {
        operation: CreatorOutreachOperation,
        phase: CreatorRetiredOutreachPhase,
        marked: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum CreatorOutreachRecord {
    Relationship(CommitRelationshipActionResult),
    ContentReply(CommitContentReplyActionResult),
    DirectMessage(CommitMessageActionResult),
}

/// The verified message body is supplied in memory and is written only to the
/// normal messages table. It is absent from the recovery envelope and Debug.
#[derive(Clone)]
pub struct CreatorOutreachMessageRecord {
    pub external_message_id: String,
    pub provider_thread_id: String,
    pub body: String,
    pub sent_at: u64,
}

impl std::fmt::Debug for CreatorOutreachMessageRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreatorOutreachMessageRecord")
            .field("external_message_id", &self.external_message_id)
            .field("provider_thread_id", &self.provider_thread_id)
            .field("sent_at", &self.sent_at)
            .finish_non_exhaustive()
    }
}

/// Shared ownership keeps the existing same-key guard alive through every
/// blocking repository segment and the admitted runtime future.
#[derive(Clone)]
pub struct CreatorOutreachLease {
    pub(crate) key: String,
    _guard: Arc<crate::action_guard::InFlightActionGuard>,
}

impl CreatorOutreachLease {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl std::fmt::Debug for CreatorOutreachLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreatorOutreachLease")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl CreatorRuntime {
    pub async fn creator_outreach_read(
        &self,
        key: &str,
        intent: &PreparedCreatorOutreachIntent,
    ) -> Result<Option<CreatorOutreachActionState>, CreatorOutreachError> {
        let repository = self.repository.clone();
        let key = key.to_owned();
        let intent = intent.clone();
        self.repository_read_segment(move || repository.read_creator_outreach(&key, &intent))
            .await
            .map_err(super::context::read_error)
    }
    pub async fn creator_outreach_status(
        &self,
        key: &str,
    ) -> Result<Option<CreatorOutreachStatus>, CreatorOutreachError> {
        let repository = self.repository.clone();
        let key = key.to_owned();
        self.repository_read_segment(move || repository.creator_outreach_status(&key))
            .await
            .map_err(super::context::read_error)
    }
    pub async fn validate_new_outreach_intent(
        &self,
        intent: &PreparedCreatorOutreachIntent,
    ) -> Result<(), CreatorOutreachError> {
        let repository = self.repository.clone();
        let intent = intent.clone();
        self.repository_read_segment(move || repository.validate_creator_outreach(&intent))
            .await
            .map_err(super::context::read_error)
    }
    pub async fn creator_outreach_claim(
        &self,
        lease: &CreatorOutreachLease,
        intent: &PreparedCreatorOutreachIntent,
    ) -> Result<CreatorOutreachClaim, CreatorOutreachError> {
        let repository = self.repository.clone();
        let held = lease.clone();
        let intent = intent.clone();
        self.repository_write_segment(move || {
            repository.claim_creator_outreach(held.key(), &intent)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_stage_accepted(
        &self,
        lease: &CreatorOutreachLease,
        intent: &PreparedCreatorOutreachIntent,
        receipt: CreatorOutreachReceipt,
    ) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
        let repository = self.repository.clone();
        let held = lease.clone();
        let intent = intent.clone();
        self.repository_write_segment(move || {
            repository.accept_creator_outreach(held.key(), &intent, receipt)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_stage_verified(
        &self,
        lease: &CreatorOutreachLease,
        intent: &PreparedCreatorOutreachIntent,
        verification: CreatorOutreachVerification,
    ) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
        let repository = self.repository.clone();
        let held = lease.clone();
        let intent = intent.clone();
        self.repository_write_segment(move || {
            repository.verify_creator_outreach(held.key(), &intent, verification)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_refused(
        &self,
        lease: &CreatorOutreachLease,
        intent: &PreparedCreatorOutreachIntent,
    ) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
        let repository = self.repository.clone();
        let held = lease.clone();
        let intent = intent.clone();
        self.repository_write_segment(move || {
            repository.refuse_creator_outreach(held.key(), &intent)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_recorded(
        &self,
        lease: &CreatorOutreachLease,
    ) -> Result<CreatorOutreachRecord, CreatorOutreachError> {
        let repository = self.repository.clone();
        let held = lease.clone();
        self.repository_read_segment(move || repository.recorded_creator_outreach(held.key()))
            .await
            .map_err(super::context::read_error)
    }
    pub async fn creator_outreach_record_relationship(
        &self,
        lease: &CreatorOutreachLease,
    ) -> Result<CommitRelationshipActionResult, CreatorOutreachError> {
        let runtime = self.clone();
        let held = lease.clone();
        self.repository_write_segment(move || {
            let record = runtime.repository.record_creator_relationship(held.key())?;
            runtime.publish_changes([CreatorChangeScope::Creators]);
            Ok(record)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_record_content_reply(
        &self,
        lease: &CreatorOutreachLease,
    ) -> Result<CommitContentReplyActionResult, CreatorOutreachError> {
        let runtime = self.clone();
        let held = lease.clone();
        self.repository_write_segment(move || {
            let record = runtime
                .repository
                .record_creator_content_reply(held.key())?;
            runtime.publish_changes([CreatorChangeScope::Creators]);
            Ok(record)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub async fn creator_outreach_record_message(
        &self,
        lease: &CreatorOutreachLease,
        message: CreatorOutreachMessageRecord,
    ) -> Result<CommitMessageActionResult, CreatorOutreachError> {
        let runtime = self.clone();
        let held = lease.clone();
        self.repository_write_segment(move || {
            let record = runtime
                .repository
                .record_creator_message(held.key(), message)?;
            runtime.publish_changes([CreatorChangeScope::Creators]);
            Ok(record)
        })
        .await
        .map_err(super::context::write_error)
    }
    pub fn acquire_creator_outreach(
        &self,
        key: &str,
    ) -> Result<CreatorOutreachLease, CreatorOutreachError> {
        super::shared::validate_idempotency_key(key)
            .map_err(|_| CreatorOutreachError::InvalidInput)?;
        let guard = crate::action_guard::InFlightActionGuard::acquire(self, key)
            .map_err(|_| CreatorOutreachError::ActionInProgress)?;
        Ok(CreatorOutreachLease {
            key: key.to_owned(),
            _guard: Arc::new(guard),
        })
    }
}
