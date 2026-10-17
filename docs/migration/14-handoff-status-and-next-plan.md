# 新版交接、验证结果和下一步计划

更新日期：2026-10-17。交付仓库为 `yyq8548/onecrew_lite2`，分支 `codex/typescript-unified-gmail`。本文件说明当前开发候选，不代表已经完成客户发布。用户已恢复开发，当前在独立的 `onecrew_lite2` 工作目录继续 Mac ARM 验收，详见 [Mac ARM 本机记录](15-mac-arm-verification.md)。

## 目标与固定范围

最新本机版本为 candidate-24，已安装配套插件并恢复原专用浏览器身份，实际服务、HTTP 和 SEA 验收通过。当前网页控制工具的宿主授权不可用，两页重新打开已排队，尚未独立确认新版页面可见状态。详见 [Instagram 程序验收](27-instagram-visible-program.md)；既有连接修复见 [管理端与资料库连接修复](22-connection-stability.md)。Windows 本机验收仍待完成。

当前进度保存：Mac ARM candidate-24 已包含现有连接修复及 Instagram 正常网页读取。原有 12 平台已取得资格判断证据；Instagram 也已通过程序实际读取粉丝、简介、公开邮箱和原创正文，约数粉丝以范围参与初评和复评，语言判断直接复用正文。详见 [Instagram 程序验收](27-instagram-visible-program.md)。成功平台不重复外部验收。本次 GitHub 同步保存源码、测试和文档，不包含账号数据、浏览器配置及本地构建，也不执行客户发布。

将原 Lite 主业务服务迁入 TypeScript/Node.js，与现有 Library、工作台和 MCP 卡片组成统一 SEA 程序，保留历史数据及明确的账号归属。源码基线为 `efc3c6eb69507b9809bb30279d9af26156d075e3`，本轮上游改动固定为 `8d931ab08448bcded456c210ebc29ad9709a68a5`；来源清单见 [source-ledger.json](source-ledger.json)。

- 保留公开搜索、资料和内容读取、计划审核与调整、资格证据、评分、预算、暂停恢复、Creator、导入导出、历史记录、本地 Library 和 Codex 接入。
- 保留专用 Chrome、OpenCLI 扩展和真实身份绑定。工作台与 Library 默认使用 Codex 内置浏览器。
- 对外发送仅保留邮件页 GmailBulk。MCP 不发送邮件，X Chat、Instagram/Telegram 私信、关注、回复、Resend、单封用户邮件发送及对应审核入口均退役；历史回执继续可读。
- 依赖包、冻结历史合同和 Rust 对照测试不代表产品重新提供对应功能。必要的公开读取、认证和非发送审核共享逻辑保留，完整边界见 [08-current-scope.md](08-current-scope.md)。
- 不部署旧 Access 快照，不上传账号、密钥、浏览器身份或客户数据库，不接受远端表格协议代替用户同意，不发送真实邮件作验收。

## 当前代码结构

| 目录 | 当前职责 |
| --- | --- |
| `onecrew_lite-app/src` | CLI、统一服务、业务、账号上下文、兼容仓储、迁移、原生能力、客户安装、SEA 打包 |
| `onecrew_lite-contracts/src` | 共享的运行、状态、浏览器和接口校验合同 |
| `onecrew_lite-rust/apps/web` | 复用并修改的 React 工作台，实际连接统一 TypeScript 服务 |
| `onecrew_lite-rust/apps/mcp-ui` | MCP 状态和结果卡片 |
| `onecrew_lite-rust/integrations/opencli` | 保留平台的只读适配器及测试 |
| `onecrew_lite-library` | 嵌入式 PGlite/Teable、动态表格、作者视图、导出和网页 |
| `onecrew_lite-rust/crates` | 来源实现、冻结合同、数据库迁移及行为对照；不作为新版主程序启动入口 |
| `onecrew_lite-access` | 旧源码快照，只作兼容参考；当前生产源码及发布权限须另外核实 |

测试目录内受版本管理的 SQLite 和其他 fixture 为原实现生成的对照材料。运行中的账号数据库、OAuth 凭据、Chrome 配置、构建包和原始本机日志不提交。根 `.gitignore` 排除 `.scratch`、依赖、缓存及运行数据库，fixture 的例外只限测试目录。

## 本轮完成的改动

