/* Generated. Do not edit. */

export interface DataStartupStatus {
  diagnostic_code: string | null;
  failure: {
    kind:
      | "insufficient_space"
      | "busy"
      | "backup_failed"
      | "newer_version"
      | "not_writable"
      | "initialization_failed"
      | "unsafe_data";
    message: string;
    results: {
      from_version: number;
      impact: {
        cleared_audit_campaign_refs: number;
        cleared_reply_triage_task_refs: number;
        /**
         * Community-administrator sources (candidate snapshots and Creator
         * origins) that lost their Campaign reference and kept their operator
         * account.
         */
        converted_campaign_community_sources: number;
        /**
         * Creator origins whose campaign search became a platform search with the
         * operator account recovered from the matching candidate snapshot.
         */
        converted_campaign_search_origins: number;
        /**
         * Candidate snapshots whose campaign search origin became a platform
         * search origin.
         */
        converted_campaign_search_snapshots: number;
        deleted_task_attention: number;
        deleted_task_idempotency_records: number;
        deleted_task_runs: number;
        deleted_tasks: number;
        removed_campaign_account_bindings: number;
        removed_campaign_delete_operations: number;
        /**
         * Receipts of the retired `campaign_create` / `campaign_template_create`
         * actions; nothing references them after the Campaign tables are dropped.
         */
        removed_campaign_idempotency_records: number;
        removed_campaign_template_delete_operations: number;
        removed_campaign_templates: number;
        removed_campaigns: number;
        removed_candidate_snapshots: number;
        removed_contact_confirmations: number;
        removed_creator_save_confirmations: number;
        removed_tiktok_one_account_bindings: number;
        /**
         * Campaign search origins with no recoverable operator account; the
         * Creator and its observation source remain.
         */
        removed_unattributable_campaign_origins: number;
        /**
         * Number of stored impact objects rewritten, including a completed
         * operation's separate result impact when present.
         */
        rewritten_contact_route_delete_impacts: number;
        unresolved_retired_outreach_actions: number;
      } | null;
      recovery_point_created: boolean;
      status: "completed";
      store: "account" | "business";
      to_version: number;
    }[];
    storage_upgraded: boolean;
    support_code: string;
  } | null;
  phase: "inspecting" | "upgrading" | "ready" | "failed";
  postflight: {
    blocked_store: ("account" | "business") | null;
    failure_kind:
      | (
          | "insufficient_space"
          | "busy"
          | "backup_failed"
          | "newer_version"
          | "not_writable"
          | "initialization_failed"
          | "unsafe_data"
        )
      | null;
    message: string | null;
    primary_action: string | null;
    recoverable_backup: boolean;
    remote_data_deleted: boolean;
    secondary_action: string | null;
    state: "fresh" | "ready" | "upgrade_required" | "blocked";
    support_code: string | null;
    targets: {
      from_version: number;
      impact: {
        cleared_audit_campaign_refs: number;
        cleared_reply_triage_task_refs: number;
        /**
         * Community-administrator sources (candidate snapshots and Creator
         * origins) that lost their Campaign reference and kept their operator
         * account.
         */
        converted_campaign_community_sources: number;
        /**
         * Creator origins whose campaign search became a platform search with the
         * operator account recovered from the matching candidate snapshot.
         */
        converted_campaign_search_origins: number;
        /**
         * Candidate snapshots whose campaign search origin became a platform
         * search origin.
         */
        converted_campaign_search_snapshots: number;
        deleted_task_attention: number;
        deleted_task_idempotency_records: number;
        deleted_task_runs: number;
        deleted_tasks: number;
        removed_campaign_account_bindings: number;
        removed_campaign_delete_operations: number;
        /**
         * Receipts of the retired `campaign_create` / `campaign_template_create`
         * actions; nothing references them after the Campaign tables are dropped.
         */
        removed_campaign_idempotency_records: number;
        removed_campaign_template_delete_operations: number;
        removed_campaign_templates: number;
        removed_campaigns: number;
        removed_candidate_snapshots: number;
        removed_contact_confirmations: number;
        removed_creator_save_confirmations: number;
        removed_tiktok_one_account_bindings: number;
        /**
         * Campaign search origins with no recoverable operator account; the
         * Creator and its observation source remain.
         */
        removed_unattributable_campaign_origins: number;
        /**
         * Number of stored impact objects rewritten, including a completed
         * operation's separate result impact when present.
         */
        rewritten_contact_route_delete_impacts: number;
        unresolved_retired_outreach_actions: number;
      } | null;
      store: "account" | "business";
      to_version: number;
    }[];
    title: string | null;
  } | null;
  preflight: {
    blocked_store: ("account" | "business") | null;
    failure_kind:
      | (
          | "insufficient_space"
          | "busy"
          | "backup_failed"
          | "newer_version"
          | "not_writable"
          | "initialization_failed"
          | "unsafe_data"
        )
      | null;
    message: string | null;
    primary_action: string | null;
    recoverable_backup: boolean;
    remote_data_deleted: boolean;
    secondary_action: string | null;
    state: "fresh" | "ready" | "upgrade_required" | "blocked";
    support_code: string | null;
    targets: {
      from_version: number;
      impact: {
        cleared_audit_campaign_refs: number;
        cleared_reply_triage_task_refs: number;
        /**
         * Community-administrator sources (candidate snapshots and Creator
         * origins) that lost their Campaign reference and kept their operator
         * account.
         */
        converted_campaign_community_sources: number;
        /**
         * Creator origins whose campaign search became a platform search with the
         * operator account recovered from the matching candidate snapshot.
         */
        converted_campaign_search_origins: number;
        /**
         * Candidate snapshots whose campaign search origin became a platform
         * search origin.
         */
        converted_campaign_search_snapshots: number;
        deleted_task_attention: number;
        deleted_task_idempotency_records: number;
        deleted_task_runs: number;
        deleted_tasks: number;
        removed_campaign_account_bindings: number;
        removed_campaign_delete_operations: number;
        /**
         * Receipts of the retired `campaign_create` / `campaign_template_create`
         * actions; nothing references them after the Campaign tables are dropped.
         */
        removed_campaign_idempotency_records: number;
        removed_campaign_template_delete_operations: number;
        removed_campaign_templates: number;
        removed_campaigns: number;
        removed_candidate_snapshots: number;
        removed_contact_confirmations: number;
        removed_creator_save_confirmations: number;
        removed_tiktok_one_account_bindings: number;
        /**
         * Campaign search origins with no recoverable operator account; the
         * Creator and its observation source remain.
         */
        removed_unattributable_campaign_origins: number;
        /**
         * Number of stored impact objects rewritten, including a completed
         * operation's separate result impact when present.
         */
        rewritten_contact_route_delete_impacts: number;
        unresolved_retired_outreach_actions: number;
      } | null;
      store: "account" | "business";
      to_version: number;
    }[];
    title: string | null;
  } | null;
  upgrade: {
    message: string;
    remote_data_deleted: boolean;
    results: {
      from_version: number;
      impact: {
        cleared_audit_campaign_refs: number;
        cleared_reply_triage_task_refs: number;
        /**
         * Community-administrator sources (candidate snapshots and Creator
         * origins) that lost their Campaign reference and kept their operator
         * account.
         */
        converted_campaign_community_sources: number;
        /**
         * Creator origins whose campaign search became a platform search with the
         * operator account recovered from the matching candidate snapshot.
         */
        converted_campaign_search_origins: number;
        /**
         * Candidate snapshots whose campaign search origin became a platform
         * search origin.
         */
        converted_campaign_search_snapshots: number;
        deleted_task_attention: number;
        deleted_task_idempotency_records: number;
        deleted_task_runs: number;
        deleted_tasks: number;
        removed_campaign_account_bindings: number;
        removed_campaign_delete_operations: number;
        /**
         * Receipts of the retired `campaign_create` / `campaign_template_create`
         * actions; nothing references them after the Campaign tables are dropped.
         */
        removed_campaign_idempotency_records: number;
        removed_campaign_template_delete_operations: number;
        removed_campaign_templates: number;
        removed_campaigns: number;
        removed_candidate_snapshots: number;
        removed_contact_confirmations: number;
        removed_creator_save_confirmations: number;
        removed_tiktok_one_account_bindings: number;
        /**
         * Campaign search origins with no recoverable operator account; the
         * Creator and its observation source remain.
         */
        removed_unattributable_campaign_origins: number;
        /**
         * Number of stored impact objects rewritten, including a completed
         * operation's separate result impact when present.
         */
        rewritten_contact_route_delete_impacts: number;
        unresolved_retired_outreach_actions: number;
      } | null;
      recovery_point_created: boolean;
      status: "completed";
      store: "account" | "business";
      to_version: number;
    }[];
    status: "completed" | "already_current" | "upgrade_applied_but_initialization_failed";
    support_code: string | null;
  } | null;
}
