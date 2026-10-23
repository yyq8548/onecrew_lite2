import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { GmailBulkRepository } from "./gmail-bulk-repository.ts";
import { z } from "zod";

export interface GmailBulkRepositoryOwner {
  readonly accountId: string;
  readonly tenantId: string;
  readonly baseId: string;
}

// 实现方必须从经过验证的安装/账号映射解析数据库归属，不能以请求参数作为归属证明。
// 应用必须提供真实账号数据库映射，禁止直接注册无归属仓储。
export interface GmailBulkAccountRepositoryResolver {
  resolve(context: AccountContext): Promise<Readonly<{
    owner: GmailBulkRepositoryOwner;
    repository: GmailBulkRepository;
  }>>;
}

// 注册实现必须在 resolve 返回后再次 assertCurrent，并比对 owner 的三个字段。
// 四个 Web 命令保持原名称：onecrew_gmail_bulk_recipients/list/prepare/control。
// prepare/control 的外层参数是 { input: ... }；不注册 worker 或底层逐封发送。
export function registerGmailBulkOperations(
  registry: OperationRegistry,
  resolver: GmailBulkAccountRepositoryResolver,
  accounts: AccountContextService,
  onRunnable?: () => void,
): void {
  async function withRepository<T>(context: AccountContext, operation: (repository: GmailBulkRepository) => T): Promise<T> {
    accounts.assertCurrent(context);
    const resolved = await resolver.resolve(context);
    accounts.assertCurrent(context);
    const principal = context.principal;
    if (resolved.owner.accountId !== principal.user.id || resolved.owner.tenantId !== principal.tenantId || resolved.owner.baseId !== principal.baseId) throw new Error("Gmail bulk repository does not belong to the verified account");
    return operation(resolved.repository);
  }
  registry.register("onecrew_gmail_bulk_recipients", z.object({}).strict(), async (_, context) => withRepository(context, repository => repository.recipients()));
  registry.register("onecrew_gmail_bulk_list", z.object({}).strict(), async (_, context) => withRepository(context, repository => repository.list()));
  registry.register("onecrew_gmail_bulk_prepare", z.object({ input: z.object({
    mailbox_ref_id: z.string().min(1), subject: z.string(), body: z.string(),
    recipients: z.array(z.object({ email: z.string(), source: z.string() }).strict()).min(1).max(100),
  }).strict() }).strict(), async ({ input }, context) => withRepository(context, repository => repository.create(input)));
  registry.register("onecrew_gmail_bulk_control", z.object({ input: z.object({ id: z.string().min(1), action: z.enum(["confirm", "pause", "resume", "cancel"]) }).strict() }).strict(), async ({ input }, context) => withRepository(context, repository => {
    repository.control(input.id, input.action);
    if (input.action === "confirm" || input.action === "resume") onRunnable?.();
    return null;
  }));
}
