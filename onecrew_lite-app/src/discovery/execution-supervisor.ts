import { z } from "zod";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { DiscoveryAdmissionGate } from "./admission-gate.ts";

const text = z.string().refine(value => value.isWellFormed());
export const discoveryExecutionInput = z.strictObject({ campaign_id: text, expected_revision: z.number().int().min(0).max(4294967295), profile_ref: text.min(1), excluded_platforms: z.array(text.regex(/^[a-z0-9-]{1,80}$/u)).max(64).default([]) });
export type DiscoveryExecutionInput = z.infer<typeof discoveryExecutionInput>;
export type DiscoveryExecutionStage = "ready" | "searching" | "verifying" | "pausing" | "paused" | "finished" | "budget_exhausted" | "needs_evidence" | "error";
export interface DiscoveryExecutionLibrary { campaign_id: string; table_id: string; web_url: string; total_hits: number; inserted_rows: number }
export interface DiscoveryExecutionStatus { campaign_id: string; revision: number; stage: DiscoveryExecutionStage; updated_at: number; used: number; reason: string | null; failure_phase: string | null; failure_code: string | null; sync_error: string | null; synced_at: number | null; deferred_scores: number; sync_pending: boolean; excluded_platforms: string[]; deferred_hits: { search_hit_id: string; reason: string }[]; library: DiscoveryExecutionLibrary | null }
export interface DiscoveryExecutionOwner { readonly context: AccountContext; readonly input: DiscoveryExecutionInput; readonly signal: AbortSignal; change(edit: (status: DiscoveryExecutionStatus) => void): void }
interface ActiveExecution { context: AccountContext; input: DiscoveryExecutionInput; cancel: AbortController; status: DiscoveryExecutionStatus }
const executing = (stage: DiscoveryExecutionStage) => ["searching", "verifying", "pausing"].includes(stage);

