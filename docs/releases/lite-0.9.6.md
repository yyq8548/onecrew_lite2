# QB-3448：Lite Mac 与 npm 0.9.6

本次将已在本机验收的 Lite 0.9.4 R3 搜索流程优化及此前知乎、小红书接入汇集为 0.9.6。公司客户端基线为 `main` 的 `4fd4dec79dec14ef6f0c420b664e0289e951a26f`，功能来源为 `165939b3b8cbd543055ce9355b2de211e55b4c86`。Mac ZIP、npm 与官网安装入口已于 2026-10-28 发布并验收。

截至 2026-10-28 20:59（America/Los_Angeles），客户端 [MR !8](https://git.digifinex.org/onecrew_lite/onecrew_lite/-/merge_requests/8) 仍为 Open，等待 River 合入 `main`。Access [MR !34](https://git.digifinex.org/onecrew/onecrew-access/-/merge_requests/34) 已由 River 于 20:44 合入 `onecrew`，合并提交为 `56d30544c8fd95fcd7a84c1ea02f15243e084f53`。已拉取公司 `onecrew` 并确认其完整文件树与实际部署提交 `3a6808e4f06579d5f7c8d579649d2c81aa99d1bb` 相同。

本次用户明确要求“我会让 River 去帮我合并，你先上传和部署”，据此从已验证并推送的功能提交先行发布。客户端尚未合并的状态如实保留，不将功能提交记为公司主分支提交。Access 合并后没有重复部署，已发布产物及其构建来源保持不变。

本版保留读取与评分并发、原生作者身份和证据校验、跨任务已成功读取材料、旧 Library 迁移、原账号和安装身份、表格及搜索结果。安装及发布验收不恢复搜索。宿主等待使用整数毫秒，联系页排除明确嘉宾与推广资源，推荐完成与可选联系资料补充分别展示。

交付范围为 Mac ARM64 ZIP、对应安装提示词、`@onecrew/lite@0.9.6` 和 Mac 平台 npm 包。npm 主包继续引用已验收的 Windows 0.9.1；`windows_installer` 保持公司基线内容，不重新打包或发布 Windows。公司 Pro、Cloud、账号及其他配置不在修改范围。

Mac 采用 ad-hoc 签名，尚未经过 Apple 公证，候选标记 `productionReady=false`。下载与安装前校验 ZIP、全部包内文件和原生程序哈希；仅在用户明确同意后处理对应程序的 quarantine 属性。

此前有效验证：工作台 820 项、MCP 界面 407 项；相关搜索与联系人回归 240 项通过、7 项依赖外部记录环境的检查跳过；真实 Mac 原生程序冷启动、重启、SQLite、PGlite、双网页、MCP 和授权会话通过。额外主程序全量回归在 900 秒内未完成，未记录为通过。0.9.6 另行执行类型检查、契约检查、构建及真实原生包验收，结果与哈希写入分发记录。

## 发布来源与验收

原生程序构建提交为 `513b6c81d6b1570808b952ef4b825571370d8f6c`，npm 构建提交为 `4a8057c2c605c6b8f3b3a037a0c15ede408c6c8c`，构建时工作区均干净。客户端实际发布提交为 `bbb830032273c19de61e58a328f87f500651cc91`；后续发布文档提交不改变构建输入。

- Mac ZIP：`OneCrew-Lite-0.9.6-Mac-arm64-R1.zip`，61,655,697 字节，SHA-256 `a7c567bb5269d149284842c4c17f28298fd95aca0ad50d782c29697a9ee9ad25`。包内 523 项与严格签名校验通过。
- `@onecrew/lite` 与 `@onecrew/lite-darwin-arm64` 的 0.9.6 已公开发布，两个 latest 均为 0.9.6。主包精确引用 Mac 0.9.6 和 Windows 0.9.1。
- Access 部署版本为 `efad3755-1e06-4268-836b-ca79d267e1c3`，流量 100%。Site 沿用 `98f7aa1d-5a20-4ab5-951c-aad92d198fea`，没有重新部署。没有新增云端数据库迁移。
- 0.9.6 构建、类型、契约与独立性检查通过；关键搜索与联系人回归 16 项通过、1 项缺少外部记录环境跳过；Access 单元测试 37 项与 Worker 测试 151 项通过。
- 真实 Mac 原生程序冷/暖启动、重启、SQLite、PGlite、双网页、live MCP 与授权会话验收通过，34.3 秒。
- R2 完整回读、Mac/Windows HEAD 与完整 GET、大小及 SHA-256 通过。16 组中英文 Lite/Pro、Mac/Windows、npm/ZIP 安装指引及提示词接口通过；Pro、Windows、账号页面及其他配置保持发布前基线。
- 两个公开 npm tarball 的完整性与本地产物一致；从公开 registry 安装后，原生程序、服务、live MCP、工作台及 Library 授权会话验收通过，9.854 秒。
- 官网四种安装选项正确：Mac npm/ZIP 为 0.9.6，Windows npm/ZIP 为 0.9.1。安装与发布验证没有恢复搜索，也没有重复读取成功账号。

## 下载入口与交接

- 官网：https://onecrewai.io/zh/product/onecrew-lite
- Mac ZIP 安装提示词：https://access.onecrewai.io/cli/install/lite?lng=zh-CN
- Mac npm 安装提示词：https://access.onecrewai.io/cli/install/lite/npm?lng=zh-CN
- Mac ZIP：https://access.onecrewai.io/cli/packages/lite/0.9.6/onecrew_lite-0.9.6-aarch64-apple-darwin-r1.zip

英文指引使用 `lng=en-US`。原始发布、下载与安装回执保存在工作区 `outputs/lite-096-release-20261028`；最终结果为 `RELEASE-RESULT.md`。发布记录同步到两个 MR 说明。客户端 MR 合并后核对实际 `main` 提交与完整构建输入；当前不恢复搜索。

恢复时先读取最新生产状态，仅将 Lite Mac 下载指向恢复至已验收的 0.9.3；保留期间同事的其他配置。npm 已发布版本不可覆盖。原始客户数据、旧程序与恢复副本保留。

工作区流程入口为 `AGENTS.md` 与 `docs/release/合并与部署流程.md`。跨电脑交接时同时携带该流程、本记录、构建提交、产物清单及未完成检查。
