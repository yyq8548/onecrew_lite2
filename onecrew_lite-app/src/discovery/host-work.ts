import { createHash, randomUUID } from "node:crypto";
import { join } from "node:path";
import { z } from "zod";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import type { OwnedDiscoveryQueueDirectory } from "./saved-scores.ts";
import type { AccountContext } from "../identity/account-context.ts";
import { DiscoverySemanticError, type SemanticStructuredClient, type SemanticStructuredRequest } from "./semantic-client.ts";
import { HostWorkStore } from "./host-work-store.ts";

const text = z.string().min(1).refine(value => value.isWellFormed());
const id = text.regex(/^[a-f0-9]{64}$/u);
export const hostWorkClaimInput = z.strictObject({ limit: z.number().int().min(1).max(5).default(3), known_rule_ids: z.array(id).max(32).default([]), campaign_id: text.optional(), wait_ms: z.number().int().min(0).max(20000).default(15000) });
export const hostWorkGetInput = z.strictObject({ task_id: id, evidence_ids: z.array(text).max(8).default([]), include_material: z.boolean().default(false) });
export const hostWorkSubmitInput = z.strictObject({ task_id: id, lease: text, request_id: text.max(128), result: z.unknown() });
const scope = z.strictObject({ kind: z.enum(["plan", "queries", "query_audit", "score", "professional_review"]), campaign_id: text, revision: z.number().int().nonnegative(), search_hit_id: text.optional() });
const task = z.strictObject({ id, scope, prompt: text, rules: z.string(), saved_evidence: z.array(z.strictObject({ id: text, citation: z.unknown() })).default([]), schema: z.record(z.string(), z.unknown()), created_at: z.number(), lease: z.string().nullable(), lease_until: z.number(), claimed_at: z.number().nullable(), result: z.unknown().optional(), receipt: z.strictObject({ request_id: text, submitted_at: z.number() }).optional() });
export type HostTask = z.infer<typeof task>;
export type HostWorkNotice = Pick<HostTask, "id" | "scope">;
export type HostWorkNotify = (notice: HostWorkNotice, active: () => boolean) => (cancelled?: boolean) => void;
const driverKey = (scope: Pick<HostWorkNotice["scope"], "campaign_id" | "revision">) => JSON.stringify([scope.campaign_id, scope.revision]);
const stateSchema = z.strictObject({ version: z.literal(1), owner: z.strictObject({ accountId: text, tenantId: text, baseId: text }), tasks: z.array(task) });
export type HostWorkState = z.infer<typeof stateSchema>;
const writing = new Map<string, Promise<void>>();
function serial<T>(path: string, body: () => Promise<T>): Promise<T> {
  const result = (writing.get(path) ?? Promise.resolve()).then(body), settled = result.then(() => undefined, () => undefined);
  writing.set(path, settled); void settled.then(() => { if (writing.get(path) === settled) writing.delete(path); }); return result;
}
const hash = (value: unknown) => createHash("sha256").update(stringifyApplicationJson(value)).digest("hex");
export const hostWorkOwner = (context: AccountContext) => stringifyApplicationJson([context.principal.user.id, context.principal.tenantId, context.principal.baseId, context.epoch]);
interface Pending { request: SemanticStructuredRequest<unknown>; resolve(value: unknown): void; abort(): void; signal: AbortSignal }
export interface HostDriverStatus { state: "running" | "planning" | "paused" | "completed" | "needs_action" | "idle"; execution: { campaign_id: string; revision: number; stage: string; used: number; reason: string | null; failure_code: string | null } | null }

