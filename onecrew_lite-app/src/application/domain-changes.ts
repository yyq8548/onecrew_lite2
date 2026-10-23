import type { AccountContext, AccountContextService } from "../identity/account-context.ts";

export type ChangeScope = "email_folders" | "email_threads" | "email_drafts" | "email_templates" | "email_sync" | "creators" | "creator_groups" | "conversations" | "outreach" | "reply_triage";
export interface DomainChange { readonly event: "runtime://changed"; readonly payload: Readonly<{ sequence: number; scopes: readonly (ChangeScope | "all")[] }> }
interface Subscription { context: AccountContext; controller: ReadableStreamDefaultController<DomainChange>; remove(): void }

// 只发布失效提示。数据和持久任务留在原仓储；慢订阅者重连后须重新读取当前状态。
export class DomainChanges {
  readonly #accounts: AccountContextService;
  readonly #subscriptions = new Set<Subscription>();
  #sequence = 0;
  constructor(accounts: AccountContextService) { this.#accounts = accounts; }
  subscribe(context: AccountContext, signal: AbortSignal): ReadableStream<DomainChange> {
    this.#accounts.assertCurrent(context);
    const cancellation = AbortSignal.any([context.signal, signal]);
    cancellation.throwIfAborted();
    let subscription: Subscription;
    return new ReadableStream<DomainChange>({
      start: controller => {
        const stop = () => { subscription.remove(); controller.close(); };
        subscription = { context, controller, remove: () => { this.#subscriptions.delete(subscription); cancellation.removeEventListener("abort", stop); } };
        this.#subscriptions.add(subscription);
        cancellation.addEventListener("abort", stop, { once: true });
        controller.enqueue(Object.freeze({ event: "runtime://changed", payload: Object.freeze({ sequence: 0, scopes: Object.freeze(["all"] as const) }) }));
      },
      cancel: () => { subscription.remove(); },
    }, { highWaterMark: 256 });
  }
  publish(context: AccountContext, scopes: readonly ChangeScope[]): void {
    this.#accounts.assertCurrent(context);
    if (!scopes.length) return;
    if (this.#sequence === Number.MAX_SAFE_INTEGER) throw new Error("Runtime change sequence exhausted");
    const event: DomainChange = Object.freeze({ event: "runtime://changed", payload: Object.freeze({ sequence: ++this.#sequence, scopes: Object.freeze([...new Set(scopes)]) }) });
    for (const entry of this.#subscriptions) {
      if (entry.context !== context) continue;
      if (entry.controller.desiredSize === null || entry.controller.desiredSize <= 0) {
        entry.remove();
        entry.controller.error(new Error("Runtime change subscriber lagged; reconnect and refresh current account"));
      } else entry.controller.enqueue(event);
    }
  }
  close(): void {
    for (const entry of this.#subscriptions) { entry.remove(); entry.controller.close(); }
  }
}
