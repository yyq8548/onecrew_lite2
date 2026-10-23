import { z } from "zod";
import { creatorU64 } from "./creators.ts";

const text = z.string();
const optionalText = text.nullable();
const u32 = z.number().int().min(0).max(4_294_967_295);
const version = z.number().int().safe();
const store = z.enum(["account", "business"]);
const failureKind = z.enum(["insufficient_space", "busy", "backup_failed", "newer_version", "not_writable", "initialization_failed", "unsafe_data"]);
export const migrationImpact = z.object({
  cleared_audit_campaign_refs: creatorU64, cleared_reply_triage_task_refs: creatorU64,
  converted_campaign_community_sources: creatorU64, converted_campaign_search_origins: creatorU64, converted_campaign_search_snapshots: creatorU64,
  deleted_task_attention: creatorU64, deleted_task_idempotency_records: creatorU64, deleted_task_runs: creatorU64, deleted_tasks: creatorU64,
  removed_campaign_account_bindings: creatorU64, removed_campaign_delete_operations: creatorU64, removed_campaign_idempotency_records: creatorU64,
  removed_campaign_template_delete_operations: creatorU64, removed_campaign_templates: creatorU64, removed_campaigns: creatorU64,
  removed_candidate_snapshots: creatorU64, removed_contact_confirmations: creatorU64, removed_creator_save_confirmations: creatorU64,
  removed_tiktok_one_account_bindings: creatorU64, removed_unattributable_campaign_origins: creatorU64,
  rewritten_contact_route_delete_impacts: creatorU64, unresolved_retired_outreach_actions: creatorU64,
}).strict();
const target = z.object({ store, from_version: version, to_version: version, impact: migrationImpact.nullable() }).strict();
const upgradeResult = target.extend({ status: z.literal("completed"), recovery_point_created: z.boolean() });
const dataFailure = z.object({ kind: failureKind, message: text, support_code: text, storage_upgraded: z.boolean(), results: z.array(upgradeResult) }).strict();
const preflight = z.object({ state: z.enum(["fresh", "ready", "upgrade_required", "blocked"]), targets: z.array(target), recoverable_backup: z.boolean(), remote_data_deleted: z.boolean(), blocked_store: store.nullable(), title: optionalText, message: optionalText, primary_action: optionalText, secondary_action: optionalText, failure_kind: failureKind.nullable(), support_code: optionalText }).strict();
export const dataStartupStatus = z.object({
  phase: z.enum(["inspecting", "upgrading", "ready", "failed"]), diagnostic_code: optionalText, failure: dataFailure.nullable(), preflight: preflight.nullable(), postflight: preflight.nullable(),
  upgrade: z.object({ status: z.enum(["completed", "already_current", "upgrade_applied_but_initialization_failed"]), message: text, remote_data_deleted: z.boolean(), support_code: optionalText, results: z.array(upgradeResult) }).strict().nullable(),
}).strict();
export const creatorSyncStatus = z.object({
  state: z.enum(["unconfigured", "pairing_required", "pairing_pending", "pairing_completing", "validation_required", "validating", "active", "revoked", "unauthorized", "recovery_required", "temporarily_unavailable"]),
  action: z.enum(["begin_pairing", "await_pairing_callback", "cancel_pairing", "validate_admission", "retry"]).nullable(), credential_id: optionalText, installation_id: optionalText,
  issue: z.enum(["configuration_unavailable", "credential_unavailable", "pairing_rejected", "pairing_expired", "pairing_cancelled", "admission_validation_required", "safe_cleanup_incomplete", "enrollment_unknown", "verification_failed", "remote_unavailable", "remote_rate_limited", "remote_unauthorized", "remote_revoked", "protocol_mismatch", "local_vault_unavailable"]).nullable(),
}).strict();
export const loginProjection = z.object({ browser_opened: z.boolean(), callback_received: z.boolean(), flow_id: optionalText, issue: z.enum(["device_name_unavailable", "device_name_invalid", "callback_expired", "canceled", "replay", "owner_changed", "pairing_rejected", "recovery_required"]).nullable(), status: creatorSyncStatus }).strict();
const stop = z.object({ reason: z.enum(["user_paused", "search_page_left", "page_closed", "request_failed", "revision_changed", "progress_unconfirmed", "busy", "needs_evidence", "search_exhausted", "target_reached", "execution_interrupted"]), recorded_at: creatorU64 }).strict();
const openCliDiagnostic = z.object({ stage: text.max(8192), exit_code: z.number().int().nullable(), stdout: text.max(8192), stderr: text.max(8192) }).strict();
const accessDiagnostic = z.object({ endpoint: text.refine(value => ["/installations/{id}", "/creator-sync/credential", "/me"].includes(value)), elapsed_ms: u32.optional(), http_status: z.number().int().min(100).max(599).optional(), transport_code: text.regex(/^[A-Za-z][A-Za-z0-9_]{0,63}$/u).optional() }).strict();
export const discoveryQueryAuditDiagnostic = z.strictObject({ stage: z.literal("query_audit"), kind: z.enum(["format", "suitability"]), issues: z.array(text.max(512)).min(1).max(32) });
export const discoveryReadFailure = z.object({
  recorded_at: creatorU64, step: text, platform: text.nullable(), target_id: text.nullable(), call_number: u32.nullable(), code: text,
  target_url: text.max(2048).nullable().optional(), elapsed_ms: u32.optional(),
  diagnostic: z.union([accessDiagnostic, openCliDiagnostic, discoveryQueryAuditDiagnostic]).optional(),
}).strict();
export type DiscoveryReadFailure = z.infer<typeof discoveryReadFailure>;
export const discoveryReadFailures = z.array(discoveryReadFailure).max(20);
const activity = z.object({
  pipeline: z.object({ active_pending: u32, queued_scores: u32, running_scores: u32, discovery_held: z.boolean(), contact_pending: z.boolean() }).strict().optional(),
  campaign_id: text, brief: text, stage: z.enum(["ready", "searching", "verifying", "pausing", "paused", "finished", "needs_evidence", "error"]), stale: z.boolean(),
  attempted: creatorU64, login_required: creatorU64, read_errors: creatorU64, restricted: creatorU64, total: creatorU64,
  pending: u32, qualified: u32, target: u32, used: u32,
  pending_breakdown: z.object({ awaiting_score: u32, needs_content: u32, needs_identity: u32, read_blocked: u32, insufficient_after_read: u32, needs_requirement: u32 }).strict().optional(),
  read_failures: discoveryReadFailures.default([]),
  current_read_stops: creatorU64.nullish(), deferred_scores: u32.nullish(), failure_code: optionalText.optional(), failure_phase: optionalText.optional(), library_sync_error: optionalText.optional(), library_sync_pending: z.boolean().nullish(), library_synced_at: creatorU64.nullish(), updated_at: creatorU64.nullish(), stop: stop.nullish(),
  read_operation_stops: z.array(z.object({ code: text, operation: text, platform: text, profile_ref: text }).passthrough()).nullish(),
}).passthrough();
export const localServiceStatus = z.object({
  workspace_connections: z.object({ browser: z.enum(["ready", "not_connected", "unavailable", "account_required"]), library: z.enum(["ready", "unavailable", "account_required"]), checked_at: creatorU64 }).strict().optional(),
  binary_sha256: text, version: text, transport: text, running: z.boolean(), locale: z.enum(["en-US", "zh-CN"]), pid: u32, mcp_clients: u32,
  cli_update: z.object({ checked_at: creatorU64.nullable(), current_version: text, diagnostic_code: z.enum(["check_failed", "timed_out", "workspace_update_required"]).nullable(), latest_version: optionalText, source: z.enum(["fresh_check", "memory_cache"]).nullable(), state: z.enum(["not_checked", "available", "current", "unavailable"]) }).strict(),
  data: preflight.nullable(), login: loginProjection.nullable(), login_error: optionalText,
  next_actions: z.array(z.object({ command: z.object({ args: z.array(text), executable: text }).strict().nullable(), instruction: optionalText.optional(), kind: z.enum(["await_startup", "login", "await_login", "install_plugin", "enable_plugin", "open_web", "inspect_data", "update_cli"]), tool: optionalText }).strict()),
  plugin: z.object({ diagnostic_code: optionalText, enabled: z.boolean().nullable(), installed_version: optionalText, last_verified_at: creatorU64.nullable(), state: z.enum(["unknown", "installed", "not_installed", "unavailable"]), verification: z.enum(["not_checked", "cached", "fresh"]) }).passthrough(),
  startup: z.object({ phase: z.enum(["inspecting", "upgrading", "ready", "failed"]), diagnostic_code: optionalText.optional(), failure: dataFailure.nullish() }).strict(),
  workspace_activity: activity.nullable(), workspace_activity_error: optionalText, workspace_history: z.array(activity),
}).strict();
export type DataStartupStatus = z.infer<typeof dataStartupStatus>;
export type LocalServiceStatus = z.infer<typeof localServiceStatus>;
export type LoginProjection = z.infer<typeof loginProjection>;
export type CreatorSyncStatus = z.infer<typeof creatorSyncStatus>;
