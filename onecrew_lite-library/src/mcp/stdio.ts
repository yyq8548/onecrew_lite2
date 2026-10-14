import { readRuntime, verifyRuntime } from '../runtime.js';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';
import { Server } from '@modelcontextprotocol/server';
import { serveStdio } from '@modelcontextprotocol/server/stdio';
import { MCP_INSTRUCTIONS } from './server.js';
import { APP_VERSION } from '../version.js';

let upstream: Client | undefined;
let generation = '';
let connecting: Promise<Client> | undefined;
// A client conversation may survive logout or a service restart. Keep its account
// binding until the MCP client reconnects; never silently send old work to a new account.
let accountId: string | undefined;

async function resolveRuntime() {
  const runtime = await readRuntime();
  if (!runtime) throw new Error('OneCrew is stopped; start the local service first');
  await verifyRuntime(runtime);
  return { url: new URL(runtime.url), key: runtime.instance_id };
}

async function client(): Promise<Client> {
  if (connecting) return connecting;
  connecting = (async () => {
    const runtime = await resolveRuntime();
    if (upstream && generation === runtime.key) return upstream;
    await upstream?.close();
    upstream = undefined;
    const next = new Client({ name: 'onecrew-local-bridge', version: APP_VERSION });
    try {
      await next.connect(
        new StreamableHTTPClientTransport(new URL('/mcp', runtime.url), {
          fetch: (input, init) => {
            const headers = new Headers(init?.headers);
            if (accountId) headers.set('X-OneCrew-Account-Id', accountId);
            return fetch(input, { ...init, headers });
          },
        }),
      );
    } catch (error) {
      await next.close().catch(() => {});
      throw error;
    }
    upstream = next;
    generation = runtime.key;
    return next;
  })().finally(() => {
    connecting = undefined;
  });
  return connecting;
}

// Forward the server's tool schemas and calls; this entry owns no business tools.
const handle = serveStdio(
  () => {
    const server = new Server(
      { name: 'onecrew_lite_tables', version: APP_VERSION },
      {
        capabilities: { tools: {} },
        instructions: MCP_INSTRUCTIONS,
      },
    );
    server.setRequestHandler('tools/list', async (request) =>
      (await client()).listTools(request.params),
    );
    server.setRequestHandler('tools/call', async (request) => {
      const upstream = await client();
      if (request.params.name !== 'account' && !accountId) {
        const reply = await upstream.callTool({ name: 'account', arguments: { action: 'status' } });
        const structured = reply.structuredContent;
        const value =
          structured && typeof structured === 'object' && 'result' in structured
            ? structured.result
            : undefined;
        if (
          value &&
          typeof value === 'object' &&
          'state' in value &&
          value.state === 'signed_in' &&
          'user' in value &&
          value.user &&
          typeof value.user === 'object' &&
          'id' in value.user &&
          typeof value.user.id === 'string'
        ) {
          accountId ??= value.user.id;
        } else {
          return { ...reply, isError: true };
        }
      }
      return upstream.callTool(request.params);
    });
    return server;
  },
  { onerror: (error) => console.error('[onecrew-stdio]', error.message) },
);

let stopping = false;
const close = async () => {
  if (stopping) return;
  stopping = true;
  await upstream?.close();
  await handle.close();
};
process.stdin.on('end', () => void close());
process.once('SIGINT', () => void close());
process.once('SIGTERM', () => void close());