1. Instagram 使用关键词搜索发现账号，通过正常网页读取四项评分资料；Pinterest 使用 `search-pins` 读取原创内容入口，LinkedIn 从作者 Activity 区域读取粉丝，Facebook 保留近似粉丝区间并标记截断正文。YouTube 区分实际读取失败，保留原生作者与内容证据；其余注册平台使用现有受控只读入口。
2. 查询指导、独立查询审核和扩词审核；计划调整绑定源计划及修订版本，继承未明确修改的人数、预算、平台、语言、粉丝条件。精确资料可以复用，旧资格不能直接复用。
3. 额外条件按索引逐项保存、引用核验和审计。证据不足保持 pending，明确不符为 rejected，评分不能覆盖硬条件；公开邮箱是软偏好，缺少邮箱不自动淘汰。
4. 评分核对原生作者是否真正创作相关内容；联系人补证有明确边界。恢复绑定实际 stop code 和时间，不绕过登录及风控停止，不增加批准预算。
5. MCP 与网页使用当前合同，退役发送目录不可重新取得执行能力。GmailBulk 保留发件人能力、确认、幂等、去重、暂停和未知结果保护。Library 授权失效明确重新认证。
6. Windows 权限管道使用 UTF-8；同账号重新验证后指标缓存绑定新 epoch；SQLite 快照覆盖 WAL；Library 打包校验 unified 前端模式。
7. Windows 当前用户、最低权限、无自动触发器的按需任务托管服务，避免随 Codex 父 Job 退出；任务和配置绑定本数据目录、文件身份及哈希。
8. 桌面宿主重定向缓存通过经过核验的完整性标记确定物理根目录，再核验全部资源；Windows 原生 API 查询实际程序路径，支持超过 260 字符的中文路径。
9. 服务程序按 SHA-256 保存在私有数据目录，原生安装锁串行化并发；原安装文件移走不影响服务。新旧构建混用明确拒绝，明确替换后保留端口和数据。
10. 新安装由操作系统分配可用 loopback 端口再保存，已有 profile 及经验证的升级端口不变；正式绑定失败不切换端口，不修改 Windows 排除端口设置。

客户安装、路径、升级和清理要求见 [13-customer-installation-paths.md](13-customer-installation-paths.md)。

## 验证结果

| 检查 | 已取得的证据与边界 |
| --- | --- |
| 上游来源完整性 | 固定提交、177 个变更路径，`git fsck --full --no-dangling` 通过；逐项迁入而非覆盖现有修复 |
| Discovery | 完整 129 项通过，含真实 SQLite、修订 CAS、资格 ledger、恢复与账号 epoch |
| 工作台 | 完整 824 项通过 |
| 只读适配器 | 完整 348 项通过，Instagram 使用上游脱敏真实响应 |
| 原生权限与迁移专项 | 中文权限 15 项、Rust SQLite 快照 18 项、激活合同 12 项通过；均为各自专项结果，不与完整套件重复相加 |
| 客户安装 | candidate-51 的 8 项真实专项通过；candidate-52 长中文安装路径及实际旧版替换通过，端口与运行时 5 项、实际 OpenCLI 5 项通过 |
| candidate-52 主程序完整套件 | 545 项通过，0 失败、0 跳过，退出码 0，约 21 分钟；包含实际 SEA、旧版升级、Windows 原生任务、真实数据库和实际安装 OpenCLI |
| 类型检查 | candidate-52 通过 |
| 实际搜索与表格 | 已批准的英文 YouTube Playwright 计划实际读取 2/8 次，合格 1、待核实 0、读取错误 0；28 条来源、1 作者分组、27 未绑定，Brandon Melville 约 6,910 粉丝；只有原生频道联系入口，没有邮箱 |
| Mac ARM 美妆搜索 | 用户在工作台批准 YouTube 10 人计划；实际读取 23 次后返回 `target_reached`，合格 10、待核实 1、拒绝 1；对应 Library 实际显示 10 个作者分组和 4 个公开邮箱，详见本机记录 |
| Mac ARM candidate-17 平台补证 | 构建、实际 SEA 和严格 codesign 验证通过；12 平台取得资格判断证据。LinkedIn 实际粉丝 734、Pinterest 样本粉丝 12，均可按最低 2000 条件排除；Facebook 约 289K 粉丝保留 288000–290000 区间。Instagram 最新重试 HTTP 429 后停止，详见 20、21 号记录 |
| GitHub 同步前本地检查 | 主程序与工作台 TypeScript 类型检查通过；会话、资格、执行失败、恢复检查点、OpenCLI 合同与进程、许可相关测试首轮 19 项通过，许可真实包检查因未指定程序跳过 1 项，随后指定 candidate-17 重跑许可文件全部通过；工作台搜索访问测试 2 项通过，`git diff --check` 通过。未重复访问外部平台 |
| Mac ARM Codex 接入 | 真实 CLI 隔离安装、插件升级及恢复、打包程序同目录服务升级与缓存插件 MCP 共 3 项通过；状态卡资源读取通过，当前宿主实际渲染待验收 |
| 重启与正式插件 | candidate-52 已官方安装并完整重启 Codex；正式 live MCP 程序哈希匹配，Access signed_in、数据 ready；工作台和当前表格恢复一致、同步新增 0 |
| Gmail | 原 Gmail 发件人及空收件人门禁已实际回读；没有创建发送批次，没有执行真实发送验收 |
| 状态卡 | 正式 MCP 数据与资源合同已验证；宿主 MCP Apps 未提供可检查的展开标签，视觉验收未完成 |
| Mac ARM / Intel | Mac ARM 已完成配套构建、真实 SEA、原生网络及独立 Chrome 专项，详见本机记录；完整安装、升级及宿主联合验收待完成。Mac Intel 原生验收未完成 |

