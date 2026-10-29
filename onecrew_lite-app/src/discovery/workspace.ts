import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { localServiceStatus, type LocalServiceStatus } from "@onecrew/runtime-contracts/status";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { DiscoveryRepositoryResolver } from "./search-execution.ts";
import type { DiscoveryExecutionSupervisor } from "./execution-supervisor.ts";
import { FileDiscoveryExecutionMetrics } from "./execution-metrics.ts";

const activitySchema = localServiceStatus.shape.workspace_activity.unwrap();
type Activity = NonNullable<LocalServiceStatus["workspace_activity"]>;
const stage = activitySchema.shape.stage;
const stop = activitySchema.shape.stop.unwrap().unwrap().refine(value => BigInt(value.recorded_at) > 0n && BigInt(value.recorded_at) <= 8640000000000000n);
export const workspacePresence = z.strictObject({ session_id: z.uuid(), sequence: creatorU64, campaign_id: z.string().refine(value => Buffer.byteLength(value) > 0 && Buffer.byteLength(value) <= 100).nullable(), stage, activate: z.boolean(), stop: stop.nullish(), excluded_platforms: z.array(z.string().regex(/^[a-z0-9-]{1,80}$/u)).max(64).default([]) });
export interface DiscoveryWorkspaceDirectories { resolve(context: AccountContext): Promise<{ directory: string; assertOwned(): void }> }
type Selection = { session: string; campaign: string; account: string; stage: z.infer<typeof stage>; seen: number; stop: z.infer<typeof stop> | null };
const key = (context: AccountContext) => JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId]);

