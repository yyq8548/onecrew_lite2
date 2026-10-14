import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { cp, mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Install only packaged artifacts in a directory outside the checkout. This catches
// source/absolute-path dependencies hidden by the developer's node_modules links.
const root = fileURLToPath(new URL('../../', import.meta.url));
const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-distribution-'));
const appDir = path.join(temp, 'app');
const workDir = path.join(temp, 'unrelated-cwd');
let child;
let output = '';
try {
  await mkdir(appDir);
  await mkdir(workDir);
  for (const name of [
    'package.json',
    'pnpm-lock.yaml',
    'pnpm-workspace.yaml',
    'vendor',
    'packages/contracts/package.json',
    'packages/contracts/dist',
    'dist',
    'web/package.json',
    'web/dist',
  ]) {
    await cp(path.join(root, name), path.join(appDir, name), {
      recursive: true,
      filter: (source) => !source.split(path.sep).includes('node_modules'),
    });
  }
  const install = spawn('pnpm', ['install', '--prod', '--offline', '--frozen-lockfile'], {
    cwd: appDir,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let installOutput = '';
  install.stdout.on('data', (data) => {
    installOutput += data;
  });
  install.stderr.on('data', (data) => {
    installOutput += data;
  });
  const [installCode] = await once(install, 'exit');
  assert.equal(installCode, 0, installOutput);

  const runtimeFile = path.join(temp, 'runtime/server.json');
  child = spawn(process.execPath, [path.join(appDir, 'dist/index.js')], {
    cwd: workDir,
    env: {
      ...process.env,
      TEABLE_REPO: path.join(temp, 'does-not-exist'),
      ONECREW_LITE_DATA_DIR: path.join(temp, 'data'),
      ONECREW_LITE_RUNTIME_FILE: runtimeFile,
      PORT: '28091',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', (data) => {
    output += data;
  });
  child.stderr.on('data', (data) => {
    output += data;
  });
  let url;
  const deadline = Date.now() + 60000;
  while (Date.now() < deadline) {
    assert.equal(child.exitCode, null, output);
    try {
      const runtime = JSON.parse(await readFile(runtimeFile, 'utf8'));
      const health = await fetch(`${runtime.url}/healthz`);
      if (health.ok) {
        url = runtime.url;
        break;
      }
    } catch {
      /* Waiting for startup. */
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  assert.ok(url, `No runtime descriptor: ${output}`);
  const page = await fetch(`${url}/library`);
  assert.equal(page.status, 200);
  assert.match(await page.text(), /<html/);
  const created = await fetch(`${url}/api/v1/dynamic-tables`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      name: 'Packaged runtime',
      columns: [{ name: 'Name', data_type: 'string' }],
    }),
  });
  assert.equal(created.status, 401, await created.clone().text());
  console.log(
    'distribution integration ok: offline production install, no external checkout, compiled Node entry, independent cwd, unsigned table creation rejected',
  );
} finally {
  if (child && child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 10000);
    await exited;
    clearTimeout(timer);
  }
  await rm(temp, { recursive: true, force: true });
}
