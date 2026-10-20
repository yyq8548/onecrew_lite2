# OneCrew Lite 0.9.0 · Mac ARM64 · R3 安装指南

适用于用户授权的 Apple Silicon Mac 安装或升级。使用 Mac 原生终端；先执行 `uname -s`、`uname -m`，确认 Darwin 和 arm64。Intel Mac 停止；Rosetta 终端需切换原生 arm64 环境后继续。

本包使用 ad-hoc 签名，未进行 Apple 公证。若系统阻止运行或工具拒绝操作，报告具体步骤并停止，由用户处理；禁止移除 quarantine、关闭 Gatekeeper 或换入口绕过拒绝。程序是独立可执行文件，无须安装 Node.js、pnpm 或编译工具。

## 1. 下载前读取发布记录

执行下面的终端命令，要求 HTTP 200，完整阅读 JSON：

```sh
curl --fail --silent --show-error --proto '=https' --max-time 30 --write-out '\nHTTP_STATUS:%{http_code}\n' 'https://raw.githubusercontent.com/yyq8548/onecrew_lite2/refs/tags/mac-installer-0.9.0-r3/mac_installer/OneCrew-Lite-0.9.0-Mac-arm64-R3.zip.release.json'
```

确认 `product=onecrew_lite`、`version=0.9.0`、`revision=R3`、`target=darwin-arm64`。记录 `bytes`、`sha256`、`executableSha256`、`sourceCommit`。本指南与发布记录及 ZIP 必须来自同一标签。

将下载文件保存到用户选定的工作目录，禁止使用 `/tmp`。ZIP 固定地址：

```text
https://raw.githubusercontent.com/yyq8548/onecrew_lite2/refs/tags/mac-installer-0.9.0-r3/mac_installer/OneCrew-Lite-0.9.0-Mac-arm64-R3.zip
```

通过 `curl --fail --silent --show-error --proto '=https' --max-time 180 --output` 下载，要求 HTTP 200。检查 `stat -f '%z'` 返回字节数与发布记录的 `bytes` 完全一致，并用 `shasum -a 256` 核对 ZIP 哈希。下载失败或校验不一致立即停止，禁止解压或运行。

## 2. 解压并校验全部文件

使用 `/usr/bin/ditto -x -k` 解压到一个不存在的新目录，完整保留 `OneCrew-Lite` 文件夹。将下面的 `packageDir` 设置为解压后该文件夹的实际绝对路径；后续路径变量都使用实际绝对路径。

```sh
packageDir='/解压后的绝对路径/OneCrew-Lite'
```

进入该目录执行校验：

```sh
cd "$packageDir"
shasum -a 256 -c SHA256SUMS
```

要求全部文件显示 OK。用 macOS 自带 `plutil -p candidate.json` 阅读包身份，核对版本、架构及 `executableSha256`；用 `shasum -a 256 onecrew_lite` 核对主程序哈希。`candidate.json`、外部发布记录和实际文件必须一致。包内 `SHA256SUMS.json` 提供相同文件的结构化校验清单。

## 3. 确认首次安装或已有 Lite 数据

仅检查 OneCrew Lite 的已有进程、启动配置和 Codex 配套插件 `.mcp.json` 中的 `ONECREW_LITE_MCP_DATA_DIR`。不要读取或输出主密钥内容、Cookie、服务控制令牌或其他产品的账号数据。

已有服务的命令参数提供 `--data-dir`、`--vault`、`--master-key`；已有插件的同名环境变量提供其原数据目录。多个真实候选目录时列出路径让用户选择，不猜测。`installation-discover` 目前仅支持 Windows，不在 Mac 上调用。

确认从未安装 Lite 且没有已有数据时，使用当前 Mac 用户的默认数据目录 `~/Library/Application Support/OneCrewLite/TypeScript`，默认程序目录 `~/Applications/OneCrewLite/releases/0.9.0-R3`。将 `~` 展开成绝对路径。已有安装沿用原数据目录、vault、主密钥和端口，程序目录使用独立的版本目录。

