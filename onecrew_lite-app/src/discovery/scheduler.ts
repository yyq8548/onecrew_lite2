import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { AccountDatabases } from "../identity/account-databases.ts";
import type { DiscoveryRepositoryResolver } from "./search-execution.ts";
import type { DiscoveryDependencyResolver } from "./dependencies.ts";
import { DiscoveryAdvanceExecutor } from "./advance.ts";
import { discoveryExecutionInput, type DiscoveryExecutionOwner, type DiscoveryExecutionStage, type DiscoveryExecutionSupervisor } from "./execution-supervisor.ts";
import type { DiscoverySemanticMatcher, PreparedSemanticDiscoveryMatch } from "./semantic-match.ts";
import { resolveDiscoveryScoreDirectory, SavedDiscoveryScores } from "./saved-scores.ts";
import { DISCOVERY_READ_AHEAD, discoveryScoreCapacity, DiscoveryProjectionLane } from "./projection-lane.ts";
import { DISCOVERY_SCORE_CONCURRENCY } from "./score-gate.ts";
import { discoveryAdvanceFailure, discoveryScoreHit, discoveryScoringPlatforms } from "./execution-material.ts";
import { discoveryExecutionFailureCode, discoveryScoreDeferred } from "./execution-failure.ts";
import { FileDiscoveryExecutionMetrics } from "./execution-metrics.ts";
import { DiscoveryQueryRoundExecutor } from "./query-round-execution.ts";
import { discoveryReadFailureLocation, recordDiscoveryReadTiming } from "./scheduler-metrics.ts";
import { RETIRED_CREATOR_SOURCES } from "../platforms/opencli/sources.ts";
import type { DiscoveryLibraryProjection } from "./runtime.ts";

interface SchedulerConfiguration {
  accounts: AccountContextService; repositories: DiscoveryRepositoryResolver; directories: AccountDatabases; supervisor: DiscoveryExecutionSupervisor;
  advances: DiscoveryAdvanceExecutor; library: DiscoveryLibraryProjection; dependencies?: DiscoveryDependencyResolver;
  matcher(context: AccountContext): Promise<DiscoverySemanticMatcher>;
  metrics(context: AccountContext): Promise<FileDiscoveryExecutionMetrics>;
  onStarted?(context: AccountContext, campaign: string): void;
}
type ScoreOutcome = { ok: true; saved: boolean; refresh?: boolean } | { ok: false; error: unknown };
interface ScoreJob { hit: string; deferred: string[]; prepared: PreparedSemanticDiscoveryMatch; state: "queued" | "running" | "settled"; task?: Promise<ScoreOutcome>; outcome?: ScoreOutcome; preparedAt: number }
const terminal = (kind: string): { stage: DiscoveryExecutionStage; reason: string } => kind === "target_reached" ? { stage: "finished", reason: kind } : kind === "budget_exhausted" ? { stage: "budget_exhausted", reason: kind } : ["needs_evidence", "search_exhausted"].includes(kind) ? { stage: "needs_evidence", reason: kind } : { stage: "paused", reason: "busy" };
async function queueWork<T>(work: () => Promise<T>): Promise<T> { return work().then(value => value, error => { throw new Error("score_queue_unavailable", { cause: error }); }); }

