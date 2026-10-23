import { PLATFORM_READ_FAILURE_CODES } from "../platforms/opencli/errors.ts";
import { z } from "zod";
import { parseApplicationJson } from "../contracts/application-json.ts";
const definiteSearchFailures = new Set(["opencli_timeout", "opencli_command_failed", "opencli_command_unavailable", "opencli_invalid_output", "opencli_output_too_large", "search_completion_failed"]);
export const discoverySearchProgress = z.strictObject({ last_query: z.string().nullable(), consecutive_failures: z.number().int().min(0).max(3) });
export function readDiscoverySearchProgress(cursor?: string | null) { return cursor == null ? { last_query: null, consecutive_failures: 0 } : discoverySearchProgress.parse(parseApplicationJson(Buffer.from(cursor))); }
const sourceReadFailures = new Set(["opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large", "opencli_timeout", "provider_response_shape_invalid", "provider_response_invalid_json", "youtube_video_unavailable", "youtube_watch_layout_unsupported", "youtube_author_identity_invalid", "youtube_description_unavailable"]);
for (const code of PLATFORM_READ_FAILURE_CODES) sourceReadFailures.add(code);
export function canAdvanceSearchAfterStop(code: string | null): boolean { return code !== null && definiteSearchFailures.has(code); }
export const browserConnectionRecoveryCodes = new Set(["opencli_daemon_unavailable", "opencli_extension_unavailable", "opencli_profile_unavailable", "opencli_timeout"]);
export const repairedSearchRecoveryCodes = new Set(["provider_endpoint_not_found", "provider_response_invalid_json", "provider_response_shape_invalid", "provider_response_http_error", "provider_source_unavailable", "opencli_command_unavailable", "opencli_command_failed", "opencli_invalid_output", "opencli_output_too_large", ...browserConnectionRecoveryCodes]);
export interface DiscoverySearchRecovery { expected_stop_code: string; expected_stopped_at: number }
export function sourceLocalReadFailure(code: string | null): boolean { return code !== null && sourceReadFailures.has(code); }
export function discoveryReadPlatformStatus(code: string | null): "completed" | "risk_control_stop" | "unauthorized" | "technical_stop" {
  if (code === null || code === "language_sources_exhausted" || sourceLocalReadFailure(code)) return "completed";
  if (code === "opencli_risk_control") return "risk_control_stop";
  if (code === "opencli_auth_required") return "unauthorized";
  return "technical_stop";
}
export function nextDiscoverySearchKeyword(keywords: readonly string[], run: Readonly<{ status: string; stop_code: string | null; active_query: string | null; calls_used: number; cursor?: string | null }>, completed: ReadonlySet<string>): string | null {
  const progress = readDiscoverySearchProgress(run.cursor);
  if (progress.consecutive_failures >= 3) return null;
  let start = progress.last_query === null ? 0 : keywords.indexOf(progress.last_query) + 1;
  if (progress.last_query !== null && start === 0) throw new Error("discovery_search_progress_changed");
  if (run.status === "technical_stop" && canAdvanceSearchAfterStop(run.stop_code)) {
    const index = run.active_query === null ? -1 : keywords.indexOf(run.active_query);
    start = Math.max(start, index < 0 ? run.calls_used : index + 1);
  }
  return keywords.slice(start).find(keyword => !completed.has(keyword)) ?? null;
}
