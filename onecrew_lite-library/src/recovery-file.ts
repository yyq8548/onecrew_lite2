import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { lstat, mkdir, open, rmdir, unlink } from 'node:fs/promises';
import path from 'node:path';

// Static script: paths are data in the environment, never PowerShell syntax.
// The directory and file receive protected ACLs at creation, before secret bytes.
const WINDOWS_WRITE = `
$ErrorActionPreference = 'Stop'
$directory = $env:ONECREW_RECOVERY_DIRECTORY
$file = [IO.Path]::Combine($directory, 'database.tar.gz')
$user = [Security.Principal.WindowsIdentity]::GetCurrent().User
$allowed = @($user.Value, 'S-1-5-18', 'S-1-5-32-544')
$createdDirectory = $false
$createdFile = $false
$stream = $null
try {
  if ([IO.Directory]::Exists($directory)) { throw 'collision' }
  $directoryAcl = New-Object Security.AccessControl.DirectorySecurity
  $directoryAcl.SetOwner($user)
  $directoryAcl.SetAccessRuleProtection($true, $false)
  foreach ($sid in $allowed) {
    $identity = New-Object Security.Principal.SecurityIdentifier($sid)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
    $directoryAcl.AddAccessRule($rule)
  }
  $info = New-Object IO.DirectoryInfo($directory)
  $info.Create($directoryAcl)
  $createdDirectory = $true
  $fileAcl = New-Object Security.AccessControl.FileSecurity
  $fileAcl.SetOwner($user)
  $fileAcl.SetAccessRuleProtection($true, $false)
  foreach ($sid in $allowed) {
    $identity = New-Object Security.Principal.SecurityIdentifier($sid)
    $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', 'Allow')
    $fileAcl.AddAccessRule($rule)
  }
  $stream = New-Object IO.FileStream($file, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::FullControl, [IO.FileShare]::Read, 65536, [IO.FileOptions]::WriteThrough, $fileAcl)
  $createdFile = $true
  foreach ($acl in @($info.GetAccessControl(), $stream.GetAccessControl())) {
    if (!$acl.AreAccessRulesProtected -or $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $user.Value) { throw 'permissions' }
    $rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
    if ($rules.Count -ne 3) { throw 'permissions' }
    foreach ($rule in $rules) {
      if ($allowed -notcontains $rule.IdentityReference.Value -or $rule.IsInherited -or $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl') { throw 'permissions' }
    }
  }
  [Console]::OpenStandardInput().CopyTo($stream)
  $stream.Flush($true)
  $stream.Dispose()
  $stream = $null
} catch {
  if ($stream) { $stream.Dispose() }
  if ($createdFile) { [IO.File]::Delete($file) }
  if ($createdDirectory) { [IO.Directory]::Delete($directory) }
  exit 1
}
`;

/** A new private recovery artifact, never overwrite an existing recovery point.
 * Caller owns the startup lease; no path or child output enters public errors. */
export async function writePrivateRecoveryFile(root: string, bytes: Uint8Array): Promise<string> {
  const rootInfo = await lstat(root).catch(() => undefined);
  if (!rootInfo?.isDirectory() || rootInfo.isSymbolicLink()) {
    throw new Error('library_recovery_directory_invalid');
  }
  const directory = path.join(path.resolve(root), `recovery-${randomUUID()}`);
  const file = path.join(directory, 'database.tar.gz');
  if (process.platform === 'win32') {
    const systemRoot = process.env.SystemRoot;
    if (!systemRoot) throw new Error('library_recovery_permissions_unavailable');
    const child = spawn(
      path.join(systemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'),
      [
        '-NoProfile',
        '-NonInteractive',
        '-EncodedCommand',
        Buffer.from(WINDOWS_WRITE, 'utf16le').toString('base64'),
      ],
      {
        windowsHide: true,
        stdio: ['pipe', 'ignore', 'ignore'],
        env: { ...process.env, ONECREW_RECOVERY_DIRECTORY: directory },
      },
    );
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => child.kill(), 30_000);
      let inputFailed = false;
      child.stdin.on('error', () => {
        inputFailed = true;
      });
      child.once('error', () => {
        clearTimeout(timer);
        reject(new Error('library_recovery_write_failed'));
      });
      child.once('close', (code) => {
        clearTimeout(timer);
        if (code === 0 && !inputFailed) resolve();
        else reject(new Error('library_recovery_write_failed'));
      });
      child.stdin.end(bytes);
    });
    return file;
  }
  if (process.platform !== 'linux' && process.platform !== 'darwin') {
    throw new Error('library_recovery_permissions_unavailable');
  }
  await mkdir(directory, { mode: 0o700 });
  let created = false;
  try {
    const handle = await open(file, 'wx', 0o600);
    created = true;
    try {
      const stat = await handle.stat();
      if (!stat.isFile() || stat.uid !== process.getuid?.() || (stat.mode & 0o077) !== 0) {
        throw new Error('library_recovery_permissions_unavailable');
      }
      await handle.writeFile(bytes);
      await handle.sync();
    } finally {
      await handle.close();
    }
    return file;
  } catch {
    if (created) await unlink(file).catch(() => undefined);
    await rmdir(directory).catch(() => undefined);
    throw new Error('library_recovery_write_failed');
  }
}
