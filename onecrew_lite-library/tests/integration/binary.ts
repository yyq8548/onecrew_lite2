import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, readFile, rm, copyFile, chmod, mkdir, realpath } from 'node:fs/promises';
import { createServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { bootstrap } from '../../src/bootstrap.js';
import { TestAccess } from '../helpers/test-access.js';
import { APP_VERSION } from '../../src/version.js';

const exec = promisify(execFile);
const artifact = process.argv[2];
if (!artifact) throw new Error('Usage: tsx tests/integration/binary.ts /path/to/onecrew');
const artifactPath = path.resolve(artifact);
if (process.platform === 'win32') assert.equal(path.extname(artifactPath).toLowerCase(), '.exe');
const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-binary-'));
const executable = path.join(temp, process.platform === 'win32' ? 'onecrew.exe' : 'onecrew');
await copyFile(artifactPath, executable);
await chmod(executable, 0o755);
const dataDir = path.join(temp, 'profile with spaces');
const env: NodeJS.ProcessEnv = { ...process.env, PATH: '' };
delete env.ONECREW_LITE_RUNTIME_FILE;
delete env.ONECREW_LITE_DATA_DIR;
delete env.PUBLIC_WEB_URL;
delete env.NODE_PATH;
delete env.NODE_OPTIONS;
const occupied = createServer();
await new Promise<void>((resolve) => occupied.listen(0, '127.0.0.1', resolve));
const port = (occupied.address() as { port: number }).port;
const run = (command: string) =>
  exec(executable, [command, '--data-dir', dataDir, '--port', String(port)], {
    cwd: temp,
    env,
    timeout: 45_000,
  });
const descriptor = async () =>
  JSON.parse(await readFile(path.join(dataDir, 'run', 'server.json'), 'utf8')) as {
    pid: number;
    instance_id: string;
    url: string;
    port: number;
  };

let baseId: string;
let tableId: string;
try {
  const originalHelp = await exec(artifactPath, ['--help'], {
    cwd: temp,
    env,
    timeout: 45_000,
  });
  assert.match(originalHelp.stdout, /Usage:\s+onecrew/);
  const fixture = await bootstrap(dataDir, new TestAccess());
  try {
    const scope = await fixture.tenants.resolve(new TestAccess().user);
    baseId = scope.baseId;
    const table = await fixture.service.createTable(scope, {
      name: 'Binary persistence fixture',
      columns: [{ name: 'Value', data_type: 'string' }],
    });
    tableId = table.id;
    await fixture.service.createRows(scope, table.id, {
      rows: [{ values: { [table.columns[0].id]: 'Persist across binary restart' } }],
    });
  } finally {
    await fixture.data.close();
  }

  const help = await exec(executable, ['--help'], { cwd: temp, env });
  assert.match(help.stdout, /start/);
  const version = await exec(executable, ['--version'], { cwd: temp, env });
  assert.equal(version.stdout.trim(), APP_VERSION);
  assert.equal(version.stderr, '');
  const pluginDir = path.join(temp, 'plugin source');
  await mkdir(path.join(pluginDir, '.codex-plugin'), { recursive: true });
  await copyFile(
    path.resolve('plugins/onecrew-tables/.codex-plugin/plugin.json'),
    path.join(pluginDir, '.codex-plugin', 'plugin.json'),
  );
  await exec(executable, ['configure-plugin', '--plugin-dir', pluginDir, '--data-dir', dataDir], {
    cwd: temp,
    env,
  });
  const pluginCache = path.join(temp, 'installed plugin cache');
  await mkdir(pluginCache);
  await copyFile(path.join(pluginDir, '.mcp.json'), path.join(pluginCache, '.mcp.json'));
  await rm(pluginDir, { recursive: true });
  const pluginMcp = JSON.parse(await readFile(path.join(pluginCache, '.mcp.json'), 'utf8'))
    .mcpServers.onecrew_lite_tables as { command: string; args: string[] };
  assert(path.isAbsolute(pluginMcp.command));
  assert.equal(await realpath(pluginMcp.command), await realpath(executable));
  assert.deepEqual(pluginMcp.args, ['mcp', '--data-dir', dataDir]);
  assert.match((await run('status')).stdout, /stopped/);
  await run('start');
  const first = await descriptor();
  assert(
    first.port > port && first.port < port + 20,
    'binary skips occupied ports within the configured discovery range',
  );
  assert.match((await run('start')).stdout, /already running/);
  const health = await (await fetch(`${first.url}/healthz`)).json();
  assert.equal(health.ok, true);
  assert.equal('base_id' in health, false, 'public health must not disclose tenant storage');
  assert.equal((await fetch(`${first.url}/api/v1/dynamic-tables`)).status, 401);
  const accountStatus = await fetch(`${first.url}/api/v1/access/status`);
  assert.equal(accountStatus.status, 200);
  assert.equal(((await accountStatus.json()) as { state: string }).state, 'signed_out');
  const connectionPage = await fetch(`${first.url}/connections`, { redirect: 'manual' });
  assert.equal(connectionPage.status, 302);
  assert.equal(connectionPage.headers.get('location'), '/login?returnTo=%2Fconnections');
  const completion = await fetch(`${first.url}/auth/complete`);
  assert.equal(completion.status, 200);
  assert.match(completion.headers.get('content-type') ?? '', /text\/html/);
  const login = await fetch(`${first.url}/login`);
  assert.equal(login.status, 200);
  const html = await login.text();
  const asset = html.match(/src="([^"]+\.js)"/)?.[1];
  assert(asset, 'binary includes the built web app');
  assert.equal((await fetch(new URL(asset, first.url))).status, 200);

  const client = new Client({ name: 'binary-acceptance', version: '1' });
  const transport = new StdioClientTransport({
    ...pluginMcp,
    cwd: pluginCache,
    env: Object.fromEntries(
      Object.entries(env).filter((e): e is [string, string] => e[1] !== undefined),
    ),
    stderr: 'pipe',
  });
  let mcpStderr = '';
  transport.stderr?.on('data', (chunk: Buffer) => {
    mcpStderr += chunk.toString();
  });
  try {
    await client.connect(transport);
    assert.equal(client.getServerVersion()?.version, APP_VERSION);
    const listed = await client.listTools();
    assert(listed.tools.some((tool) => tool.name === 'table_create'));
    assert(client.getInstructions());
    const account = await client.callTool({ name: 'account', arguments: { action: 'status' } });
    assert(!account.isError);
    const denied = await client.callTool({ name: 'table_list', arguments: {} });
    assert(denied.isError, 'packaged MCP retains the login gate');
  } catch (error) {
    throw new Error(`Binary MCP failed: ${mcpStderr.slice(-2500)}`, { cause: error });
  } finally {
    await client.close();
  }

  // The descriptor belongs to this test's isolated profile and verified binary.
  process.kill(first.pid, 'SIGKILL');
  await new Promise((resolve) => setTimeout(resolve, 250));
  assert.match((await run('status')).stdout, /stopped/);
  await run('start');
  const recovered = await descriptor();
  assert.notEqual(recovered.instance_id, first.instance_id);
  assert.equal((await fetch(`${recovered.url}/healthz`)).status, 200);
  await run('stop');
  assert.match((await run('status')).stdout, /stopped/);
  const persisted = await bootstrap(dataDir, new TestAccess());
  try {
    const scope = await persisted.tenants.resolve(new TestAccess().user);
    assert.equal(scope.baseId, baseId);
    const rows = await persisted.service.queryRows(scope, tableId, { page: 1, size: 20 });
    assert.equal(rows.rows.length, 1);
    assert.deepEqual(Object.values(rows.rows[0].values), ['Persist across binary restart']);
  } finally {
    await persisted.data.close();
  }
  console.log(
    'Binary acceptance passed: no Node in PATH, unrelated cwd, embedded web assets, MCP, login gate, occupied port, SIGKILL recovery and persisted rows.',
  );
} finally {
  await run('stop').catch(() => undefined);
  await new Promise<void>((resolve) => occupied.close(() => resolve()));
  await rm(temp, { recursive: true, force: true });
}
