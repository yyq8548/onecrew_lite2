/* Generated. Do not edit. */

export interface LocalServiceStatus {
  binary_sha256: string;
  cli_update: {
    checked_at: number | null;
    current_version: string;
    diagnostic_code: ("check_failed" | "timed_out" | "unsupported_target") | null;
    latest_version: string | null;
    source: ("fresh_check" | "memory_cache") | null;
    state: "not_checked" | "available" | "current" | "unavailable";
  };
  data: {
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
  locale: "en-US" | "zh-CN";
  login: {
    browser_opened: boolean;
    callback_received: boolean;
    flow_id: string | null;
    issue:
      | (
          | "device_name_unavailable"
          | "device_name_invalid"
          | "callback_expired"
          | "canceled"
          | "replay"
          | "owner_changed"
          | "pairing_rejected"
          | "recovery_required"
        )
      | null;
    status: {
      action: ("begin_pairing" | "await_pairing_callback" | "cancel_pairing" | "validate_admission" | "retry") | null;
      credential_id: string | null;
      installation_id: string | null;
      issue:
        | (
            | "configuration_unavailable"
            | "credential_unavailable"
            | "pairing_rejected"
            | "pairing_expired"
            | "pairing_cancelled"
            | "admission_validation_required"
            | "safe_cleanup_incomplete"
            | "enrollment_unknown"
            | "verification_failed"
            | "remote_unavailable"
            | "remote_rate_limited"
            | "remote_unauthorized"
            | "remote_revoked"
            | "protocol_mismatch"
            | "local_vault_unavailable"
          )
        | null;
      state:
        | "unconfigured"
        | "pairing_required"
        | "pairing_pending"
        | "pairing_completing"
        | "validation_required"
        | "validating"
        | "active"
        | "revoked"
        | "unauthorized"
        | "recovery_required"
        | "temporarily_unavailable";
    };
  } | null;
  login_error: string | null;
  mcp_clients: number;
  next_actions: {
    command: {
      args: string[];
      executable: string;
    } | null;
    instruction?: string | null;
    kind:
      | "await_startup"
      | "login"
      | "await_login"
      | "install_plugin"
      | "enable_plugin"
      | "open_web"
      | "inspect_data"
      | "update_cli";
    tool: string | null;
  }[];
  pid: number;
  plugin: {
    diagnostic_code: string | null;
    enabled: boolean | null;
    installed_version: string | null;
    last_verified_at: number | null;
    state: "unknown" | "installed" | "not_installed" | "unavailable";
    verification: "not_checked" | "cached" | "fresh";
    [k: string]: unknown;
  };
  running: boolean;
  startup: {
    diagnostic_code?: string | null;
    failure?: {
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
  };
  transport: string;
  version: string;
  workspace_activity: {
    attempted: number;
    brief: string;
    campaign_id: string;
    current_read_stops?: number | null;
    deferred_scores?: number | null;
    failure_code?: string | null;
    failure_phase?: string | null;
    library_sync_error?: string | null;
    library_sync_pending?: boolean | null;
    library_synced_at?: number | null;
    login_required: number;
    max: number | null;
    pending: number;
    qualified: number;
    read_errors: number;
    read_operation_stops?:
      | {
          code: string;
          operation: string;
          platform: string;
          profile_ref: string;
          [k: string]: unknown;
        }[]
      | null;
    restricted: number;
    stage:
      | "ready"
      | "searching"
      | "verifying"
      | "pausing"
      | "paused"
      | "finished"
      | "budget_exhausted"
      | "needs_evidence"
      | "error";
    stale: boolean;
    stop?: {
      reason:
        | "user_paused"
        | "search_page_left"
        | "page_closed"
        | "request_failed"
        | "revision_changed"
        | "progress_unconfirmed"
        | "busy"
        | "needs_evidence"
        | "search_exhausted"
        | "target_reached"
        | "budget_exhausted"
        | "execution_interrupted";
      recorded_at: number;
    } | null;
    target: number;
    total: number;
    updated_at?: number | null;
    used: number;
    [k: string]: unknown;
  } | null;
  workspace_activity_error: string | null;
  workspace_history: {
    attempted: number;
    brief: string;
    campaign_id: string;
    current_read_stops?: number | null;
    deferred_scores?: number | null;
    failure_code?: string | null;
    failure_phase?: string | null;
    library_sync_error?: string | null;
    library_sync_pending?: boolean | null;
    library_synced_at?: number | null;
    login_required: number;
    max: number | null;
    pending: number;
    qualified: number;
    read_errors: number;
    read_operation_stops?:
      | {
          code: string;
          operation: string;
          platform: string;
          profile_ref: string;
          [k: string]: unknown;
        }[]
      | null;
    restricted: number;
    stage:
      | "ready"
      | "searching"
      | "verifying"
      | "pausing"
      | "paused"
      | "finished"
      | "budget_exhausted"
      | "needs_evidence"
      | "error";
    stale: boolean;
    stop?: {
      reason:
        | "user_paused"
        | "search_page_left"
        | "page_closed"
        | "request_failed"
        | "revision_changed"
        | "progress_unconfirmed"
        | "busy"
        | "needs_evidence"
        | "search_exhausted"
        | "target_reached"
        | "budget_exhausted"
        | "execution_interrupted";
      recorded_at: number;
    } | null;
    target: number;
    total: number;
    updated_at?: number | null;
    used: number;
    [k: string]: unknown;
  }[];
}
