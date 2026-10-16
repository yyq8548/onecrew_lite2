import { execFile } from 'node:child_process';
import { realpath } from 'node:fs/promises';
import { isAbsolute, join, toNamespacedPath } from 'node:path';
import { promisify } from 'node:util';

const execute = promisify(execFile);
export async function readWindowsExecutablePath(script: Uint8Array): Promise<string> {
  if (process.platform !== 'win32' || !process.env.SystemRoot) throw new Error('package.windows_system_root_missing');
  const command = '& ([scriptblock]::Create([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:ONECREW_IMAGE_SCRIPT)))) -TargetProcessId $env:ONECREW_IMAGE_PROCESS_ID';
  const result = await execute(join(process.env.SystemRoot, 'System32/WindowsPowerShell/v1.0/powershell.exe'), ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', Buffer.from(command, 'utf16le').toString('base64')], { windowsHide: true, timeout: 30_000, maxBuffer: 256 * 1024, env: { ...process.env, ONECREW_IMAGE_SCRIPT: Buffer.from(script).toString('base64'), ONECREW_IMAGE_PROCESS_ID: String(process.pid) } });
  const value: unknown = JSON.parse(result.stdout);
  if (typeof value !== 'string' || !isAbsolute(value) || /[\p{Cc}]/u.test(value)) throw new Error('package.windows_executable_path_invalid');
  return toNamespacedPath(await realpath(toNamespacedPath(value)));
}
