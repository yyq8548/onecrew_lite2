// Native acceptance against an explicitly supplied unsigned installer. No login
// or search; refuse to disturb any existing test-edition registration.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile, mkdir, access } from 'node:fs/promises';
import { openSync, closeSync, readFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const installer = process.argv[2];
if (process.platform !== 'win32' || !installer || !path.isAbsolute(installer)) throw new Error('Supply the absolute Windows installer path');
const key = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\onecrew_lite_Test';
const native = { windowsHide: true, encoding: 'utf8', timeout: 120000 };
assert.equal(spawnSync('reg.exe', ['query', key], native).status, 1, 'Existing trial registration must not be touched');
const temp = await mkdtemp(path.join(os.tmpdir(), 'onecrew-installer-'));
const installed = path.join(temp, '安装 with spaces');
const profile = path.join(temp, 'workbench-profile');
const local = path.join(temp, 'local-profile');
await mkdir(local);
const env = { ...process.env, PATH: `${process.env.SystemRoot}\\System32;${process.env.SystemRoot}`, LOCALAPPDATA: local };
let invocation = 0;
const run = (file, args) => {
  // Windows descendants can retain an inherited capture pipe after the CLI has
  // exited successfully. File-backed capture waits for the command, not daemon EOF.
  const output = path.join(temp, `command-${++invocation}.out`);
  const stdout = openSync(output, 'w');
  const stderr = openSync(`${output}.err`, 'w');
  try {
    execFileSync(file, args, { ...native, env, cwd: temp, stdio: ['ignore', stdout, stderr] });
    return readFileSync(output, 'utf8');
  } finally { closeSync(stdout); closeSync(stderr); }
};
const cli = path.join(installed, 'onecrew_lite.exe');
const library = path.join(installed, 'onecrew_lite_library/onecrew_lite.exe');
let urls;
try {
  execFileSync(installer, ['/S', `/D=${installed}`], { ...native, windowsVerbatimArguments: true });
  const manifest = JSON.parse(await readFile(path.join(installed, 'manifest.json'), 'utf8'));
  for (const file of manifest.files) {
    const bytes = await readFile(path.join(installed, file.path));
    assert.equal(bytes.length, file.size);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), file.sha256);
  }
  assert.equal(run(cli, ['--version']).trim(), `onecrew_lite ${manifest.version}`);
  assert.equal(run(library, ['--version']).trim(), manifest.library_version);
  const reinstall = spawnSync(installer, ['/S', `/D=${installed}`], { ...native, windowsVerbatimArguments: true });
  assert.equal(reinstall.status, 3, 'Existing CLI must be preserved');
  await writeFile(path.join(installed, 'user-note.txt'), 'preserve unrelated file');
  urls = JSON.parse(run(cli, ['workspace', '--no-browser', '--data-dir', profile]));
  for (const url of [urls.workbench_url, urls.library_url]) {
    const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
    assert.equal(response.status, 200);
    const html = await response.text();
    assert.match(html, /<html/i);
    const script = html.match(/<script[^>]+src="([^"]+)"/i)?.[1];
    assert.ok(script, 'Embedded frontend script');
    const asset = await fetch(new URL(script, response.url), { signal: AbortSignal.timeout(15000) });
    assert.equal(asset.status, 200);
    assert.ok((await asset.arrayBuffer()).byteLength > 0);
  }
  const desktop = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', "[Environment]::GetFolderPath('Desktop')"], native).trim();
  const shortcut = path.join(desktop, 'onecrew_lite Test.lnk');
  await access(shortcut);
  assert.equal(spawnSync('reg.exe', ['query', key, '/v', 'InstallLocation'], native).status, 0);
  console.log(JSON.stringify({ installed, filesVerified: manifest.files.length, ...urls }));
} finally {
  // Stop only the isolated profiles created by this acceptance run.
  spawnSync(cli, ['stop', '--data-dir', profile], { ...native, env });
  spawnSync(library, ['stop', '--data-dir', path.join(local, 'onecrew_lite_tables')], { ...native, env });
}
await writeFile(path.join(profile, 'acceptance-preserve.txt'), 'keep business profile');
execFileSync(path.join(installed, 'Uninstall.exe'), ['/S'], native);
for (let attempt = 0; attempt < 60; attempt++) {
  if (spawnSync('reg.exe', ['query', key], native).status === 1) break;
  await new Promise(resolve => setTimeout(resolve, 250));
}
assert.equal(spawnSync('reg.exe', ['query', key], native).status, 1);
await assert.rejects(access(cli));
await assert.rejects(access(library));
assert.equal(await readFile(path.join(installed, 'user-note.txt'), 'utf8'), 'preserve unrelated file');
assert.equal(await readFile(path.join(profile, 'acceptance-preserve.txt'), 'utf8'), 'keep business profile');
console.log('PASS: installed hashes, clean PATH, reinstall refusal, workspace pages/assets, registration, uninstall and retained profiles/unrelated files');
