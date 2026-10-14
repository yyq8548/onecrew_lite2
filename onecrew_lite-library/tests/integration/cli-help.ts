import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { pathToFileURL } from 'node:url';
import { APP_VERSION } from '../../src/version.js';

const exec = promisify(execFile);
const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-cli-help-'));
const entry = path.resolve('dist/cli.js');
const dataDir = path.join(temp, 'profile');
const sideEffectDir = path.join(temp, 'help-profile');
const env = Object.fromEntries(
  Object.entries(process.env).filter(([name]) => !name.startsWith('ONECREW_')),
);

type Result = { code: number; stdout: string; stderr: string };
async function run(args: string[], profile = dataDir): Promise<Result> {
  try {
    const cliArgs = args.includes('--data-dir') ? args : [...args, '--data-dir', profile];
    const result = await exec(process.execPath, [entry, ...cliArgs], {
      cwd: temp,
      env,
      timeout: 3000,
    });
    return { code: 0, stdout: result.stdout, stderr: result.stderr };
  } catch (error) {
    const failure = error as { code?: number | string; stdout?: string; stderr?: string };
    return {
      code: typeof failure.code === 'number' ? failure.code : 1,
      stdout: failure.stdout ?? '',
      stderr: failure.stderr ?? '',
    };
  }
}

async function runDefault(args: string[]): Promise<Result> {
  try {
    const result = await exec(process.execPath, [entry, ...args], {
      cwd: temp,
      env,
      timeout: 3000,
    });
    return { code: 0, stdout: result.stdout, stderr: result.stderr };
  } catch (error) {
    const failure = error as { code?: number | string; stdout?: string; stderr?: string };
    return {
      code: typeof failure.code === 'number' ? failure.code : 1,
      stdout: failure.stdout ?? '',
      stderr: failure.stderr ?? '',
    };
  }
}

