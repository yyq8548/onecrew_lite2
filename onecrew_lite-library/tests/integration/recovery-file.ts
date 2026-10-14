import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { writePrivateRecoveryFile } from '../../src/recovery-file.js';

const root = await mkdtemp(path.join(os.tmpdir(), "onecrew-recovery-中文 ' "));
try {
  const first = await writePrivateRecoveryFile(root, Buffer.from('synthetic recovery one'));
  const second = await writePrivateRecoveryFile(root, Buffer.from('synthetic recovery two'));
  assert.notEqual(first, second);
  assert.equal((await readFile(first)).toString(), 'synthetic recovery one');
  assert.equal((await readFile(second)).toString(), 'synthetic recovery two');
  assert.ok(path.relative(root, first).startsWith('recovery-'));
  if (process.platform === 'win32') {
    const script = `
      $ErrorActionPreference = 'Stop'
      $current = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
      foreach ($target in @($env:ONECREW_TEST_ARCHIVE, [IO.Path]::GetDirectoryName($env:ONECREW_TEST_ARCHIVE))) {
        $acl = if ([IO.Directory]::Exists($target)) { [IO.Directory]::GetAccessControl($target) } else { [IO.File]::GetAccessControl($target) }
        if (!$acl.AreAccessRulesProtected) { exit 1 }
        if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $current) { exit 2 }
        $sids = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]) | ForEach-Object { $_.IdentityReference.Value }) | Sort-Object
        $expected = @($current, 'S-1-5-18', 'S-1-5-32-544') | Sort-Object
        if (Compare-Object $sids $expected) { exit 3 }
      }
    `;
    execFileSync(
      path.join(process.env.SystemRoot!, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
      [
        '-NoProfile',
        '-NonInteractive',
        '-EncodedCommand',
        Buffer.from(script, 'utf16le').toString('base64'),
      ],
      {
        windowsHide: true,
        env: { ...process.env, ONECREW_TEST_ARCHIVE: first },
        timeout: 10_000,
        stdio: 'pipe',
      },
    );
  } else {
    assert.equal((await stat(first)).mode & 0o077, 0);
    assert.equal((await stat(path.dirname(first))).mode & 0o077, 0);
  }
  await assert.rejects(
    writePrivateRecoveryFile(path.join(root, 'missing'), Buffer.from('no write')),
    /library_recovery_directory_invalid/,
  );
  console.log('Private recovery file creation passed on ' + process.platform);
} finally {
  // Only the exact mkdtemp workspace created above; no user data directories.
  await rm(root, { recursive: true, force: true });
}
