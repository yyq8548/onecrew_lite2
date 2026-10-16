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
import { DiscoveryTranscriptExecutor } from "./transcript-execution.ts";
import { DiscoveryContentExecutor } from "./content-execution.ts";
import { DiscoveryLanguageExecutor } from "./language-execution.ts";
import { DiscoveryAdvanceExecutor, discoveryAdvanceInput } from "./advance.ts";
import { discoveryMatchFailure } from "./execution-failure.ts";
import { DiscoveryScheduler } from "./scheduler.ts";
import { discoveryExecutionInput } from "./execution-supervisor.ts";
import { DiscoveryWorkspace, workspacePresence } from "./workspace.ts";

export interface DiscoveryLibraryProjection { readiness(context: AccountContext): Promise<void>; publish(context: AccountContext, campaign: string, revision: number): Promise<DiscoveryExecutionLibrary> }
export interface DiscoveryRuntimeConfiguration { accounts: AccountContextService; repositories: DiscoveryRepositoryResolver; directories: AccountDatabases; dependencies?: DiscoveryDependencyResolver; supervisor: DiscoveryExecutionSupervisor; library: DiscoveryLibraryProjection; onStarted?(context: AccountContext, campaign: string): void }

// 统一主程序创建并注入唯一supervisor；本模块不创建第二个准入锁或账号选择器。
export function createDiscoveryRuntime(configuration: DiscoveryRuntimeConfiguration) {
  const { accounts, repositories, supervisor, library } = configuration, drafts = new DiscoveryDraftRuntime(accounts, repositories, configuration.dependencies);
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
      } }, rubric));
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
  function stopAcceptingWork(): void { closing = true; drafts.stopAcceptingWork(); supervisor.stopAcceptingWork(); }
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
      registry.register("onecrew_discovery_library_sync", z.strictObject({ campaignId: z.string().min(1) }), async ({ campaignId }, context) => supervisor.ownTask(context, () => publish(context, campaignId)));
      registry.register("onecrew_discovery_execution_get", z.strictObject({}), async (_input, context) => supervisor.snapshot(context));
      registry.register("onecrew_discovery_workspace_presence", z.strictObject({ input: workspacePresence }), async ({ input }, context) => workspace.presence(context, input));
      registry.register("onecrew_discovery_execution_start", z.strictObject({ input: discoveryExecutionInput }), async ({ input }, context) => { assertAccepting(context); return scheduler.start(context, input); });
      registry.register("onecrew_discovery_advance", z.strictObject({ input: discoveryAdvanceInput }), async ({ input }, context) => { assertAccepting(context); return supervisor.ownTask(context, () => advances.advance(context, input)); });
      registry.register("onecrew_discovery_evidence_collect", z.strictObject({ input: z.strictObject({ campaign_id: z.string().min(1), expected_revision: z.number().int().min(0).max(4294967295), search_hit_id: z.string().min(1), profile_ref: z.string().min(1), kind: z.enum(["transcript", "profile", "content"]) }) }), async ({ input }, context) => {
        assertAccepting(context); const running = supervisor.snapshot(context);
        if (running && ["searching", "verifying", "pausing"].includes(running.stage)) throw new Error("evidence_execution_busy");
        const owned = await repositories.resolve(context); owned.assertOwned(); assertAccepting(context);
        if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
        const campaign = owned.repository.getPlan(input.campaign_id);
        if (campaign.phase !== "active" || campaign.revision !== input.expected_revision || !campaign.scopes.some(scope => scope.profile_ref === input.profile_ref)) throw new Error("evidence_approval_changed");
        if (!configuration.dependencies) throw new Error("discovery_browser_unavailable");
        const environment = await configuration.dependencies.reads(context); assertAccepting(context);
        return supervisor.ownTask(context, async () => {
          const parameters = [accounts, repositories, environment.browsers, environment.reader] as const;
          const receipt = input.kind === "transcript" ? await new DiscoveryTranscriptExecutor(...parameters).readOnce(context, input) : input.kind === "profile" ? await new DiscoveryProfileExecutor(...parameters).readOnce(context, input) : await new DiscoveryLanguageExecutor(...parameters).readOnce(context, input) ?? await new DiscoveryContentExecutor(...parameters).readNativeOnce(context, input);
          return { campaign_id: input.campaign_id, search_hit_id: input.search_hit_id, status: receipt.status };
        });
      });
      const matchInput = z.strictObject({ input: z.strictObject({ campaign_id: z.string().min(1), expected_revision: z.number().int().min(1).max(4294967295), search_hit_id: z.string().min(1) }) });
      for (const reassess of [false, true]) registry.register(reassess ? "onecrew_discovery_match_reevaluate" : "onecrew_discovery_match_evaluate", matchInput, async ({ input }, context) => {
        assertAccepting(context);
        return supervisor.ownTask(context, async () => {
          const scorer = await matcher(context), prepared = await scorer.prepare(context, input, reassess);
          return prepared.state === "skipped" ? null : prepared.state === "cached" ? prepared.receipt : scorer.evaluate(context, prepared.prepared);
        });
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
