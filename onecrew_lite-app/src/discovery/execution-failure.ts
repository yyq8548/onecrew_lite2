import { DiscoverySemanticError } from "./semantic-client.ts";
import { PLATFORM_READ_FAILURE_CODES } from "../platforms/opencli/errors.ts";
const deferred = new Set(["score_queued", "review_submit_outcome_unknown", "review_read_unavailable", "initial_match_material_unavailable", "initial_match_busy", "initial_match_discovery_draft_codex_unavailable", "initial_match_discovery_draft_codex_login_required", "initial_match_discovery_draft_codex_failed", "initial_match_discovery_draft_codex_timeout", "initial_match_discovery_draft_plan_invalid", "initial_match_single_finding_required", "initial_match_finding_invalid", "initial_match_language_request_invalid", "initial_match_required_failure_must_be_low", "initial_match_required_count_unknown", "initial_match_author_evidence_required", "initial_match_content_language_unavailable", "initial_match_audience_language_invalid", "initial_match_audience_sample_insufficient", "review_material_changed", "review_input_too_large", "review_input_invalid", "review_conditions_mismatch", "review_citation_invalid", "review_first_hand_evidence_required", "review_transcript_required", "review_match_level_invalid"]);
const safe = new Set(["score_queue_unavailable", "query_round_authentication_required", "query_round_campaign_changed", "query_round_no_available_platform", "query_round_no_new_queries", "discovery_draft_codex_unavailable", "discovery_draft_codex_login_required", "discovery_draft_codex_failed", "discovery_draft_codex_timeout", "discovery_draft_plan_invalid", "discovery_draft_login_required", "discovery_draft_account_changed", "host_busy", "host_work_failed", "discovery_dependency_unavailable", "discovery_read_outcome_unknown", "discovery_provider_read_stopped", "discovery_read_operation_stopped", "opencli_timeout", "opencli_output_too_large", "opencli_version_changed", "opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_command_unavailable", "opencli_invalid_output", "opencli_invalid_input", "opencli_unavailable", "opencli_command_failed", "opencli_risk_control", "opencli_auth_required", "search_completion_failed", "language_sources_exhausted", "contact_lookup_stale", "library_source_stale", "library_busy", "library_unavailable", "library_identity_unavailable", "library_account_changed", "library_request_failed", "library_request_invalid", "library_sync_interrupted", "library_table_schema_changed", "library_row_identity_changed", "library_write_outcome_unknown", "library_account_mismatch", "library_login_required", "library_authorization_pending", "library_auth_unavailable", "library_instance_changed", "library_invalid_response", "library_projection_conflict", "library_read_failed", "library_readback_incomplete", "library_request_unavailable", "library_runtime_unavailable", "library_service_unavailable", "library_sync_upgrade_required", "library_table_missing", "library_unmanaged_table", "library_write_failed", "library_no_search_hits", "library_capacity_exceeded"]);
export const discoveryFindingFailureCodes = ["initial_match_criterion_invalid", "initial_match_outcome_invalid", "initial_match_level_invalid", "initial_match_rationale_empty", "initial_match_rationale_too_large", "initial_match_auxiliary_field_invalid", "initial_match_citation_count_invalid", "initial_match_citation_not_saved"] as const;
for (const code of discoveryFindingFailureCodes) deferred.add(code);
for (const code of ["score_queue_exceeds_capacity", "score_queue_file_invalid", "score_queue_write_and_cleanup_failed"]) safe.add(code);
export function discoveryScoreDeferred(code: string): boolean { return deferred.has(code); }
for (const code of PLATFORM_READ_FAILURE_CODES) safe.add(code);
safe.add("opencli_cancel_pending");
for (const code of ["provider_request_failed", "provider_response_http_error", "provider_endpoint_not_found", "provider_response_redirected", "provider_response_invalid_json", "provider_response_shape_invalid", "provider_source_unavailable", "youtube_video_unavailable", "youtube_watch_layout_unsupported", "youtube_author_identity_invalid", "youtube_description_unavailable", "discovery_draft_intent_invalid", "discovery_draft_scope_invalid"]) safe.add(code);
for (const code of ["review_requirements_invalid", "review_requirements_incomplete", "review_requirements_mismatch", "review_requirement_citation_invalid"]) deferred.add(code);
for (const code of ["discovery_repository_busy", "discovery_plan_missing", "discovery_repository_stopped", "discovery_repository_read_failed"]) safe.add(code);
export function discoveryExecutionFailureCode(error: unknown): string {
  if (error instanceof Error && "code" in error) {
    if (error.code === "ERR_SQLITE_ERROR" && "errcode" in error && typeof error.errcode === "number") {
      const primary = error.errcode & 0xff;
      return primary === 5 || primary === 6 ? "discovery_repository_busy" : "discovery_repository_read_failed";
    }
    if (error.code === "ERR_INVALID_STATE" && error.message === "database is not open") return "discovery_repository_stopped";
  }
  const code = error instanceof Error ? "code" in error && typeof error.code === "string" ? error.code : error.message : typeof error === "string" ? error : "execution_failed";
  if (code === "opencli_read_command_unavailable") return "opencli_command_unavailable";
  if (code === "discovery_plan_missing") return code;
  return safe.has(code) || deferred.has(code) ? code : "execution_failed";
}
export function discoveryMatchFailure(error: unknown): unknown {
  if (!(error instanceof DiscoverySemanticError)) return error;
  const code = ({ semantic_unavailable: "initial_match_discovery_draft_codex_unavailable", semantic_login_required: "initial_match_discovery_draft_codex_login_required", semantic_failed: "initial_match_discovery_draft_codex_failed", semantic_timeout: "initial_match_discovery_draft_codex_timeout", semantic_invalid_output: "initial_match_discovery_draft_plan_invalid" } as const)[error.code];
  return new Error(code, { cause: error });
}
