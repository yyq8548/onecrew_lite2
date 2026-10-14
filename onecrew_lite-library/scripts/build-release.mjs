import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import {
  access,
  chmod,
  cp,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const version = JSON.parse(await readFile(path.join(root, 'package.json'), 'utf8')).version;
const nodeVersion = 'v26.7.0';
const targetInfo = {
  'darwin-arm64': {
    artifact: 'onecrew_lite-macos-arm64',
    platform: 'darwin',
    arch: 'arm64',
  },
  'darwin-x64': {
    artifact: 'onecrew_lite-macos-x64',
    platform: 'darwin',
    arch: 'x64',
  },
  'win32-arm64': {
    artifact: 'onecrew_lite-windows-arm64.exe',
    platform: 'win32',
    arch: 'arm64',
  },
  'win32-x64': {
    artifact: 'onecrew_lite-windows-x64.exe',
    platform: 'win32',
    arch: 'x64',
  },
};
const defaultTargets = Object.keys(targetInfo);
const usage = `Usage: node scripts/build-release.mjs [options]

Options:
  --target <target>  Build one target (${defaultTargets.join(', ')}); default: all four
  --output <path>    Output directory; default: ~/Downloads/onecrew_lite-${version}-<git-short>
  --skip-build       Skip the one-time pnpm build
  --help             Show this help`;
const scriptArgs = process.argv.slice(2);
if (scriptArgs[0] === '--') scriptArgs.shift();
const { values, positionals } = parseArgs({
  args: scriptArgs,
  options: {
    target: { type: 'string' },
    output: { type: 'string' },
    'skip-build': { type: 'boolean', default: false },
    help: { type: 'boolean', short: 'h' },
  },
  allowPositionals: true,
});
if (values.help) {
  console.log(usage);
  process.exit(0);
}
if (positionals.length) throw new Error(`Unexpected argument: ${positionals[0]}\n\n${usage}`);
if (process.platform !== 'darwin')
  throw new Error('Release packaging requires macOS with the system tar and zip commands.');
const [hostNodeMajor, hostNodeMinor] = process.versions.node.split('.').map(Number);
if (hostNodeMajor < 18 || (hostNodeMajor === 18 && hostNodeMinor < 17))
  throw new Error(`Release packaging requires Node 18.17 or newer; found ${process.version}.`);

const targets = values.target ? [values.target] : defaultTargets;
for (const target of targets) {
  if (!targetInfo[target]) throw new Error(`Unsupported target ${target}.\n\n${usage}`);
}
const commit = execFileSync('git', ['rev-parse', '--short=12', 'HEAD'], {
  cwd: root,
  encoding: 'utf8',
}).trim();
const pluginVersion = `${version}+${commit}`;
const defaultOutput = path.join(os.homedir(), 'Downloads', `onecrew_lite-${version}-${commit}`);
const outputRoot = path.resolve(root, values.output || defaultOutput);
const releaseCache = path.join(root, '.cache', 'release-node', nodeVersion);
const checksumPath = path.join(releaseCache, 'SHASUMS256.txt');
const releaseBinaries = path.join(root, 'release');
const hostTarget = `${process.platform}-${process.arch}`;
if (!targetInfo[hostTarget])
  throw new Error(`Unsupported macOS build host ${hostTarget}; use an arm64 or x64 Mac.`);

await mkdir(releaseCache, { recursive: true });
await ensureChecksums();
const buildNode = await ensureTargetNode(hostTarget, targetInfo[hostTarget]);
const buildNodeVersion = execFileSync(buildNode, ['--version'], { encoding: 'utf8' }).trim();
if (buildNodeVersion !== nodeVersion)
  throw new Error(`Expected build generator ${nodeVersion}; found ${buildNodeVersion}.`);

const destinationNames = targets.flatMap((target) => {
  const stem = `onecrew_lite-${version}-${commit}-${target}`;
  return [`${stem}`, `${stem}.zip`];
});
for (const name of destinationNames) {
  try {
    await access(path.join(outputRoot, name));
    throw new Error(
      `Output already exists: ${path.join(outputRoot, name)}. Choose a new --output path.`,
    );
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('Output already exists:')) throw error;
  }
}
try {
  await access(path.join(outputRoot, 'SHA256SUMS'));
  throw new Error(
    `Output already exists: ${path.join(outputRoot, 'SHA256SUMS')}. Choose a new --output path.`,
  );
} catch (error) {
  if (error instanceof Error && error.message.startsWith('Output already exists:')) throw error;
}

