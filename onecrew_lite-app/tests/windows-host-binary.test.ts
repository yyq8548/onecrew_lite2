import assert from 'node:assert/strict';
import { test } from 'node:test';
import { copyFile, mkdir, readFile, unlink } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID, createHash } from 'node:crypto';
import { createPrivateDirectory } from '../src/storage/private-backup.ts';
import { retainWindowsServiceBinary } from '../src/service/windows-host.ts';

const executable = process.env.ONECREW_SEA_CANDIDATE_EXECUTABLE;
test('real customer binary is retained by hash and concurrent installers reuse one private immutable file', { skip: process.platform !== 'win32' || !executable, timeout: 120_000 }, async () => {
  const directory = await createPrivateDirectory(fileURLToPath(new URL(`../../.scratch/windows-binary-${randomUUID()}/`, import.meta.url)));
  let installation = join(directory, '客户 安装目录');
  while (installation.length < 275) installation = join(installation, '安装路径 带空格');
  await mkdir(installation, { recursive: true });
  const source = join(installation, 'onecrew_lite.exe');
  await copyFile(resolve(executable!), source);
  const paths = await Promise.all([retainWindowsServiceBinary(source, directory), retainWindowsServiceBinary(source, directory)]);
  assert.equal(paths[0], paths[1]);
  assert.ok(paths[0]!.startsWith(join(directory, '.windows-host', 'bin')));
  await unlink(source);
  const digest = (bytes: Uint8Array) => createHash('sha256').update(bytes).digest('hex');
  assert.equal(digest(await readFile(paths[0]!)), digest(await readFile(resolve(executable!))));
});
