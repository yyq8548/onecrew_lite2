// Customer-facing wrapper around the verified workspace installer.
import { packageWorkspace } from './package-workspace.mjs';
import { mkdir, writeFile, readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export async function packageMacTrial(cliDir, libraryDir, output) {
  const cli = JSON.parse(await readFile(path.join(cliDir, 'manifest.json'), 'utf8'));
  if (cli.target !== 'aarch64-apple-darwin') throw new Error('Apple Silicon artifact required');
  await mkdir(output);
  await packageWorkspace(cliDir, libraryDir, path.join(output, '程序文件'));
  const header = `#!/bin/sh
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
if [ "$(/usr/bin/uname -s)" != Darwin ]; then echo '此包只支持 Apple 芯片 Mac。'; exit 1; fi
if [ "$(/usr/bin/uname -m)" != arm64 ] && [ "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null || :)" != 1 ]; then
  echo '此包不支持 Intel Mac，请联系提供安装包的人。'; exit 1
fi
OS_VERSION="$(/usr/bin/sw_vers -productVersion)"
MAJOR="$(printf '%s' "$OS_VERSION" | /usr/bin/cut -d. -f1)"
MINOR="$(printf '%s' "$OS_VERSION" | /usr/bin/cut -d. -f2)"
if [ "$MAJOR" -lt 13 ] || { [ "$MAJOR" -eq 13 ] && [ "$MINOR" -lt 5 ]; }; then
  echo '需要 macOS 13.5 或更新版本。'; exit 1
fi
`;
  await writeFile(path.join(output, '安装或更新.command'), header + `
printf '旧 OneCrew 无需卸载，程序和数据均保留。已有 onecrew_lite 将更新。\\n'
printf '请先暂停正在运行的 Lite 搜索，并安装、登录 Codex 和 Chrome。\\n'
/bin/sh "$ROOT/程序文件/install-core.sh"
"$HOME/.local/bin/onecrew_lite" opencli repair --json
/bin/sh "$ROOT/程序文件/connect-codex.sh"
`, { mode: 0o755 });
  await writeFile(path.join(output, '打开 onecrew_lite.command'), header + `
/bin/sh "$ROOT/程序文件/open-onecrew_lite.command" "$@"
`, { mode: 0o755 });
  await writeFile(path.join(output, '先看我.txt'), `onecrew_lite ${cli.version} · 同事测试包 · Apple 芯片 Mac

首次使用或更新，都是同一操作
1. 解压整个 ZIP，保留完整文件夹，不要只拖出一个文件。
2. 先暂停旧 onecrew_lite 正在运行的搜索；安装并登录 Codex，安装 Chrome。保持联网。
3. 双击「安装或更新.command」，等待安装完成。黑色文字窗口正常，不用输入技术命令。
4. 完全退出并重新打开 Codex，新建聊天输入：使用 onecrew_lite。
5. 在工作台与动态表格分别登录你自己的账号。按工作台提示安装 Chrome 扩展、选浏览器身份、登录需要搜索的平台。

以后使用：双击「打开 onecrew_lite.command」，或在 Codex 聊天中输入「使用 onecrew_lite」。
安装不会自动开始搜索或发送邮件、私信。

已经装过旧版本怎么办？
• 旧名字 OneCrew / 高级版：不必卸载，新旧程序共存。旧账号数据、计划、结果均保留在原位置。
• 已经叫 onecrew_lite：此包更新同一程序，原 Lite 数据保留。新版插件需重启 Codex 后加载。
• 旧 OneCrew 的历史不会自动复制到 Lite，以免混入高级版数据。需要迁移时请联系提供安装包的人；不要自行删除任何数据目录。
• 启动时认准 onecrew_lite。旧 OneCrew 快捷方式仍会打开旧版。
• 安装失败后可以再次运行安装入口；请截图报错，不要先卸载程序。

系统提示不可信任怎么办？
这是未经过 Apple 开发者签名和公证的内部测试包。首次打开可能被系统拦截，程序无法保证消除该提示。
请将提示截图发给提供安装包的人，由其协助核对来源及系统允许打开步骤。不要关闭系统整体安全保护。

要求：Apple 芯片 M 系列、macOS 13.5 或更新、联网。无需安装 Node.js 或 Rust。
本包仅含程序、依赖许可和说明；不含开发者登录信息、平台账号或客户结果。
推荐表在搜索结束或暂停且结果整理完成后同步后台，搜索运行期间不持续上传。邮件页只记录通过 OneCrew 发出的邮件及其会话回复，不导入其他邮件。
`);
  const files = [];
  async function scan(dir, prefix = '') {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const relative = path.posix.join(prefix, entry.name);
      if (entry.isDirectory()) await scan(path.join(dir, entry.name), relative);
      else if (entry.isFile()) {
        const data = await readFile(path.join(dir, entry.name));
        files.push({ path: relative, size: data.length, sha256: createHash('sha256').update(data).digest('hex') });
      } else throw new Error('Unexpected package entry');
    }
  }
  await scan(output);
  await writeFile(path.join(output, 'package-manifest.json'), JSON.stringify({ product: 'onecrew_lite-mac-trial', version: cli.version, target: cli.target, files }, null, 2) + '\n');
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [cli, library, output] = process.argv.slice(2);
  if (!cli || !library || !output) throw new Error('Usage: node package-mac-trial.mjs CLI_DIR LIBRARY_DIR NEW_OUTPUT');
  await packageMacTrial(path.resolve(cli), path.resolve(library), path.resolve(output));
  console.log(`Trial package: ${path.resolve(output)}`);
}
