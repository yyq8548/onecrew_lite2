use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorDatabaseStatus {
    Fresh,
    Ready,
    UpgradeRequired {
        from_version: i64,
        to_version: i64,
        impact: CreatorDatabaseUpgradeImpact,
    },
    Blocked {
        version: i64,
        blocker: CreatorDatabaseUpgradeBlocker,
    },
    Unsupported {
        version: i64,
        reason: String,
    },
}

/// A source ambiguity that must be resolved before backup or migration writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorDatabaseUpgradeBlocker {
    AmbiguousContactRouteDeleteImpact,
}
impl CreatorDatabaseUpgradeBlocker {
    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::AmbiguousContactRouteDeleteImpact => {
                "Stored contact-route deletion impact is ambiguous; local data was not changed."
            }
        }
    }
    pub const fn support_code(self) -> &'static str {
        match self {
            Self::AmbiguousContactRouteDeleteImpact => {
                "LOCAL_DATA_CONTACT_ROUTE_DELETE_IMPACT_UNSAFE"
            }
        }
    }
}

/// Named per-category counts of what an upgrade deletes or converts, built
/// read-only from persisted facts and compared for equality under the data
/// lock. The Task counters come from the 33 -> 34 edge; the Campaign counters
/// from the 35 -> 36 edge, which retires the persisted Campaign plan model:
/// Campaign rows are removed outright, campaign-bound search provenance is
/// converted to the operator account that ran the search, a campaign search
/// origin whose operator cannot be recovered is removed, the Campaign column
/// of historical audit rows is cleared, and the idempotency receipts of the
/// retired Campaign actions are removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorDatabaseUpgradeImpact {
    pub deleted_tasks: u64,
    pub deleted_task_runs: u64,
    pub deleted_task_attention: u64,
    pub deleted_task_idempotency_records: u64,
    pub cleared_reply_triage_task_refs: u64,
    pub removed_campaigns: u64,
    pub removed_campaign_templates: u64,
    pub removed_campaign_account_bindings: u64,
    pub removed_campaign_delete_operations: u64,
    pub removed_campaign_template_delete_operations: u64,
    /// Candidate snapshots whose campaign search origin became a platform
    /// search origin.
    pub converted_campaign_search_snapshots: u64,
    /// Creator origins whose campaign search became a platform search with the
    /// operator account recovered from the matching candidate snapshot.
    pub converted_campaign_search_origins: u64,
    /// Campaign search origins with no recoverable operator account; the
    /// Creator and its observation source remain.
    pub removed_unattributable_campaign_origins: u64,
    /// Community-administrator sources (candidate snapshots and Creator
    /// origins) that lost their Campaign reference and kept their operator
    /// account.
    pub converted_campaign_community_sources: u64,
    pub cleared_audit_campaign_refs: u64,
    /// Receipts of the retired `campaign_create` / `campaign_template_create`
    /// actions; nothing references them after the Campaign tables are dropped.
    pub removed_campaign_idempotency_records: u64,
    pub removed_candidate_snapshots: u64,
    /// Number of stored impact objects rewritten, including a completed
    /// operation's separate result impact when present.
    pub rewritten_contact_route_delete_impacts: u64,
    pub removed_tiktok_one_account_bindings: u64,
    pub removed_creator_save_confirmations: u64,
    pub removed_contact_confirmations: u64,
    pub unresolved_retired_outreach_actions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorDatabaseUpgradeResult {
    pub from_version: i64,
    pub to_version: i64,
    pub backup_path: String,
    pub impact: CreatorDatabaseUpgradeImpact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorDatabaseUpgradeFailureKind {
    Busy,
    InsufficientSpace,
    BackupFailed,
    UnsafeData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorDatabaseUpgradeError {
    pub kind: CreatorDatabaseUpgradeFailureKind,
    pub support_code: String,
    /// True only when the migration commit completed and a later validation failed.
    pub storage_upgraded: bool,
}

impl fmt::Display for CreatorDatabaseUpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Creator data upgrade failed ({})",
            self.support_code
        )
    }
}

impl std::error::Error for CreatorDatabaseUpgradeError {}

#[cfg(test)]
mod tests {
    use schemars::schema_for;

    use super::{
        CreatorDatabaseStatus, CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeImpact,
        CreatorDatabaseUpgradeResult,
    };

    #[test]
    fn database_management_contracts_are_flat_strict_and_typed() {
        assert_eq!(
            serde_json::to_value(CreatorDatabaseStatus::UpgradeRequired {
                from_version: 27,
                to_version: 28,
                impact: CreatorDatabaseUpgradeImpact::default(),
            })
            .unwrap(),
            serde_json::json!({
                "status": "upgrade_required",
                "from_version": 27,
                "to_version": 28,
                "impact": {
                    "deleted_tasks": 0, "deleted_task_runs": 0, "deleted_task_attention": 0,
                    "deleted_task_idempotency_records": 0, "cleared_reply_triage_task_refs": 0,
                    "removed_campaigns": 0, "removed_campaign_templates": 0,
                    "removed_campaign_account_bindings": 0, "removed_campaign_delete_operations": 0,
                    "removed_campaign_template_delete_operations": 0,
                    "converted_campaign_search_snapshots": 0, "converted_campaign_search_origins": 0,
                    "removed_unattributable_campaign_origins": 0,
                    "converted_campaign_community_sources": 0, "cleared_audit_campaign_refs": 0,
                    "removed_campaign_idempotency_records": 0,
                    "removed_candidate_snapshots": 0, "rewritten_contact_route_delete_impacts": 0,
                    "removed_tiktok_one_account_bindings": 0, "removed_creator_save_confirmations": 0,
                    "removed_contact_confirmations": 0, "unresolved_retired_outreach_actions": 0
                }
            })
        );
        assert!(
            serde_json::from_value::<CreatorDatabaseUpgradeResult>(serde_json::json!({
                "from_version": 27,
                "to_version": 28,
                "backup_path": "/tmp/backup",
                "impact": serde_json::to_value(CreatorDatabaseUpgradeImpact::default()).unwrap(),
                "unexpected": true
            }))
            .is_err()
        );
        let schema = serde_json::to_value(schema_for!(CreatorDatabaseStatus)).unwrap();
        assert!(schema.to_string().contains("upgrade_required"));
        assert!(schema.to_string().contains("additionalProperties"));
        assert!(
            serde_json::from_value::<CreatorDatabaseUpgradeError>(serde_json::json!({
                "kind": "busy",
                "support_code": "LOCAL_DATA_BUSY",
                "storage_upgraded": false,
                "detail": "do not expose this"
            }))
            .is_err()
        );
    }
}
