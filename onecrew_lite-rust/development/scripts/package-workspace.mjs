// Offline assembly. Core installation and client adapters have separate entries.
import { createHash } from 'node:crypto';
import { chmod, cp, lstat, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const libraryTargets = {
  'aarch64-apple-darwin': 'darwin-arm64', 'x86_64-apple-darwin': 'darwin-x64',
  'x86_64-pc-windows-msvc': 'win32-x64', 'x86_64-pc-windows-gnu': 'win32-x64',
};
async function digest(file) {
  const bytes = await readFile(file);
  return { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
}
async function verify(file, expected) {
  if (!(await lstat(file)).isFile()) throw Error('Artifact must be a regular file');
  const actual = await digest(file);
  if (!Number.isSafeInteger(expected.size) || expected.size <= 0 || !/^[a-f0-9]{64}$/.test(expected.sha256) ||
      actual.size !== expected.size || actual.sha256 !== expected.sha256) throw Error('Artifact integrity mismatch');
}
function safeRelative(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes(':') || value.includes('\0') ||
      value.startsWith('/') || value.split('/').some(p => !p || p === '..' || p === '.')) throw Error('Invalid Library manifest path');
  return value;
}
async function verifyParents(root, relative) {
  let current = root;
  for (const part of relative.split('/').slice(0, -1)) {
    current = path.join(current, part);
    if (!(await lstat(current)).isDirectory()) throw Error('Library parent must be a real directory');
  }
}
export async function packageWorkspace(cliDir, libraryDir, output) {
  if (!(await lstat(cliDir)).isDirectory() || !(await lstat(libraryDir)).isDirectory()) throw Error('Real artifact directories required');
  const cli = JSON.parse(await readFile(path.join(cliDir, 'manifest.json'), 'utf8'));
  const library = JSON.parse(await readFile(path.join(libraryDir, 'manifest.json'), 'utf8'));
  if (cli.product !== 'onecrew_lite-cli' || cli.schema_version !== 1 || !libraryTargets[cli.target] || library.target !== libraryTargets[cli.target]) throw Error('Workbench and Library targets must match');
  if (!/^\d+\.\d+\.\d+$/.test(cli.version) || !/^\d+\.\d+\.\d+$/.test(library.version)) throw Error('Invalid component version');
  const windows = cli.target.includes('windows');
  const binary = windows ? 'onecrew_lite.exe' : 'onecrew_lite';
  await verify(path.join(cliDir, binary), cli);
  if (!Array.isArray(library.files) || !library.files.some(f => f.path === binary)) throw Error('Library executable missing from manifest');
  const seen = new Set();
  for (const file of library.files) {
    const relative = safeRelative(file.path);
    if (seen.has(relative)) throw Error('Duplicate Library manifest path');
    seen.add(relative);
    await verifyParents(libraryDir, relative);
    await verify(path.join(libraryDir, relative), file);
  }
  await mkdir(output); // Refuse overwriting an existing artifact.
  await cp(path.join(cliDir, binary), path.join(output, binary));
  await cp(path.join(cliDir, 'manifest.json'), path.join(output, 'workbench-manifest.json'));
  const companion = path.join(output, 'onecrew_lite_library');
  await mkdir(companion);
  const companionFiles = library.files.filter(f => f.path === binary || f.path.startsWith('licenses/'));
  for (const file of companionFiles) {
    await mkdir(path.dirname(path.join(companion, file.path)), {recursive: true});
    await cp(path.join(libraryDir, file.path), path.join(companion, file.path));
  }
  await writeFile(path.join(companion, 'manifest.json'), JSON.stringify({...library, files: companionFiles}, null, 2) + '\n');
  const libraryBinary = companionFiles.find(f => f.path === binary);
  const installerName = windows ? 'install-core.ps1' : 'install-core.sh';
  let installer = await readFile(new URL(`../installers/${installerName}`, import.meta.url), 'utf8');
  for (const [key, value] of Object.entries({VERSION: cli.version, TARGET: cli.target, CLI_SHA: cli.sha256,
    CLI_SIZE: cli.size, LIB_SHA: libraryBinary.sha256, LIB_SIZE: libraryBinary.size})) {
    installer = installer.replaceAll(`__${key}__`, String(value));
  }
  if (/__[A-Z_]+__/.test(installer)) throw Error('Unresolved installer substitution');
  await writeFile(path.join(output, installerName), installer, {mode: 0o755});
  if (windows) {
    let steps = await readFile(new URL('../installers/windows-setup-step.ps1', import.meta.url), 'utf8');
    for (const [key, value] of Object.entries({CLI_SHA: cli.sha256, CLI_SIZE: cli.size,
      LIB_SHA: libraryBinary.sha256, LIB_SIZE: libraryBinary.size})) steps = steps.replaceAll(`__${key}__`, String(value));
    await writeFile(path.join(output, 'windows-setup-step.ps1'), steps);
    const windowsSteps = [['verify', '01-校验安装文件'], ['service', '02-注册服务'],
      ['start', '03-启动主程序'], ['opencli', '04-安装OpenCLI'], ['codex', '05-安装Codex插件'], ['library', '06-检查Library']];
    for (const [step, name] of windowsSteps) {
      await writeFile(path.join(output, `${name}.cmd`), `@echo off\r\n"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe" -NoProfile -File "%~dp0windows-setup-step.ps1" -Step ${step}\r\nexit /b %errorlevel%\r\n`);
    }
    await writeFile(path.join(output, '首次使用.cmd'), '@echo off\r\nchcp 65001 >nul\r\necho onecrew_lite: 一次执行一步，失败后停止。\r\n' +
      windowsSteps.map(([, name]) => `echo ${name}\r\n`).join('') +
      'choice /c 123456Q /n /m "选择步骤 1-6，Q 退出："\r\nif errorlevel 7 exit /b 0\r\n' +
      [...windowsSteps].reverse().map(([, name], index) => `if errorlevel ${6-index} goto step${6-index}\r\n`).join('') +
      'exit /b 1\r\n' + windowsSteps.map(([, name], index) => `:step${index+1}\r\ncall "%~dp0${name}.cmd"\r\nexit /b %errorlevel%\r\n`).join(''));
    await writeFile(path.join(output, '连接 Codex.cmd'), '@echo off\r\ncall "%~dp005-安装Codex插件.cmd"\r\nexit /b %errorlevel%\r\n');
    await writeFile(path.join(output, 'connect-codex.ps1'), `$ErrorActionPreference = 'Stop'\n& (Join-Path $PSScriptRoot 'windows-setup-step.ps1') -Step codex\n`);
    await writeFile(path.join(output, 'open-onecrew_lite.cmd'), '@echo off\r\n"%LOCALAPPDATA%\\onecrew_lite\\bin\\onecrew_lite.exe" workspace\r\nif errorlevel 1 exit /b 1\r\n');
    if (cli.target.endsWith('-gnu')) {
      await mkdir(path.join(output, 'licenses'));
      for (const name of ['GCC-RUNTIME-EXCEPTION-3.1.txt', 'GPL-3.0.txt']) await cp(new URL(`../installers/runtime-licenses/${name}`, import.meta.url), path.join(output, 'licenses', name));
    }
  } else {
    await chmod(path.join(output, binary), 0o755);
    await chmod(path.join(companion, binary), 0o755);
    await writeFile(path.join(output, 'connect-codex.sh'), `#!/bin/sh\nset -eu\nCLI="$HOME/.local/bin/onecrew_lite"\n[ -f "$CLI" ] && [ ! -L "$CLI" ] || { printf 'Install onecrew_lite core first.\\n' >&2; exit 1; }\n"$CLI" plugin install\nprintf 'Restart Codex, then verify onecrew_lite tools in the client.\\n'\n`, {mode: 0o755});
    await writeFile(path.join(output, 'open-onecrew_lite.command'), '#!/bin/sh\nset -eu\nexec "$HOME/.local/bin/onecrew_lite" workspace "$@"\n', {mode: 0o755});
  }
  const instructions = `onecrew_lite ${cli.version}\n\n目标：${cli.target}；Library：${library.version}。\n本体安装：${installerName}。先验证完整 ZIP 和 manifest.json，再运行。\n本体安装不依赖 Codex，不登录账号、不打开浏览器、不下载额外依赖。\n安装后使用已安装的 onecrew_lite opencli repair 准备搜索依赖（需联网），再运行 connect-codex.${windows ? 'ps1' : 'sh'} 接入 Codex。\n首次正式支持 Codex；其他客户端尚未验收。插件注册后可能需要完全退出并重启 Codex，再验证真实工具调用。\n使用 open-onecrew_lite.${windows ? 'cmd' : 'command'} 打开工作台与 Library，按提示分别完成登录和浏览器授权。\n${windows ? 'Windows 原生 x64；最低支持系统需以发布验收记录为准。' : 'macOS 13.5 或更新；芯片架构必须与包一致。'}\n不删除旧 OneCrew、Pro 或用户业务数据；不单独更新 CLI。失败时保留报错和恢复目录。\n包是否完成签名/公证以发布验收记录为准，不关闭系统安全保护。\n`;
  const readinessInstructions = '\n下载安装要求：使用目标电脑的终端 HTTP 命令读取私人安装指引，确认 HTTP 200，禁止跟随重定向；不要将私人链接交给网页搜索，也不要把响应直接作为脚本执行。执行前校验 ZIP 大小、SHA256 和包内清单。\n登录验收：工作台与 Library 分别授权，确认登录有效且账号一致。不复制凭据或自动切换账号。通过 Codex 执行 onecrew_components_check，确认 library_state=ready，并验证真实 MCP 状态调用。页面打开、端口在线或命令退出成功不能替代这些检查。\n授权未完成、账号不同或组件检查失败时停止搜索，按具体错误恢复登录或连接，保留已有任务和数据。需要浏览器授权或重启 Codex 时明确提示，不自动发起搜索。\n';
  await writeFile(path.join(output, 'README.md'), instructions + readinessInstructions);
  await writeFile(path.join(output, '使用说明.txt'), instructions + readinessInstructions);
  if (windows) {
    const windowsInstructions = `\nWindows 本体安装后按顺序执行 windows-setup-step.ps1 -Step 参数：\n1. verify：校验两份程序后读取版本。\n2. service：注册当前用户后台服务，记录物理安装目录。\n3. start：启动主程序。\n4. opencli：联网准备搜索运行环境。\n5. codex：安装插件，再验证实际工具调用；必要时保存进度、退出并重开宿主应用。\n6. library：检查工作台和 Library 页面。\n每次只运行一步，失败立即停止；日志位于安装目录 installation-logs。不要关闭安全策略，不自动结束 Codex/ChatGPT 或后台服务。\n完成命令不等于插件连接、账号登录或真实搜索验收通过。\n`;
    await writeFile(path.join(output, 'README.md'), instructions + readinessInstructions + windowsInstructions);
    await writeFile(path.join(output, '使用说明.txt'), instructions + readinessInstructions + windowsInstructions);
    await writeFile(path.join(output, 'Windows测试版使用说明.txt'), instructions + readinessInstructions + windowsInstructions);
  }
  const files = [];
  async function list(dir, prefix = '') {
    for (const entry of await readdir(dir, {withFileTypes: true})) {
      const relative = path.posix.join(prefix, entry.name);
      if (entry.isDirectory()) await list(path.join(dir, entry.name), relative);
      else if (entry.isFile()) files.push({path: relative, ...await digest(path.join(dir, entry.name))});
      else throw Error('Workspace artifacts must not contain symlinks');
    }
  }
  await list(output);
  files.sort((a, b) => a.path.localeCompare(b.path));
  const manifest = {schema_version: 1, product: 'onecrew_lite-workspace', version: cli.version,
    target: cli.target, library_version: library.version, files};
  await writeFile(path.join(output, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  return manifest;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const {values} = parseArgs({options: {'cli-dir': {type: 'string'}, 'library-dir': {type: 'string'}, output: {type: 'string'}}});
  if (!values['cli-dir'] || !values['library-dir'] || !values.output) throw Error('Usage: package-workspace.mjs --cli-dir DIR --library-dir DIR --output NEW_DIR');
  await packageWorkspace(path.resolve(values['cli-dir']), path.resolve(values['library-dir']), path.resolve(values.output));
}
