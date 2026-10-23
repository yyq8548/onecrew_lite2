import { z } from "zod";
import { setImmediate as yieldTurn } from "node:timers/promises";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import type { DiscoveryDependencyResolver } from "./dependencies.ts";
import { DiscoveryDraftRuntime } from "./draft-runtime.ts";
import type { DiscoveryExecutionLibrary, DiscoveryExecutionSupervisor } from "./execution-supervisor.ts";
import { registerDiscoveryDraftOperations, registerDiscoveryReadOperations, registerDiscoveryWriteOperations } from "./operations.ts";
import type { DiscoveryRepositoryResolver } from "./search-execution.ts";
import { DiscoverySearchExecutor } from "./search-execution.ts";
import { DiscoveryVideoExecutor } from "./video-execution.ts";
import { DiscoverySemanticMatcher, loadBundledDiscoveryMatchRubric } from "./semantic-match.ts";
import { DiscoveryProfileExecutor } from "./profile-execution.ts";
import { DiscoveryContentExecutor } from "./content-execution.ts";
import { DiscoveryLanguageExecutor } from "./language-execution.ts";
import { DiscoveryAdvanceExecutor, discoveryAdvanceInput } from "./advance.ts";
import { discoveryExecutionFailureCode, discoveryMatchFailure } from "./execution-failure.ts";
import { discoveryAdvanceFailure } from "./execution-material.ts";
import { discoveryReadFailureLocation, recordDiscoveryReadTiming } from "./scheduler-metrics.ts";
import { DiscoveryQueryRoundExecutor } from "./query-round-execution.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { INACTIVE_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import { DiscoveryScheduler } from "./scheduler.ts";
import { discoveryExecutionInput } from "./execution-supervisor.ts";
import { DiscoveryWorkspace, workspacePresence } from "./workspace.ts";
import { HostSemanticClient, HostOperationRuns, hostWorkOwner, hostWorkClaimInput, hostWorkSubmitInput, hostWorkGetInput } from "./host-work.ts";

export interface DiscoveryLibraryProjection { readiness(context: AccountContext): Promise<void>; publish(context: AccountContext, campaign: string, revision: number): Promise<DiscoveryExecutionLibrary> }
export interface DiscoveryRuntimeConfiguration { accounts: AccountContextService; repositories: DiscoveryRepositoryResolver; directories: AccountDatabases; dependencies?: DiscoveryDependencyResolver; supervisor: DiscoveryExecutionSupervisor; library: DiscoveryLibraryProjection; onStarted?(context: AccountContext, campaign: string): void }

