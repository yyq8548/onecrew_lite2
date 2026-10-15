# Pro 架构与发布方式分析

分析日期：2026-10-15。固定来源：公司 `onecrew/onecrew` 仓库 `onecrew-pro` 分支提交 `6558828624368addba6b4583078e7928577da284`，应用版本 0.1.8。本文为结构、接口及主要数据流的源码分析；未执行 Pro 全量测试、三平台打包或供应商依赖逐行安全审计。公司 Pro 实现继续保留在公司仓库，本交接包不复制其实现。

## 1. 完整目录范围

| 目录 | 跟踪文件数 | 作用 |
| --- | ---: | --- |
| src | 217 | CLI、Hono 服务、数据库、MCP 和业务模块 |
| packages | 20 | 前后端共享契约及构建配置 |
| web | 142 | React、TypeScript、Vite 网页 |
| scripts | 25 | SEA 构建、原生依赖、上传、发布说明 |
| tests | 141 | 集成、浏览器、真实服务及打包验证入口 |
| vendor | 2221 | Teable 等供应商代码和必要元数据 |
| plugins | 说明文件 | 插件实现已迁至独立仓库 |

根目录还有依赖锁文件、构建配置、架构与功能文档。供应商文件数量不代表已逐行审核。固定提交共 2816 个 Git 跟踪文件。

| src/modules 模块 | 文件数 | 职责与迁移关注点 |
| --- | ---: | --- |
| access | 5 | 官网设备配对、身份校验、授权代理 |
| tenancy | 1 | 用户、tenant、space、base 隔离 |
| search / details / enrich | 2 / 2 / 7 | 搜索、详情、公开联系方式原子任务 |
| platforms | 65 | YouTube、X、Instagram、Facebook 请求、证据和分页 |
| platform-connections | 14 | 浏览器会话、API Key、加密凭据、账号租约、代理 |
| platform-tasks | 1 | 并发、取消、超时和内存结果缓存 |
| tables | 30 | 动态表格、语义字段、导入导出和容量限制 |
| table-sync | 15 | 同步授权、分片、摘要、暂存与恢复 |
| mail | 37 | Gmail、SMTP/IMAP、Resend、草稿、发送队列和历史 |
| leads | 8 | 线索、过滤、来源与邮件关联 |
| runtime-changes | 2 | 用户隔离的变更通知 |
| web-research | 13 | 网页搜索、正文提取、访问检查与独立 worker |

## 2. 语言、运行时和启动流程

主体 TypeScript，运行于 Node.js；Web 使用 React 19、Vite；服务使用 Hono；数据库使用 PGlite、Kysely 和 Teable。`package.json` 是版本和脚本入口。

`src/cli.ts` 管理 start/serve/stop/status/restart/mcp。`src/index.ts:31,45` 启动本机 HTTP 服务并绑定 loopback。`src/bootstrap.ts:15–55` 初始化运行时变更通知、数据库、AccessService、租户、表格队列及同步。`src/app.ts:75–170` 注入各业务服务。

一个 profile 对应一个数据库和服务器，profile 不等于 tenant。`src/runtime.ts:13,39–128` 管理锁文件、PID、端口、实例 ID 和 control token，验证返回地址和数据目录。数据目录为 OneCrewConnect，Lite 必须使用自己的命名，不能照搬目录、环境变量、端口和插件 ID。

## 3. 统一身份的实际做法

所有业务依赖同一个 AccessService。身份不可用返回 503、未登录返回 401；`src/modules/tenancy/service.ts:26–39` 校验请求携带的账号 ID，旧页面跨账号操作返回 409。

`src/modules/access/service.ts:102–138` 在调用前后核对 epoch、userId、sourceId、credentialId 和取消信号。登出取消该账号任务并关闭事件流（`src/app.ts:245–252`）；网页切换账号取消查询并清空缓存（`web/src/layout/WorkspaceRuntime.tsx:28–42`）。

这是 Lite 应借鉴的统一身份边界。它没有提供 Lite 两份旧凭据的迁移器。两侧账号不一致时必须中止迁移并要求选择账号，不能静默合并或选最后登录的一方。

社交凭据使用 AES-256-GCM 与本地 0600 密钥文件，AAD 绑定身份；已有凭据而密钥缺失会报错（`platform-connections/credential-seal.ts:19–85`）。这与系统 Keychain/DPAPI 是不同层次。

## 4. 搜索模式与 Lite 的区别

`src/modules/search/service.ts:21–70` 实现 create/continue/status/results 原子操作。`src/mcp/server.ts:22–36` 要求宿主模型筛选、评分、保存表格，不自动补人数、不创建空表。对一方 src 和 contracts 的模型关键词搜索未发现 Lite 式内置模型规划及自动补词循环。

