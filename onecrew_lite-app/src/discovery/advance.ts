import { z } from "zod";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import { OpenCliError, type OpenCliDiagnostics } from "../platforms/opencli/errors.ts";
import { creatorSearchSource, INACTIVE_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import type { DiscoveryRepository } from "./repository.ts";
import type { DiscoveryDependencyResolver } from "./dependencies.ts";
import { DiscoverySearchExecutor, type DiscoveryRepositoryResolver, type DiscoveryBrowserResolver } from "./search-execution.ts";
import { DiscoveryVideoExecutor } from "./video-execution.ts";
import { DiscoveryProfileExecutor } from "./profile-execution.ts";
import { DiscoveryContentExecutor } from "./content-execution.ts";
import { DiscoveryLanguageExecutor } from "./language-execution.ts";
import { DiscoveryPostAuthorExecutor } from "./post-author-execution.ts";
import { DiscoverySourceAuthorExecutor } from "./source-author-execution.ts";
import { DiscoveryFacebookExecutor } from "./facebook-execution.ts";
import { DiscoveryPublicPageExecutor } from "./public-page-execution.ts";
import { DiscoveryWebSourceExecutor } from "./web-source-execution.ts";
import { FileDiscoveryContactAudit } from "./public-page-audit.ts";
import type { DiscoveryStep } from "./step.ts";
import { DiscoveryWebAuthorExecutor } from "./web-author-execution.ts";
import { DiscoveryReadGate } from "./read-capacity.ts";

export const discoveryAdvanceInput = z.strictObject({ campaign_id: z.string().min(1), expected_revision: z.number().int().min(0).max(4294967295), excluded_platforms: z.array(z.string()).default([]) });
export type DiscoveryAdvanceInput = z.infer<typeof discoveryAdvanceInput>;
export interface DiscoveryAdvanceResult {
  step: DiscoveryStep; campaign: ReturnType<DiscoveryRepository["getPlan"]>; counts: ReturnType<DiscoveryRepository["countCandidates"]>; provider_stop_code?: string;
  provider_diagnostics?: OpenCliDiagnostics;
  transcript: ReturnType<DiscoveryRepository["getTranscript"]> | null; profile: ReturnType<DiscoveryRepository["getProfile"]> | null;
  content: ReturnType<DiscoveryRepository["getContent"]> | null; identity: ReturnType<DiscoveryRepository["getIdentity"]> | null;
  content_targets?: ReturnType<DiscoveryRepository["getFacebookTargets"]>; facebook_identity?: ReturnType<DiscoveryRepository["getFacebookIdentity"]>;
  source_author?: ReturnType<DiscoveryRepository["getSourceAuthor"]>; source_author_timings?: { content_ms: number | null; profile_ms: number | null };
  web_source?: ReturnType<DiscoveryRepository["getWebSource"]>; public_page?: ReturnType<DiscoveryRepository["getPublicPage"]>;
  web_author?: ReturnType<DiscoveryRepository["finishWebAuthor"]> | ReturnType<DiscoveryRepository["finishWebAuthorRead"]>;
}
export class DiscoveryAdvanceExecutor {
  readonly #readGates = new WeakMap<DiscoveryRepository, DiscoveryReadGate>();
  readonly accounts: AccountContextService; readonly repositories: DiscoveryRepositoryResolver; readonly directories: AccountDatabases; readonly dependencies: DiscoveryDependencyResolver | undefined;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, directories: AccountDatabases, dependencies?: DiscoveryDependencyResolver) { this.accounts = accounts; this.repositories = repositories; this.directories = directories; this.dependencies = dependencies; }
  async advance(context: AccountContext, raw: unknown, deferred: readonly string[] = [], scoring: readonly string[] = [], signal: AbortSignal = context.signal, onAttempt?: (result: DiscoveryAdvanceResult) => void, policy: import("./step.ts").DiscoverySchedulingPolicy = {}): Promise<DiscoveryAdvanceResult> {
    const input = discoveryAdvanceInput.parse(raw); this.accounts.assertCurrent(context); signal.throwIfAborted();
    const owned = await this.repositories.resolve(context); owned.assertOwned(); this.accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
    let gate = this.#readGates.get(owned.repository);
    if (!gate) { gate = new DiscoveryReadGate(); this.#readGates.set(owned.repository, gate); }
    const excluded = [...new Set([...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES])];
    let step: DiscoveryStep = gate.full ? { kind: "busy" } : owned.repository.planStepScoring(input.campaign_id, input.expected_revision, excluded, deferred, scoring, { ...policy, busyPlatforms: [...gate.occupied, ...policy.busyPlatforms ?? []] });
    const resources = owned.repository.readResources(input.campaign_id, step);
    if (resources.some(resource => gate.occupied.includes(resource))) step = { kind: "busy" };
    const permit = step.kind !== "busy" && resources.length ? gate.acquire(resources) : null;
    return this.#perform(context, input, step, owned, signal, onAttempt).finally(() => permit?.release());
  }
  async #perform(context: AccountContext, input: DiscoveryAdvanceInput, step: DiscoveryStep, owned: Awaited<ReturnType<DiscoveryRepositoryResolver["resolve"]>>, signal: AbortSignal, onAttempt?: (result: DiscoveryAdvanceResult) => void): Promise<DiscoveryAdvanceResult> {
    const result: DiscoveryAdvanceResult = { step, campaign: owned.repository.getPlan(input.campaign_id), counts: owned.repository.countCandidates(input.campaign_id), transcript: null, profile: null, content: null, identity: null };
    onAttempt?.(result);
    if (!["busy", "target_reached", "needs_evidence", "search_exhausted"].includes(step.kind)) {
      if (!this.dependencies) throw new Error("discovery_browser_unavailable");
      const environment = await this.dependencies.reads(context); this.accounts.assertCurrent(context); owned.assertOwned(); signal.throwIfAborted();
      const browsers: DiscoveryBrowserResolver = { resolve: async (owner, profile) => {
        const binding = await environment.browsers.resolve(owner, profile), combined = AbortSignal.any([binding.signal, signal]);
        return { profile: binding.profile, signal: combined, assertCurrent: async () => { await binding.assertCurrent(); combined.throwIfAborted(); } };
      } };
      const reader = environment.reader.withDiagnostics(error => { if (error.diagnostics) result.provider_diagnostics = error.diagnostics; });
      const parameters = [this.accounts, this.repositories, browsers, reader] as const, base = { campaign_id: input.campaign_id, expected_revision: input.expected_revision };
      const observation = await (async () => {
        switch (step.kind) {
          case "read_article": case "resolve_article": {
            const executor = new DiscoveryWebAuthorExecutor(...parameters, this.dependencies!);
            result.web_author = step.kind === "read_article" ? await executor.readOnce(context, { ...base, search_hit_id: step.search_hit_id }) : await executor.resolveOnce(context, { ...base, search_hit_id: step.search_hit_id }); break;
          }
          case "search": await new DiscoverySearchExecutor(...parameters).searchOnce(context, { planId: input.campaign_id, revision: input.expected_revision, platform: step.platform, profileRef: step.profile_ref, limit: Math.min(20, creatorSearchSource(step.platform).maximum) }); break;
          case "read_video": await new DiscoveryVideoExecutor(...parameters).readOnce(context, { ...base, profile_ref: step.profile_ref, search_hit_id: step.search_hit_id }); break;
          case "read_profile": result.profile = await new DiscoveryProfileExecutor(...parameters).readOnce(context, { ...base, search_hit_id: step.search_hit_id }); break;
          case "read_content": result.content = await new DiscoveryLanguageExecutor(...parameters).readOnce(context, { ...base, search_hit_id: step.search_hit_id }) ?? await new DiscoveryContentExecutor(...parameters).readNativeOnce(context, { ...base, search_hit_id: step.search_hit_id }); break;
          case "read_identity": {
            result.identity = await new DiscoveryPostAuthorExecutor(...parameters).readOnce(context, { ...base, search_hit_id: step.search_hit_id });
            if (owned.repository.combinedPostReady(input.campaign_id, step.search_hit_id)) { result.profile = owned.repository.getProfile(input.campaign_id, step.search_hit_id); result.content = owned.repository.getContent(input.campaign_id, step.search_hit_id); }
            break;
          }
          case "read_source_author": { const observed = await new DiscoverySourceAuthorExecutor(...parameters).readOnce(context, { ...base, search_hit_id: step.search_hit_id }); result.source_author = observed.read; result.source_author_timings = observed.timings; break; }
          case "read_facebook_identity": result.facebook_identity = await new DiscoveryFacebookExecutor(...parameters).readIdentityOnce(context, { ...base, search_hit_id: step.search_hit_id }); if (result.facebook_identity.stop_code !== null) result.provider_stop_code = result.facebook_identity.stop_code; break;
          case "read_content_targets": result.content_targets = await new DiscoveryFacebookExecutor(...parameters).readTargetsOnce(context, { ...base, search_hit_id: step.search_hit_id }); break;
          case "read_public_page": case "read_web_source": {
            const directory = await this.directories.accountDirectory(context); directory.assertOwned(); this.accounts.assertCurrent(context);
            const audit = new FileDiscoveryContactAudit(directory.directory, () => directory.assertOwned());
            if (step.kind === "read_public_page") result.public_page = await new DiscoveryPublicPageExecutor(...parameters, audit).readOnce(context, { ...base, profile_ref: step.profile_ref, candidate_id: step.candidate_id, requested_url: step.requested_url });
            else result.web_source = await new DiscoveryWebSourceExecutor(...parameters, audit).readOnce(context, { ...base, search_hit_id: step.search_hit_id }); break;
          }
        }
      })().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
      owned.assertOwned();
      if (!observation.ok) {
        // 仅已落库的搜索停止可转成下一步检查点；依赖、程序和未知结果保持失败。
        const code = observation.error instanceof OpenCliError ? observation.error.code === "opencli_read_command_unavailable" ? "opencli_command_unavailable" : observation.error.code : null;
        const committed = step.kind === "search" && code !== null && owned.repository.getPlan(input.campaign_id).platform_runs.some(run => run.platform === step.platform && run.profile_ref === step.profile_ref && run.stop_code === code && run.status !== "searching");
        if (!committed) throw observation.error; result.provider_stop_code = code!;
      }
    }
    owned.assertOwned(); result.campaign = owned.repository.getPlan(input.campaign_id); result.counts = owned.repository.countCandidates(input.campaign_id); return result;
  }
}
