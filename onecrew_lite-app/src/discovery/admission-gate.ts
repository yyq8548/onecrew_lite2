import type { AccountContext, AccountContextService } from "../identity/account-context.ts";

export interface DiscoveryAdmissionLease { readonly context: AccountContext; assertCurrent(): void; release(): void }
interface WaitingAdmission { grant(): void; cancel(): void }

// Discovery与后台表格上传必须共享这一实例。取消持有者时仍由其排空事务后释放。
export class DiscoveryAdmissionGate {
  readonly #accounts: AccountContextService;
  #held = false;
  readonly #waiting: WaitingAdmission[] = [];
  readonly #background = new Set<AbortController>();
  constructor(accounts: AccountContextService) { this.#accounts = accounts; }
  async acquire(context: AccountContext, signal: AbortSignal = context.signal, background?: AbortController): Promise<DiscoveryAdmissionLease> {
    this.#accounts.assertCurrent(context);
    const stopped = AbortSignal.any([context.signal, signal, ...(background ? [background.signal] : [])]); stopped.throwIfAborted();
    if (background) this.#background.add(background);
    else for (const controller of this.#background) controller.abort(new Error("table_sync_preempted"));
    const lease = await new Promise<DiscoveryAdmissionLease>((resolve, reject) => {
      const waiter: WaitingAdmission = {
        grant: () => {
          stopped.removeEventListener("abort", waiter.cancel);
          this.#held = true; let released = false;
          resolve(Object.freeze({ context, assertCurrent: () => { this.#accounts.assertCurrent(context); stopped.throwIfAborted(); }, release: () => {
            if (released) return; released = true; if (background) this.#background.delete(background); this.#held = false; this.#pump();
          } }));
        },
        cancel: () => {
          const position = this.#waiting.indexOf(waiter); if (position < 0) return;
          this.#waiting.splice(position, 1); if (background) this.#background.delete(background); stopped.removeEventListener("abort", waiter.cancel); reject(stopped.reason);
        },
      };
      this.#waiting.push(waiter); stopped.addEventListener("abort", waiter.cancel, { once: true }); this.#pump();
    });
    let admitted = false;
    try { lease.assertCurrent(); admitted = true; return lease; }
    finally { if (!admitted) lease.release(); }
  }
  #pump(): void { if (this.#held) return; const next = this.#waiting.shift(); if (next) next.grant(); }
}
