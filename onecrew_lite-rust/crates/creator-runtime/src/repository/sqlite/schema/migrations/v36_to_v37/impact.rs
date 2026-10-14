//! Frozen v36 route-delete payload. Do not replace this with the live model.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImpactV36 {
    pub(super) contact_routes: u64,
    pub(super) dependencies: u64,
    pub(super) email_threads: u64,
    pub(super) email_messages: u64,
    pub(super) email_recipients: u64,
    pub(super) email_drafts: u64,
    pub(super) direct_message_conversations: u64,
    pub(super) direct_messages: u64,
    pub(super) outreach_touchpoints: u64,
    pub(super) audit_events: u64,
    pub(super) idempotency_records: u64,
    pub(super) target_resolutions: u64,
    pub(super) identity_conflicts: u64,
    pub(super) observations: u64,
    pub(super) contact_locators: u64,
    pub(super) observation_sources: u64,
    pub(super) resolved_targets: u64,
    pub(super) candidate_snapshots_detached: u64,
    pub(super) creator_origins_detached: u64,
    pub(super) local_records_total: u64,
}
impl ImpactV36 {
    pub(super) fn rewrite(&self) -> Result<serde_json::Value, ()> {
        let sum = [
            self.contact_routes,
            self.dependencies,
            self.email_threads,
            self.email_messages,
            self.email_recipients,
            self.email_drafts,
            self.direct_message_conversations,
            self.direct_messages,
            self.outreach_touchpoints,
            self.audit_events,
            self.idempotency_records,
            self.target_resolutions,
            self.identity_conflicts,
            self.observations,
            self.contact_locators,
            self.observation_sources,
            self.resolved_targets,
            self.candidate_snapshots_detached,
            self.creator_origins_detached,
        ]
        .into_iter()
        .try_fold(0_u64, |sum, count| sum.checked_add(count))
        .ok_or(())?;
        if sum != self.local_records_total {
            return Err(());
        }
        let mut value = serde_json::to_value(self).map_err(|_| ())?;
        let object = value.as_object_mut().ok_or(())?;
        object.remove("candidate_snapshots_detached");
        object.insert(
            "local_records_total".into(),
            (self.local_records_total - self.candidate_snapshots_detached).into(),
        );
        Ok(value)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StaleReasonV36 {
    PlanChanged,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ResultV36 {
    Deleted {
        operation_id: String,
        creator_id: String,
        contact_route_id: String,
        deleted: ImpactV36,
        remote_not_deleted: bool,
    },
    Stale {
        operation_id: String,
        reason: StaleReasonV36,
        next_tool: String,
    },
    Expired {
        operation_id: String,
        next_tool: String,
    },
}
impl ResultV36 {
    pub(super) fn operation_id(&self) -> &str {
        match self {
            Self::Deleted { operation_id, .. }
            | Self::Stale { operation_id, .. }
            | Self::Expired { operation_id, .. } => operation_id,
        }
    }
}
