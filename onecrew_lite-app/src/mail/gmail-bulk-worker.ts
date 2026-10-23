import { AccountChangedError } from "../identity/account-context.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { ChangeScope } from "../application/domain-changes.ts";
import type { GmailBulkAccountRepositoryResolver, GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";
import type { GmailBulkRepository } from "./gmail-bulk-repository.ts";
import { bulkIdempotencyKey } from "./gmail-bulk.ts";
import type { BulkBatch, BulkItem } from "./gmail-bulk.ts";
import { messageForGmailBulkClaim, messageForGmailBulkReceipt } from "./gmail-mime.ts";
import type { VerifiedGmailSendReceipt } from "./gmail-mime.ts";
import { GmailAcceptedProcessingError, readGmailBulkReceipt, sendGmailBulkMessage } from "./gmail-transport.ts";
import type { GmailAccessProvider, GmailBulkTransportResult } from "./gmail-transport.ts";

export class GmailBulkWorkerNotReadyError extends Error {
  readonly code = "gmail_bulk_service_not_ready";
  constructor() { super("Verified Gmail access and account-owned repository resolver are required before claiming bulk mail"); }
}
class AttemptAlreadyReservedError extends Error {}
function sameOwner(a: GmailBulkRepositoryOwner, b: GmailBulkRepositoryOwner): boolean {
  return a.accountId === b.accountId && a.tenantId === b.tenantId && a.baseId === b.baseId;
}

function persistOriginalReceipt(repository: GmailBulkRepository, batch: BulkBatch, item: BulkItem, attempted: boolean, receipt?: VerifiedGmailSendReceipt): void {
  // 实际调用后的已知回执在同一事务内接受 attempting 或中断后的 unknown，避免读取后竞态。
  if (item.state !== "unknown" && attempted) {
    repository.reconcileReceipt(batch.id, item, bulkIdempotencyKey(item), true, receipt);
    return;
  }
  const prior = repository.get(batch.id).items.find(stored => stored.id === item.id);
  if (!prior) throw new Error("Gmail receipt item is missing from its original repository");
  if (prior.state === "unknown") {
    if (item.state === "unknown") return;
    repository.reconcileReceipt(batch.id, item, bulkIdempotencyKey(item), false, receipt);
  } else repository.finish(batch.id, item);
}

export type GmailBulkTickResult = { state: "idle" } | { state: "processed"; batchId: string; itemId: string; result: GmailBulkTransportResult | "duplicate_blocked" };

// 仅执行一次领取；生产 AccessProvider 和真实数据库归属未装配时，领取前明确失败。
export class GmailBulkWorker {
  readonly #accounts: AccountContextService;
  readonly #resolver: GmailBulkAccountRepositoryResolver | undefined;
  readonly #access: GmailAccessProvider | undefined;
  readonly #changed: ((context: AccountContext, scopes: readonly ChangeScope[]) => void) | undefined;
  #running = false;
  constructor(accounts: AccountContextService, resolver?: GmailBulkAccountRepositoryResolver, access?: GmailAccessProvider, onChanged?: (context: AccountContext, scopes: readonly ChangeScope[]) => void) {
    this.#accounts = accounts;
    this.#resolver = resolver;
    this.#access = access;
    this.#changed = onChanged;
  }

  async readback(context: AccountContext, batchId: string): Promise<void> {
    if (!this.#resolver || !this.#access) throw new GmailBulkWorkerNotReadyError();
    this.#accounts.assertCurrent(context);
    const resolved = await this.#resolver.resolve(context);
    this.#accounts.assertCurrent(context);
    const owner = resolved.owner;
    if (!sameOwner(owner, { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId })) throw new Error("Gmail readback repository owner mismatch");
    const repository = resolved.repository, batch = repository.get(batchId);
    const items = batch.items.filter(item => ["accepted", "unknown"].includes(item.state) && item.provider_message_id !== null);
    if (!items.length) return;
    const auth = await this.#access.authorize(owner, batch.mailbox_ref_id);
    this.#accounts.assertCurrent(context);
    if (!sameOwner(owner, auth.owner) || auth.mailboxRefId !== batch.mailbox_ref_id) throw new Error("Gmail readback authorization owner mismatch");
    for (const item of items) {
      const message = messageForGmailBulkReceipt(batch, item, auth.emailAddress);
      const receipt = await readGmailBulkReceipt(auth, item.provider_message_id!, message, async () => this.#accounts.assertCurrent(context), context.signal);
      this.#accounts.assertCurrent(context);
      if (repository.get(batchId).items.find(current => current.id === item.id)?.state === "sent") continue;
      repository.stageVerifiedReceipt(batchId, item, receipt);
      repository.recoverVerifiedReceipts();
    }
    this.#changed?.(context, ["email_folders", "email_threads"]);
  }

  async tick(shutdown: AbortSignal): Promise<GmailBulkTickResult> {
    if (!this.#resolver || !this.#access) throw new GmailBulkWorkerNotReadyError();
    if (this.#running) throw new Error("Gmail bulk tick is already running");
    this.#running = true;
    try {
      if (shutdown.aborted) return { state: "idle" };
      const context = this.#accounts.current();
      const resolved = await this.#resolver.resolve(context);
      this.#accounts.assertCurrent(context);
      if (shutdown.aborted) return { state: "idle" };
      const owner = Object.freeze({ ...resolved.owner });
      if (!sameOwner(owner, { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId })) throw new Error("Gmail worker repository owner mismatch");
      const repository = resolved.repository;
      if (repository.recoverVerifiedReceipts() > 0) this.#changed?.(context, ["email_folders", "email_threads"]);
      const claim = repository.claim();
      if (!claim) return { state: "idle" };
      const { batch, item } = claim;
      if (item.state === "failed") {
        repository.finish(batch.id, item);
        return { state: "processed", batchId: batch.id, itemId: item.id, result: "duplicate_blocked" };
      }
      const signal = AbortSignal.any([shutdown, context.signal]);
      let postStarted = false;
      const execution = (async () => {
        const auth = await this.#access!.authorize(owner, batch.mailbox_ref_id);
        this.#accounts.assertCurrent(context);
        if (!sameOwner(owner, auth.owner) || auth.mailboxRefId !== batch.mailbox_ref_id) throw new Error("Gmail access authorization owner mismatch");
        const message = messageForGmailBulkClaim(batch, item, auth.emailAddress);
        return sendGmailBulkMessage(auth, message, signal, () => {
          this.#accounts.assertCurrent(context);
          if (signal.aborted) throw new AccountChangedError();
          if (!repository.reserveSendAttempt(batch.id, item, message)) throw new AttemptAlreadyReservedError();
          postStarted = true;
        }, providerMessageId => repository.recordProviderAcceptance(batch.id, item, providerMessageId));
      })();
      const result = await execution.then(value => value, error => {
        const settled: BulkItem = error instanceof GmailAcceptedProcessingError
          ? { ...item, state: "accepted", provider_message_id: error.providerMessageId, detail: "gmail_readback_processing_failed" }
          : { ...item, state: postStarted || error instanceof AttemptAlreadyReservedError ? "unknown" : "failed", detail: error instanceof AttemptAlreadyReservedError ? "gmail_send_already_attempted" : postStarted ? "gmail_send_processing_failed_after_attempt" : "gmail_bulk_pre_send_failed" };
        persistOriginalReceipt(repository, batch, settled, postStarted);
        repository.pauseIfRunning(batch.id);
        throw error;
      });
      const settled: BulkItem = result.state === "sent" ? { ...item, state: "sent", provider_message_id: result.providerMessageId }
        : result.state === "accepted_readback_pending" ? { ...item, state: "accepted", provider_message_id: result.providerMessageId, detail: result.code }
        : { ...item, state: result.state === "unknown" ? "unknown" : "failed", detail: result.code };
      if (result.state === "sent") repository.stageVerifiedReceipt(batch.id, item, result.receipt);
      persistOriginalReceipt(repository, batch, settled, postStarted, result.state === "sent" ? result.receipt : undefined);
      if (result.state === "sent" && !context.signal.aborted) this.#changed?.(context, ["email_folders", "email_threads"]);
      if (signal.aborted) repository.pauseIfRunning(batch.id);
      return { state: "processed", batchId: batch.id, itemId: item.id, result };
    } finally {
      this.#running = false;
    }
  }
}