if (!values['skip-build']) execFileSync('pnpm', ['build'], { cwd: root, stdio: 'inherit' });
await mkdir(outputRoot, { recursive: true });

for (const target of targets) {
  const info = targetInfo[target];
  const targetNode = target === hostTarget ? buildNode : await ensureTargetNode(target, info);
  const binary = path.join(releaseBinaries, info.artifact);
  execFileSync(buildNode, ['scripts/build-binary.mjs', '--target', target, '--node', targetNode], {
    cwd: root,
    stdio: 'inherit',
  });
  try {
    if (!(await stat(binary)).isFile()) throw new Error('not a file');
  } catch {
    throw new Error(
      `Missing built binary ${path.relative(root, binary)}. Run pnpm build:binary first.`,
    );
  }

  const stem = `onecrew_lite-${version}-${commit}-${target}`;
  const packageDir = path.join(outputRoot, stem);
  await mkdir(packageDir, { recursive: true });
  await assemblePackage({ packageDir, binary, target });
  execFileSync('zip', ['-qr', path.join(outputRoot, `${stem}.zip`), stem], {
    cwd: outputRoot,
    stdio: 'inherit',
  });
  console.log(`Packaged ${target}: ${path.join(outputRoot, `${stem}.zip`)}`);
}

const sums = [];
for (const target of targets) {
  const archiveName = `onecrew_lite-${version}-${commit}-${target}.zip`;
  sums.push(`${await sha256(path.join(outputRoot, archiveName))}  ${archiveName}`);
}
await writeFile(path.join(outputRoot, 'SHA256SUMS'), `${sums.join('\n')}\n`);
console.log(`Wrote ${path.join(outputRoot, 'SHA256SUMS')}`);

async function ensureChecksums() {
  try {
    await access(checksumPath);
  } catch {
    await download(`https://nodejs.org/dist/${nodeVersion}/SHASUMS256.txt`, checksumPath);
  }
  const checksumText = await readFile(checksumPath, 'utf8');
  if (!checksumText.includes(`node-${nodeVersion}-darwin-arm64.tar.gz`))
    throw new Error(`Invalid Node checksum list: ${checksumPath}`);
}

async function ensureTargetNode(target, info) {
  const nodeAsset =
    info.platform === 'darwin'
      ? `node-${nodeVersion}-${target}.tar.gz`
      : `win-${info.arch}/node.exe`;
  const assetPath = path.join(releaseCache, nodeAsset);
  const expectedHash = checksumFor(nodeAsset);
  await mkdir(path.dirname(assetPath), { recursive: true });
  try {
    if ((await sha256(assetPath)) !== expectedHash) throw new Error('checksum mismatch');
  } catch {
    await download(`https://nodejs.org/dist/${nodeVersion}/${nodeAsset}`, assetPath);
    if ((await sha256(assetPath)) !== expectedHash)
      throw new Error(`SHA256 verification failed for Node ${nodeVersion} asset ${nodeAsset}`);
  }

  if (info.platform !== 'darwin') return assetPath;

  const archiveName = path.basename(nodeAsset, '.tar.gz');
  const nodeRoot = path.join(releaseCache, archiveName);
  await rm(nodeRoot, { recursive: true, force: true });
  execFileSync(
    'tar',
    ['-xzf', assetPath, '-C', releaseCache, `${archiveName}/bin/node`, `${archiveName}/LICENSE`],
    { stdio: 'inherit' },
  );
  const nodePath = path.join(nodeRoot, 'bin', 'node');
  await chmod(nodePath, 0o755);
  return nodePath;
}

function checksumFor(asset) {
  const text = readFileSyncText(checksumPath);
  const expected = text
    .split(/\r?\n/)
    .map((line) => line.match(/^([a-f\d]{64})\s+\*?(.+)$/i))
    .find((match) => match?.[2] === asset)?.[1];
  if (!expected) throw new Error(`No SHA256 entry for Node asset ${asset}`);
  return expected.toLowerCase();
}

function readFileSyncText(file) {
  return readFileSync(file, 'utf8');
}

