# 当前交接与构建

更新日期：2026-10-17。仓库 `yyq8548/onecrew_lite2`，默认开发分支 `codex/typescript-unified-gmail`。产品0.7.1为开发候选，最新 Mac ARM 本地候选37。最新行为见[搜索、评分与连接现状](current-platforms.md)。

## 范围与来源

当前主程序为 TypeScript/Node.js，包含本地服务、工作台、Library 与 Codex 插件。`onecrew_lite-rust/` 保留来源实现、工作台、只读适配器和真实行为对照，Rust 业务不是新版启动入口。`onecrew_lite-access/` 为旧快照。

保留公开搜索、证据、评分、预算、暂停恢复、Creator、导入导出、历史及动态表格。对外发送仅保留邮件页 GmailBulk，MCP 不发送邮件。公开转载、整理及实际相关标签可作为主题证据，原创要求仅按用户明确条件执行。

来源基线 `efc3c6eb69507b9809bb30279d9af26156d075e3`，批准迁入的上游 `8d931ab08448bcded456c210ebc29ad9709a68a5`；详见[source-ledger.json](source-ledger.json)。克隆后以实际 Git 提交为准，旧候选二进制哈希不能代替新构建证据。

## 新电脑构建

固定 Node.js **26.7.0**、pnpm **10.32.1** 与根锁文件。Windows 原生依赖需要 Visual Studio C++ Build Tools 和 SDK。Rust 仅在重新生成来源对照或构建旧程序时需要，按项目工具链配置准备。

```powershell
git clone --branch codex/typescript-unified-gmail https://github.com/yyq8548/onecrew_lite2.git
cd onecrew_lite2
pnpm install --frozen-lockfile
pnpm build:tables
pnpm --dir onecrew_lite-rust/apps/mcp-ui check
pnpm --dir onecrew_lite-rust/apps/mcp-ui build
pnpm --dir onecrew_lite-rust/apps/web build
$env:VITE_ONECREW_UNIFIED='1'
pnpm --dir onecrew_lite-library build:web
pnpm typecheck
```

Windows 独立候选的输出目录必须尚不存在：

```powershell
New-Item -ItemType Directory -Force .artifacts | Out-Null
$nodePath = (Get-Command node).Source
node onecrew_lite-app/src/packaging/build-candidate.ts --output .artifacts/windows-candidate --product-version 0.7.1 --target win32-x64 --node $nodePath --web-assets onecrew_lite-rust/apps/web/dist --library-web-assets onecrew_lite-library/web/dist
```

Mac 使用对应架构 Node，将目标改为 `darwin-arm64` 或 `darwin-x64`，输出到新的独立目录。主程序、工作台、MCP UI 和 Library 必须配套构建。不得复制开发电脑的账号、密钥、Chrome 身份和绝对路径作为默认配置。

## 测试入口

```powershell
node --test --test-concurrency=2 onecrew_lite-app/tests/*.test.ts
pnpm --dir onecrew_lite-rust/apps/web test
$adapterTests = Get-ChildItem onecrew_lite-rust/integrations/opencli -Recurse -File -Filter '*.test.mjs' | ForEach-Object FullName
node --test @adapterTests
```

真实原生验收按测试合同提供以下材料，缺少材料产生的跳过不得算通过：

| 环境变量 | 要求 |
| --- | --- |
| `ONECREW_SEA_CANDIDATE_EXECUTABLE` | 本机刚构建并核对哈希的真实 SEA |
| `ONECREW_PREVIOUS_SEA_EXECUTABLE` | 真实旧版本，用于明确替换和拒绝混用验证 |
| `ONECREW_CREATOR_ORACLE` | 原 Rust schema90 对照数据库的隔离副本 |
| `ONECREW_LEGACY_CLI_TEST_EXECUTABLE` | 原 Windows CLI，仅控制测试自己的任务 |
| `ONECREW_OPENCLI_TEST_INSTALLATION=1` | 检查实际 OpenCLI 1.8.8 安装 |

重新生成 Rust 对照时使用未占用目录：

```powershell
New-Item -ItemType Directory .scratch/schema-oracle | Out-Null
$env:ONECREW_SCHEMA_ORACLE_DIRECTORY = (Resolve-Path .scratch/schema-oracle).Path
cargo test --manifest-path onecrew_lite-rust/Cargo.toml -p onecrew-creator-runtime export_schema90_node_compatibility_fixture -- --nocapture
$env:ONECREW_CREATOR_ORACLE = Join-Path $env:ONECREW_SCHEMA_ORACLE_DIRECTORY 'creator-schema90-oracle.sqlite'
```

## 当前验证与未完成事项

- Mac ARM 候选37完成配套构建、严格签名、相关测试与真实 Instagram 读取和保存；工作台、专用浏览器及当前动态表格已恢复连接。服务重启使旧页面会话失效，需要新授权链接。
- Windows 历史验证见[Windows 记录](11-windows-verification.md)；当前提交须重新构建并完成 Windows 本机验收。Mac Intel 也需要独立原生验收。
- Gmail 已读取实际发件人并验证空收件人门禁，尚未进行真实发送及送达验收。发送测试需另有具体收件人和内容授权。
- 仍需完成客户安装、升级、宿主状态卡联合验收、许可全文、正式签名、公证和发布来源核实。构建保持 `productionReady: false`。

## 交付与清理

同步源码、测试、锁文件、构建脚本及必要文档。账号数据、浏览器配置、本地日志、依赖和安装包不进入 Git。测试目录中的真实 fixture 按各自来源保留。

保留当前、上一版和仍被运行或验收引用的构建；只清理确认不用的产物。跨电脑交接同时携带[合并与部署流程](../release/合并与部署流程.md)，按[客户安装约束](13-customer-installation-paths.md)核对归属。GitHub 推送不会自动合并公司主分支、发布客户端或切换生产下载。
