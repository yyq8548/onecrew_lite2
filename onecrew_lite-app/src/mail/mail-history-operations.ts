import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";
import { historyDraftListInput, historyPageInput, historyThreadListInput } from "./mail-history.ts";
import type { MailHistoryRepository } from "./mail-history.ts";

// 实现方必须使用验证过的账号数据库映射；不能从 HTTP 请求中的归属字段构造。
export interface MailHistoryRepositoryResolver {
  resolve(context: AccountContext): Promise<Readonly<{ owner: GmailBulkRepositoryOwner; repository: MailHistoryRepository }>>;
}
const id = z.string().min(1);
export function registerMailHistoryOperations(registry: OperationRegistry, resolver: MailHistoryRepositoryResolver, accounts: AccountContextService, syncStatus?: (context: AccountContext, mailboxId: string) => Promise<ReturnType<MailHistoryRepository["syncStatus"]>>): void {
  async function read<T>(context: AccountContext, operation: (repository: MailHistoryRepository) => T): Promise<T> {
    accounts.assertCurrent(context);
    const resolved = await resolver.resolve(context);
    accounts.assertCurrent(context);
    const expected = { accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId };
    for (const field of ["accountId", "tenantId", "baseId"] as const) if (resolved.owner[field] !== expected[field] || resolved.repository.owner[field] !== expected[field]) throw new Error("Mail history repository does not belong to the verified account");
    return operation(resolved.repository);
  }
  registry.register("onecrew_email_mailbox_list", z.object({}).strict(), async (_, context) => read(context, repository => repository.mailboxes()));
  registry.register("onecrew_email_mailbox_get", z.object({ input: z.object({ mailbox_ref_id: id }).strict() }).strict(), async ({ input }, context) => read(context, repository => repository.mailboxGet(input.mailbox_ref_id)));
  registry.register("onecrew_email_folder_list", z.object({ input: z.object({ mailbox_ref_id: id.nullish() }).strict() }).strict(), async ({ input }, context) => read(context, repository => repository.folders(input.mailbox_ref_id ?? null)));
  registry.register("onecrew_email_thread_list", z.object({ input: historyThreadListInput }).strict(), async ({ input }, context) => read(context, repository => repository.threads(input)));
  registry.register("onecrew_email_thread_get", z.object({ input: historyPageInput.extend({ thread_id: id }) }).strict(), async ({ input }, context) => read(context, repository => repository.thread(input)));
  registry.register("onecrew_email_message_get", z.object({ input: z.object({ email_id: id }).strict() }).strict(), async ({ input }, context) => read(context, repository => repository.message(input.email_id)));
  registry.register("onecrew_email_template_list", z.object({ input: historyPageInput }).strict(), async ({ input }, context) => read(context, repository => repository.templates(input)));
  registry.register("onecrew_email_template_get", z.object({ input: z.object({ template_id: id }).strict() }).strict(), async ({ input }, context) => read(context, repository => repository.template(input.template_id)));
  registry.register("onecrew_email_draft_list", z.object({ input: historyDraftListInput }).strict(), async ({ input }, context) => read(context, repository => repository.drafts(input)));
  registry.register("onecrew_email_draft_get", z.object({ input: z.object({ draft_id: id }).strict() }).strict(), async ({ input }, context) => read(context, repository => repository.draft(input.draft_id)));
  registry.register("onecrew_email_mailbox_sync_status", z.object({ input: z.object({ mailbox_ref_id: id }).strict() }).strict(), async ({ input }, context) => syncStatus ? syncStatus(context, input.mailbox_ref_id) : read(context, repository => repository.syncStatus(input.mailbox_ref_id)));
}
