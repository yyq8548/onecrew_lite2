import { z } from "zod";
import type { OperationRegistry } from "../application/operation-registry.ts";
import type { AccountContext, AccountContextService } from "../identity/account-context.ts";
import type { ChangeScope } from "../application/domain-changes.ts";
import { AccessGmailProvider } from "./gmail-access.ts";
import type { GmailMailboxRepositoryResolver, VerifiedMailIdentityProvider } from "./gmail-access.ts";

export const gmailAccessCommandInput = z.object({}).strict();

// 只返回 Access 原确认 URL，浏览器是否打开仍由界面和用户控制。
export function registerGmailAccessOperations(
  registry: OperationRegistry,
  accounts: AccountContextService,
  identities: VerifiedMailIdentityProvider,
  repositories: GmailMailboxRepositoryResolver,
  onChanged?: (context: AccountContext, scopes: readonly ChangeScope[]) => void,
): void {
  const access = new AccessGmailProvider(identities, repositories);
  async function run<T>(context: AccountContext, operation: (access: AccessGmailProvider, owner: { accountId: string; tenantId: string; baseId: string }) => Promise<T>): Promise<T> {
    accounts.assertCurrent(context);
    const owner = Object.freeze({ accountId: context.principal.user.id, tenantId: context.principal.tenantId, baseId: context.principal.baseId });
    const value = await operation(access, owner);
    accounts.assertCurrent(context);
    return value;
  }
  registry.register("onecrew_gmail_connect_url", gmailAccessCommandInput, async (_, context) => run(context, (access, owner) => access.connectGoogle(owner)));
  registry.register("onecrew_gmail_refresh", gmailAccessCommandInput, async (_, context) => {
    const value = await run(context, (access, owner) => access.refreshMailboxes(owner));
    onChanged?.(context, ["email_folders"]);
    return value;
  });
  registry.register("onecrew_mail_access_connections", gmailAccessCommandInput, async (_, context) => run(context, (access, owner) => access.connections(owner)));
}
