import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { copyFile, mkdir, readFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { readVerifiedRuntime } from '../src/service/runtime.ts';

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
const previous = process.env.ONECREW_PREVIOUS_SEA_EXECUTABLE;
const execute = promisify(execFile);

test('customer launcher handles a fresh upgrade, repeated start and stop, and a stopped installation', { skip: process.platform !== 'win32' || !executable, timeout: 240_000 }, async () => {
  const root = fileURLToPath(new URL(`../../.scratch/customer-launcher-${randomUUID()}/`, import.meta.url));
  const install = join(root, '客户 程序'), directory = join(root, '客户 数据');
  await mkdir(install, { recursive: true });
  for (const name of ['onecrew_lite.exe', 'candidate.json']) await copyFile(join(dirname(resolve(executable!)), name), join(install, name));
  const source = fileURLToPath(new URL('../../development/windows-customer-package/OneCrew.ps1', import.meta.url));
  await copyFile(source, join(install, 'OneCrew.ps1'));
  const args = ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', join(install, 'OneCrew.ps1'), '-DataDirectory', directory, '-SkipCodex'];
  let running = false;
  try {
    const absent = await execute('powershell.exe', [...args, '-Action', 'Check'], { windowsHide: true, timeout: 30_000 });
    assert.equal(JSON.parse(absent.stdout).state, 'not_installed');
    await execute('powershell.exe', [...args, '-Action', 'Upgrade'], { windowsHide: true, timeout: 120_000 }); running = true;
    const owner = await readVerifiedRuntime(directory);
    const key = await readFile(join(directory, 'masterkey'));
    await execute('powershell.exe', [...args, '-Action', 'Start'], { windowsHide: true, timeout: 120_000 });
    const checked = await execute('powershell.exe', [...args, '-Action', 'Check'], { windowsHide: true, timeout: 30_000 });
    assert.equal(JSON.parse(checked.stdout).instance_id, owner.instance_id);
    await execute('powershell.exe', [...args, '-Action', 'Stop'], { windowsHide: true, timeout: 120_000 }); running = false;
    await execute('powershell.exe', [...args, '-Action', 'Stop'], { windowsHide: true, timeout: 30_000 });
    await execute('powershell.exe', [...args, '-Action', 'Upgrade'], { windowsHide: true, timeout: 120_000 }); running = true;
    assert.equal((await readVerifiedRuntime(directory)).port, owner.port);
    assert.deepEqual(await readFile(join(directory, 'masterkey')), key);
  } finally {
    if (running) await execute('powershell.exe', [...args, '-Action', 'Stop'], { windowsHide: true, timeout: 120_000 });
  }
});

test('customer launcher upgrades the real previous binary and preserves its key and port', { skip: process.platform !== 'win32' || !executable || !previous, timeout: 240_000 }, async () => {
  const root = fileURLToPath(new URL(`../../.scratch/customer-launcher-upgrade-${randomUUID()}/`, import.meta.url));
  const install = join(root, '客户 程序'), directory = join(root, '客户 数据');
  await mkdir(install, { recursive: true });
  for (const name of ['onecrew_lite.exe', 'candidate.json']) await copyFile(join(dirname(resolve(executable!)), name), join(install, name));
  await copyFile(fileURLToPath(new URL('../../development/windows-customer-package/OneCrew.ps1', import.meta.url)), join(install, 'OneCrew.ps1'));
  await execute(resolve(previous!), ['start', '--data-dir', directory, '--vault', join(directory, 'vault.sqlite'), '--master-key', join(directory, 'masterkey')], { windowsHide: true, timeout: 120_000 });
  try {
    const original = await readVerifiedRuntime(directory), key = await readFile(join(directory, 'masterkey'));
    await execute('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', join(install, 'OneCrew.ps1'), '-Action', 'Upgrade', '-DataDirectory', directory, '-SkipCodex'], { windowsHide: true, timeout: 180_000 });
    const upgraded = await readVerifiedRuntime(directory);
    assert.equal(upgraded.port, original.port);
    assert.notEqual(upgraded.instance_id, original.instance_id);
    assert.deepEqual(await readFile(join(directory, 'masterkey')), key);
  } finally {
    await execute(resolve(executable!), ['stop', '--data-dir', directory], { windowsHide: true, timeout: 120_000 });
  }
});
