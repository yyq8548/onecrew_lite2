import type { AccountTools } from '../modules/access/mcp.js';
import { createMcpHandler } from '@modelcontextprotocol/server';
import type { TableService } from '../modules/tables/service.js';
import { createMcpServer } from './server.js';

export type McpTransport = {
  fetch(request: Request): Promise<Response>;
  close(): Promise<void>;
};

/** Create the official v2 web handler. It serves modern 2026 and legacy 2025
 * traffic through the SDK's native per-request/stateless lifecycle. */
export const createMcpTransport = (
  service: TableService,
  account: Omit<AccountTools, 'loginUrl' | 'resolveContext'>,
  resolveTenant: (
    request: Request,
  ) => Promise<import('../modules/tenancy/service.js').TenantContext>,
): McpTransport => {
  const handler = createMcpHandler(
    (context) => {
      if (!context.requestInfo) throw new Error('HTTP request context is required');
      const request = context.requestInfo;
      const url = new URL(request.url);
      const loginUrl = `http://127.0.0.1:${url.port}/login`;
      return createMcpServer(service, {
        ...account,
        loginUrl,
        resolveContext: () => resolveTenant(request),
      });
    },
    {
      legacy: 'stateless',
      responseMode: 'auto',
      onerror: (error) => console.error('[mcp]', error),
    },
  );
  return { fetch: handler.fetch, close: handler.close };
};
