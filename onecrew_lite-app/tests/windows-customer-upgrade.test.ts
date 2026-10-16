import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { readVerifiedRuntime } from '../src/service/runtime.ts';
import { acquireInstanceLock } from '../src/service/instance-lock.ts';
import { loadOrCreateServiceProfile } from '../src/service/profile.ts';
import { createPrivateDirectory } from '../src/storage/private-backup.ts';

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE, previous = process.env.ONECREW_PREVIOUS_SEA_EXECUTABLE;
const execute = promisify(execFile);
test('real customer upgrade refuses a different running build and retains data and port for an explicit replacement', { skip: process.platform !== 'win32' || !executable || !previous, timeout: 240_000 }, async () => {
  const directory = fileURLToPath(new URL(`../../.scratch/customer-upgrade-${randomUUID()}/`, import.meta.url));
  const args = ['--data-dir', directory, '--vault', join(directory, 'vault.sqlite'), '--master-key', join(directory, 'masterkey')];
  let owner: string | undefined;
  try {
    // 升级测试提供真实可用的既有 profile，旧版程序仍负责创建和打开实际数据。
    await createPrivateDirectory(directory);
    const lease = await acquireInstanceLock(directory);
    const profile = await loadOrCreateServiceProfile(lease).finally(() => lease.close());
    await execute(resolve(previous!), ['start', ...args], { windowsHide: true, timeout: 120_000 }); owner = previous;
    const original = await readVerifiedRuntime(directory);
    assert.equal(original.port, profile.port);
    await assert.rejects(execute(resolve(executable!), ['start', ...args], { windowsHide: true, timeout: 120_000 }), error => {
      const rejected = error as { stdout?: string };
      assert.equal(JSON.parse(rejected.stdout!).code, 'service.build_update_required'); return true;
    });
    assert.equal((await readVerifiedRuntime(directory)).instance_id, original.instance_id);
    await execute(resolve(previous!), ['stop', '--data-dir', directory], { windowsHide: true, timeout: 120_000 }); owner = undefined;
    await execute(resolve(executable!), ['start', ...args], { windowsHide: true, timeout: 120_000 }); owner = executable;
    const upgraded = await readVerifiedRuntime(directory);
    assert.equal(upgraded.port, original.port); assert.notEqual(upgraded.instance_id, original.instance_id);
  } finally {
    if (owner) await execute(resolve(owner), ['stop', '--data-dir', directory], { windowsHide: true, timeout: 120_000 });
  }
});
