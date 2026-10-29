import type { DiscoveryRepository } from "./repository.ts";
import { canAdvanceSearchAfterStop } from "./stops.ts";
import { DISCOVERY_READ_CONCURRENCY } from "./read-capacity.ts";

export interface DiscoverySchedulingPolicy { holdDiscovery?: boolean; heldQueryHits?: readonly string[]; busyPlatforms?: readonly string[] }

export type DiscoveryStep = { kind: "search"; platform: string; profile_ref: string }
  | { kind: "read_video"; profile_ref: string; search_hit_id: string }
  | { kind: "read_profile" | "read_identity" | "read_facebook_identity" | "read_web_source" | "read_source_author" | "read_content" | "read_content_targets" | "read_article" | "resolve_article"; search_hit_id: string }
  | { kind: "read_public_page"; profile_ref: string; candidate_id: string; requested_url: string }
  | { kind: "busy" | "target_reached" | "needs_evidence" | "search_exhausted" };
export interface DiscoveryStepSource extends Pick<DiscoveryRepository, "getPlan" | "countCandidates" | "readPlatforms" | "nextPublicPage" | "nextSourceAuthor" | "nextProfileHit" | "profileNeedsIdentity" | "nextVideoHit" | "sourceProgress"> {
  webAuthorReady(campaign: string, revision: number, excluded?: readonly string[]): ReturnType<DiscoveryRepository["webAuthorReady"]>;
  nextWebAuthor(campaign: string, excluded?: readonly string[]): string | null;
  nextContentHit(campaign: string, platform: string, held?: readonly string[]): string | null;
  nextFacebookTargets(campaign: string): string | null;
  nextFacebookIdentity(campaign: string, excluded?: readonly string[], held?: readonly string[]): string | null;
  nextWebSource(campaign: string, excluded?: readonly string[], held?: readonly string[]): string | null;
  nextSearchKeyword(campaign: string, scope: { platform: string; profile_ref: string; keywords: string[] }): string | null;
}
export interface DiscoveryPlanningSnapshot {
  campaign: ReturnType<DiscoveryRepository["getPlan"]>;
  counts: ReturnType<DiscoveryRepository["countCandidates"]>;
  progress: ReturnType<DiscoveryRepository["sourceProgress"]>;
  platforms: readonly string[];
}