async function download(url, destination) {
  const response = await fetch(url, { signal: AbortSignal.timeout(120_000) });
  if (!response.ok) throw new Error(`Download failed (${response.status}): ${url}`);
  const temporary = `${destination}.download`;
  await writeFile(temporary, new Uint8Array(await response.arrayBuffer()));
  await rename(temporary, destination);
}

async function sha256(file) {
  const hash = createHash('sha256');
  const data = await readFile(file);
  hash.update(data);
  return hash.digest('hex');
}

async function assemblePackage({ packageDir, binary, target }) {
  const windows = target.startsWith('win32-');
  const extension = path.join(packageDir, 'browser-extension');
  const pluginRoot = path.join(packageDir, 'plugins', 'onecrew_lite-tables');
  const sourcePlugin = path.join(root, 'plugins', 'onecrew-tables');
  const sourceMetadata = JSON.parse(
    await readFile(path.join(sourcePlugin, '.codex-plugin', 'plugin.json'), 'utf8'),
  );
  const packageName = windows ? 'onecrew_lite.exe' : 'onecrew_lite';

  await cp(binary, path.join(packageDir, packageName));
  await mkdir(path.join(pluginRoot, 'skills'), { recursive: true });
  await copyTree(path.join(sourcePlugin, 'skills'), path.join(pluginRoot, 'skills'));
  await writeJson(path.join(pluginRoot, '.mcp.json'), {
    mcpServers: {
      onecrew_lite_tables: {
        type: 'stdio',
        command: 'onecrew_lite',
        args: ['mcp'],
      },
    },
  });
  sourceMetadata.version = pluginVersion;
  sourceMetadata.mcpServers = './.mcp.json';
  await writeJson(path.join(pluginRoot, '.codex-plugin', 'plugin.json'), sourceMetadata);
  await cp(
    path.join(root, '.agents', 'plugins', 'marketplace.json'),
    path.join(packageDir, '.agents', 'plugins', 'marketplace.json'),
  );

  await copyBrowserExtension(path.join(root, 'browser-extension'), extension);
  await mkdir(path.join(packageDir, 'licenses', 'sea'), { recursive: true });
  await cp(
    path.join(root, 'THIRD_PARTY_NOTICES.md'),
    path.join(packageDir, 'licenses', 'THIRD_PARTY_NOTICES.md'),
  );
  await copyTree(
    path.join(root, '.cache', 'sea', target, 'stage', 'licenses'),
    path.join(packageDir, 'licenses', 'sea'),
  );
  const nodeLicense = await findNodeLicense();
  await cp(nodeLicense, path.join(packageDir, 'licenses', 'Node-LICENSE'));
  await writeFile(
    path.join(packageDir, windows ? 'install.cmd' : 'install.command'),
    installWrapper(windows),
  );
  if (!windows) {
    await chmod(path.join(packageDir, 'install.command'), 0o755);
  }
  await writeFile(path.join(packageDir, 'README.md'), packageReadme(target, windows));

  const files = await listFiles(packageDir);
  await writeJson(path.join(packageDir, 'manifest.json'), {
    version,
    commit,
    target,
    nodeVersion,
    files: await Promise.all(
      files.map(async (file) => ({
        path: file,
        size: (await stat(path.join(packageDir, file))).size,
        sha256: await sha256(path.join(packageDir, file)),
      })),
    ),
  });
}

async function findNodeLicense() {
  const knownMacLicenses = [
    path.join(releaseCache, `node-${nodeVersion}-darwin-arm64`, 'LICENSE'),
    path.join(releaseCache, `node-${nodeVersion}-darwin-x64`, 'LICENSE'),
  ];
  for (const candidate of knownMacLicenses) {
    try {
      if ((await stat(candidate)).isFile()) return candidate;
    } catch {
      // A Windows-only build may not have extracted either macOS archive.
    }
  }
  const licensePath = path.join(releaseCache, 'Node-LICENSE');
  try {
    if ((await stat(licensePath)).isFile()) return licensePath;
  } catch {
    // Fetch the license from the official Node.js source tag below.
  }
  await download(
    `https://raw.githubusercontent.com/nodejs/node/${nodeVersion}/LICENSE`,
    licensePath,
  );
  return licensePath;
}

