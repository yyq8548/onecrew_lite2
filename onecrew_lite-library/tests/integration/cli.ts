import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, unlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';
import { bootstrap } from '../../src/bootstrap.js';
import { TestAccess } from '../helpers/test-access.js';

const exec = promisify(execFile);
const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-cli-'));
const entry = path.resolve('dist/cli.js');
const dataDir = path.join(temp, 'profile with spaces');
const runtimeFile = path.join(dataDir, 'run/server.json');
const env = { ...process.env };
delete env.ONECREW_LITE_RUNTIME_FILE;
delete env.ONECREW_LITE_DATA_DIR;
const runAt = (profile: string, command: string, port = '28091') =>
  exec(process.execPath, [entry, command, '--data-dir', profile, '--port', port], {
    cwd: os.tmpdir(),
    env,
  });
const run = (command: string) => runAt(dataDir, command);
const attempt = async (profile: string, command: string, port = '28091') => {
  try {
    const result = await runAt(profile, command, port);
    return { code: 0, stdout: result.stdout, stderr: result.stderr };
  } catch (error) {
    const failure = error as { code?: number | string; stdout?: string; stderr?: string };
    return {
      code: typeof failure.code === 'number' ? failure.code : 1,
      stdout: failure.stdout ?? '',
      stderr: failure.stderr ?? '',
    };
  }
};
const descriptor = async () =>
  JSON.parse(await readFile(runtimeFile, 'utf8')) as {
    pid: number;
    url: string;
    control_token: string;
    instance_id: string;
  };
