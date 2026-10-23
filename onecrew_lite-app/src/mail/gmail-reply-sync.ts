import { z } from "zod";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { ChangeScope } from "../application/domain-changes.ts";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";
import type { GmailAccessProvider } from "./gmail-transport.ts";
import { readGmailOutreachReplyIds, readGmailReplyMessage } from "./gmail-transport.ts";
import type { VerifiedMailIdentityProvider } from "./gmail-access.ts";
import type { GmailReplyRepository } from "./gmail-reply-repository.ts";
import type { MailHistoryRepository } from "./mail-history.ts";
import { decodeGmailReplyMessage } from "./gmail-reply-message.ts";

export interface GmailReplyRepositoryResolver {
  resolve(context: AccountContext): Promise<Readonly<{ owner: GmailBulkRepositoryOwner; repository: GmailReplyRepository; history: MailHistoryRepository }>>;
}
export class GmailReplySyncStoppedError extends Error {
  constructor() { super("gmail_reply_sync_stopped"); }
}
const emptyDirection = () => ({ pages_listed: 0, summaries_listed: 0, details_fetched: 0, skipped_complete: 0, imported: 0, reconciled_statuses: 0, merged_threads: 0, missing_provider_message_ids: [] as string[], backfill_complete: false, last_full_sync_at: null as number | null, failure: null as unknown });
export interface GmailReplySyncResult {
  mailbox: ReturnType<MailHistoryRepository["mailbox"]>;
  inbound: ReturnType<typeof emptyDirection>;
  outbound: ReturnType<typeof emptyDirection>;
  changed_thread_ids: string[];
  pending_triage: ReturnType<MailHistoryRepository["syncReadback"]>["pending_triage"];
}

