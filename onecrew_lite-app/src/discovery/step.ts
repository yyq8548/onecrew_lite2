import type { DiscoveryRepository } from "./repository.ts";
import { canAdvanceSearchAfterStop } from "./stops.ts";

export type DiscoveryStep = { kind: "search"; platform: string; profile_ref: string }
  | { kind: "read_video" | "read_transcript"; profile_ref: string; search_hit_id: string }
  | { kind: "read_profile" | "read_identity" | "read_facebook_identity" | "read_web_source" | "read_source_author" | "read_content" | "read_content_targets"; search_hit_id: string }
  | { kind: "read_public_page"; profile_ref: string; candidate_id: string; requested_url: string }
  | { kind: "busy" | "target_reached" | "budget_exhausted" | "needs_evidence" | "search_exhausted" };
export interface DiscoveryStepSource extends Pick<DiscoveryRepository, "getPlan" | "countCandidates" | "nextTranscriptHit" | "nextPublicPage" | "nextSourceAuthor" | "nextProfileHit" | "profileNeedsIdentity" | "nextVideoHit"> {
  nextContentHit(campaign: string, platform: string, held?: readonly string[]): string | null;
  nextFacebookTargets(campaign: string): string | null;
  nextFacebookIdentity(campaign: string, excluded?: readonly string[]): string | null;
  nextWebSource(campaign: string, excluded?: readonly string[], held?: readonly string[]): string | null;
  nextSearchKeyword(campaign: string, scope: { platform: string; profile_ref: string; keywords: string[] }): string | null;
}

// 仅依据持久已批准状态提出下一步；实际执行仍须在事务内领取预算。
export function planDiscoveryStep(repository: DiscoveryStepSource, campaignId: string, revision: number, excluded: readonly string[] = [], deferred: readonly string[] = []): DiscoveryStep {
  const campaign = repository.getPlan(campaignId);
  if (campaign.phase !== "active" || campaign.revision !== revision) throw new Error("discovery_step_approval_changed");
  if (campaign.platform_runs.some(run => run.status === "searching")) return { kind: "busy" };
  const counts = repository.countCandidates(campaignId, excluded);
  if (counts.qualified >= campaign.target_count) return { kind: "target_reached" };
  const blocked = [...new Set([...excluded, ...deferred])], scopes = campaign.scopes.filter(scope => !blocked.includes(scope.platform)), runs = campaign.platform_runs.filter(run => !blocked.includes(run.platform));
  const first = scopes.flatMap(scope => runs.filter(run => run.platform === scope.platform && run.status === "unsearched"))[0];
  if (campaign.max_calls !== null && campaign.used_calls >= campaign.max_calls) return { kind: "budget_exhausted" };
  if (!blocked.includes("youtube")) { const transcript = repository.nextTranscriptHit(campaignId); if (transcript) return { kind: "read_transcript", profile_ref: transcript.profile_ref, search_hit_id: transcript.id }; }
  const page = repository.nextPublicPage(campaignId, blocked); if (page) return { kind: "read_public_page", profile_ref: page.profile_ref, candidate_id: page.candidate_id, requested_url: page.requested_url };
  for (const platform of ["youtube", "twitter", "bilibili", "reddit", "tiktok", "instagram", "weibo", "bluesky", "zhihu", "hackernews", "pinterest", "pixiv", "linkedin", "douyin"]) {
    if (!blocked.includes(platform)) { const hit = repository.nextContentHit(campaignId, platform); if (hit) return { kind: "read_content", search_hit_id: hit }; }
  }
  const author = repository.nextSourceAuthor(campaignId, blocked); if (author) return { kind: "read_source_author", search_hit_id: author };
  if (!blocked.includes("facebook")) {
    const hit = repository.nextContentHit(campaignId, "facebook"); if (hit) return { kind: "read_content", search_hit_id: hit };
    const target = repository.nextFacebookTargets(campaignId); if (target) return { kind: "read_content_targets", search_hit_id: target };
  }
  const profile = repository.nextProfileHit(campaignId, blocked); if (profile) return { kind: repository.profileNeedsIdentity(campaignId, profile) ? "read_identity" : "read_profile", search_hit_id: profile };
  const facebook = repository.nextFacebookIdentity(campaignId, blocked); if (facebook) return { kind: "read_facebook_identity", search_hit_id: facebook };
  if (!blocked.includes("youtube")) { const video = repository.nextVideoHit(campaignId); if (video) return { kind: "read_video", profile_ref: video.profile_ref, search_hit_id: video.id }; }
  for (const platform of ["google", "yahoo"]) if (!blocked.includes(platform) && !blocked.includes("youtube")) { const hit = repository.nextContentHit(campaignId, platform); if (hit) return { kind: "read_content", search_hit_id: hit }; }
  const web = repository.nextWebSource(campaignId, blocked); if (web) return { kind: "read_web_source", search_hit_id: web };
  if (first) return { kind: "search", platform: first.platform, profile_ref: first.profile_ref };
  for (const phase of ["technical_stop", "completed"]) for (const scope of scopes) {
    if (runs.some(run => run.platform === scope.platform && run.profile_ref === scope.profile_ref && run.status === phase && (phase !== "technical_stop" || canAdvanceSearchAfterStop(run.stop_code))) && repository.nextSearchKeyword(campaignId, scope) !== null) return { kind: "search", platform: scope.platform, profile_ref: scope.profile_ref };
  }
  return { kind: deferred.length ? "busy" : counts.pending > 0 ? "needs_evidence" : "search_exhausted" };
}
