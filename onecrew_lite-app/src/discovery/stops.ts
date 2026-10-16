const definiteSearchFailures = new Set(["opencli_command_failed", "opencli_command_unavailable", "opencli_invalid_output", "opencli_output_too_large", "search_completion_failed"]);
const sourceReadFailures = new Set(["opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large", "opencli_timeout", "youtube_video_unavailable", "youtube_watch_layout_unsupported", "youtube_author_identity_invalid", "youtube_description_unavailable"]);
export function canAdvanceSearchAfterStop(code: string | null): boolean { return code !== null && definiteSearchFailures.has(code); }
export const repairedSearchRecoveryCodes = new Set(["provider_endpoint_not_found", "provider_response_invalid_json", "provider_response_shape_invalid", "provider_response_http_error", "provider_source_unavailable", "opencli_command_unavailable", "opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large"]);
export interface DiscoverySearchRecovery { expected_stop_code: string; expected_stopped_at: number }
export function sourceLocalReadFailure(code: string | null): boolean { return code !== null && sourceReadFailures.has(code); }
export function discoveryReadPlatformStatus(code: string | null): "completed" | "risk_control_stop" | "unauthorized" | "technical_stop" {
  if (code === null || code === "language_sources_exhausted" || sourceLocalReadFailure(code)) return "completed";
  if (code === "opencli_risk_control") return "risk_control_stop";
  if (code === "opencli_auth_required") return "unauthorized";
  return "technical_stop";
}
export function nextDiscoverySearchKeyword(keywords: readonly string[], run: Readonly<{ status: string; stop_code: string | null; active_query: string | null; calls_used: number }>, completed: ReadonlySet<string>): string | null {
  let start = 0;
  if (run.status === "technical_stop" && canAdvanceSearchAfterStop(run.stop_code)) {
    const index = run.active_query === null ? -1 : keywords.indexOf(run.active_query);
    start = index < 0 ? run.calls_used : index + 1;
  }
  return keywords.slice(start).find(keyword => !completed.has(keyword)) ?? null;
}
