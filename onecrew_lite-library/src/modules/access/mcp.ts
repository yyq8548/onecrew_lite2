import { McpServer } from '@modelcontextprotocol/server';
import { z } from 'zod';
import type { AccessService } from './service.js';
import type { TenantContext } from '../tenancy/service.js';

export type AccountTools = {
  service: AccessService;
  loginUrl: string;
  onLogout: (userId: string | null) => void;
  resolveContext: () => Promise<TenantContext>;
};
const result = (value: unknown) => ({
  content: [{ type: 'text' as const, text: JSON.stringify(value) }],
  structuredContent: { result: value },
});

export async function authenticatedResult(
  account: AccountTools,
  operation: (context: TenantContext) => Promise<unknown>,
) {
  let context: TenantContext;
  try {
    context = await account.resolveContext();
  } catch (error) {
    const details = error !== null && typeof error === 'object' ? error : {};
    const status = 'status' in details ? details.status : undefined;
    const code = 'code' in details ? details.code : undefined;
    if (
      status === 401 ||
      status === 503 ||
      code === 'access.login_required' ||
      code === 'access.unavailable'
    ) {
      return {
        ...result({
          code:
            code === 'access.unavailable' || status === 503
              ? 'access.unavailable'
              : 'access.login_required',
          message: 'Use account with action login, then complete authorization in the browser.',
          login_url: account.loginUrl,
        }),
        isError: true,
      };
    }
    throw error;
  }
  return result(await operation(context));
}

export function registerAccountTool(server: McpServer, account: AccountTools) {
  server.registerTool(
    'account',
    {
      title: 'OneCrew account',
      description:
        'Check local login status; login returns the local web page URL for user-completed Access authorization. Logout clears the local credential and locks business tools; only use logout when the user asks. Never request passwords, authorization codes or tokens in chat.',
      inputSchema: z
        .object({ action: z.enum(['status', 'login', 'logout']).default('status') })
        .strict(),
      annotations: { readOnlyHint: false, destructiveHint: true, openWorldHint: true },
    },
    async ({ action }) => {
      if (action === 'logout') {
        const current = await account.service.status();
        const status = await account.service.logout();
        account.onLogout(current.state === 'signed_in' && current.user ? current.user.id : null);
        return result(status);
      }
      return result({ ...(await account.service.status()), login_url: account.loginUrl });
    },
  );
}