// 心跳仅租用显示位置，执行权与进度始终读取实际账号仓储。
export class DiscoveryWorkspace {
  readonly #accounts: AccountContextService; readonly #repositories: DiscoveryRepositoryResolver; readonly #supervisor: DiscoveryExecutionSupervisor; readonly #directories: DiscoveryWorkspaceDirectories;
  #selected: Selection | null = null; #activated: number | null = null;
  readonly #sequences = new Map<string, { sequence: bigint; seen: number }>();
  readonly #history = new Map<string, Activity[]>(); readonly #metrics = new Map<string, FileDiscoveryExecutionMetrics>();
  constructor(accounts: AccountContextService, repositories: DiscoveryRepositoryResolver, supervisor: DiscoveryExecutionSupervisor, directories: DiscoveryWorkspaceDirectories) { this.#accounts = accounts; this.#repositories = repositories; this.#supervisor = supervisor; this.#directories = directories; }
  async metrics(context: AccountContext): Promise<FileDiscoveryExecutionMetrics> {
    this.#accounts.assertCurrent(context); const owned = await this.#directories.resolve(context); owned.assertOwned(); this.#accounts.assertCurrent(context);
    const id = JSON.stringify([key(context), context.epoch, owned.directory]); let metrics = this.#metrics.get(id);
    if (!metrics) { metrics = new FileDiscoveryExecutionMetrics(owned.directory, () => owned.assertOwned()); this.#metrics.set(id, metrics); }
    return metrics;
  }
  history(context: AccountContext): Activity[] { this.#accounts.assertCurrent(context); return structuredClone(this.#history.get(key(context)) ?? []); }
  async presence(context: AccountContext, raw: unknown): Promise<void> {
    const input = workspacePresence.parse(raw), issued = performance.now(); this.#accounts.assertCurrent(context);
    if (input.campaign_id !== null) { const owned = await this.#repositories.resolve(context); owned.assertOwned(); owned.repository.getPlan(input.campaign_id); owned.assertOwned(); }
    this.#accounts.assertCurrent(context); const now = performance.now();
    for (const [id, entry] of this.#sequences) if (id !== this.#selected?.session && now - entry.seen >= 300000) this.#sequences.delete(id);
    const previous = this.#sequences.get(input.session_id);
    if (!previous || previous.sequence < BigInt(input.sequence)) {
      if (!previous && this.#sequences.size >= 64) throw new Error("workspace_presence_capacity");
      this.#sequences.set(input.session_id, { sequence: BigInt(input.sequence), seen: now });
      if (!(input.activate && this.#activated !== null && this.#activated >= issued)) {
        if (input.activate) this.#activated = issued;
        if (input.activate || this.#selected?.session === input.session_id) this.#selected = input.campaign_id === null ? null : { session: input.session_id, campaign: input.campaign_id, account: key(context), stage: input.stage, seen: now, stop: input.stop ?? null };
      }
    }
    await this.snapshot(context);
  }
  async snapshot(context: AccountContext): Promise<Activity | null> {
    this.#accounts.assertCurrent(context); const account = key(context), selected = this.#selected;
    let execution = this.#supervisor.snapshot(context);
    if (execution && !["searching", "verifying", "pausing"].includes(execution.stage) && selected && selected.account === account && selected.campaign !== execution.campaign_id) execution = null;
    const parsedStop = execution ? stop.safeParse({ reason: execution.reason, recorded_at: execution.updated_at }) : null;
    const selection: Selection | null = execution ? { session: "00000000-0000-0000-0000-000000000000", campaign: execution.campaign_id, account, stage: execution.stage, seen: performance.now(), stop: parsedStop?.success ? parsedStop.data : null } : selected;
    if (!selection || selection.account !== account) return null;
    const owned = await this.#repositories.resolve(context); owned.assertOwned();
    if (owned.owner.accountId !== context.principal.user.id || owned.owner.tenantId !== context.principal.tenantId || owned.owner.baseId !== context.principal.baseId) throw new Error("workspace_account_changed");
    const campaign = owned.repository.getPlan(selection.campaign), counts = owned.repository.countCandidates(selection.campaign), stops = owned.repository.readStops(selection.campaign);
    const metrics = await this.metrics(context), failures = await metrics.failureCount(campaign.id), readFailures = await metrics.recentFailures(campaign.id); owned.assertOwned(); this.#accounts.assertCurrent(context);
    if (!execution && (!this.#selected || this.#selected.session !== selection.session || this.#selected.campaign !== selection.campaign || this.#selected.account !== account)) throw new Error("workspace_selection_changed");
    const stale = !execution && performance.now() - selection.seen > 45000, reading = campaign.platform_runs.some(run => run.status === "searching");
    const reported = selection.stage;
    const targets = owned.repository.sourceProgress(campaign.id), reached = targets.length ? targets.every(source => source.qualified >= source.target_count) : counts.qualified >= campaign.target_count;
    const currentStage = execution?.stage ?? (["pausing", "paused", "error"].includes(reported) ? reported : !reading && reached ? "finished" : reading ? "searching" : stale ? "paused" : reported);
    const count = (status: string) => campaign.platform_runs.filter(run => run.status === status).length;
    const pipeline = execution?.pipeline;
    const activity = activitySchema.parse({ campaign_id: campaign.id, brief: campaign.brief, stage: currentStage, qualified: counts.qualified, pending: counts.pending, pending_breakdown: owned.repository.pendingReviewCounts(campaign.id, campaign.revision), target: campaign.target_count, attempted: campaign.platform_runs.length - count("unsearched"), total: campaign.scopes.length, used: campaign.used_calls, login_required: count("unauthorized"), read_errors: BigInt(failures) > BigInt(counts.public_page_errors) ? failures : counts.public_page_errors, read_failures: readFailures, current_read_stops: count("technical_stop"), read_operation_stops: stops, restricted: count("risk_control_stop"), stale,
      ...(execution ? { updated_at: execution.updated_at, deferred_scores: execution.deferred_scores, ...(execution.library || execution.sync_pending ? { library_sync_pending: execution.sync_pending } : {}), ...(execution.failure_phase !== null ? { failure_phase: execution.failure_phase } : {}), ...(execution.failure_code !== null ? { failure_code: execution.failure_code } : {}), ...(execution.sync_error !== null ? { library_sync_error: execution.sync_error } : {}), ...(execution.synced_at !== null ? { library_synced_at: execution.synced_at } : {}) } : {}),
      ...(pipeline ? { pipeline: { ...pipeline, recommendations_complete: reached } } : {}),
      ...(!["searching", "verifying"].includes(currentStage) && selection.stop ? { stop: selection.stop } : {}) });
    const history = this.#history.get(account) ?? [], index = history.findIndex(item => item.campaign_id === activity.campaign_id);
    if (index >= 0) history[index] = structuredClone(activity); else history.push(structuredClone(activity)); this.#history.set(account, history);
    return activity;
  }
  async close(): Promise<void> { await Promise.all([...this.#metrics.values()].map(metrics => metrics.flushPending())); }
}