详细包哈希、实际日志位置和验收限制见 [11-windows-verification.md](11-windows-verification.md)。原始本机日志和带账号的截图保留在开发环境，不随源码上传；接手者须重新运行验证，不能依据旧 PID、端口在线或本文结果代替本机验收。

机器可读的最终结果见 [verification-results-20261016.json](verification-results-20261016.json)。当前包构建于本次 Git 提交之前，manifest 的 checkoutCommit 为原基线、dirty 为 true；已核对实际业务源码指纹和锁文件与运行包一致。后续 clean checkout 重新构建应重新记录包哈希，不能复用本文的二进制哈希作为新包证据。

当前 Windows candidate-52：程序 SHA-256 `eb1b201cf9b525e91e325c2b7618c122190ddfa2eab767bea5ad8f1d911fb387`；payload `917fe18315f83a853dc270b39b1d81dfbc375e2159eee71e43091008a40c39c4`；源码 `581d8b75f2d4e1bfbeabdbc449fd63baa6b494e34e415cf906e79f43bfa17a44`。官方插件 `0.7.1+codex.87b56e411320e1199f4ab5da890d757a81b12b4d7cb2ab473b42e7d65a198d08`。构建保持 `productionReady: false`。

## 新电脑构建与验证

固定 Node.js 26.7.0、pnpm 10.32.1，使用根锁文件。Windows 需要匹配的 C++ 原生构建工具。Rust 仅用于重新生成来源对照及旧版本验收；其工具链按原项目固定配置准备。不要复制开发电脑的账号数据或绝对路径。

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

构建独立 Windows 候选，输出目录必须尚不存在；不要覆盖已验收包：

```powershell
New-Item -ItemType Directory -Force .artifacts | Out-Null
$nodePath = (Get-Command node).Source
node onecrew_lite-app/src/packaging/build-candidate.ts --output .artifacts/windows-candidate --product-version 0.7.1 --target win32-x64 --node $nodePath --web-assets onecrew_lite-rust/apps/web/dist --library-web-assets onecrew_lite-library/web/dist
```

普通测试入口：

```powershell
node --test --test-concurrency=2 onecrew_lite-app/tests/*.test.ts
pnpm --dir onecrew_lite-rust/apps/web test
$adapterTests = Get-ChildItem onecrew_lite-rust/integrations/opencli -Recurse -File -Filter '*.test.mjs' | ForEach-Object FullName
node --test @adapterTests
```

适配器命令同时覆盖根目录和平台子目录。主程序的真实验收另外显式配置下列环境，未提供的跳过项必须列出：

