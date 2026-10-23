import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs, promisify } from 'node:util';

const require = createRequire(new URL('../onecrew_lite-app/package.json', import.meta.url));
const { Client } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/index.js')));
const { StdioClientTransport } = await import(pathToFileURL(require.resolve('@modelcontextprotocol/sdk/client/stdio.js')));
const { values } = parseArgs({ options: { 'node-modules': { type: 'string' }, output: { type: 'string' } } });
assert.ok(values['node-modules'] && values.output, 'Required: --node-modules <installed node_modules> --output <new smoke directory>');
const modules = path.resolve(values['node-modules']);
const output = path.resolve(values.output);
await mkdir(output);
const data = path.join(output, 'data');
const entry = path.join(modules, '@onecrew/lite/bin/onecrew-lite.cjs');
const execute = promisify(execFile);
const catalog = JSON.parse(await readFile(new URL('./npm-release-inputs.json', import.meta.url), 'utf8'));
const startedAt = performance.now();
const timings = {};

async function invoke(command) {
  const start = performance.now();
  const result = await execute(process.execPath, [entry, command, '--data-dir', data, ...(command === 'start' ? ['--vault', path.join(data, 'vault.sqlite'), '--master-key', path.join(data, 'masterkey')] : [])], { timeout: 120_000, maxBuffer: 128 * 1024, windowsHide: true });
  timings[command] = Math.round(performance.now() - start);
  assert.ok(!result.stdout.includes('control_token') && !result.stdout.includes('Bearer'));
  return JSON.parse(result.stdout);
}

const resolved = await execute(process.execPath, [entry, '--print-binary-path']);
const binary = resolved.stdout.trim();
const target = `${process.platform}-${process.arch}`;
assert.equal(binary, path.join(modules, `@onecrew/lite-${target}`, catalog.platforms[target].executable));
assert.equal(createHash('sha256').update(await readFile(binary)).digest('hex'), catalog.platforms[target].executableSha256);
assert.equal(JSON.parse(await readFile(path.join(path.dirname(binary), 'candidate.json'), 'utf8')).productVersion, catalog.version);
let running = false;
const client = new Client({ name: 'onecrew-lite-npm-acceptance', version: catalog.version });
const transport = new StdioClientTransport({ command: process.execPath, args: [entry, 'mcp', '--stdio', '--data-dir', data], stderr: 'pipe' });
try {
  const first = await invoke('start');
  running = true;
  assert.equal(first.state, 'running');
  assert.equal((await invoke('status')).instance_id, first.instance_id);
  for (const route of ['/', '/library']) {
    const response = await fetch(`http://127.0.0.1:${first.port}${route}`);
    assert.equal(response.status, 200);
    assert.match(await response.text(), /<script[^>]+src=/u);
  }
  await client.connect(transport);
  assert.ok((await client.listTools()).tools.some(tool => tool.name === 'onecrew_status'));
  const status = await client.callTool({ name: 'onecrew_status', arguments: {} });
  assert.ok(!status.isError);
  const account = JSON.parse(status.content.find(block => block.type === 'text').text);
  assert.equal(account.instance_id, first.instance_id);
  for (const destination of ['workbench', 'library']) {
    const opened = await client.callTool({ name: 'onecrew_workspace_open', arguments: { destination } });
    assert.ok(!opened.isError);
    assert.equal(opened.structuredContent.instance_id, first.instance_id);
    const url = new URL(opened.structuredContent.url);
    assert.equal(url.pathname, destination === 'library' ? '/library' : '/');
    const session = await fetch(`${url.origin}/api/local/session`, { method: 'POST', headers: { origin: url.origin, 'content-type': 'application/json' }, body: JSON.stringify({ ticket: new URLSearchParams(url.hash.slice(1)).get('onecrew_ticket') }) });
    assert.equal(session.status, 200);
    const cookie = session.headers.get('set-cookie').split(';')[0];
    assert.equal((await fetch(`${url.origin}/api/session`, { headers: { cookie } })).status, 200);
  }
  const result = { version: catalog.version, target, executableSha256: catalog.platforms[target].executableSha256, checks: ['npm-wrapper-binary-resolution', 'binary-sha256', 'native-service-start-status', 'workbench-html', 'library-html', 'stdio-mcp-account-status', 'workbench-and-library-authenticated-session'], timingsMilliseconds: timings, totalMilliseconds: Math.round(performance.now() - startedAt) };
  await writeFile(path.join(output, 'acceptance.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify(result, null, 2));
} finally {
  await client.close();
  await transport.close();
  if (running) assert.equal((await invoke('stop')).state, 'stopped');
}
