import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { ChangeScope } from "../application/domain-changes.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";
import { MailHistoryRepository } from "./mail-history.ts";

export const mailThreadMarkReadInput = z.object({ input: z.object({ thread_id: z.string().min(1) }).strict() }).strict();
const disposition = z.enum(["archive", "unarchive", "restore", "delete"]);
export const mailThreadDispositionInput = z.object({ input: z.object({ thread_id: z.string().min(1), action: disposition }).strict() }).strict();

// 仅维护本地收件注意状态和文件夹归属，不具有 provider 或发送能力。
export class MailThreadMaintenanceRepository {
  readonly #db: DatabaseSync;
  readonly #history: MailHistoryRepository;
  readonly owner: Readonly<GmailBulkRepositoryOwner>;
  constructor(db: DatabaseSync, owner: GmailBulkRepositoryOwner) {
    this.#history = new MailHistoryRepository(db, owner);
    this.owner = this.#history.owner;
    this.#db = db;
  }
  #write(operation: () => void): void {
    const sync = this.#db.prepare("PRAGMA synchronous").get()!.synchronous;
    if (this.#db.prepare("PRAGMA journal_mode").get()!.journal_mode !== "wal" || (sync !== 2 && sync !== 3)) throw new Error("Mail thread maintenance requires durable WAL storage");
    this.#db.exec("BEGIN IMMEDIATE");
    let committed = false;
    try { operation(); this.#db.exec("COMMIT"); committed = true; }
    finally { if (!committed && this.#db.isTransaction) this.#db.exec("ROLLBACK"); }
  }
  markRead(threadId: string) {
    this.#write(() => {
      if (!this.#db.prepare("SELECT id FROM email_threads WHERE id=?").get(threadId)) throw new Error("email thread not found");
      this.#db.prepare("UPDATE emails SET delivery_status='read' WHERE thread_id=? AND direction='inbound' AND delivery_status='unread'").run(threadId);
      const updated = this.#db.prepare("UPDATE email_threads SET unread_count=0 WHERE id=?").run(threadId);
      if (updated.changes !== 1) throw new Error("email thread read state update did not affect exactly one thread");
    });
    return this.#history.thread({ thread_id: threadId });
  }
  updateDisposition(threadId: string, action: z.infer<typeof disposition>) {
    disposition.parse(action);
    this.#write(() => {
      const thread = this.#db.prepare("SELECT id,EXISTS(SELECT 1 FROM emails WHERE thread_id=email_threads.id AND direction='inbound') AS has_inbound FROM email_threads WHERE id=?").get(threadId);
      if (!thread) throw new Error("email thread not found");
      if ((action === "archive" || action === "unarchive") && thread.has_inbound !== 1) throw new Error("outbound-only email thread cannot be archived");
      const sql = {
        archive: "UPDATE email_threads SET archived_at=?,updated_at=? WHERE id=?",
        unarchive: "UPDATE email_threads SET archived_at=NULL,active_folder='inbox',updated_at=? WHERE id=?",
        delete: "UPDATE email_threads SET deleted_at=?,updated_at=? WHERE id=?",
        restore: "UPDATE email_threads SET deleted_at=NULL,updated_at=? WHERE id=?",
      }[action];
      const now = Math.floor(Date.now() / 1000);
      const updated = action === "archive" || action === "delete" ? this.#db.prepare(sql).run(now, now, threadId) : this.#db.prepare(sql).run(now, threadId);
      if (updated.changes !== 1) throw new Error("email thread disposition did not affect exactly one thread");
    });
    return this.#history.thread({ thread_id: threadId });
  }
}

export interface MailThreadMaintenanceResolver {
  resolve(context: AccountContext): Promise<Readonly<{ owner: GmailBulkRepositoryOwner; repository: MailThreadMaintenanceRepository }>>;
}
export function registerMailThreadMaintenanceOperations(registry: OperationRegistry, resolver: MailThreadMaintenanceResolver, accounts: AccountContextService, onChanged?: (context: AccountContext, scopes: readonly ChangeScope[]) => void): void {
  async function maintain<T>(context: AccountContext, operation: (repository: MailThreadMaintenanceRepository) => T): Promise<T> {
    accounts.assertCurrent(context);
    const resolved = await resolver.resolve(context);
    accounts.assertCurrent(context);
    const owner = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (owner[field] !== resolved.owner[field] || owner[field] !== resolved.repository.owner[field]) throw new Error("Mail thread maintenance repository owner mismatch");
    return operation(resolved.repository);
  }
  registry.register("onecrew_email_thread_mark_read", mailThreadMarkReadInput, async ({ input }, context) => {
    const value = await maintain(context, repository => repository.markRead(input.thread_id));
    onChanged?.(context, ["email_threads"]);
    return value;
  });
  registry.register("onecrew_email_thread_update_disposition", mailThreadDispositionInput, async ({ input }, context) => {
    const value = await maintain(context, repository => repository.updateDisposition(input.thread_id, input.action));
    onChanged?.(context, ["email_folders", "email_threads"]);
    return value;
  });
}