将 `dataDir`、`vaultPath`、`masterKeyPath`、`installDir` 设置为核实后的实际路径。已有目录只允许检查并保留：`vault.sqlite` 和主密钥必须成对存在；历史主密钥可能叫 `masterkey` 或 `master.key`，有多个或缺失时停止。首次安装时二者必须同时不存在，由程序安全创建，禁止自行生成密钥或数据库。

新建数据目录时使用 `install -d -m 700 "$dataDir"`；已有目录不执行清空、重建或递归权限修改。备份、升级和数据准备由程序的既有流程负责。

## 4. 安装主程序并启动

已有数据时先运行已校验的新程序检查原服务：

```sh
"$packageDir/onecrew_lite" status --data-dir "$dataDir"
```

状态检查失败立即停止。若原服务运行，记录真实 PID、可执行文件路径、SHA-256 和端口。若运行中的主程序已为本发布记录对应哈希，复用该程序和服务，直接进入客户端配置；程序版本相同但哈希不同仍需升级。

需要替换不同构建且已确认原服务归属时，先停止原服务并确认返回 `state=stopped`：

```sh
"$packageDir/onecrew_lite" stop --data-dir "$dataDir"
```

使用独立版本目录安装全部包内容，保留旧程序及原数据：

```sh
mkdir -p "$installDir"
/usr/bin/ditto "$packageDir" "$installDir"
shasum -a 256 "$installDir/onecrew_lite"
```

再次核对已安装主程序哈希。然后启动：

```sh
"$installDir/onecrew_lite" start --data-dir "$dataDir" --vault "$vaultPath" --master-key "$masterKeyPath"
```

若升级前记录了端口，启动命令增加 `--legacy-port` 和该实际端口。首次安装使用程序分配的端口。要求返回 `state=running`，再执行 `status --data-dir "$dataDir"` 核对。读取必要日志定位失败；遇到数据升级或授权要求时按程序界面完成，禁止手动修改 SQLite 或换成空数据目录。

## 5. 配置 Codex 或其他 MCP 客户端

Codex 使用本包自动生成的 Lite 配套插件：

```sh
"$installDir/onecrew_lite" codex-package --data-dir "$dataDir"
"$installDir/onecrew_lite" connect-codex --data-dir "$dataDir" --refresh
"$installDir/onecrew_lite" codex-status --data-dir "$dataDir" --refresh
```

分别核对主程序与插件版本。预期插件版本从本机 `codex-package` 的 `version` 字段取得，格式为 `0.9.0+codex.<64位摘要>`；完整摘要绑定本机程序和数据目录，不能套用打包电脑的插件摘要。要求安装结果为 `state=installed`、`enabled=true`、`verification=fresh`，并与本机生成的预期版本一致。已有客户端的其他配置和插件保留。

找不到已安装 Codex CLI 时，定位官方 CLI，再通过 `--codex-executable` 指定其绝对路径。宿主需要重新加载或重启时，明确报告程序和插件已完成的步骤；用户重启后继续 live MCP 验证。保留同一 OneCrew Lite 连接，不安装 Pro 插件。

其他兼容 MCP 客户端使用同一主程序绝对路径，参数为 `mcp --stdio --data-dir <原数据目录的绝对路径>`。MCP 连接不替代前面的服务启动；客户端是否需要重载按其实际提示处理。

## 6. 实际验收与停止边界

调用 live `onecrew_status`，确认服务版本为 0.9.0、实际主程序哈希等于发布记录、原账号状态可读；需要用户登录时报告登录要求，不能当成连接故障。由用户完成账号登录、验证码和浏览器授权。

调用 `onecrew_web` 取得本次有效工作台及 Library 入口。如果用户要求查看且客户端具备浏览器工具，复用工作台和已有结果表，重新加载并读取实际内容；没有计划则查看 Library。只返回 URL 或排队打开不代表页面验收完成。用户明确只看状态时仅展示只读状态卡。

连接原来的日常 Chrome 和官方 OpenCLI 扩展，保留其既有网站登录。报告实际程序版本、哈希、生成的插件版本、服务状态、live MCP 账号状态，以及仍需的重启、登录或界面检查。安装验收不启动创作者搜索，不改变条件，不发送消息或邮件。版本记录及已知限制见 RELEASE-STATUS.md。
