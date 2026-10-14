// Assemble verified, self-contained workbench and Library artifacts. No runtime
// downloads, dependency installation, account copying or release publication.
import { createHash } from 'node:crypto';
import { chmod, cp, lstat, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const libraryTargets = {
  'aarch64-apple-darwin': 'darwin-arm64',
  'x86_64-apple-darwin': 'darwin-x64',
  'x86_64-pc-windows-msvc': 'win32-x64',
  'x86_64-pc-windows-gnu': 'win32-x64',
};

async function digest(file) {
  const bytes = await readFile(file);
  return { size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
}

async function verify(file, expected) {
  if (!(await lstat(file)).isFile()) throw new Error('Artifact must be a regular file');
  if (!Number.isSafeInteger(expected.size) || expected.size <= 0 || !/^[a-f0-9]{64}$/.test(expected.sha256))
    throw new Error('Invalid artifact size or SHA-256');
  const actual = await digest(file);
  if (actual.size !== expected.size || actual.sha256 !== expected.sha256)
    throw new Error(`Artifact integrity mismatch: ${path.basename(file)}`);
}

function safeRelative(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes('\0') ||
      path.posix.normalize(value) !== value || value.startsWith('/') ||
      value.split('/').some(part => part === '..' || part === '.'))
    throw new Error('Invalid Library manifest path');
  return value;
}

export async function packageWorkspace(cliDir, libraryDir, output) {
  const cli = JSON.parse(await readFile(path.join(cliDir, 'manifest.json'), 'utf8'));
  const library = JSON.parse(await readFile(path.join(libraryDir, 'manifest.json'), 'utf8'));
  if (cli.product !== 'onecrew_lite-cli' || cli.schema_version !== 1 ||
      !libraryTargets[cli.target] || library.target !== libraryTargets[cli.target])
    throw new Error('Workbench and Library targets must match');
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(cli.version))
    throw new Error('Invalid workbench version');
  const windows = cli.target.includes('windows');
  const name = windows ? 'onecrew_lite.exe' : 'onecrew_lite';
  await verify(path.join(cliDir, name), cli);
  if (!Array.isArray(library.files) || !library.files.some(file => file.path === name))
    throw new Error('Library executable missing from manifest');
  const paths = new Set();
  for (const entry of library.files) {
    const relative = safeRelative(entry.path);
    if (paths.has(relative)) throw new Error('Duplicate Library manifest path');
    paths.add(relative);
    await verify(path.join(libraryDir, relative), entry);
  }
  // Never replace an existing download/install directory.
  await mkdir(output);
  await cp(path.join(cliDir, name), path.join(output, name));
  await cp(path.join(cliDir, 'manifest.json'), path.join(output, 'workbench-manifest.json'));
  const libraryOutput = path.join(output, 'onecrew_lite_library');
  await mkdir(libraryOutput);
  // The companion needs its standalone binary and notices. Its separate
  // historical installer, extension and marketplace are not customer entrypoints.
  const companionFiles = library.files.filter(entry => entry.path === name || entry.path.startsWith('licenses/'));
  for (const { path: relative } of companionFiles) {
    const dest = path.join(libraryOutput, relative);
    await mkdir(path.dirname(dest), { recursive: true });
    await cp(path.join(libraryDir, relative), dest);
  }
  await writeFile(path.join(libraryOutput, 'manifest.json'),
    `${JSON.stringify({ ...library, files: companionFiles }, null, 2)}\n`);
  const launcher = windows ? '打开 onecrew_lite.cmd' : '安装并打开 onecrew_lite.command';
  const libraryBinary = companionFiles.find(entry => entry.path === name);
  const launchText = windows
    ? '@echo off\r\n"%~dp0onecrew_lite.exe" workspace >nul\r\nif errorlevel 1 pause\r\n'
    : `#!/bin/sh
set -eu
WORKSPACE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
INSTALL_BIN="$HOME/.local/bin"
STAGED_LIBRARY=''
cleanup() { if [ -n "$STAGED_LIBRARY" ]; then /bin/rm -f "$STAGED_LIBRARY"; fi; }
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
fail() {
  printf '\\n安装或启动未完成。请保留本窗口，将上方报错截图发给提供安装包的人。\\n' >&2
  if [ -t 0 ]; then printf '按回车关闭窗口。'; read -r reply || :; fi
  exit 1
}
case "\${1-}" in ''|--no-browser) ;; *) printf '不支持此启动选项。\\n' >&2; exit 1 ;; esac
printf '正在安装并打开 onecrew_lite 工作台和动态表格，请稍等。\\n'
if [ -L "$HOME/.local" ] || [ -L "$INSTALL_BIN" ] || [ -L "$INSTALL_BIN/onecrew_lite_library" ] || [ -L "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" ]; then
  printf '安装目录包含符号链接，请联系提供安装包的人核对。\\n' >&2; fail
fi
/bin/mkdir -p "$INSTALL_BIN/onecrew_lite_library" || fail
SOURCE_HASH="$(/usr/bin/shasum -a 256 "$WORKSPACE_DIR/onecrew_lite_library/onecrew_lite" | /usr/bin/awk '{print $1}')"
if [ "$SOURCE_HASH" != '${libraryBinary.sha256}' ]; then printf '动态表格程序校验失败。\\n' >&2; fail; fi
"$WORKSPACE_DIR/onecrew_lite" __bootstrap-update --expected-version '${cli.version}' --expected-sha256 '${cli.sha256}' --expected-size '${cli.size}' --install-path "$INSTALL_BIN/onecrew_lite" --json >/dev/null || fail
CURRENT_HASH=''
if [ -f "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" ]; then
  CURRENT_HASH="$(/usr/bin/shasum -a 256 "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" | /usr/bin/awk '{print $1}')"
fi
if [ "$CURRENT_HASH" != "$SOURCE_HASH" ]; then
  if [ -f "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" ]; then "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" stop >/dev/null || fail; fi
  STAGED_LIBRARY="$(/usr/bin/mktemp "$INSTALL_BIN/onecrew_lite_library/.onecrew_lite.XXXXXX")" || fail
  /bin/cp "$WORKSPACE_DIR/onecrew_lite_library/onecrew_lite" "$STAGED_LIBRARY" || fail
  /bin/chmod 755 "$STAGED_LIBRARY" || fail
  STAGED_HASH="$(/usr/bin/shasum -a 256 "$STAGED_LIBRARY" | /usr/bin/awk '{print $1}')"
  if [ "$STAGED_HASH" != "$SOURCE_HASH" ]; then printf '动态表格安装副本校验失败。\\n' >&2; fail; fi
  /bin/mv -f "$STAGED_LIBRARY" "$INSTALL_BIN/onecrew_lite_library/onecrew_lite" || fail
  STAGED_LIBRARY=''
fi
if [ "\${1-}" = '--no-browser' ]; then
  "$INSTALL_BIN/onecrew_lite" workspace --no-browser || fail
elif "$INSTALL_BIN/onecrew_lite" workspace >/dev/null; then
  printf '页面已请求打开。请在浏览器完成登录和连接设置，可以关闭本窗口。\\n'
else
  fail
fi
`;
  await writeFile(path.join(output, launcher), launchText);
  if (windows) {
    await cp(fileURLToPath(new URL('../installers/windows-first-use.cmd', import.meta.url)), path.join(output, '首次使用.cmd'));
    await cp(fileURLToPath(new URL('../installers/WINDOWS_TEST_GUIDE.zh-CN.txt', import.meta.url)),
      path.join(output, 'Windows测试版使用说明.txt'));
    if (cli.target.endsWith('-gnu')) {
      const notices = fileURLToPath(new URL('../installers/runtime-licenses/', import.meta.url));
      await mkdir(path.join(output, 'licenses'));
      for (const notice of ['GCC-RUNTIME-EXCEPTION-3.1.txt', 'GPL-3.0.txt'])
        await cp(path.join(notices, notice), path.join(output, 'licenses', notice));
    }
    await writeFile(path.join(output, '连接 Codex.cmd'), '@echo off\r\nchcp 65001 >nul\r\n"%~dp0onecrew_lite.exe" plugin install\r\nif errorlevel 1 (\r\n  echo 连接未完成。请确认已安装 Codex，将上方报错截图发给提供安装包的人。\r\n  pause\r\n  exit /b 1\r\n)\r\necho 插件已安装。请完全退出并重新打开 Codex，新建对话输入“使用 onecrew_lite”。\r\npause\r\n');
  }
  if (!windows) {
    await chmod(path.join(output, launcher), 0o755);
    await chmod(path.join(output, name), 0o755);
    await chmod(path.join(libraryOutput, name), 0o755);
    const connect = path.join(output, '连接 Codex.command');
    await writeFile(connect, `#!/bin/sh
set -eu
WORKSPACE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
INSTALLED_ONECREW="$HOME/.local/bin/onecrew_lite"
if [ ! -f "$INSTALLED_ONECREW" ]; then printf '请先双击“安装并打开 onecrew_lite.command”。\\n' >&2; exit 1; fi
printf '正在为当前 Mac 用户安装 onecrew_lite 的 Codex 插件，请稍等。\\n'
if "$INSTALLED_ONECREW" plugin install >/dev/null; then
  printf '插件已安装并启用。请完全退出并重新打开 Codex，再新建对话输入“使用 onecrew_lite”。\\n'
  printf '可以关闭本窗口。工作台中的连接状态会在重新检测后更新。\\n'
else
  printf '\\n连接未完成。请确认 Codex 已安装；将上方报错截图发给提供安装包的人。\\n' >&2
  if [ -t 0 ]; then printf '按回车关闭窗口。'; read -r reply || :; fi
  exit 1
fi
`);
    await chmod(connect, 0o755);
    const firstInstall = path.join(output, '首次安装 onecrew_lite.command');
    await writeFile(firstInstall, `#!/bin/sh
set -eu
WORKSPACE_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
case "\${1-}" in ''|--no-browser) ;; *) printf '不支持此启动选项。\\n' >&2; exit 1 ;; esac
printf '首次安装：准备工作台、动态表格和 Codex 状态卡。\\n'
# Each existing owner handles installation and startup; no second install path.
if [ "\${1-}" = '--no-browser' ]; then
  /bin/sh "$WORKSPACE_DIR/安装并打开 onecrew_lite.command" --no-browser || exit 1
else
  /bin/sh "$WORKSPACE_DIR/安装并打开 onecrew_lite.command" || exit 1
fi
printf '正在自动安装并核验搜索组件，首次安装需要联网，请稍等。\\n'
if ! "$HOME/.local/bin/onecrew_lite" opencli repair --json; then
  printf '\\n搜索组件安装未完成。请检查网络后重试；也可在已打开的工作台点击“一键安装／修复连接”。\\n' >&2
  if [ -t 0 ]; then printf '按回车关闭窗口。'; read -r reply || :; fi
  exit 1
fi
/bin/sh "$WORKSPACE_DIR/连接 Codex.command" || exit 1
printf '\\n下一步：完全退出并重新打开 Codex，新建对话输入“使用 onecrew_lite”。\\n'
printf '看到聊天状态卡、工作台和动态表格三个界面后，在工作台完成设置和搜索。\\n'
printf '状态卡由 Codex 对话生成；安装窗口本身不能生成卡片。\\n'
`);
    await chmod(firstInstall, 0o755);
  }
  const instructions = `onecrew_lite 工作台 ${cli.version}

开始使用
1. 解压 ZIP，把整个文件夹放到方便找到的位置。保留文件夹里的所有内容。
2. ${windows ? `双击「${launcher}」。` : '先安装并登录 Codex，再双击「首次安装 onecrew_lite.command」。此入口会安装两个程序、打开工作台和动态表格，并为当前用户安装、启用 onecrew_lite 的 Codex 插件。'}${windows ? '' : '程序会安装到当前用户的 ~/.local/bin，已有不同版本会通过程序自己的更新流程替换；计划与账号数据保留。'}无需安装 Node.js、输入命令或填写网址。
3. ${windows ? '' : '完全退出并重新打开 Codex，新建对话输入“使用 onecrew_lite”。Codex 会生成只读状态卡，并打开工作台和动态表格。请确认这三个界面都在；目标和操作在工作台完成。'}首次使用，在两个页面分别登录你的 onecrew_lite 账号。
4. ${windows ? '首次搜索前，在工作台「连接登录」点击「一键安装／修复连接」，联网准备 OpenCLI 和运行环境。' : '首次安装会自动准备兼容的 OpenCLI 和运行环境，需联网。旧命令不会再被 onecrew_lite 使用，已有登录和计划保留。安装失败时，在工作台「连接登录」点击「一键安装／修复连接」。'}然后按提示为搜索用的 Chrome 身份授权扩展、选择身份并登录平台账号。
5. 点击「搜索」，描述商品和需求，确认计划后开始搜索。新结果会同步到动态表格；点击「打开搜索结果表格」查看当前计划的结果。

${windows ? '' : `三个界面的作用
工作台：连接账号、创建计划、确认条件、开始或暂停搜索。
动态表格：查看实时同步的线索、合格结果和证据。尚未创建计划时显示表格列表。
Codex 状态卡：只读显示当前阶段、进度和异常。安装程序不能直接在聊天里生成卡片；需要 Codex 加载插件后，在对话里输入“使用 onecrew_lite”。
如果只看到两个网页，请确认已重启 Codex，并在对话中输入“使用 onecrew_lite”；如果仍缺界面，输入“检查 onecrew_lite 启动，补齐状态卡、工作台和动态表格”，并将异常截图发给提供安装包的人。

单独修复 Codex 连接
已安装程序的用户可双击「连接 Codex.command」，安装成功后完全退出并重新打开 Codex，再输入“使用 onecrew_lite”。连接失败会保留报错，不代表三个界面已就绪。
如果使用网页的一句话生成计划，仍需本机 Codex 模型登录可用；账号或连接缺失时，请按工作台提示处理。

` }日常使用
再次双击「${launcher}」即可重新打开两个页面。启动不会自动开始搜索或增加预算。
${windows ? '' : '使用 Codex 时，在每个新对话输入“使用 onecrew_lite”，让 Codex 生成本对话的状态卡并补齐两个页面。\n'}
没有搜索结果时，动态表格页面显示表格列表。搜索命中和待核实线索不计入合格人数。
计划与表格保存在当前系统用户的数据目录。移动完整程序文件夹不会删除这些数据；首次在另一台电脑使用需要重新登录和配置连接。

安装包信息
目标：${cli.target === 'aarch64-apple-darwin' ? 'Apple 芯片 Mac（M 系列），不适用于 Intel Mac' : cli.target}。
${windows ? '' : '当前随包的动态表格程序要求 macOS 13.5 或更高版本。\n'}工作台：${cli.version}；动态表格：${library.version}。
${windows ? '这是供同事测试的内测包，尚未经过代码签名，首次打开可能被系统拦截。' : '这是供同事测试的内测包，未经过 Apple 开发者签名和公证，首次打开可能被 macOS 拦截。'}请将拦截提示或启动报错截图发给提供安装包的人。
本包仅含程序和说明，不含开发者的登录信息、平台账号、计划或搜索结果。
`;
  await writeFile(path.join(output, 'README.md'), instructions);
  await writeFile(path.join(output, '使用说明.txt'), instructions);
  if (!windows) {
    await writeFile(path.join(output, '交给Codex安装.txt'), `把解压后的整个文件夹拖进 Codex，再发送下面这段话：

请帮我安装这个文件夹中的 onecrew_lite ${cli.version} 测试版。我不懂技术，请完成你能自动完成的步骤，再清楚告诉我需要自己点击什么。
先阅读使用说明，核对 Mac 是 Apple 芯片、macOS 13.5 或以上，并按 manifest.json 校验包内文件。使用包内“首次安装 onecrew_lite.command”完成安装；已有 onecrew_lite 时使用其自带更新流程，保留账号、计划和表格数据。
检查工作台、动态表格服务及 Codex 插件，分别报告实际结果。需要完全退出并重新打开 Codex 时，明确提示我重启后输入“使用 onecrew_lite”；不要把插件安装成功说成聊天状态卡已经打开。
Chrome 扩展授权、选择浏览器身份和平台登录需要我操作时，请逐步提示并核对连接状态。任何步骤失败都保留具体报错，不要继续宣称安装完成。
这是未公证测试版。若 macOS 拦截，说明如何在系统设置中仅允许这个应用；不要关闭系统整体安全保护。
`);
  }
  const files = [];
  async function list(dir, relative = '') {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const item = path.posix.join(relative, entry.name);
      if (entry.isDirectory()) await list(path.join(dir, entry.name), item);
      else if (entry.isFile()) files.push({ path: item, ...await digest(path.join(dir, entry.name)) });
      else throw new Error('Workspace artifacts must not contain symlinks');
    }
  }
  await list(output);
  files.sort((a, b) => a.path.localeCompare(b.path));
  const manifest = { schema_version: 1, product: 'onecrew_lite-workspace', version: cli.version,
    target: cli.target, library_version: library.version, files };
  await writeFile(path.join(output, 'manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: {
    'cli-dir': { type: 'string' }, 'library-dir': { type: 'string' }, output: { type: 'string' },
  } });
  if (!values['cli-dir'] || !values['library-dir'] || !values.output)
    throw new Error('Usage: node package-workspace.mjs --cli-dir DIR --library-dir DIR --output NEW_DIR');
  await packageWorkspace(path.resolve(values['cli-dir']), path.resolve(values['library-dir']), path.resolve(values.output));
  console.log(`Workspace package: ${path.resolve(values.output)}`);
}
