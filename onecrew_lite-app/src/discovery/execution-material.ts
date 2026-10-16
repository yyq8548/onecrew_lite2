import type { DiscoveryAdvanceResult } from "./advance.ts";

export function discoveryScoringPlatforms(result: DiscoveryAdvanceResult): string[] {
  const platform = result.profile?.claim.platform ?? result.content?.claim.platform ?? (["read_video", "read_transcript"].includes(result.step.kind) ? "youtube" : null);
  const deferred = platform !== null && !["google", "yahoo"].includes(platform) ? [platform] : result.campaign.scopes.map(scope => scope.platform);
  for (const origin of ["google", "yahoo"]) if (!deferred.includes(origin)) deferred.push(origin); return deferred;
}
export function discoveryScoreHit(result: DiscoveryAdvanceResult): string | null {
  if (result.provider_stop_code !== undefined) return null;
  const step = result.step;
  switch (step.kind) {
    case "read_video": return step.search_hit_id;
    case "read_web_source": return result.web_source?.status === "completed" && result.web_source.is_primary_source && result.web_source.candidate_id !== null ? step.search_hit_id : null;
    case "read_source_author": return result.source_author?.status === "completed" && result.source_author.duplicate_of === null ? step.search_hit_id : null;
    case "read_profile": return result.profile?.status === "completed" ? step.search_hit_id : null;
    case "read_content": return result.content?.status === "completed" ? step.search_hit_id : null;
    case "read_transcript": {
      const read = result.transcript, campaign = result.campaign;
      return read && campaign.phase === "active" && read.search_hit_id === step.search_hit_id && ["completed", "unavailable", "failed"].includes(read.status) && read.call_number > 0 && read.call_number <= campaign.used_calls && campaign.platform_runs.some(run => run.platform === "youtube" && run.profile_ref === read.profile_ref && run.status === "completed" && run.stop_code === null) ? step.search_hit_id : null;
    }
    case "read_public_page": return result.public_page?.status === "completed" ? result.public_page.search_hit_id : null;
    default: return null;
  }
}
export function discoveryAdvanceFailure(result: DiscoveryAdvanceResult): string | null {
  return result.provider_stop_code ?? result.profile?.stop_code ?? result.facebook_identity?.stop_code ?? result.source_author?.stop_code ?? result.web_source?.stop_code ?? result.content?.stop_code ?? result.identity?.stop_code ?? result.transcript?.stop_code ?? result.public_page?.stop_code ?? null;
}