// 只交接模型任务；原文归属、条件和最终推荐继续通过原审核入口校验。
export class HostSemanticClient implements SemanticStructuredClient {
  readonly #owned: OwnedDiscoveryQueueDirectory; readonly #path: string; readonly #owner: AbortSignal; readonly #store: HostWorkStore;
  readonly #changes = new Set<() => void>();
  readonly #pending = new Map<string, Pending>();
  readonly #driverSeen = new Map<string, number>();
  readonly #leases = new Map<string, number>();
  readonly #notify: HostWorkNotify | undefined;
  constructor(owned: OwnedDiscoveryQueueDirectory, owner: AbortSignal, notify?: HostWorkNotify) {
    this.#owned = owned; this.#path = join(owned.directory, "discovery-host-work", "tasks.sqlite"); this.#owner = owner; this.#notify = notify;
    this.#store = new HostWorkStore(owned, owner, value => task.parse(value), value => stateSchema.parse(value));
  }
  async scoringIdentity(): Promise<string> { return "current-host-conversation-v1"; }
  #active(scope: HostWorkNotice["scope"]): boolean {
    if (this.#owner.aborted) return false;
    const now = Date.now(), key = driverKey(scope);
    return now - (this.#driverSeen.get(key) ?? 0) < 60_000 || [...this.#pending].some(([id, pending]) => pending.request.scope && driverKey(pending.request.scope) === key && (this.#leases.get(id) ?? 0) > now);
  }
  #touch(campaign?: string): void {
    const now = Date.now();
    for (const pending of this.#pending.values()) {
      const scope = pending.request.scope;
      if (scope && (campaign === undefined || scope.campaign_id === campaign)) this.#driverSeen.set(driverKey(scope), now);
    }
  }
  #changed(): void { for (const notify of this.#changes) notify(); }
  async structured<T>(input: SemanticStructuredRequest<T>): Promise<T> {
    input.signal.throwIfAborted(); this.#owner.throwIfAborted(); await input.assertCurrent();
    if (!input.scope) throw new Error("host_work_scope_required");
    if (!input.prompt.isWellFormed() || Buffer.byteLength(input.prompt) > 4 * 1024 * 1024) throw new DiscoverySemanticError("semantic_invalid_output");
    const requestScope = scope.parse(input.scope), rules = input.scoringContext ?? "";
    const taskId = hash({ owner: this.#owned.owner, scope: requestScope, prompt: input.prompt, rules, saved_evidence: input.savedEvidence ?? [], schema: input.schema });
    const started = performance.now();
    const row = await serial(this.#path, async () => {
      const prior = await this.#store.get(taskId);
      if (prior) {
        if (!prior.receipt && !this.#pending.has(taskId)) { prior.lease = null; prior.lease_until = 0; await this.#store.put(prior); }
        return prior;
      }
      const next: HostTask = { id: taskId, scope: requestScope, prompt: input.prompt, rules, saved_evidence: [...input.savedEvidence ?? []], schema: { ...input.schema }, created_at: Date.now(), lease: null, lease_until: 0, claimed_at: null };
      await this.#store.put(next); return next;
    });
    await input.assertCurrent(); input.signal.throwIfAborted(); this.#owner.throwIfAborted();
    if (row.receipt) return input.decode.parse(row.result);
    if (this.#pending.has(taskId)) throw new Error("host_work_already_waiting");
    const signal = AbortSignal.any([input.signal, this.#owner]);
    let retireNotice: ((cancelled?: boolean) => void) | undefined;
    const value = await new Promise<unknown>((resolve, reject) => {
      const abort = () => { this.#pending.delete(taskId); this.#leases.delete(taskId); this.#driverSeen.delete(driverKey(requestScope)); this.#changed(); reject(signal.reason); };
      this.#pending.set(taskId, { request: input as SemanticStructuredRequest<unknown>, resolve, abort, signal });
      this.#changed();
      retireNotice = this.#notify?.({ id: taskId, scope: requestScope }, () => this.#active(requestScope));
      signal.addEventListener("abort", abort, { once: true }); if (signal.aborted) abort();
    }).finally(() => { this.#pending.delete(taskId); retireNotice?.(signal.aborted); });
    input.onTiming?.("host_evaluation", Math.floor(performance.now() - started));
    await input.assertCurrent(); input.signal.throwIfAborted(); return input.decode.parse(value);
  }
  async claim(raw: unknown, activity: () => HostDriverStatus = () => ({ state: "idle", execution: null })) {
    const input = hostWorkClaimInput.parse(raw);
    const deadline = performance.now() + input.wait_ms;
    for (;;) {
      const current = activity();
      if (current.execution && (current.state === "running" || current.state === "planning")) this.#driverSeen.set(driverKey(current.execution), Date.now());
      this.#touch(input.campaign_id);
      const batch = await this.#claimOnce(input), driver = activity();
      const continuing = driver.state === "running" || driver.state === "planning";
      if (batch.tasks.length || batch.waiting || !continuing || performance.now() >= deadline) {
        return { ...batch, driver, next_action: batch.tasks.length ? "evaluate_and_submit" : batch.waiting ? "finish_claimed_work" : continuing ? "continue_claim" : "stop" };
      }
      // 等待不持有队列写锁；后台读取和提交可以同时产生下一批模型任务。
      const timeout = AbortSignal.timeout(Math.min(200, Math.max(1, Math.ceil(deadline - performance.now())))), signal = AbortSignal.any([this.#owner, timeout]);
      await new Promise<void>((resolve, reject) => {
        const done = () => { this.#changes.delete(changed); signal.removeEventListener("abort", aborted); resolve(); };
        const changed = () => done(), aborted = () => { this.#changes.delete(changed); signal.removeEventListener("abort", aborted); if (this.#owner.aborted) reject(this.#owner.reason); else resolve(); };
        this.#changes.add(changed); signal.addEventListener("abort", aborted, { once: true }); if (signal.aborted) aborted();
      });
    }
  }
  async #claimOnce(input: z.infer<typeof hostWorkClaimInput>) {
    this.#owner.throwIfAborted(); this.#owned.assertOwned();
    const ids = [...this.#pending].filter(([, value]) => input.campaign_id === undefined || value.request.scope?.campaign_id === input.campaign_id).map(([id]) => id);
    if (!ids.length) return { protocol_version: 1, tasks: [], rules: {}, waiting: 0 };
    return serial(this.#path, async () => {
      const rows = (await this.#store.getMany(ids)).sort((a, b) => a.created_at - b.created_at), now = Date.now(); let bytes = 0;
      const picked: HostTask[] = [];
      for (const row of rows) {
        if (row.receipt || row.lease_until > now || !this.#pending.has(row.id) || input.campaign_id !== undefined && row.scope.campaign_id !== input.campaign_id) continue;
        const pending = this.#pending.get(row.id)!; await pending.request.assertCurrent(); pending.signal.throwIfAborted();
        const size = Buffer.byteLength(row.prompt); if (picked.length && bytes + size > 24000) continue;
        row.lease = randomUUID(); row.lease_until = now + 10 * 60 * 1000; row.claimed_at = now;
        picked.push(row); bytes += size;
        pending.request.onStarted?.(); pending.request.onTiming?.("host_queue", now - row.created_at);
        if (picked.length >= input.limit) break;
      }
      if (picked.length) await this.#store.putMany(picked);
      for (const row of picked) { this.#leases.set(row.id, row.lease_until); this.#driverSeen.set(driverKey(row.scope), now); }
      const rules: Record<string, string> = {};
      const tasks = picked.map(row => {
        const rule_id = hash(row.rules); if (!input.known_rule_ids.includes(rule_id)) rules[rule_id] = row.rules;
        return { task_id: row.id, kind: row.scope.kind, scope: row.scope, lease: row.lease!, lease_until: row.lease_until, rule_id, material: row.prompt, output_schema: row.schema };
      });
      const waiting = [...this.#pending.values()].filter(value => input.campaign_id === undefined || value.request.scope?.campaign_id === input.campaign_id).length;
      return { protocol_version: 1, tasks, rules, waiting };
    });
  }
  async get(raw: unknown) {
    const input = hostWorkGetInput.parse(raw);
    return serial(this.#path, async () => {
      const row = await this.#store.get(input.task_id); if (!row) throw new Error("host_work_not_found");
      if (input.evidence_ids.length) {
        const evidence = input.evidence_ids.map(id => { const entry = row.saved_evidence.find(value => value.id === id); if (!entry) throw new Error("host_work_evidence_not_found"); return entry; });
        return { protocol_version: 1, task_id: row.id, scope: row.scope, evidence };
      }
      return { protocol_version: 1, task_id: row.id, scope: row.scope, state: row.receipt ? "submitted" : this.#pending.has(row.id) ? "waiting_for_host" : "suspended", receipt: row.receipt ?? null, ...(input.include_material ? { rules: row.rules, material: row.prompt, output_schema: row.schema } : {}) };
    });
  }
  async submit(raw: unknown) {
    const input = hostWorkSubmitInput.parse(raw);
    return serial(this.#path, async () => {
      const row = await this.#store.get(input.task_id); if (!row) throw new Error("host_work_not_found");
      if (row.receipt) {
        if (row.receipt.request_id !== input.request_id || hash(row.result) !== hash(input.result)) throw new Error("host_work_receipt_conflict");
        return { task_id: row.id, state: "submitted" as const, receipt: row.receipt };
      }
      const pending = this.#pending.get(row.id);
      if (!pending || row.lease !== input.lease || row.lease_until < Date.now()) throw new Error("host_work_lease_expired");
      await pending.request.assertCurrent(); pending.signal.throwIfAborted();
      const checked = pending.request.decode.safeParse(input.result);
      if (!checked.success) return { task_id: row.id, state: "invalid" as const, issues: checked.error.issues.map(issue => ({ path: issue.path, message: issue.message })) };
      row.result = input.result; row.receipt = { request_id: input.request_id, submitted_at: Date.now() }; await this.#store.put(row);
      this.#leases.delete(row.id); this.#driverSeen.set(driverKey(row.scope), Date.now());
      pending.signal.removeEventListener("abort", pending.abort); this.#pending.delete(row.id); this.#changed(); pending.resolve(checked.data);
      return { task_id: row.id, state: "submitted" as const, receipt: row.receipt };
    });
  }
  waiting(): number { return this.#pending.size; }
}

export class HostOperationRuns {
  readonly #runs = new Map<string, { owner: string; key: string; campaignId?: string; state: "pending" | "completed" | "failed"; value?: unknown; error?: string }>();
  pending(owner: string, campaignId?: string): boolean { return [...this.#runs.values()].some(run => run.owner === owner && run.state === "pending" && (campaignId === undefined || run.campaignId === campaignId)); }
  start(owner: string, key: unknown, run: () => Promise<unknown>, campaignId?: string) {
    const identity = hash({ owner, key }), prior = [...this.#runs].find(([, state]) => state.owner === owner && state.key === identity && state.state === "pending");
    const operation_id = prior?.[0] ?? hash({ owner, identity, attempt: randomUUID() }); let state = prior?.[1];
    if (!state) {
      state = { owner, key: identity, ...(campaignId === undefined ? {} : { campaignId }), state: "pending" }; this.#runs.set(operation_id, state); const target = state;
      void Promise.resolve().then(run).then(value => { target.value = value; target.state = "completed"; }, error => { target.error = error instanceof Error ? error.message : "host_operation_failed"; target.state = "failed"; });
    }
    const { owner: _owner, key: _key, campaignId: _campaign, ...result } = state; return { operation_id, ...result };
  }
  get(owner: string, operation: string) {
    const state = this.#runs.get(operation); if (!state || state.owner !== owner) throw new Error("host_operation_not_found");
    const { owner: _owner, key: _key, campaignId: _campaign, ...result } = state; return { operation_id: operation, ...result };
  }
}