// 统一主程序创建并注入唯一supervisor；本模块不创建第二个准入锁或账号选择器。
export function createDiscoveryRuntime(configuration: DiscoveryRuntimeConfiguration) {
  const { accounts, repositories, supervisor, library } = configuration, runs = new HostOperationRuns(), drafts = new DiscoveryDraftRuntime(accounts, repositories, configuration.dependencies, runs);
  const advances = new DiscoveryAdvanceExecutor(accounts, repositories, configuration.directories, configuration.dependencies);
  const workspace = new DiscoveryWorkspace(accounts, repositories, supervisor, { resolve: context => configuration.directories.accountDirectory(context) });
  let closing = false;
  const matchers = new WeakMap<AccountContext, Promise<DiscoverySemanticMatcher>>();
  function matcher(context: AccountContext): Promise<DiscoverySemanticMatcher> {
    let selected = matchers.get(context);
    if (!selected) {
      selected = loadBundledDiscoveryMatchRubric().then(rubric => new DiscoverySemanticMatcher(accounts, repositories, { async structured(request) {
        assertAccepting(context); if (!configuration.dependencies) throw new Error("initial_match_discovery_draft_codex_unavailable");
        const outcome = await (async () => { const client = await configuration.dependencies!.semantic(context); assertAccepting(context); return client.structured(request); })().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
        if (!outcome.ok) throw discoveryMatchFailure(outcome.error); return outcome.value;
      }, async scoringIdentity() { assertAccepting(context); if (!configuration.dependencies) throw new Error("initial_match_discovery_draft_codex_unavailable"); const client = await configuration.dependencies.semantic(context); assertAccepting(context); return await client.scoringIdentity?.() ?? "cli-default"; } }, rubric));
      matchers.set(context, selected);
    }
    return selected;
  }
  function assertAccepting(context: AccountContext): void { accounts.assertCurrent(context); if (closing) throw new Error("discovery_execution_stopping"); }
  async function publish(context: AccountContext, campaign: string, draining = false, expectedRevision?: number): Promise<DiscoveryExecutionLibrary> {
    const assertAdmission = () => { if (draining) accounts.assertCurrent(context); else assertAccepting(context); };
    assertAdmission(); const owned = await repositories.resolve(context); owned.assertOwned(); assertAdmission();
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
    const plan = owned.repository.getPlan(campaign); if (expectedRevision !== undefined && plan.revision !== expectedRevision) throw new Error("discovery_revision_changed");
    await library.readiness(context); assertAdmission(); owned.assertOwned();
    for (let attempt = 0; ; attempt++) {
      const outcome = await library.publish(context, campaign, plan.revision).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
      if (outcome.ok) return outcome.value;
      if (!(outcome.error instanceof Error) || !("code" in outcome.error) || outcome.error.code !== "library_source_stale" || attempt === 2) throw outcome.error;
      await yieldTurn(); assertAdmission(); owned.assertOwned();
      if (owned.repository.getPlan(campaign).revision !== plan.revision) throw new Error("discovery_revision_changed");
    }
  }
  const scheduler = new DiscoveryScheduler({ accounts, repositories, directories: configuration.directories, supervisor, advances, matcher, metrics: context => workspace.metrics(context), library: { readiness: context => library.readiness(context), publish: (context, campaign, revision) => publish(context, campaign, true, revision) }, ...(configuration.dependencies ? { dependencies: configuration.dependencies } : {}), ...(configuration.onStarted ? { onStarted: configuration.onStarted } : {}) });
  function stopAcceptingWork(): void { closing = true; configuration.dependencies?.stopSemantic?.(); drafts.stopAcceptingWork(); supervisor.stopAcceptingWork(); }
  return Object.freeze({
    blocksBackendSync: () => supervisor.blocksBackendSync(),
    workspaceActivity: (context: AccountContext) => workspace.snapshot(context),
    workspaceHistory: (context: AccountContext) => workspace.history(context),
    stopAcceptingWork,
    async close(): Promise<void> {
      stopAcceptingWork(); const results = await Promise.allSettled([drafts.close(), supervisor.close()]);
      const errors = results.filter((result): result is PromiseRejectedResult => result.status === "rejected").map(result => result.reason as unknown);
      const flushed = await workspace.close().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
      if (!flushed.ok) errors.push(flushed.error);
      if (errors.length) throw new AggregateError(errors, "discovery_runtime_drain_failed");
    },
    registerOperations(registry: OperationRegistry): void {
      registerDiscoveryReadOperations(registry, accounts, repositories); registerDiscoveryWriteOperations(registry, accounts, repositories); registerDiscoveryDraftOperations(registry, drafts);
      async function host(context: AccountContext) { assertAccepting(context); const client = await configuration.dependencies?.semantic(context); if (!(client instanceof HostSemanticClient)) throw new Error("host_work_unavailable"); assertAccepting(context); return client; }
      registry.register("onecrew_discovery_host_work_claim", z.strictObject({ input: hostWorkClaimInput }), async ({ input }, context) => (await host(context)).claim(input, () => {
        assertAccepting(context);
        const current = supervisor.snapshot(context);
        const execution = current && (input.campaign_id === undefined || input.campaign_id === current.campaign_id) ? { campaign_id: current.campaign_id, revision: current.revision, stage: current.stage, used: current.used, reason: current.reason, failure_code: current.failure_code } : null;
        if (runs.pending(hostWorkOwner(context), input.campaign_id)) return { state: "planning", execution };
        if (execution) {
          const state = ["searching", "verifying"].includes(execution.stage) ? "running" : ["paused", "pausing"].includes(execution.stage) ? "paused" : execution.stage === "finished" ? "completed" : "needs_action";
          return { state, execution };
        }
        return { state: "idle", execution: null };
      }));
      registry.register("onecrew_discovery_host_work_get", z.strictObject({ input: hostWorkGetInput }), async ({ input }, context) => (await host(context)).get(input));
      registry.register("onecrew_discovery_host_work_submit", z.strictObject({ input: hostWorkSubmitInput }), async ({ input }, context) => (await host(context)).submit(input));
      registry.register("onecrew_discovery_host_operation_get", z.strictObject({ input: z.strictObject({ operation_id: z.string().regex(/^[a-f0-9]{64}$/u) }) }), async ({ input }, context) => { assertAccepting(context); return runs.get(hostWorkOwner(context), input.operation_id); });
      registry.register("onecrew_discovery_library_sync", z.strictObject({ campaignId: z.string().min(1) }), async ({ campaignId }, context) => supervisor.ownTask(context, () => publish(context, campaignId)));
      registry.register("onecrew_discovery_execution_get", z.strictObject({}), async (_input, context) => supervisor.snapshot(context));
      registry.register("onecrew_discovery_read_failures", z.strictObject({ campaignId: z.string().min(1) }), async ({ campaignId }, context) => {
        assertAccepting(context); const owned = await repositories.resolve(context); owned.assertOwned(); assertAccepting(context);
        if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
        owned.repository.getPlan(campaignId);
        const failures = await (await workspace.metrics(context)).recentFailures(campaignId); owned.assertOwned(); assertAccepting(context); return failures;
      });
      registry.register("onecrew_discovery_workspace_presence", z.strictObject({ input: workspacePresence }), async ({ input }, context) => workspace.presence(context, input));
      registry.register("onecrew_discovery_execution_start", z.strictObject({ input: discoveryExecutionInput }), async ({ input }, context) => { assertAccepting(context); return scheduler.start(context, input); });
      registry.register("onecrew_discovery_queries_expand", z.strictObject({ input: discoveryAdvanceInput }), async ({ input }, context) => {
        assertAccepting(context);
        return runs.start(hostWorkOwner(context), { kind: "queries", input }, () => supervisor.ownTask(context, async () => {
          const expanded = await new DiscoveryQueryRoundExecutor(accounts, repositories, configuration.dependencies).expand(context, input.campaign_id, input.expected_revision, input.excluded_platforms);
          const owned = await repositories.resolve(context); owned.assertOwned(); assertAccepting(context);
          return { expanded, campaign: owned.repository.getPlan(input.campaign_id) };
        }), input.campaign_id);
      });
      registry.register("onecrew_discovery_advance", z.strictObject({ input: discoveryAdvanceInput }), async ({ input }, context) => {
        assertAccepting(context);
        return supervisor.ownTask(context, async () => {
          const owned = await repositories.resolve(context); owned.assertOwned(); assertAccepting(context);
          if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
          const campaign = owned.repository.getPlan(input.campaign_id);
          if (campaign.phase !== "active" || campaign.revision !== input.expected_revision) throw new Error("discovery_step_approval_changed");
          const ready = owned.repository.readyReviews(input.campaign_id, input.expected_revision, [...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES]);
          if (ready.length) return { step: { kind: "needs_evidence" }, campaign, counts: owned.repository.countCandidates(input.campaign_id), transcript: null, profile: null, content: null, identity: null, next_action: "review_saved_material", review_ready: ready };
          const web_author_ready = owned.repository.webAuthorReady(input.campaign_id, input.expected_revision, [...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES, ...owned.repository.reachedSources(input.campaign_id)]);
          if (web_author_ready.length) return { step: { kind: "needs_evidence" }, campaign, counts: owned.repository.countCandidates(input.campaign_id), transcript: null, profile: null, content: null, identity: null, next_action: "resolve_web_author", review_ready: [], web_author_ready };
          const metrics = await workspace.metrics(context), began = performance.now();
          let attempted: import("./advance.ts").DiscoveryAdvanceResult | undefined;
          const outcome = await advances.advance(context, input, [], [], context.signal, value => { attempted = value; }).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
          owned.assertOwned(); assertAccepting(context);
          const result = outcome.ok ? outcome.value : attempted;
          if (result) {
            recordDiscoveryReadTiming(metrics, input.campaign_id, result, Math.floor(performance.now() - began));
            const failure = outcome.ok ? discoveryAdvanceFailure(result) : discoveryExecutionFailureCode(outcome.error);
            const diagnostic = !outcome.ok && outcome.error instanceof OpenCliError ? outcome.error.diagnostics : result.provider_diagnostics;
            if (failure !== null) metrics.enqueueFailure(input.campaign_id, { recorded_at: Date.now(), ...discoveryReadFailureLocation(result), elapsed_ms: Math.floor(performance.now() - began), call_number: owned.repository.getPlan(input.campaign_id).used_calls, code: discoveryExecutionFailureCode(failure), ...(diagnostic ? { diagnostic } : {}) });
          }
          await metrics.flushPending();
          if (!outcome.ok) throw outcome.error;
          const review_ready = owned.repository.readyReviews(input.campaign_id, input.expected_revision, [...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES]);
          const authors = owned.repository.webAuthorReady(input.campaign_id, input.expected_revision, [...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES, ...owned.repository.reachedSources(input.campaign_id)]);
          const unavailable = [...input.excluded_platforms, ...INACTIVE_CREATOR_SOURCES, ...owned.repository.reachedSources(input.campaign_id), ...owned.repository.readStops(input.campaign_id).map(stop => stop.platform)];
          const expandable = outcome.value.campaign.platform_runs.some(run => run.status === "completed" && !unavailable.includes(run.platform));
          return { ...outcome.value, next_action: review_ready.length ? "review_saved_material" : authors.length ? "resolve_web_author" : "advance", review_ready, web_author_ready: authors, query_expansion_available: ["needs_evidence", "search_exhausted"].includes(outcome.value.step.kind) && expandable };
        });
      });
      registry.register("onecrew_discovery_evidence_collect", z.strictObject({ input: z.strictObject({ campaign_id: z.string().min(1), expected_revision: z.number().int().min(0).max(4294967295), search_hit_id: z.string().min(1), profile_ref: z.string().min(1), kind: z.enum(["transcript", "profile", "content"]) }) }), async ({ input }, context) => {
        assertAccepting(context); const running = supervisor.snapshot(context);
        if (running && ["searching", "verifying", "pausing"].includes(running.stage)) throw new Error("evidence_execution_busy");
        const owned = await repositories.resolve(context); owned.assertOwned(); assertAccepting(context);
        if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
        const campaign = owned.repository.getPlan(input.campaign_id);
        if (campaign.phase !== "active" || campaign.revision !== input.expected_revision || !campaign.scopes.some(scope => scope.profile_ref === input.profile_ref)) throw new Error("evidence_approval_changed");
        if (input.kind === "transcript") throw new Error("discovery_transcript_disabled");
        if (!configuration.dependencies) throw new Error("discovery_browser_unavailable");
        const environment = await configuration.dependencies.reads(context); assertAccepting(context);
        return supervisor.ownTask(context, async () => {
          const parameters = [accounts, repositories, environment.browsers, environment.reader] as const;
          const receipt = input.kind === "profile" ? await new DiscoveryProfileExecutor(...parameters).readOnce(context, input) : await new DiscoveryLanguageExecutor(...parameters).readOnce(context, input) ?? await new DiscoveryContentExecutor(...parameters).readNativeOnce(context, input);
          return { campaign_id: input.campaign_id, search_hit_id: input.search_hit_id, status: receipt.status };
        });
      });
      const matchInput = z.strictObject({ input: z.strictObject({ campaign_id: z.string().min(1), expected_revision: z.number().int().min(1).max(4294967295), search_hit_id: z.string().min(1) }) });
      for (const reassess of [false, true]) registry.register(reassess ? "onecrew_discovery_match_reevaluate" : "onecrew_discovery_match_evaluate", matchInput, async ({ input }, context) => {
        assertAccepting(context);
        return runs.start(hostWorkOwner(context), { kind: "score", reassess, input }, () => supervisor.ownTask(context, async () => {
          const scorer = await matcher(context), prepared = await scorer.prepare(context, input, reassess);
          return prepared.state === "skipped" ? null : prepared.state === "cached" ? prepared.receipt : scorer.evaluate(context, prepared.prepared);
        }));
      });
      const searchInput = z.strictObject({ id: z.string().min(1), expectedRevision: z.number().int().min(0).max(4294967295), profileRef: z.string().min(1), limit: z.number().int().min(0).max(255), recovery: z.strictObject({ expected_stop_code: z.string().min(1), expected_stopped_at: z.number().int().safe() }).nullish() });
      async function search(context: AccountContext, input: z.infer<typeof searchInput>, platform: string) {
        assertAccepting(context);
        if (!configuration.dependencies) throw new Error("discovery_browser_unavailable");
        const environment = await configuration.dependencies.reads(context); assertAccepting(context);
        return supervisor.ownTask(context, async () => {
          const result = await new DiscoverySearchExecutor(accounts, repositories, environment.browsers, environment.reader).searchOnce(context, { planId: input.id, revision: input.expectedRevision, platform, profileRef: input.profileRef, limit: input.limit, explicitRecovery: true, recovery: input.recovery ?? null });
          return { campaign: result.plan, returned_hits: result.returned_hits, search_claim: { campaign_id: result.search_claim.planId, expected_revision: result.search_claim.expectedRevision, platform: result.search_claim.platform, profile_ref: result.search_claim.profileRef, call_number: result.search_claim.callNumber, query: result.search_claim.query }, returned_sources: result.returned_sources };
        });
      }
      registry.register("onecrew_discovery_youtube_search_once", searchInput, async (input, context) => search(context, input, "youtube"));
      registry.register("onecrew_discovery_search_once", searchInput.extend({ platform: z.string().min(1) }), async (input, context) => search(context, input, input.platform));
      registry.register("onecrew_discovery_youtube_verify_hit_once", searchInput.omit({ limit: true }).extend({ searchHitId: z.string().min(1) }), async (input, context) => {
        assertAccepting(context); if (!configuration.dependencies) throw new Error("discovery_browser_unavailable");
        const environment = await configuration.dependencies.reads(context); assertAccepting(context);
        return supervisor.ownTask(context, () => new DiscoveryVideoExecutor(accounts, repositories, environment.browsers, environment.reader).readOnce(context, { campaign_id: input.id, expected_revision: input.expectedRevision, profile_ref: input.profileRef, search_hit_id: input.searchHitId }));
      });
      registry.register("onecrew_discovery_execution_pause", z.strictObject({ campaignId: z.string().min(1) }), async ({ campaignId }, context) => {
        assertAccepting(context); const status = supervisor.snapshot(context);
        if (!status || status.campaign_id !== campaignId) throw new Error("discovery_execution_campaign_changed");
        return supervisor.pause(context);
      });
    },
  });
}