// 仅依据持久已批准状态提出下一步；实际执行仍须在事务内领取读取步骤。
export function planDiscoveryStep(repository: DiscoveryStepSource, campaignId: string, revision: number, excluded: readonly string[] = [], deferred: readonly string[] = [], policy: DiscoverySchedulingPolicy = {}, snapshot?: DiscoveryPlanningSnapshot): DiscoveryStep {
  const campaign = snapshot?.campaign ?? repository.getPlan(campaignId);
  if (campaign.phase !== "active" || campaign.revision !== revision) throw new Error("discovery_step_approval_changed");
  const active = campaign.platform_runs.filter(run => run.status === "searching");
  if (active.length >= DISCOVERY_READ_CONCURRENCY) return { kind: "busy" };
  const occupied = [...new Set([...active.map(run => run.platform), ...policy.busyPlatforms ?? []])];
  const counts = snapshot?.counts ?? repository.countCandidates(campaignId, excluded);
  const progress = snapshot?.progress ?? repository.sourceProgress(campaignId), reached = progress.filter(source => source.qualified >= source.target_count).map(source => source.platform);
  const platforms = new Set(snapshot?.platforms ?? repository.readPlatforms(campaignId));
  const blocked = [...new Set([...excluded, ...deferred, ...reached, ...occupied])], scopes = campaign.scopes.filter(scope => !blocked.includes(scope.platform)), runs = campaign.platform_runs.filter(run => !blocked.includes(run.platform));
  const contactBlocked = [...new Set([...excluded, ...deferred, ...occupied])];
  if (progress.length ? progress.every(source => source.qualified >= source.target_count) : counts.qualified >= campaign.target_count) {
    const page = repository.nextPublicPage(campaignId, contactBlocked);
    if (page) return { kind: "read_public_page", profile_ref: page.profile_ref, candidate_id: page.candidate_id, requested_url: page.requested_url };
    return { kind: deferred.length || occupied.length ? "busy" : "target_reached" };
  }
  const first = scopes.flatMap(scope => runs.filter(run => run.platform === scope.platform && run.status === "unsearched"))[0];
  const held = policy.heldQueryHits ?? [];
  const authorReview = policy.holdDiscovery ? undefined : repository.webAuthorReady(campaignId, revision, blocked)[0]; if (authorReview) return { kind: "resolve_article", search_hit_id: authorReview.claim.search_hit_id };
  const requiredPage = repository.nextPublicPage(campaignId, contactBlocked, true); if (requiredPage) return { kind: "read_public_page", profile_ref: requiredPage.profile_ref, candidate_id: requiredPage.candidate_id, requested_url: requiredPage.requested_url };
  for (const platform of ["youtube", "twitter", "bilibili", "tiktok", "instagram", "weibo", "bluesky", "zhihu", "hackernews", "pixiv", "linkedin", "douyin", "xiaohongshu"]) {
    if (platforms.has(platform) && !blocked.includes(platform)) { const hit = repository.nextContentHit(campaignId, platform); if (hit) return { kind: "read_content", search_hit_id: hit }; }
  }
  if (!policy.holdDiscovery) {
    const author = repository.nextSourceAuthor(campaignId, blocked, held); if (author) return { kind: "read_source_author", search_hit_id: author };
  }
  if (platforms.has("facebook") && !blocked.includes("facebook")) {
    const hit = repository.nextContentHit(campaignId, "facebook"); if (hit) return { kind: "read_content", search_hit_id: hit };
    const target = repository.nextFacebookTargets(campaignId); if (target) return { kind: "read_content_targets", search_hit_id: target };
  }
  if (!policy.holdDiscovery && scopes.some(scope => scope.platform === "linkedin")) {
    const profile = repository.nextProfileHit(campaignId, blocked, held, "linkedin"); if (profile) return { kind: "read_profile", search_hit_id: profile };
  }
  const skippedProfiles: string[] = [...held];
  for (let profile = repository.nextProfileHit(campaignId, blocked, skippedProfiles); profile; profile = repository.nextProfileHit(campaignId, blocked, skippedProfiles)) {
    const identity = repository.profileNeedsIdentity(campaignId, profile);
    if (identity || !policy.holdDiscovery) return { kind: identity ? "read_identity" : "read_profile", search_hit_id: profile };
    skippedProfiles.push(profile);
  }
  const facebook = platforms.has("facebook") ? repository.nextFacebookIdentity(campaignId, blocked, held) : null; if (facebook) return { kind: "read_facebook_identity", search_hit_id: facebook };
  for (const platform of ["google", "yahoo"]) if (platforms.has(platform) && !blocked.includes(platform)) { const hit = repository.nextContentHit(campaignId, platform); if (hit) return { kind: "read_content", search_hit_id: hit }; }
  if (policy.holdDiscovery) return { kind: "busy" };
  if (platforms.has("youtube") && !blocked.includes("youtube")) { const video = repository.nextVideoHit(campaignId, held); if (video) return { kind: "read_video", profile_ref: video.profile_ref, search_hit_id: video.id }; }
  const web = repository.nextWebSource(campaignId, blocked); if (web) return { kind: "read_web_source", search_hit_id: web };
  const article = repository.nextWebAuthor(campaignId, blocked); if (article) return { kind: "read_article", search_hit_id: article };
  if (first) return { kind: "search", platform: first.platform, profile_ref: first.profile_ref };
  for (const phase of ["technical_stop", "completed"]) for (const scope of scopes) {
    if (runs.some(run => run.platform === scope.platform && run.profile_ref === scope.profile_ref && run.status === phase && (phase !== "technical_stop" || canAdvanceSearchAfterStop(run.stop_code))) && repository.nextSearchKeyword(campaignId, scope) !== null) return { kind: "search", platform: scope.platform, profile_ref: scope.profile_ref };
  }
  const page = repository.nextPublicPage(campaignId, contactBlocked); if (page) return { kind: "read_public_page", profile_ref: page.profile_ref, candidate_id: page.candidate_id, requested_url: page.requested_url };
  return { kind: deferred.length || occupied.length ? "busy" : counts.pending > 0 ? "needs_evidence" : "search_exhausted" };
}
