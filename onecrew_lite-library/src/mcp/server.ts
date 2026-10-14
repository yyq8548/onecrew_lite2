import { registerAccountTool, type AccountTools } from '../modules/access/mcp.js';
import { McpServer } from '@modelcontextprotocol/server';
import type { TableService } from '../modules/tables/service.js';
import { registerTableTools } from '../modules/tables/mcp.js';
import { APP_VERSION } from '../version.js';

export const MCP_INSTRUCTIONS = [
  'onecrew_lite provides persistent dynamic tables.',
  'Use account with action status to check login; action login returns a local sign-in page for the user. Business tools require login. Never request credentials in chat; logout only when asked.',
  'Use the connected tools directly and follow their descriptions and schemas. A onecrew_lite skill provides optional examples; no plugin or skill is required to call these tools. Do not look for service source code or start unrelated services.',
  'Creator discovery is owned by the onecrew_lite CLI. This table service does not start independent platform searches.',
  'Report confirmed results and return the exact web_url from successful table operations. An empty table is not saved records; after an uncertain write, inspect existing data before retrying.',
].join(' ');

export const createMcpServer = (
  service: TableService,
  account: AccountTools,
) => {
  const server = new McpServer(
    { name: 'onecrew_lite_tables', version: APP_VERSION },
    { instructions: MCP_INSTRUCTIONS },
  );
  registerAccountTool(server, account);
  registerTableTools(server, service, account);
  return server;
};