function installWrapper(windows) {
  if (windows)
    return `@echo off
setlocal
if not defined LOCALAPPDATA goto no_localappdata
for %%I in ("%~dp0.") do set "SOURCE_DIR=%%~fI"
set "INSTALL_DIR=%LOCALAPPDATA%\\Programs\\onecrew_lite_tables"
set "DATA_DIR=%LOCALAPPDATA%\\onecrew_lite_tables"
if /I "%SOURCE_DIR%"=="%INSTALL_DIR%" goto configure
if not exist "%INSTALL_DIR%\\onecrew_lite.exe" goto copy_files
"%INSTALL_DIR%\\onecrew_lite.exe" stop --data-dir "%DATA_DIR%"
if errorlevel 1 goto stop_failed
:copy_files
robocopy "%SOURCE_DIR%" "%INSTALL_DIR%" /E /COPY:DAT /R:1 /W:1
if errorlevel 8 goto failed
:configure
"%INSTALL_DIR%\\onecrew_lite.exe" configure-plugin --plugin-dir "%INSTALL_DIR%\\plugins\\onecrew_lite-tables" --data-dir "%DATA_DIR%"
if errorlevel 1 goto failed
"%INSTALL_DIR%\\onecrew_lite.exe" start --data-dir "%DATA_DIR%"
if errorlevel 1 goto failed
echo Installed onecrew_lite_tables to "%INSTALL_DIR%"
echo Codex plugin commands:
echo   codex plugin marketplace add "%INSTALL_DIR%"
echo   codex plugin add onecrew_lite-tables@onecrew_lite-tables
set "RESULT=0"
goto finish
:no_localappdata
echo LOCALAPPDATA is not set. Installation cannot continue.
set "RESULT=1"
goto finish
:stop_failed
set "RESULT=%ERRORLEVEL%"
echo Could not stop the installed onecrew_lite_tables service. Files were not copied.
goto finish
:failed
set "RESULT=%ERRORLEVEL%"
if "%RESULT%"=="0" set "RESULT=1"
echo onecrew_lite_tables installation failed with code %RESULT%.
:finish
echo.
pause
exit /b %RESULT%
`.replaceAll('\n', '\r\n');
  return `#!/bin/sh
set -eu
SOURCE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
DATA_DIR="$HOME/Library/Application Support/onecrew_lite_tables"
mkdir -p "$HOME/Applications"
INSTALL_PARENT="$(CDPATH= cd -- "$HOME/Applications" && pwd -P)"
INSTALL_DIR="$INSTALL_PARENT/onecrew_lite_tables"
if [ -d "$INSTALL_DIR" ]; then INSTALL_DIR="$(CDPATH= cd -- "$INSTALL_DIR" && pwd -P)"; fi
if [ "$SOURCE_DIR" != "$INSTALL_DIR" ]; then
  if [ -e "$INSTALL_DIR/onecrew_lite" ]; then
    "$INSTALL_DIR/onecrew_lite" stop --data-dir "$DATA_DIR"
  fi
  mkdir -p "$INSTALL_DIR"
  /usr/bin/ditto "$SOURCE_DIR" "$INSTALL_DIR"
fi
"$INSTALL_DIR/onecrew_lite" configure-plugin --plugin-dir "$INSTALL_DIR/plugins/onecrew_lite-tables" --data-dir "$DATA_DIR"
"$INSTALL_DIR/onecrew_lite" start --data-dir "$DATA_DIR"
printf 'Installed onecrew_lite_tables to %s\\n' "$INSTALL_DIR"
printf 'Codex plugin commands:\\n  codex plugin marketplace add "%s"\\n  codex plugin add onecrew_lite-tables@onecrew_lite-tables\\n' "$INSTALL_DIR"
`;
}

