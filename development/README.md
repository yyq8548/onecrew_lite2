# TypeScript 独立开发与配套验收

Node.js 固定为 26.7.0，pnpm 固定为 10.32.1。使用仓库根目录的 pnpm-lock.yaml；Windows 构建使用匹配版本的原生模块预构建文件。业务运行、契约生成和测试不调用 Rust 编译器或旧版本程序。

## 目录归属

- onecrew_lite-app：CLI、本地服务、业务、数据升级、原生系统接口和打包。
- onecrew_lite-web：工作台、邮件页和用户日常 Chrome 的身份授权页面。
- onecrew_lite-mcp-ui：MCP 状态卡和操作卡，构建输出为自身 dist。
- onecrew_lite-opencli：审核过的只读适配器和适配器测试。
- onecrew_lite-contracts/schemas：当前项目维护的网页和 MCP schema；development/schema-codegen 从这些 schema 检查和生成客户端类型。
- onecrew_lite-library：Library 服务和表格网页。

历史数据库测试使用仓库内带 SHA-256 校验的固定数据库，验证数据升级、结构、约束和事务行为，不启动旧程序。历史来源说明仅记录证据，不构成构建输入。

主程序回归最多并行执行 4 个测试文件，避免数十个原生服务、数据库和权限检查同时竞争资源。服务启动超时、断言和实际检查内容保持原值；不跳过检查。

## 构建

在根目录依次运行 pnpm install --frozen-lockfile、pnpm build:tables、pnpm typecheck、pnpm check:contracts、pnpm build:ui。主程序的类型检查读取 Library 构建生成的类型声明，因此首次安装必须先构建表格组件。

Library 的统一安装包网页使用 pnpm --filter onecrew_lite_tables build:web:unified，根目录 build:tables 已包含这一步。该命令明确设置统一运行模式和 /library/ 资源路径，无须依赖当前终端的环境变量。build:web 保留给 Library 单独运行时使用。

使用 pnpm build:candidate，并明确提供 --output、--product-version、--target、--node、--web-assets 和 --library-web-assets。后两项分别指向 onecrew_lite-web/dist 和 onecrew_lite-library/web/dist。支持 win32-x64、darwin-arm64、darwin-x64；每个目标使用配套的 Node 26.7.0 程序。输出目录必须不存在。

Mac 可以直接构建 Windows x64：使用校验后的官方 `node-v26.7.0-win-x64/node.exe`，安装锁文件版本的 `@wreq-js/binding-win32-x64-msvc`，并确认 `fs-native-extensions` 和 `@primno/dpapi` 的 Windows 预构建文件存在。用 `node development/prepare-windows-runtime.mjs --seven-zip <7zz绝对路径> --output <新的运行库目录>` 下载、核对并提取 Microsoft VC++ 运行库；已有完整下载可通过 `--redist` 指定。构建 Windows 时额外传入 `--windows-runtime <运行库目录>`。运行库随经过校验的内嵌资源加载，用户无须安装编译工具。

每次更新使用独立语义版本号，安装包、安装目录和提示词使用该版本，不增加 R1、R2 等修订后缀。Mac 客户包使用 `development/package-macos-customer.mts`，明确提供 `--candidate-zip`、`--source-commit`、`--output` 和 `--product-version`；文件名为 `OneCrew-Lite-<版本>-Mac-arm64.zip`。

## npm 分发

Windows 后续只用 npm 分发。`@onecrew/lite` 是命令入口，按精确依赖版本选择 `@onecrew/lite-darwin-arm64` 或 `@onecrew/lite-win32-x64`；两个平台可以独立更新。Windows 直接从已构建候选目录生成平台 `.tgz`，Mac 使用已验证 ZIP 或复用已发布的 npm 平台包。

```sh
pnpm build:npm --catalog <本次发布输入JSON绝对路径> --windows-candidate <Windows候选目录> --output <新的输出目录>
```

发布输入使用 `development/npm-release-inputs.json` 的结构，记录主包版本和各平台版本。新 Windows 候选记录 `executableBytes`、`executableSha256`、`runtimeManifestSha256`、`buildCheckout` 和 `sourceDirty`。复用平台填写 `version` 与 `reusePublished: true`，构建会核对该版本确已发布。新 Mac 包另传 `--mac-zip`，核对完整 ZIP 摘要及全部文件。Windows 核对程序、资源清单、内嵌 VC++ DLL 和许可证文件，保留实际构建来源。

安装没有生命周期脚本或额外下载。npm 命令为 `onecrew-lite`，`--print-binary-path` 返回真实程序路径；其余参数透传，已有数据目录不变。npm 入口需要 Node.js 20 或更新版本，应用运行时内置于原生程序。Windows 使用 `npm.cmd` 和 `onecrew-lite.cmd`，每条原生命令检查退出码。

先用 npm 官方登录流程授权发布账号；按 `release.json` 先发布本次更新的平台 `.tgz`，再发布主包，验证指定版本安装、服务和 MCP 后设置 latest。复用的平台不重新发布。首次公开发布使用 `--access public`，同名同版本不可覆盖。Windows 的 npm 原生安装验收单独记录；Mac 上生成和校验 PE 不等于 Windows 运行通过。源码没有声明新的开源许可，因此 npm 元数据保留 UNLICENSED，随包分发第三方许可证。

安装到独立目录后，使用 `node development/npm-smoke.mjs --node-modules <安装目录/node_modules> --output <新的验收目录>` 检查 npm 入口、原生主程序 SHA-256、服务启动与停止、两个网页、live MCP 和网页会话。验收创建独立数据目录，不使用客户数据，不启动搜索。

## 安装与运行完整性

主程序、内嵌 Codex 插件和 Library 配套构建。组件版本号可以独立演进，但目标架构、二进制哈希、资源哈希和接口能力必须匹配。保留已有账号数据、主密钥、数据目录、端口和用户已授权的 Chrome 身份。不创建独立浏览器。

安装后核对真实运行的可执行文件与哈希。宿主重启后通过 live MCP 回读，确认状态卡、工作台与当前计划的动态表格同时可用、连接新鲜、业务结果一致。进程存在、端口在线或返回 URL 都不能替代界面验收。

复用已打开的工作台或 Library 标签页时，重新加载页面，并核对 DOM 中实际加载的脚本路径与当前安装包 manifest 的网页资源一致。

Windows 必须执行真实原生安装、升级、退出、数据保留、日常 Chrome 身份授权和页面验证。Mac ARM 与 Mac Intel 分别记录，未执行的项目保持未完成。候选产物保留 productionReady: false，直到全部发布条件通过。

Windows 新版安装提示词见 `development/windows-npm-INSTALL.md`。通过 npm 安装主包和匹配的平台包，发现并复用原数据目录，再分别验证服务、配套插件或 MCP、浏览器与表格。`development/windows-customer-package` 与 `windows_installer` 保留已有 ZIP 安装资料，后续不再生成或更新 Windows ZIP。私有验收日志不提交仓库。

对外发送只保留邮件页 Gmail 批量发送，发送前必须获得用户的具体确认。MCP 不提供发送操作。验证期间不发送邮件，不接受新的远端同步协议。

## 清理

只清理本任务生成且已确认不再使用的旧文件。保留当前版、上一版以及仍在运行、验证或恢复所需的产物；删除前检查真实进程、注册任务、插件和引用。客户数据、浏览器身份、源码与必要验收证据不得混入构建缓存清理。