try {
  const prepared = await bootstrap(dataDir, new TestAccess());
  const scope = await prepared.tenants.resolve(new TestAccess().user);
  const table = await prepared.data.createTable(scope, 'CLI persistence', [
    { name: 'Name', data_type: 'string' },
  ]);
  await prepared.data.createRows(scope, table.id, [
    { values: { [table.columns[0].id]: 'persisted' } },
  ]);
  await prepared.data.close();
  await run('start');
  const first = await descriptor();
  assert.match((await run('start')).stdout, /already running/);
  assert.equal((await descriptor()).pid, first.pid);
  const libraryPage = JSON.parse((await run('web')).stdout) as { url: string };
  assert.deepEqual(libraryPage, { url: `${first.url}/library` });
  assert.equal((await descriptor()).pid, first.pid, 'web must reuse the existing instance');
  assert.equal((await fetch(libraryPage.url)).status, 200);
  assert.equal((await fetch(`${first.url}/_local/shutdown`, { method: 'POST' })).status, 403);
  assert.equal(
    (
      await fetch(`${first.url}/_local/shutdown`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${first.control_token}`,
          Origin: 'https://example.com',
        },
      })
    ).status,
    403,
  );
  // Changing the discovery file must not bypass the per-database lock.
  await assert.rejects(
    exec(process.execPath, [entry, 'serve', '--data-dir', dataDir, '--port', '28120'], {
      env: { ...env, ONECREW_LITE_RUNTIME_FILE: path.join(temp, 'another-runtime.json') },
    }),
    /already running/,
  );
  const response = await fetch(`${first.url}/api/v1/dynamic-tables`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      name: 'CLI persistence',
      columns: [{ name: 'Name', data_type: 'string' }],
    }),
  });
  assert.equal(response.status, 401);
  await run('restart');
  const second = await descriptor();
  assert.notEqual(second.instance_id, first.instance_id);
  assert.notEqual(second.control_token, first.control_token);
  assert.match((await run('status')).stdout, /running/);
  await run('stop');
  const restartedPage = JSON.parse((await run('web')).stdout) as { url: string };
  assert.equal(restartedPage.url, `${(await descriptor()).url}/library`);
  assert.notEqual((await descriptor()).instance_id, second.instance_id);
  await run('stop');
  const reopened = await bootstrap(dataDir, new TestAccess());
  assert.ok(
    (
      await reopened.data.listTables(await reopened.tenants.resolve(new TestAccess().user), 1, 100)
    ).tables.some((value) => value.name === 'CLI persistence'),
  );
  await reopened.data.close();
  assert.match((await run('status')).stdout, /stopped/);
  await run('stop');

  const crashDir = path.join(temp, 'crash');
  await runAt(crashDir, 'start', '28101');
  const crashDescriptor = JSON.parse(
    await readFile(path.join(crashDir, 'run/server.json'), 'utf8'),
  ) as { pid: number };
  process.kill(crashDescriptor.pid, 'SIGKILL');
  let recovered = false;
  for (let attemptNumber = 0; attemptNumber < 20; attemptNumber += 1) {
    const status = await attempt(crashDir, 'status', '28101');
    if (status.code === 0 && /stopped/.test(status.stdout)) {
      recovered = true;
      break;
    }
    await delay(100);
  }
  assert.equal(recovered, true, 'SIGKILL left the profile unavailable');
  await runAt(crashDir, 'start', '28101');
  await runAt(crashDir, 'stop', '28101');

  const corruptDir = path.join(temp, 'corrupt');
  await mkdir(path.join(corruptDir, 'run'), { recursive: true });
  await writeFile(path.join(corruptDir, 'run/server.json'), '{broken');
  assert.match((await runAt(corruptDir, 'status', '28102')).stdout, /stopped/);
  await runAt(corruptDir, 'start', '28102');
  await runAt(corruptDir, 'stop', '28102');

  const emptyLockDir = path.join(temp, 'empty-lock');
  await mkdir(emptyLockDir, { recursive: true });
  await writeFile(path.join(emptyLockDir, 'server.lock'), '');
  assert.match((await runAt(emptyLockDir, 'status', '28103')).stdout, /stopped/);
  await runAt(emptyLockDir, 'start', '28103');
  await runAt(emptyLockDir, 'stop', '28103');

  const fakePidDir = path.join(temp, 'fake-pid');
  await mkdir(path.join(fakePidDir, 'run'), { recursive: true });
  await writeFile(
    path.join(fakePidDir, 'run/server.json'),
    `${JSON.stringify({
      service: 'onecrew_lite',
      protocol_version: 1,
      pid: process.pid,
      port: 28104,
      url: 'http://127.0.0.1:28104',
      instance_id: 'fake-instance',
      control_token: 'fake-token',
      data_dir: fakePidDir,
    })}\n`,
  );
  assert.match((await runAt(fakePidDir, 'status', '28104')).stdout, /stopped/);
  await runAt(fakePidDir, 'start', '28104');
  await runAt(fakePidDir, 'stop', '28104');

  const concurrentDir = path.join(temp, 'concurrent');
  const concurrent = await Promise.all([
    attempt(concurrentDir, 'start', '28105'),
    attempt(concurrentDir, 'start', '28105'),
  ]);
  assert.equal(concurrent.filter((result) => result.code === 0).length, 1);
  assert.match((await runAt(concurrentDir, 'status', '28105')).stdout, /running/);
  await runAt(concurrentDir, 'stop', '28105');

  const hiddenDescriptorDir = path.join(temp, 'hidden-descriptor');
  await runAt(hiddenDescriptorDir, 'start', '28106');
  const hiddenRuntime = path.join(hiddenDescriptorDir, 'run/server.json');
  const hiddenContents = await readFile(hiddenRuntime, 'utf8');
  await unlink(hiddenRuntime);
  const unavailableStatus = await attempt(hiddenDescriptorDir, 'status', '28106');
  assert.equal(unavailableStatus.code, 1);
  const unavailableStop = await attempt(hiddenDescriptorDir, 'stop', '28106');
  assert.equal(unavailableStop.code, 1);
  await writeFile(hiddenRuntime, hiddenContents);
  await runAt(hiddenDescriptorDir, 'stop', '28106');

  console.log(
    'CLI integration ok: independent cwd, spaces, idempotent start/stop, authenticated shutdown, database lock, restart persistence',
  );
} finally {
  await run('stop').catch(() => undefined);
  await rm(temp, { recursive: true, force: true });
}
