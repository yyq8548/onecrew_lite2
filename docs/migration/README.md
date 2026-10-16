# 另一台电脑接手：onecrew_lite 迁移

本分支已经包含 TypeScript/Node.js 统一程序实现。请先读 [当前交接、验证和下一步计划](14-handoff-status-and-next-plan.md)。当前保留范围见 [产品范围](08-current-scope.md)，接口见 [统一 HTTP 合同](10-unified-http-contract.md)，精确候选和实证见 [Windows 验证记录](11-windows-verification.md)。源码交付后按用户要求暂停，后续实现等待明确恢复。

**当前仓库：`yyq8548/onecrew_lite2`；分支：`codex/typescript-unified-gmail`。来源基线为 `efc3c6eb69507b9809bb30279d9af26156d075e3`，本轮批准迁入的上游为 `8d931ab08448bcded456c210ebc29ad9709a68a5`。不得用其他 main 替换该基线。**

## 阅读顺序

1. [Lite 全模块分析](01-lite-codebase-analysis.md)：现有全部35个Rust模块、Library、网页、插件、数据和发行边界。
2. [Pro 架构分析](02-pro-reference-analysis.md)：统一服务与SEA的实际机制、可借鉴内容及功能差异。
3. [详细迁移计划](03-detailed-migration-plan.md)：M00–M17任务、依赖、产出、完成条件和发布门槛。
4. [验收矩阵](04-acceptance-and-release.md)：数据、账号、搜索、平台、邮件、MCP、安装、官网和回退逐项用例。
5. [接手环境与操作流程](05-new-computer-handoff.md)：克隆、基线核对、构建入口和首轮开发顺序。
6. [可直接交给新电脑 Codex 的提示词](06-agent-handoff-prompt.md)。
7. [机器可读来源记录](source-ledger.json)与[已发布版本记录](07-published-baseline.md)。

## 关键事实

- 基线来自公司main `26eefcab4dab3b836937c2922ec33789c6c10975`，与Mac r4构建源码 `8cedccf2a29433728e9c81f052c9d485df39a1ad` 文件内容相同。
- 基线有5546个Git跟踪文件，包含所有已知客户端源码、依赖锁文件、供应商代码、测试和构建脚本。运行依赖、安装包、账号、数据库和凭据不提交GitHub。
- 原交接记录中的 Windows 0.7.1 r3 声明源码 `aef10db...` 尚未核实，不能声称本分支可重建该原包；新版 Windows 候选有独立哈希和实际验收记录。
- Library本来就是TypeScript/Node SEA，迁移会复用它。统一程序不要求同时合并SQLite与PGlite格式。
- Pro业务实现不复制到个人GitHub；文档提供固定来源与设计分析。
- 内附 `onecrew_lite-access/` 是旧快照。部署网站必须先取得公司当前Access源码，不能从旧快照直接部署。
- 原发布版本快照见 `07-published-baseline.md`；它不表示线上当前状态。本仓库的开发候选不会自动部署或升级客户电脑。

## 接手完成条件

新电脑克隆正确分支，核对实际提交，按 `14-handoff-status-and-next-plan.md` 构建并进行本机验收。原 `verify-handoff.mjs` 仅用于未修改的原始基线。用户恢复开发后从当前未完成项继续，记录失败与跳过，不把未来目标标为已经实现。