export class DiscoveryScheduler {
  readonly #configuration: SchedulerConfiguration; readonly #recovered = new WeakSet<object>();
  constructor(configuration: SchedulerConfiguration) { this.#configuration = configuration; }
  async start(context: AccountContext, raw: unknown) {
    const input = discoveryExecutionInput.parse(raw), configuration = this.#configuration, { accounts, repositories, supervisor } = configuration;
    accounts.assertCurrent(context); const owned = await repositories.resolve(context); owned.assertOwned(); accounts.assertCurrent(context);
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("discovery_repository_owner_mismatch");
    const campaign = owned.repository.getPlan(input.campaign_id);
    const validate = async () => {
      accounts.assertCurrent(context); owned.assertOwned();
      if (!this.#recovered.has(owned.repository)) { owned.repository.recoverInterruptedReads(); this.#recovered.add(owned.repository); }
      const current = owned.repository.getPlan(input.campaign_id);
      if (current.phase !== "active" || current.revision !== input.expected_revision || current.scopes.some(scope => scope.profile_ref !== input.profile_ref)) throw new Error("discovery_execution_approval_changed");
      await configuration.library.readiness(context); accounts.assertCurrent(context); owned.assertOwned();
      const next = owned.repository.planStep(input.campaign_id, input.expected_revision, [...new Set([...input.excluded_platforms, ...RETIRED_CREATOR_SOURCES])]);
      if (!["target_reached", "budget_exhausted", "needs_evidence", "search_exhausted", "busy"].includes(next.kind)) {
        if (!configuration.dependencies) throw new Error("discovery_dependency_unavailable");
        const environment = await configuration.dependencies.reads(context), browser = await environment.browsers.resolve(context, input.profile_ref); await browser.assertCurrent();
        if (browser.profile !== input.profile_ref) throw new Error("discovery_browser_profile_mismatch");
        await environment.reader.checkBrowserProfile(browser); accounts.assertCurrent(context); owned.assertOwned();
      }
    };
    return supervisor.start(context, input, campaign.used_calls, validate, owner => this.#run(owner), () => configuration.onStarted?.(context, input.campaign_id));
  }
  async #run(owner: DiscoveryExecutionOwner): Promise<void> {
    const configuration = this.#configuration, { context, input } = owner, owned = await configuration.repositories.resolve(context); owned.assertOwned();
    const directory = await resolveDiscoveryScoreDirectory(configuration.directories, context), queue = new SavedDiscoveryScores(directory, input.campaign_id, input.expected_revision), metrics = await configuration.metrics(context);
    const restored = await queueWork(() => queue.load()), jobs: ScoreJob[] = [], attempted = new Set<string>(), lane = new DiscoveryProjectionLane(), failures: unknown[] = [];
    const restoredHits = new Set(restored.map(item => item.hit));
    for (const hit of owned.repository.pendingScoreHits(input.campaign_id, input.expected_revision, [...input.excluded_platforms, ...RETIRED_CREATOR_SOURCES])) {
      if (!restoredHits.has(hit)) { restored.push({ hit, deferred_platforms: [] }); restoredHits.add(hit); }
    }
    const recommended = new Set(owned.repository.recommendedCandidateIds(input.campaign_id));
    let recommendationVersion = 0;
    let readAhead = 0, used = owned.repository.getPlan(input.campaign_id).used_calls, finalStage: DiscoveryExecutionStage = "paused", finalReason = "execution_interrupted", fatal = false;
    const elapsed = (start: number) => Math.floor(performance.now() - start), started = performance.now(); metrics.enqueue(input.campaign_id, "pipeline.started", 0);
    const sync = async () => {
      const began = performance.now();
      const version = recommendationVersion;
      owner.change(status => { status.sync_pending = true; });
      const result = await configuration.library.publish(context, input.campaign_id, input.expected_revision).then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
      owner.change(status => {
        if (result.ok) { status.library = result.value; status.sync_error = null; status.sync_pending = version !== recommendationVersion; status.synced_at = Date.now(); }
        else { status.sync_error = discoveryExecutionFailureCode(result.error); status.sync_pending = true; }
      });
      metrics.enqueue(input.campaign_id, "sync", elapsed(began));
      if (!result.ok && !discoveryExecutionFailureCode(result.error).startsWith("library_")) throw result.error;
    };
    const publishChanged = () => {
      owned.assertOwned(); const current = owned.repository.recommendedCandidateIds(input.campaign_id);
      const added = current.filter(id => !recommended.has(id));
      if (added.length) {
        for (const id of added) recommended.add(id);
        recommendationVersion++;
        owner.change(status => { status.sync_pending = true; });
        metrics.enqueue(input.campaign_id, "recommendation.changed", 0); lane.request(sync);
      }
    };
    const recordScore = (hit: string, outcome: ScoreOutcome): boolean => {
      const code = outcome.ok ? null : discoveryExecutionFailureCode(outcome.error), proceed = code === null || discoveryScoreDeferred(code);
      owner.change(status => {
        if (outcome.ok) status.deferred_hits = status.deferred_hits.filter(item => item.search_hit_id !== hit);
        else { const prior = status.deferred_hits.find(item => item.search_hit_id === hit); if (prior) prior.reason = code!; else if (status.deferred_hits.length < 1000) status.deferred_hits.push({ search_hit_id: hit, reason: code! }); }
        status.deferred_scores = status.deferred_hits.length;
        if (!proceed) { status.stage = "error"; status.reason = "request_failed"; status.failure_phase = "score"; status.failure_code = code; }
      });
      if (!proceed && !outcome.ok) { failures.push(outcome.error); fatal = true; finalStage = "error"; finalReason = "request_failed"; }
      if (outcome.ok && outcome.saved) publishChanged(); return proceed;
    };
    const launch = () => {
      let active = jobs.filter(job => job.state === "running").length;
      for (const job of jobs) {
        if (active >= DISCOVERY_SCORE_CONCURRENCY || owner.signal.aborted) break;
        if (job.state !== "queued") continue; active++; job.state = "running";
        job.task = (async (): Promise<ScoreOutcome> => {
          const matcher = await configuration.matcher(context), receipt = await matcher.evaluate(context, job.prepared);
          if (receipt !== null) await queueWork(() => queue.remove(job.hit));
          return { ok: true, saved: receipt !== null, refresh: receipt === null };
        })().then(value => value, error => ({ ok: false as const, error: error as unknown })).then(outcome => { metrics.enqueue(input.campaign_id, "score", elapsed(job.preparedAt)); job.outcome = outcome; job.state = "settled"; return outcome; });
      }
    };
    const finishOne = async (): Promise<boolean> => {
      let job = jobs.find(item => item.state === "settled");
      if (!job) {
        const active = jobs.filter(item => item.task !== undefined); if (!active.length) return true;
        owner.change(status => { if (status.stage === "searching") status.stage = "verifying"; });
        const began = performance.now(); await Promise.race(active.map(item => item.task!)); metrics.enqueue(input.campaign_id, "pipeline.score_wait", elapsed(began)); job = jobs.find(item => item.state === "settled");
      }
      if (!job || !job.outcome) throw new Error("discovery_score_completion_missing");
      jobs.splice(jobs.indexOf(job), 1);
      if (job.outcome.ok && job.outcome.refresh && !restored.some(item => item.hit === job.hit)) restored.push({ hit: job.hit, deferred_platforms: job.deferred });
      const proceed = recordScore(job.hit, job.outcome); if (proceed) launch(); return proceed;
    };
    const enqueue = async (hit: string, deferred: string[]): Promise<boolean> => {
      if (jobs.some(job => job.hit === hit)) return true;
      await queueWork(() => queue.add({ hit, deferred_platforms: deferred })); const preparedAt = performance.now();
      const prepared = await (async () => (await configuration.matcher(context)).prepare(context, { campaign_id: input.campaign_id, expected_revision: input.expected_revision, search_hit_id: hit }))().then(value => ({ ok: true as const, value }), error => ({ ok: false as const, error: error as unknown }));
      metrics.enqueue(input.campaign_id, "score.prepare", elapsed(preparedAt));
      if (!prepared.ok) return recordScore(hit, prepared);
      if (prepared.value.state !== "model") { await queueWork(() => queue.remove(hit)); return recordScore(hit, { ok: true, saved: prepared.value.state === "cached" }); }
      if (owner.signal.aborted) return true;
      if (attempted.has(prepared.value.prepared.evidenceKey)) { await queueWork(() => queue.remove(hit)); return true; }
      attempted.add(prepared.value.prepared.evidenceKey); jobs.push({ hit, deferred, prepared: prepared.value.prepared, preparedAt, state: "queued" }); launch(); return true;
    };
    const work = await (async () => {
      while (!fatal) {
        await metrics.flushPending(); await lane.reap();
        if (jobs.some(job => job.state === "settled")) { if (!await finishOne()) break; readAhead = 0; continue; }
        if (owner.signal.aborted) break;
        configuration.accounts.assertCurrent(context); owned.assertOwned();
        if (jobs.length || restored.length) {
          const capacity = discoveryScoreCapacity(owned.repository.remainingSlots(input.campaign_id, input.excluded_platforms));
          if (!capacity && !jobs.length) restored.length = 0;
          if (jobs.length && (jobs.length >= capacity || readAhead >= DISCOVERY_READ_AHEAD)) { if (!await finishOne()) break; readAhead = 0; continue; }
        }
        const saved = restored.shift(); if (saved) { if (!await enqueue(saved.hit, saved.deferred_platforms)) break; continue; }
        owner.change(status => { status.stage = "searching"; }); const began = performance.now();
        const result = await configuration.advances.advance(context, { campaign_id: input.campaign_id, expected_revision: input.expected_revision, excluded_platforms: input.excluded_platforms }, jobs.flatMap(job => job.deferred), jobs.map(job => job.hit), owner.signal);
        const advancing = result.step.kind.startsWith("read_") || result.step.kind === "search";
        if (advancing) metrics.enqueue(input.campaign_id, "read", elapsed(began));
        recordDiscoveryReadTiming(metrics, input.campaign_id, result, elapsed(began));
        const failure = discoveryAdvanceFailure(result);
        if (failure !== null) metrics.enqueueFailure(input.campaign_id, { recorded_at: Date.now(), ...discoveryReadFailureLocation(result), call_number: result.campaign.used_calls, code: discoveryExecutionFailureCode(failure) });
        owner.change(status => { status.used = result.campaign.used_calls; });
        const hadScores = jobs.length > 0; if (hadScores && advancing) readAhead++;
        let hit = discoveryScoreHit(result); if (result.public_page && owned.repository.candidateIsQualified(input.campaign_id, result.public_page.candidate_id)) hit = null;
        if (hit !== null && !owner.signal.aborted) { const previous = jobs.length; if (!await enqueue(hit, discoveryScoringPlatforms(result))) break; if (previous === 1 && jobs.length === 2) readAhead = 0; }
        publishChanged();
        if (!advancing && hadScores) { if (!await finishOne()) break; readAhead = 0; continue; }
        if (!advancing) {
          if (["search_exhausted", "needs_evidence"].includes(result.step.kind) && result.campaign.criteria.automatic_query_expansion === true
            && result.campaign.platform_runs.some(run => run.status === "completed" && !input.excluded_platforms.includes(run.platform) && !RETIRED_CREATOR_SOURCES.includes(run.platform))) {
            if (!configuration.dependencies) throw new Error("discovery_dependency_unavailable");
            const expanded = await new DiscoveryQueryRoundExecutor(configuration.accounts, configuration.repositories, configuration.dependencies).expand(context, input.campaign_id, input.expected_revision, input.excluded_platforms, owner.signal);
            if (expanded) continue;
          }
          ({ stage: finalStage, reason: finalReason } = terminal(result.step.kind)); break;
        }
        if (result.campaign.used_calls <= used) throw new Error("progress_unconfirmed"); used = result.campaign.used_calls;
      }
    })().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    const deliberateStop = !work.ok && owner.signal.aborted && (work.error === owner.signal.reason || work.error === context.signal.reason || work.error instanceof DOMException && work.error.name === "AbortError");
    if (!work.ok && !deliberateStop) { finalStage = "error"; finalReason = "request_failed"; failures.push(work.error); owner.change(status => { status.failure_phase = "read"; status.failure_code = discoveryExecutionFailureCode(work.error); }); }
    owner.change(status => { status.stage = "pausing"; });
    // 暂停只留下未启动队列；已经接纳的模型和本地投影完整排空。
    const scores = await Promise.allSettled(jobs.filter(job => job.state !== "queued").map(async job => { const outcome = await job.task!; recordScore(job.hit, outcome); }));
    for (const score of scores) if (score.status === "rejected") failures.push(score.reason);
    const drained = await lane.close().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown })); if (!drained.ok) failures.push(drained.error);
    if (!context.signal.aborted) {
      const finalSync = await sync().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown })); if (!finalSync.ok) failures.push(finalSync.error);
    }
    metrics.enqueue(input.campaign_id, "pipeline.wall", elapsed(started));
    const flushed = await metrics.flushPending().then(() => ({ ok: true as const }), error => ({ ok: false as const, error: error as unknown }));
    if (!flushed.ok) { failures.push(flushed.error); owner.change(status => { status.failure_phase = "metrics"; status.failure_code = "execution_failed"; }); }
    owner.change(status => { status.stage = failures.length ? "error" : finalStage; status.reason = owner.signal.aborted && !failures.length ? status.reason ?? "execution_interrupted" : finalReason; });
    if (failures.length) throw new AggregateError(failures, "discovery_execution_failed");
  }
}
