import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { AccountChangedError, type AccountContext, type AccountContextService } from "../identity/account-context.ts";
import { OpenCliError } from "../platforms/opencli/errors.ts";
import { TEMPORARILY_DISABLED_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import { prepareDiscoveryDraftInput, DiscoveryDraftError } from "./draft.ts";
import { DiscoveryDraftProgressState } from "./draft-progress.ts";
import type { DiscoveryDependencyResolver } from "./dependencies.ts";
import type { DiscoveryRepository } from "./repository.ts";
import type { DiscoveryRepositoryResolver, OwnedDiscoveryRepository } from "./search-execution.ts";
import { DiscoverySemanticPlanner, loadBundledDiscoveryQueryGuidance } from "./semantic-planner.ts";
import { DiscoverySemanticError } from "./semantic-client.ts";
import { HostOperationRuns, hostWorkOwner } from "./host-work.ts";

export function discoveryDraftMatchesRequest(campaign: ReturnType<DiscoveryRepository["getPlan"]>, input: z.output<typeof prepareDiscoveryDraftInput>): boolean {
  const criteria = campaign.criteria;
  return campaign.id === input.id && campaign.brief === input.brief && (input.refinement === null || JSON.stringify(criteria.refinement) === JSON.stringify(input.refinement)) && campaign.opencli_version === input.opencli_version && campaign.phase === "draft" && criteria.region === "" && campaign.used_calls === 0
    && (input.target_count === null || campaign.target_count === input.target_count)
    && (input.min_followers_override === null || BigInt(input.min_followers_override) === (typeof criteria.min_subscribers === "bigint" ? criteria.min_subscribers : typeof criteria.min_subscribers === "number" && Number.isSafeInteger(criteria.min_subscribers) ? BigInt(criteria.min_subscribers) : null))
    && (input.criteria_overrides === null || ["language", "category"].every(key => input.criteria_overrides![key as "language" | "category"] === null || criteria[key] === input.criteria_overrides![key as "language" | "category"]) && criteria.requires_ai_tool_demo === input.criteria_overrides.requires_ai_tool_demo && criteria.requires_public_tool_url === input.criteria_overrides.requires_public_tool_url)
    && campaign.scopes.length > 0 && input.scope_overrides.every(requested => campaign.scopes.some(scope => scope.platform === requested.platform && (JSON.stringify(scope.keywords) === JSON.stringify(requested.keywords) || typeof criteria.language === "string" && !!criteria.language.trim() && !/-tag$|-node$/u.test(requested.platform) && scope.keywords.length > 0 && scope.keywords.length <= requested.keywords.length)))
    && campaign.scopes.every(scope => scope.profile_ref === input.profile_ref);
}
function draftFailure(error: unknown): DiscoveryDraftError {
  if (error instanceof DiscoveryDraftError) return error;
  if (error instanceof AccountChangedError) return new DiscoveryDraftError("discovery_draft_account_changed");
  if (error instanceof OpenCliError) return new DiscoveryDraftError("discovery_draft_browser_unavailable");
  if (error instanceof DiscoverySemanticError) return new DiscoveryDraftError(({ semantic_unavailable: "discovery_draft_codex_unavailable", semantic_login_required: "discovery_draft_codex_login_required", semantic_failed: "discovery_draft_codex_failed", semantic_timeout: "discovery_draft_codex_timeout", semantic_invalid_output: "discovery_draft_plan_invalid" } as const)[error.code]);
  return new DiscoveryDraftError("discovery_draft_read_unavailable");
}

export class DiscoveryDraftRuntime {
  readonly #accounts: AccountContextService; readonly #repositories: DiscoveryRepositoryResolver; readonly #dependencies: DiscoveryDependencyResolver | undefined;
  #busy = false; #progress: DiscoveryDraftProgressState | undefined; readonly #shutdown = new AbortController(); readonly #tasks = new Set<Promise<unknown>>();
  readonly #runs: HostOperationRuns;
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, dependencies?: DiscoveryDependencyResolver, runs = new HostOperationRuns()) { this.#accounts = accounts; this.#repositories = repositories; this.#dependencies = dependencies; this.#runs = runs; }
  launch(context: AccountContext, raw: unknown) { this.#accounts.assertCurrent(context); const input = prepareDiscoveryDraftInput.parse(raw); return { id: input.id, ...this.#runs.start(hostWorkOwner(context), { kind: "draft", input }, () => this.prepare(context, input), input.id) }; }
  progress(context: AccountContext, id: string) { this.#accounts.assertCurrent(context); return this.#progress?.snapshot(context, id) ?? null; }
  #assert(context: AccountContext, owned: OwnedDiscoveryRepository): void {
    this.#accounts.assertCurrent(context); owned.assertOwned();
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_draft_repository_owner_mismatch");
  }
  async prepare(context: AccountContext, raw: unknown) {
    const input = prepareDiscoveryDraftInput.parse(raw);
    input.scope_overrides = input.scope_overrides.filter(scope => !TEMPORARILY_DISABLED_CREATOR_SOURCES.includes(scope.platform));
    this.#accounts.assertCurrent(context); this.#shutdown.signal.throwIfAborted();
    if (this.#busy) throw new DiscoveryDraftError("discovery_draft_busy");
    this.#busy = true; const progress = new DiscoveryDraftProgressState(context, input.id); this.#progress = progress;
    const signal = AbortSignal.any([context.signal, this.#shutdown.signal]);
    const work = (async () => {
      const owned = await this.#repositories.resolve(context); this.#assert(context, owned);
      const assertCurrent = async () => { this.#assert(context, owned); signal.throwIfAborted(); };
      const previous = input.refinement === null ? undefined : owned.repository.getPlan(input.refinement.campaign_id);
      if (input.refinement !== null) {
        if (!previous) throw new DiscoveryDraftError("discovery_draft_conflict");
        if (previous.revision !== input.refinement.expected_revision || previous.approved_at === null || previous.id === input.id) throw new DiscoveryDraftError("discovery_draft_conflict");
        input.brief = `原任务需求：\n${previous.brief}\n已确认的计划设定：\n${stringifyApplicationJson({ criteria: previous.criteria, target_count: previous.target_count, scopes: previous.scopes })}\n本次修改（仅覆盖明确修改的条件）：\n${input.brief}`;
        prepareDiscoveryDraftInput.parse(input);
      }
      const prior = owned.repository.findPlan(input.id);
      if (prior) { if (!discoveryDraftMatchesRequest(prior, input)) throw new DiscoveryDraftError("discovery_draft_conflict"); return prior; }
      if (!this.#dependencies) throw new DiscoveryDraftError("discovery_draft_browser_unavailable");
      const environment = await this.#dependencies.reads(context), browser = await environment.browsers.resolve(context, input.profile_ref);
      if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
      await environment.reader.checkBrowserProfile(browser); await assertCurrent();
      if (environment.reader.version !== input.opencli_version) throw new DiscoveryDraftError("discovery_draft_browser_unavailable");
      const capabilities = await environment.reader.creatorSearchCapabilities(); await assertCurrent();
      const sources = capabilities.filter(source => !TEMPORARILY_DISABLED_CREATOR_SOURCES.includes(source.platform)).map(source => ({ platform: source.platform, registered: source.registered, command: source.command, allowed_values: [] as string[] }));
      progress.phase("checking_codex"); const client = await this.#dependencies.semantic(context), planner = new DiscoverySemanticPlanner(client, await loadBundledDiscoveryQueryGuidance()); await assertCurrent();
      const draft = await planner.prepare(input, sources, signal, assertCurrent, phase => progress.phase(phase), previous);
      await assertCurrent(); progress.phase("saving_plan");
      const saved = await Promise.resolve().then(() => owned.repository.create(draft)).then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
      owned.assertOwned();
      const campaign = owned.repository.findPlan(input.id);
      if (!saved.ok) {
        if (campaign && discoveryDraftMatchesRequest(campaign, input)) { await assertCurrent(); return campaign; }
        throw new DiscoveryDraftError("discovery_draft_outcome_unknown");
      }
      await assertCurrent(); if (!campaign || !discoveryDraftMatchesRequest(campaign, input)) throw new DiscoveryDraftError("discovery_draft_outcome_unknown");
      return campaign;
    })();
    this.#tasks.add(work);
    const result = await work.then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
    this.#tasks.delete(work); this.#busy = false;
    if (result.ok) { progress.phase("finished"); return result.value; }
    const failure = draftFailure(result.error), excerpts = failure.excerpts.filter(excerpt => !!excerpt.trim() && input.brief.includes(excerpt));
    progress.fail(failure.code, excerpts);
    // 只有原本已分类的业务故障转换成合同错误；未知库/程序故障保留原异常。
    if (result.error instanceof DiscoveryDraftError || result.error instanceof OpenCliError || result.error instanceof DiscoverySemanticError || result.error instanceof AccountChangedError) throw failure;
    throw result.error;
  }
  stopAcceptingWork(): void { this.#shutdown.abort(new Error("discovery_draft_stopping")); }
  async close(): Promise<void> {
    this.stopAcceptingWork(); const outcomes = await Promise.allSettled([...this.#tasks]);
    const errors = outcomes.filter((result): result is PromiseRejectedResult => result.status === "rejected").map(result => result.reason as unknown).filter(error => error !== this.#shutdown.signal.reason && !(error instanceof AccountChangedError));
    if (errors.length) throw new AggregateError(errors, "discovery_draft_drain_failed");
  }
}
