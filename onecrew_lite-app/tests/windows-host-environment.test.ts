import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdir, readFile, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { createPrivateDirectory } from '../src/storage/private-backup.ts';
import { removeWindowsServiceHost, startWindowsServiceHost } from '../src/service/windows-host.ts';

test('real Windows task preserves the launching installation physical local data directory', { skip: process.platform !== 'win32', timeout: 60_000 }, async () => {
  const directory = await createPrivateDirectory(fileURLToPath(new URL(`../../.scratch/windows-host-env-${randomUUID()}/`, import.meta.url)));
  const local = join(directory, 'local'); await mkdir(local);
  const original = process.env.LOCALAPPDATA;
  try {
    process.env.LOCALAPPDATA = local;
    await startWindowsServiceHost(join(process.env.SystemRoot!, 'System32/WindowsPowerShell/v1.0/powershell.exe'), ['-NoProfile', '-NonInteractive', '-Command', '[Console]::WriteLine($env:LOCALAPPDATA)'], directory);
    const deadline = performance.now() + 30_000;
    let output = '';
    while (!output.trim() && performance.now() < deadline) {
      output = await readFile(join(directory, 'service.log'), 'utf8').then(value => value, error => { if (error.code === 'ENOENT') return ''; throw error; });
      if (!output.trim()) await delay(100);
    }
    assert.equal(output.trim().toLowerCase(), (await realpath(local)).toLowerCase());
  } finally {
    if (original === undefined) delete process.env.LOCALAPPDATA; else process.env.LOCALAPPDATA = original;
    await removeWindowsServiceHost(directory);
  }
});