try {
  const runtimeModule = pathToFileURL(path.resolve('dist/runtime.js')).href;
  const defaults = await exec(
    process.execPath,
    [
      '--input-type=module',
      '-e',
      `import { runtimePaths } from ${JSON.stringify(runtimeModule)}; console.log(JSON.stringify(runtimePaths()));`,
    ],
    { cwd: temp, env, timeout: 3000 },
  );
  const defaultPaths = JSON.parse(defaults.stdout);
  assert.equal(
    path.basename(defaultPaths.dataDir),
    process.platform === 'linux' ? 'onecrew_lite_tables' : 'onecrew_lite_tables',
    'the new service must use its own data directory instead of the Rust edition namespace',
  );
  for (const flag of ['--version', '-v']) {
    const result = await run([flag], sideEffectDir);
    assert.equal(result.code, 0);
    assert.equal(result.stdout.trim(), APP_VERSION);
    assert.equal(result.stderr, '');
    await assert.rejects(access(sideEffectDir), /ENOENT/);
  }
  const helpInvocations = [
    ['-h'],
    ['--help'],
    ['help'],
    ['help', 'mcp'],
    ['mcp', '--help'],
    ['mcp', '-h'],
    ['help', 'start'],
    ['help', 'stop'],
    ['help', 'restart'],
    ['help', 'status'],
    ['help', 'web'],
    ['help', 'serve'],
    ['help', 'configure-plugin'],
    ['configure-plugin', '--help'],
  ];
  const noArgs = await runDefault([]);
  assert.equal(noArgs.code, 0, 'no-argument help failed');
  assert.equal(noArgs.stderr, '', 'no-argument help wrote stderr');
  assert.ok(noArgs.stdout.length > 40, 'no-argument help was too short');

  for (const args of helpInvocations) {
    const result = await run(args);
    assert.equal(result.code, 0, `help failed: ${args.join(' ')}`);
    assert.equal(result.stderr, '', `help wrote stderr: ${args.join(' ')}`);
    assert.ok(result.stdout.length > 40, `help was too short: ${args.join(' ')}`);
  }

  const optionsOnly = await run(['--data-dir', dataDir]);
  assert.equal(optionsOnly.code, 0, 'options-only help failed');
  assert.equal(optionsOnly.stderr, '', 'options-only help wrote stderr');

  await mkdir(path.join(dataDir, 'run'), { recursive: true });
  await writeFile(path.join(dataDir, 'run', 'server.json'), '{ not valid json');
  for (const args of [
    ['start', '--help'],
    ['status', '--help'],
    ['serve', '--help'],
    ['mcp', '--help'],
  ]) {
    const result = await run(args);
    assert.equal(result.code, 0, `help read corrupt runtime: ${args.join(' ')}`);
    assert.equal(result.stderr, '', `help wrote stderr with corrupt runtime: ${args.join(' ')}`);
  }

  for (const args of [
    ['start', '--help'],
    ['stop', '--help'],
    ['restart', '--help'],
    ['status', '--help'],
  ]) {
    const result = await run(args, sideEffectDir);
    assert.equal(result.code, 0, `command help failed: ${args.join(' ')}`);
    await assert.rejects(access(sideEffectDir), /ENOENT/);
  }

  const general = await run(['--help']);
  assert.match(general.stdout, /ONECREW_LITE_DATA_DIR/);
  assert.match(general.stdout, /8091/);
  assert.match(general.stdout, /next 19/);
  const mcp = await run(['help', 'mcp']);
  for (const phrase of [
    'JSON-RPC',
    'already-running local HTTP',
    'does not start the local service',
    'does not .*browser',
    'run/server.json',
    'follows port\\s+changes',
    'actual service URL shown',
    'ONECREW_LITE_DATA_DIR',
    'Library/Application Support/onecrew_lite_tables',
    '%LOCALAPPDATA%/onecrew_lite_tables',
    'XDG_DATA_HOME/onecrew_lite_tables',
    '/path/to/installed/onecrew mcp',
    '--port does not',
  ]) {
    assert.match(mcp.stdout, new RegExp(phrase));
  }

  for (const args of [
    ['unknown'],
    ['help', 'unknown'],
    ['help', 'mcp', 'extra'],
    ['--unknown'],
    ['configure-plugin'],
    ['status', '--plugin-dir', temp],
  ]) {
    const result = await run(args);
    assert.equal(result.code, 1, `invalid invocation did not exit 1: ${args.join(' ')}`);
    assert.equal(result.stdout, '', `invalid invocation wrote stdout: ${args.join(' ')}`);
    assert.ok(result.stderr.length > 0, `invalid invocation was silent: ${args.join(' ')}`);
    assert.doesNotMatch(result.stderr, / at (file:|node:|\/)/, `stack leaked: ${args.join(' ')}`);
  }

  const pluginDir = path.join(temp, 'plugin with spaces');
  const metadataDir = path.join(pluginDir, '.codex-plugin');
  await mkdir(metadataDir, { recursive: true });
  await writeFile(
    path.join(metadataDir, 'plugin.json'),
    JSON.stringify({ name: 'onecrew-tables', mcpServers: './.mcp.json' }),
  );
  const configured = await run(['configure-plugin', '--plugin-dir', pluginDir], sideEffectDir);
  assert.equal(configured.code, 0, configured.stderr);
  const config = JSON.parse(await readFile(path.join(pluginDir, '.mcp.json'), 'utf8'));
  assert.deepEqual(config.mcpServers.onecrew_tables, {
    command: process.execPath,
    args: [entry, 'mcp', '--data-dir', sideEffectDir],
  });
  await assert.rejects(access(sideEffectDir), /ENOENT/, 'configuration must not start the service');
  await writeFile(path.join(metadataDir, 'plugin.json'), JSON.stringify({ name: 'other-plugin' }));
  const rejected = await run(['configure-plugin', '--plugin-dir', pluginDir]);
  assert.equal(rejected.code, 1);
  assert.deepEqual(JSON.parse(await readFile(path.join(pluginDir, '.mcp.json'), 'utf8')), config);

  await access(path.join(dataDir, 'run', 'server.json'));
  console.log(
    'CLI integration ok: offline help, plugin configuration, validation, and no profile side effects',
  );
} finally {
  await rm(temp, { recursive: true, force: true });
}