| 环境变量 | 要求 |
| --- | --- |
| `ONECREW_SEA_CANDIDATE_EXECUTABLE` | 本机刚构建、核对哈希的真实 SEA 程序 |
| `ONECREW_PREVIOUS_SEA_EXECUTABLE` | 保留的真实旧版本，供拒绝混用与明确替换测试使用 |
| `ONECREW_CREATOR_ORACLE` | 原 Rust current 构造器生成的真实 schema90 对照数据库；使用隔离副本 |
| `ONECREW_LEGACY_CLI_TEST_EXECUTABLE` | 原 Windows CLI，测试仅复制到隔离目录并控制该测试自己的任务 |
| `ONECREW_OPENCLI_TEST_INSTALLATION=1` | 强制通过产品解析器检查实际安装 OpenCLI 1.8.8，缺失应失败 |

实际平台、浏览器扩展和宿主联合验收不能用 fixture 代替。仅在用户批准的条件、目标和预算内运行；未获发送授权时，不执行 Gmail 发送。

重新生成 Rust schema 对照时，在项目根目录为本次运行创建尚未使用的输出目录；Rust 工具链为 1.98.0，原生编译依赖按其实际要求准备：

```powershell
New-Item -ItemType Directory .scratch/schema-oracle | Out-Null
$env:ONECREW_SCHEMA_ORACLE_DIRECTORY = (Resolve-Path .scratch/schema-oracle).Path
cargo test --manifest-path onecrew_lite-rust/Cargo.toml -p onecrew-creator-runtime export_schema90_node_compatibility_fixture -- --nocapture
$env:ONECREW_CREATOR_ORACLE = Join-Path $env:ONECREW_SCHEMA_ORACLE_DIRECTORY 'creator-schema90-oracle.sqlite'
```

构造器拒绝覆盖已有输出，生成真实 current schema 及历史迁移对照，不读取客户数据库。上一实际候选和原 Windows CLI 保留在开发环境，不随 Git 源码提交；新电脑需要另外取得并核对这些真实产物，才能完成相关升级验收。缺少时必须保留未完成，不能用替代内容消除跳过。

## 下一步计划与完成条件

顺序继承 [M00–M17 完整计划](03-detailed-migration-plan.md)，当前产品范围优先于原设计中已删除的功能。用户已恢复开发，按以下顺序继续；需要外部账号、独立设备或发布授权的项目，先完成可在本机独立验证的工作。

| 顺序 | 工作 | 完成条件 |
| --- | --- | --- |
| 1 | 完成状态卡视觉与联合验收 | 正式 Codex 卡片可直接检查；状态、工作台、当前表格同一账号和计划，断线、过期及恢复状态真实显示 |
| 2 | Instagram 网页读取 | 四项资料程序路径已验收并加入 candidate-24；私密账号跳过，未知资料保持未知；其他成功平台不重复验收 |
| 3 | Mac ARM 与 Intel 原生验收 | 两种真实环境分别完成原生依赖、Chrome、权限、SEA、安装、插件、Library、重启及升级；保留各自哈希和日志 |
| 4 | 完成发布依赖与来源核实 | 补齐当前 13 项许可全文，核实 Windows r3 原包来源、公司当前 Access 源码和发行合同；已补齐的三项见 [许可全文证据](16-license-evidence.md)，不得伪造授权文本或部署旧快照 |
| 5 | 冻结三平台客户安装候选 | 主程序、插件、Library 的来源、哈希、目标平台和接口一致；执行安装、中文路径、权限、并发、升级、数据恢复和必要失败用例 |
| 6 | 签名与发布 | 取得实际签名、Mac 公证和发布权限，核对官方下载合同；用户明确批准后再替换线上版本或迁移客户数据 |
| 7 | 升级后验收与旧组件退役 | 真实升级后回读三界面及历史数据；按已验证回退边界处理旧服务，保留仍在使用或必要回退的产物 |

Gmail 的真实发送验证须另外取得具体发件人、收件人和内容授权，并核对逐封结果及未知结果保护。Gmail API 接受不等于实际送达。

## 当前执行边界

当前授权继续开发和本机验证。平台搜索仍须具备明确目标、账号和预算，并完成既有计划确认。发布、客户数据迁移及真实邮件发送按各自授权执行。原 Rust 目录、已有插件和账号数据保持原归属；本轮在独立目录构建和验收。从未完成项继续，不重复迁移已完成部分。

开发期间只清理本任务确认不用的旧构建和缓存，保留当前、上一版及实际运行、验证和回退引用。源码、客户数据、浏览器身份和必要验证记录保持原归属。
