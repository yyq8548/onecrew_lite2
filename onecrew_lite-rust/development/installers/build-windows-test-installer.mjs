// Unsigned, opt-in customer trial. Production signing pipeline stays separate.
import { readFile, writeFile, mkdir, lstat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [workspaceArg, outputArg] = process.argv.slice(2);
if (!workspaceArg || !outputArg) throw new Error('Usage: node build-windows-test-installer.mjs WORKSPACE OUTPUT');
const workspace = path.resolve(workspaceArg);
const output = path.resolve(outputArg);
const manifest = JSON.parse(await readFile(path.join(workspace, 'manifest.json'), 'utf8'));
if (manifest.product !== 'onecrew_lite-workspace' || !/^x86_64-pc-windows-(gnu|msvc)$/.test(manifest.target) || !/^\d+\.\d+\.\d+$/.test(manifest.version)) throw new Error('Expected verified Windows workspace');
const files = [...manifest.files.map(f => f.path), 'manifest.json'];
if (new Set(files).size !== files.length || !files.includes('onecrew_lite.exe') || !files.includes('onecrew_lite_library/onecrew_lite.exe')) throw new Error('Incomplete workspace');
for (const file of files) {
  if (!file || /[\\"$:\r\n]/.test(file) || file.startsWith('/') || file.split('/').some(p => !p || p === '.' || p === '..')) throw new Error('Unsafe payload path');
  const absolute = path.join(workspace, file);
  for (let item = absolute; item !== workspace; item = path.dirname(item)) {
    if ((await lstat(item)).isSymbolicLink()) throw new Error('Symlink in payload');
  }
  if (!(await lstat(absolute)).isFile()) throw new Error('Payload must be regular file');
  const expected = manifest.files.find(f => f.path === file);
  const bytes = await readFile(absolute);
  if (expected && (expected.size !== bytes.length || expected.sha256 !== createHash('sha256').update(bytes).digest('hex'))) throw new Error(`Integrity mismatch: ${file}`);
}
for (const value of [workspace, output]) if (/["$\r\n]/.test(value)) throw new Error('Unsupported build path');
// A successful launch on a developer PATH can hide missing MinGW/Node runtimes.
const cleanEnv = { ...process.env, PATH: `${process.env.SystemRoot}\\System32;${process.env.SystemRoot}` };
for (const [file, version] of [['onecrew_lite.exe', manifest.version], ['onecrew_lite_library/onecrew_lite.exe', manifest.library_version]]) {
  const reported = execFileSync(path.join(workspace, file), ['--version'], {
    cwd: workspace, env: cleanEnv, encoding: 'utf8', timeout: 15000, windowsHide: true,
  }).trim();
  if (reported !== `onecrew_lite ${version}` && reported !== version) throw new Error(`Wrong executable version: ${file}`);
}
await mkdir(output, { recursive: true });
const directories = [...new Set(files.flatMap(file => {
  const dirs = []; let dir = path.posix.dirname(file);
  while (dir !== '.') { dirs.push(dir); dir = path.posix.dirname(dir); }
  return dirs;
}))].sort((a, b) => b.length - a.length);
const install = files.map(file => `SetOutPath "$INSTDIR\\${path.win32.dirname(file).replaceAll('/', '\\')}"\nFile "${path.join(workspace, file)}"`).join('\n');
const uninstall = files.map(file => `Delete "$INSTDIR\\${file.replaceAll('/', '\\')}"`).join('\n') + '\n' + directories.map(dir => `RMDir "$INSTDIR\\${dir.replaceAll('/', '\\')}"`).join('\n');
await writeFile(path.join(output, 'payload.nsh'), `!macro InstallPayload\n${install}\n!macroend\n!macro UninstallPayload\n${uninstall}\n!macroend\n`, 'utf8');
const compiler = path.join(process.env['ProgramFiles(x86)'] || 'C:/Program Files (x86)', 'NSIS/makensis.exe');
const source = path.join(path.dirname(fileURLToPath(import.meta.url)), 'onecrew-windows-test.nsi');
execFileSync(compiler, ['/INPUTCHARSET', 'UTF8', `/DOneCrewVersion=${manifest.version}`, `/DOneCrewOutput=${output}`, source], { stdio: 'inherit', windowsHide: true });
const installer = path.join(output, `onecrew_lite-Test-Setup-${manifest.version}-windows-x64-unsigned.exe`);
const bytes = await readFile(installer);
await writeFile(`${installer}.sha256`, `${createHash('sha256').update(bytes).digest('hex')}  ${path.basename(installer)}\n`);
console.log(installer);
