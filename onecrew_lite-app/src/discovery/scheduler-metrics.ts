import type { DiscoveryAdvanceResult } from "./advance.ts";
import type { FileDiscoveryExecutionMetrics } from "./execution-metrics.ts";

export function recordDiscoveryReadTiming(metrics: FileDiscoveryExecutionMetrics, campaign: string, result: DiscoveryAdvanceResult, elapsed: number): void {
  const kind = result.step.kind;
  const phase = kind === "search" ? "platform_search" : kind === "read_content" && result.content && (result.content.language_sample != null || ["google", "yahoo"].includes(result.content.claim.platform)) ? "language" : kind.startsWith("read_") ? kind.slice(5) : null;
  if (phase === null) return;
  const targeted = new Set(["read_profile", "read_content", "read_source_author", "read_web_source", "read_identity", "read_transcript", "read_content_targets"]);
  const target = targeted.has(kind) && "search_hit_id" in result.step ? result.step.search_hit_id : null;
  metrics.enqueue(campaign, `read.${phase}`, elapsed, target);
  if (result.source_author_timings?.content_ms !== null && result.source_author_timings?.content_ms !== undefined) metrics.enqueue(campaign, "read.source_author.content", result.source_author_timings.content_ms);
  if (result.source_author_timings?.profile_ms !== null && result.source_author_timings?.profile_ms !== undefined) metrics.enqueue(campaign, "read.source_author.profile", result.source_author_timings.profile_ms);
}

export function discoveryReadFailureLocation(result: DiscoveryAdvanceResult) {
  const step = result.step;
  const kind = step.kind === "search" ? "search" : step.kind === "read_facebook_identity" ? "identity" : step.kind.startsWith("read_") ? step.kind.slice(5) : "read";
  const fixed = ({ read_video: "youtube", read_transcript: "youtube", read_identity: "twitter", read_facebook_identity: "facebook", read_source_author: "douyin", read_web_source: "youtube", read_content_targets: "facebook" } as Record<string, string>)[step.kind];
  const platform = fixed ?? (step.kind === "search" ? step.platform : result.profile?.claim.platform ?? result.web_source?.claim.target_platform ?? result.content?.claim.platform ?? null);
  return { step: kind, platform, target_id: "search_hit_id" in step ? step.search_hit_id : result.public_page?.candidate_id ?? null };
}
