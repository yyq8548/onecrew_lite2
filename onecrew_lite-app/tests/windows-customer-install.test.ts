import assert from 'node:assert/strict';
import { test } from 'node:test';
import { copyFile, chmod, mkdir, readFile, readdir, unlink } from 'node:fs/promises';
import { dirname, join, resolve, toNamespacedPath } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID, createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readVerifiedRuntime } from '../src/service/runtime.ts';

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
const execute = promisify(execFile);
test('real Windows customer installation supports a long Chinese path with spaces and a separate data directory', { skip: process.platform !== 'win32' || !executable, timeout: 180_000 }, async () => {
  const root = fileURLToPath(new URL(`../../.scratch/customer-install-${randomUUID()}/`, import.meta.url));
  let install = join(root, '客户 安装目录');
  while (install.length < 275) install = join(install, '安装路径 带空格');
  await mkdir(install, { recursive: true });
  const binary = join(install, 'onecrew_lite.exe');
  await copyFile(resolve(executable!), binary); await chmod(binary, 0o444);
  const directory = join(root, '客户 数据目录');
  let running = false;
  try {
    const started = await execute(toNamespacedPath(binary), ['start', '--data-dir', directory, '--vault', join(directory, 'vault.sqlite'), '--master-key', join(directory, 'masterkey')], { windowsHide: true, timeout: 120_000 });
    assert.equal(JSON.parse(started.stdout).state, 'running'); running = true;
    const owner = await readVerifiedRuntime(directory);
    assert.equal((await fetch(owner.url + '/_local/status', { headers: { authorization: `Bearer ${owner.control_token}` } })).status, 200);
    assert.deepEqual(await readdir(dirname(binary)), ['onecrew_lite.exe']);
    const configurations = (await readdir(join(directory, '.windows-host'))).filter(name => name.endsWith('.json'));
    assert.equal(configurations.length, 1);
    const config = JSON.parse(await readFile(join(directory, '.windows-host', configurations[0]!), 'utf8'));
    assert.ok(config.executable.startsWith(join(directory, '.windows-host', 'bin')));
    const prepared = await execute(toNamespacedPath(binary), ['codex-package', '--data-dir', directory, '--codex-home', join(root, 'isolated-codex')], { windowsHide: true, timeout: 120_000 });
    assert.equal(JSON.parse(prepared.stdout).binary_sha256, createHash('sha256').update(await readFile(resolve(executable!))).digest('hex'));
    await chmod(binary, 0o600); await unlink(binary);
    const warm = await execute(resolve(executable!), ['start', '--data-dir', directory, '--vault', join(directory, 'vault.sqlite'), '--master-key', join(directory, 'masterkey')], { windowsHide: true, timeout: 120_000 });
    assert.equal(JSON.parse(warm.stdout).instance_id, owner.instance_id);
    console.log(`customer executable path length=${binary.length}`);
  } finally {
    if (running) await execute(resolve(executable!), ['stop', '--data-dir', directory], { windowsHide: true, timeout: 120_000 });
  }
});