function packageReadme(target, windows) {
  const binary = windows ? '.\\onecrew_lite.exe' : './onecrew_lite';
  const installDirectory = windows
    ? '%LOCALAPPDATA%\\Programs\\onecrew_lite_tables'
    : '~/Applications/onecrew_lite_tables';
  const marketplaceCommand = windows
    ? 'codex plugin marketplace add "${env:LOCALAPPDATA}\\Programs\\onecrew_lite_tables"'
    : 'codex plugin marketplace add "$HOME/Applications/onecrew_lite_tables"';
  const platformNotes = windows
    ? 'Windows binaries are not code-signed. Both Windows targets have passed static architecture checks; runtime acceptance is pending.'
    : target === 'darwin-x64'
      ? '此 macOS x64 程序尚未公证。在 Apple Silicon 上通过 Rosetta 的运行验收；尚未在 Intel Mac 实机验收。若系统拦截，请到“系统设置 → 隐私与安全性”选择“仍要打开”。'
      : '此 macOS arm64 程序未公证。若系统拦截，请到“系统设置 → 隐私与安全性”选择“仍要打开”。';
  return (
    `# onecrew_lite ${version} (${target})

这是 onecrew_lite 独立分发包，包含服务程序、Codex 插件文件和 Chrome 扩展。使用时不需要安装 Node.js，不需要改 PATH，也不需要源码仓库。

## 安装、启动与登录

先双击 \`${windows ? 'install.cmd' : 'install.command'}\`。程序会复制到 \`${installDirectory}\`，配置插件路径并启动服务。新数据空间首次使用需要登录，旧版 onecrew_lite 或 \`io.onecrewai.onecrew_lite\` 数据不会自动迁移。

服务在安装后会自动启动并打印本机 URL，登录后可在安装目录直接运行 \`${binary} status\` / \`${binary} stop\` / \`${binary} start\`。服务与 MCP 共用系统数据目录：macOS ` +
    '`~/Library/Application Support/onecrew_lite_tables`' +
    `，Windows ` +
    '`%LOCALAPPDATA%\\onecrew_lite_tables`' +
    `。MCP 命令不会自动启动服务；若服务已停止，请从安装目录运行 \`${binary} start\`。

Chrome 扩展会自动发现本机默认端口范围内的服务。更新程序时可替换整个分发目录，数据仍保留在系统用户数据目录。

## 配置并安装 Codex 插件

完成安装后，在${windows ? ' PowerShell' : '终端'}运行以下命令；可以从任意目录执行：

\`\`\`${windows ? 'powershell' : 'sh'}
${marketplaceCommand}
codex plugin add onecrew_lite-tables@onecrew_lite-tables
\`\`\`

本地市场固定注册到 \`${installDirectory}\`，manifest 位于其中的 \`.agents/plugins/marketplace.json\`。不要在 Downloads 中用 \`marketplace add .\` 注册解压目录。安装程序只准备文件和连接配置；上述命令才会安装 Codex 插件。

移动安装程序后，在安装目录运行 \`${binary} configure-plugin --plugin-dir <插件目录> --data-dir <数据目录>\` 并重新安装插件缓存，让 Codex 使用更新后的绝对路径。MCP 不会自行启停服务。

## Chrome 扩展

打开 ` +
    '`chrome://extensions`' +
    `，启用开发者模式，选择“加载已解压的扩展程序”，再选安装目录 \`${installDirectory}\` 内的 ` +
    '`browser-extension`' +
    ` 文件夹。插件市场和 Chrome 扩展都应使用安装目录；完成后可删除 Downloads 中的解压包。

## 平台与许可证

${platformNotes}

许可证和第三方声明位于 ` +
    '`licenses/`' +
    `。` +
    '`manifest.json`' +
    ` 记录来源提交、目标、Node 版本及包内文件散列。
`
  );
}

async function copyBrowserExtension(source, destination) {
  await mkdir(destination, { recursive: true });
  for (const entry of await readdir(source, { withFileTypes: true })) {
    if (entry.name === 'tests' || entry.name.startsWith('.') || entry.name === 'node_modules')
      continue;
    const sourcePath = path.join(source, entry.name);
    const destinationPath = path.join(destination, entry.name);
    if (entry.isDirectory()) await copyBrowserExtension(sourcePath, destinationPath);
    else if (entry.isFile()) await cp(sourcePath, destinationPath);
  }
}

async function copyTree(source, destination) {
  await mkdir(path.dirname(destination), { recursive: true });
  await cp(source, destination, { recursive: true, dereference: true });
}

async function listFiles(directory, relative = '') {
  const entries = await readdir(path.join(directory, relative), { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const child = path.join(relative, entry.name);
    if (entry.isDirectory()) files.push(...(await listFiles(directory, child)));
    else if (entry.isFile() && child !== 'manifest.json')
      files.push(child.split(path.sep).join('/'));
  }
  return files.sort();
}

async function writeJson(file, value) {
  await mkdir(path.dirname(file), { recursive: true });
  await writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}