// 运行独立于 HTTP 调用者取消；账号 epoch 变化会在每次远端前后及本地提交前阻止后续工作。
export class GmailReplySync {
  readonly #accounts: AccountContextService;
  readonly #identities: VerifiedMailIdentityProvider;
  readonly #repositories: GmailReplyRepositoryResolver;
  readonly #access: GmailAccessProvider;
  readonly #changed: ((context: AccountContext, scopes: readonly ChangeScope[]) => void) | undefined;
  readonly #runs = new Map<string, Promise<GmailReplySyncResult>>();
  readonly #shutdown = new AbortController();
  constructor(accounts: AccountContextService, identities: VerifiedMailIdentityProvider, repositories: GmailReplyRepositoryResolver, access: GmailAccessProvider, onChanged?: (context: AccountContext, scopes: readonly ChangeScope[]) => void) {
    this.#accounts = accounts; this.#identities = identities; this.#repositories = repositories; this.#access = access; this.#changed = onChanged;
  }
  #key(context: AccountContext, mailboxId: string): string { return JSON.stringify([context.principal.user.id, context.principal.tenantId, context.principal.baseId, context.epoch, mailboxId]); }
  async #resolve(context: AccountContext) {
    this.#accounts.assertCurrent(context);
    const resolved = await this.#repositories.resolve(context);
    this.#accounts.assertCurrent(context);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (resolved.owner[field] !== expected[field] || resolved.repository.owner[field] !== expected[field] || resolved.history.owner[field] !== expected[field]) throw new Error("Gmail reply repository owner mismatch");
    return resolved;
  }
  async status(context: AccountContext, mailboxId: string) {
    const resolved = await this.#resolve(context);
    return { ...resolved.history.syncStatus(mailboxId), running: this.#runs.has(this.#key(context, mailboxId)), progress: null };
  }
  run(context: AccountContext, mailboxId: string): Promise<GmailReplySyncResult> {
    this.#shutdown.signal.throwIfAborted();
    this.#accounts.assertCurrent(context);
    const key = this.#key(context, mailboxId);
    const current = this.#runs.get(key);
    if (current) return current;
    const operation = this.#run(context, mailboxId).finally(() => {
      this.#runs.delete(key);
      if (!context.signal.aborted) this.#changed?.(context, ["email_sync"]);
    });
    this.#runs.set(key, operation);
    this.#changed?.(context, ["email_sync"]);
    return operation;
  }
  async #run(context: AccountContext, mailboxId: string): Promise<GmailReplySyncResult> {
    const resolved = await this.#resolve(context);
    const seeds = resolved.repository.seeds(mailboxId);
    const identity = await this.#identities.snapshot(resolved.owner);
    const signal = AbortSignal.any([this.#shutdown.signal, context.signal]);
    const assertCurrent = async () => { signal.throwIfAborted(); this.#accounts.assertCurrent(context); await this.#identities.assertCurrent(identity); signal.throwIfAborted(); this.#accounts.assertCurrent(context); };
    await assertCurrent();
    const inbound = emptyDirection();
    const changed = new Set<string>();
    const detailErrors: unknown[] = [];
    const execution = (async () => {
      const auth = seeds.length > 0 ? await this.#access.authorize(resolved.owner, mailboxId) : null;
      await assertCurrent();
      if (auth && (auth.owner.accountId !== resolved.owner.accountId || auth.owner.tenantId !== resolved.owner.tenantId || auth.owner.baseId !== resolved.owner.baseId || auth.mailboxRefId !== mailboxId)) throw new Error("gmail_account_mismatch");
      for (const seed of seeds) {
        const ids = await readGmailOutreachReplyIds(auth!, seed.providerMessageId, assertCurrent, signal);
        inbound.pages_listed++; inbound.summaries_listed += ids.length;
        for (let offset = 0; offset < ids.length; offset += 100) {
          await assertCurrent();
          const page = ids.slice(offset, offset + 100);
          const complete = resolved.repository.complete(mailboxId, page);
          inbound.skipped_complete += complete.size;
          const missing = page.filter(id => !complete.has(id));
          if (!missing.length) continue;
          const details = [];
          for (const id of missing) {
            const outcome = await readGmailReplyMessage(auth!, id, assertCurrent, signal).then(value => decodeGmailReplyMessage(value, id, auth!.emailAddress)).then(detail => ({ detail }), error => ({ error }));
            await assertCurrent();
            inbound.details_fetched++;
            if ("detail" in outcome) details.push(outcome.detail);
            else { inbound.missing_provider_message_ids.push(id); detailErrors.push(outcome.error); }
          }
          await assertCurrent();
          const committed = resolved.repository.commitReplies(mailboxId, seed.threadId, details);
          inbound.imported += details.length;
          for (const id of committed) changed.add(id);
          if (committed.length) this.#changed?.(context, ["email_folders", "email_threads"]);
        }
      }
      await assertCurrent();
      if (detailErrors.length) throw new AggregateError(detailErrors, "gmail_reply_details_incomplete");
      resolved.repository.completeCheckpoint(mailboxId);
      const readback = resolved.history.syncReadback(mailboxId);
      inbound.backfill_complete = true;
      inbound.last_full_sync_at = readback.inbound?.last_full_sync_at ?? null;
      inbound.failure = readback.inbound?.failure ?? null;
      return { mailbox: readback.mailbox, inbound, outbound: emptyDirection(), changed_thread_ids: [...changed], pending_triage: readback.pending_triage };
    })();
    return execution.then(result => result, error => {
      // 失败状态提交原账号；不把原始 provider 正文或异常文本存入公开状态。
      const stopped = this.#shutdown.signal.aborted && (error === this.#shutdown.signal.reason || error instanceof Error && (error.name === "AbortError" || error.message === "gmail_read_cancelled"));
      const failure = stopped ? this.#shutdown.signal.reason as unknown : error;
      resolved.repository.recordFailure(mailboxId, failure instanceof Error ? failure.message : "mailbox_sync_failed");
      throw failure;
    });
  }
  stopAcceptingWork(): void { if (!this.#shutdown.signal.aborted) this.#shutdown.abort(new GmailReplySyncStoppedError()); }
  async close(): Promise<void> {
    this.stopAcceptingWork();
    const outcomes = await Promise.allSettled([...this.#runs.values()]);
    const failures = outcomes.flatMap(outcome => outcome.status === "rejected" && outcome.reason !== this.#shutdown.signal.reason ? [outcome.reason as unknown] : []);
    if (failures.length) throw new AggregateError(failures, "Gmail reply sync did not drain successfully");
  }
}

export function registerGmailReplySyncOperation(registry: OperationRegistry, sync: GmailReplySync): void {
  registry.register("onecrew_email_mailbox_sync", z.object({ input: z.object({ mailbox_ref_id: z.string().min(1), mode: z.literal("incremental") }).strict() }).strict(), async ({ input }, context) => sync.run(context, input.mailbox_ref_id));
}