PlatformTaskRuntime 使用 active Map 和完成结果 LRU。默认 4 并发、200 项、总 32 MiB、单项 4 MiB、15 分钟 TTL、240 秒超时（`platform-tasks/runtime.ts:20–26,86–110`）。这些是内存任务与缓存，不能代替 Lite 持久搜索状态、后台继续、暂停恢复和重启恢复。

迁移后必须单独保留 Lite 的意图解析、完整产品实体、搜索目标、自动补词、去重和任务恢复逻辑。“苹果手机”回归属于意图层，不可仅靠更改一段宿主提示词交付。

## 5. 平台、邮件、表格与同步

平台 registry 为 YouTube 使用 API、X/Instagram/Facebook 使用浏览器会话（`platforms/index.ts:53–160`）。账号执行器按 tenant/platform/provider_account_id 隔离，维护租约、轮换、冷却、revision 和取消（`platform-connections/account-executor.ts:39–50,93–285`）。这些机制可作为设计参考；Lite 的 Telegram、X Chat 等现有能力必须另行迁移。

`src/database.ts:40–150` 初始化 PGlite，严格执行 migration，要求 PostgreSQL 兼容版本至少 16，并注册 Teable 的命令/查询、事务和导入服务。`vendor/teable/README.md:3–27` 固定上游 `5ef2238883cad7c3980084de9a9031135fb9734f`，只保留 15 个源码包，未包含 Teable 应用或 UI。

`tables/service.ts:77–219` 及同目录模块处理表结构、记录、语义字段、排序、导入导出。`table-sync/runtime.ts:23–56,132–192,258–295,495–606,644` 处理设备授权、同意、串行队列、dirty 扫描、分片、恢复与关闭。

邮件服务具备草稿、模板、发送队列、历史、附件和回复等完整状态。`mail/send-state.ts:31–104` 事务检查 tenant、连接状态、revision、发送能力与收件人来源，再将 queued 转为 submitting；`mail/task-runtime.ts:87–100,173–245` 串行执行与恢复。迁移不能把提交结果未知当作未发送并自动重试。

## 6. MCP 与插件

`src/mcp/server.ts:42–67` 注册各领域工具。`src/mcp/stdio.ts:15–17,35–45,61,79–105` 将 stdio 桥接到本机 HTTP，并绑定会话账号、传播取消。新 Lite 可以借鉴分离传输层与业务层，但必须保持 Lite 原工具契约与资源 URI 的兼容适配。

`plugins/README.md:3` 指向 `onecrewai/onecrew-plugins`。当前 Pro 仓库不含插件完整实现，本轮没有分析该外部插件全部代码。Lite 接手仓库必须继续保存自己的插件来源、模板和兼容记录。

## 7. 打包为何操作集中

`pnpm build:release` 将构建、组装、三平台 ZIP 和 SHA256 串成一条命令。`scripts/build-release.mjs:23,52,77,109` 固定 Node v26.7.0，要求在 Mac 上打包，下载官方各平台运行时并校验，然后逐个平台组装。

`scripts/build-binary.mjs` 用 esbuild 打包业务输出、嵌入 PGlite WASM/data（209 起）、wreq 原生绑定（222–240）、Windows 浏览器插件（242 起）、fs-native-extensions（258–266）、许可证（273 起）、逐文件清单（416–431）；Intel Mac 使用特定 postject 过程（459–476），Mac 最后做 ad-hoc 签名（482–483）。

`scripts/sea-launcher.cjs:112–158` 按 payload 哈希把文件释放到用户缓存，验证路径和文件哈希，再加载业务入口。SEA 是统一分发入口，运行时仍有缓存和原生文件。

`scripts/RELEASE.md:17–24,44,56,60–62` 要求三个平台相同版本/提交/Node、完整 ZIP 校验、对象条件上传、完整回读后最后发布清单。上传与 Access 版本切换分开。Windows 未签名、Mac 未公证仍在限制中。

## 8. 对 Lite 迁移的结论

采用统一 TypeScript 业务服务、现有 Lite Library 的 PGlite/Teable 和 SEA 基础、独立宿主适配器、标准化发布流程。继续保留 Lite 持久任务和独有平台能力，按自身数据格式写迁移器。

不要复制 Pro 的产品目录、账号存储、工具提示词、任务缓存、发布 manifest schema 或外部插件作为 Lite 的直接替代。Pro 的 `schema_version:2` 与 Lite 现有 workspace manifest 是不同契约，需要显式兼容设计。

本轮结论可信度：结构、配置和主要代码调用关系高；实际构建耗时、三平台运行质量、外部插件完整性及全量安全性未验证。测试入口存在不代表测试已通过。