// 进程唯一执行所有者。最终状态与已开始工作分别跟踪，排空前持续阻止后台上传。
export class DiscoveryExecutionSupervisor {
  readonly #accounts: AccountContextService; readonly #admission: DiscoveryAdmissionGate;
  #active: ActiveExecution | undefined; readonly #tasks = new Set<Promise<void>>(); #errors: unknown[] = []; #closing = false;
  constructor(accounts: AccountContextService, admission: DiscoveryAdmissionGate) { this.#accounts = accounts; this.#admission = admission; }
  blocksBackendSync(): boolean { return this.#tasks.size > 0 || this.#active !== undefined && executing(this.#active.status.stage); }
  snapshot(context: AccountContext): DiscoveryExecutionStatus | null {
    this.#accounts.assertCurrent(context);
    const active = this.#active;
    return active && active.context.principal.user.id === context.principal.user.id && active.context.principal.tenantId === context.principal.tenantId && active.context.principal.baseId === context.principal.baseId ? structuredClone(active.status) : null;
  }
  async ownTask<T>(context: AccountContext, work: () => Promise<T>): Promise<T> {
    const lease = await this.#admission.acquire(context);
    try {
      lease.assertCurrent(); if (this.#closing) throw new Error("discovery_execution_stopping");
      const result = Promise.resolve().then(work);
      const draining = result.then(() => undefined, error => { this.#errors.push(error); }).finally(() => { this.#tasks.delete(draining); });
      this.#tasks.add(draining); return result;
    } finally { lease.release(); }
  }
  async start(context: AccountContext, raw: unknown, used: number, validate: () => Promise<void>, work: (owner: DiscoveryExecutionOwner) => Promise<void>, onAdmitted?: () => void): Promise<DiscoveryExecutionStatus> {
    this.#accounts.assertCurrent(context); const input = discoveryExecutionInput.parse(raw); z.number().int().min(0).max(4294967295).parse(used);
    if (this.#closing) throw new Error("discovery_execution_stopping");
    // 共同mutex覆盖所有最终准入校验及running登记，上传排空后方能启动搜索。
    const lease = await this.#admission.acquire(context);
    try {
      lease.assertCurrent(); if (this.#closing) throw new Error("discovery_execution_stopping");
      const previous = this.#active;
      if (previous && executing(previous.status.stage)) {
        if (previous.context === context && previous.input.campaign_id === input.campaign_id && previous.input.expected_revision === input.expected_revision) return structuredClone(previous.status);
        throw new Error("discovery_execution_busy");
      }
      if (this.#tasks.size) throw new Error("discovery_execution_busy");
      this.#report(); await validate(); lease.assertCurrent(); if (this.#closing) throw new Error("discovery_execution_stopping");
      const same = previous?.context.principal.user.id === context.principal.user.id && previous.context.principal.tenantId === context.principal.tenantId && previous.context.principal.baseId === context.principal.baseId && previous.input.campaign_id === input.campaign_id && previous.input.expected_revision === input.expected_revision ? previous.status : undefined;
      const active: ActiveExecution = { context, input, cancel: new AbortController(), status: { campaign_id: input.campaign_id, revision: input.expected_revision, stage: "searching", updated_at: Date.now(), used, reason: null, failure_phase: null, failure_code: null, sync_error: same?.sync_error ?? null, synced_at: same?.synced_at ?? null, deferred_scores: same?.deferred_scores ?? 0, sync_pending: same?.sync_pending ?? false, excluded_platforms: [...input.excluded_platforms], deferred_hits: structuredClone(same?.deferred_hits ?? []), library: structuredClone(same?.library ?? null) } };
      this.#active = active;
      const owner: DiscoveryExecutionOwner = Object.freeze({ context, input: structuredClone(input), signal: AbortSignal.any([context.signal, active.cancel.signal]), change: (edit: (status: DiscoveryExecutionStatus) => void) => {
        if (this.#active !== active) throw new Error("discovery_execution_owner_changed");
        edit(active.status);
        if (active.cancel.signal.aborted && executing(active.status.stage)) active.status.stage = "pausing";
        active.status.updated_at = Date.now();
      } });
      const task = Promise.resolve().then(() => work(owner)).then(() => {
        if (executing(active.status.stage)) throw new Error("discovery_execution_terminal_status_missing");
      }).then(() => undefined, error => {
        active.status.stage = "error"; active.status.reason = "worker_failed"; active.status.failure_phase ??= "execution"; active.status.updated_at = Date.now(); this.#errors.push(error);
      }).finally(() => { this.#tasks.delete(task); });
      this.#tasks.add(task);
      onAdmitted?.();
      return structuredClone(active.status);
    } finally { lease.release(); }
  }
  pause(context: AccountContext): DiscoveryExecutionStatus | null {
    this.#accounts.assertCurrent(context); const status = this.snapshot(context);
    if (status && this.#active) this.#cancel("user_paused");
    return this.snapshot(context);
  }
  #cancel(reason: string): void {
    const active = this.#active; if (!active || !executing(active.status.stage)) return;
    if (!active.cancel.signal.aborted) { active.status.reason = reason; active.cancel.abort(new Error(reason)); }
    active.status.stage = "pausing"; active.status.updated_at = Date.now();
  }
  #report(): void {
    if (!this.#errors.length) return;
    const errors = this.#errors; this.#errors = []; throw new AggregateError(errors, "discovery_execution_failed");
  }
  async waitForIdle(context: AccountContext): Promise<DiscoveryExecutionStatus | null> {
    this.#accounts.assertCurrent(context);
    while (this.#tasks.size) await Promise.all([...this.#tasks]);
    this.#accounts.assertCurrent(context); this.#report(); return this.snapshot(context);
  }
  stopAcceptingWork(): void { this.#closing = true; this.#cancel("service_stopping"); }
  async close(): Promise<void> { this.stopAcceptingWork(); await Promise.all([...this.#tasks]); this.#report(); }
}
