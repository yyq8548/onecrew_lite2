// Local startup acceptance only: no provider reads, Campaign writes or approvals.
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
const require = createRequire(new URL('../../apps/mcp-ui/package.json', import.meta.url));
const { Client } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/index.js')));
const { StdioClientTransport } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/stdio.js')));
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const client = new Client({ name: 'onecrew_lite-workspace-acceptance', version: '1' });
try {
  await client.connect(new StdioClientTransport({ command: process.argv[2], args: ['mcp', '--stdio'] }));
  const { tools } = await client.listTools();
  const web = tools.find(tool => tool.name === 'onecrew_web');
  const uri = web?._meta?.ui?.resourceUri;
  // These wire identifiers are retained by the host and embedded card; they
  // are independent of the public onecrew_lite executable/product name.
  assert(typeof uri === 'string' && uri.startsWith('ui://onecrew/operation-inspector/'), 'Workspace App metadata missing');
  const resource = await client.readResource({ uri });
  assert(resource.contents.some(content => content.mimeType === 'text/html;profile=mcp-app' && content.text?.includes('onecrew/workspace')), 'Workspace UI contract missing');
  const reply = await client.callTool({ name: 'onecrew_web', arguments: {} }, undefined, { timeout: 60000 });
  assert(!reply.isError, 'Workspace startup failed');
  const receipt = reply.structuredContent;
  assert(receipt?.kind === 'workspace' && receipt.startup === 'ready', 'Workbench not ready');
  assert(typeof receipt.library_url === 'string' && receipt.library_error === null,
    `Library not ready: ${receipt.library_error ?? 'missing URL'}`);
  assert(JSON.stringify(reply._meta?.['onecrew/workspace']) === JSON.stringify(receipt), 'Card and model receipt differ');
  assert(typeof reply._meta?.viewUUID === 'string', 'Owner view identity missing');
  assert(Object.hasOwn(receipt, 'workspace_activity') && Object.hasOwn(receipt, 'workspace_activity_error'), 'Workspace progress contract missing');
  const statusReply = await client.callTool({ name: 'onecrew_status', arguments: {} }, undefined, { timeout: 60000 });
  const status = statusReply.structuredContent;
  assert(!statusReply.isError && status, 'Workspace status read failed');
  const signedIn = status.login?.status?.state === 'active';
  assert(signedIn ? status.workspace_activity_error === null
    : status.workspace_activity === null && [null, 'workspace_activity_unavailable'].includes(status.workspace_activity_error),
    'Workspace progress read failed');
  const activity = statusReply.structuredContent.workspace_activity;
  assert(activity === null || (typeof activity.campaign_id === 'string' && Number.isSafeInteger(activity.qualified) && Number.isSafeInteger(activity.pending)), 'Workspace counts missing');
  for (const url of [receipt.url, receipt.library_url]) {
    let response = await fetch(url, { signal: AbortSignal.timeout(5000), redirect: 'manual' });
    if (response.status === 302 || response.status === 303) {
      // A new customer's Library legitimately opens its local sign-in page.
      // Do not follow other redirects or weaken the authenticated-page lane.
      const login = new URL(response.headers.get('location'), url);
      assert(url === receipt.library_url && login.origin === new URL(url).origin &&
        login.pathname === '/login' && login.searchParams.get('returnTo') === '/library' &&
        !login.username && !login.password && !login.hash, 'Unexpected workspace redirect');
      response = await fetch(login, { signal: AbortSignal.timeout(5000), redirect: 'error' });
    }
    assert(response.ok && (await response.text()).toLowerCase().includes('<html'), 'Workspace page unavailable');
  }
  console.log(JSON.stringify({ pass: true, workbench_url: receipt.url, library_url: receipt.library_url, status_card_resource: uri,
    activity: activity ? { campaign_id: activity.campaign_id, stage: activity.stage, qualified: activity.qualified, pending: activity.pending, used: activity.used, max: activity.max, login_required: activity.login_required, read_errors: activity.read_errors } : null }));
} finally { await client.close(); }
