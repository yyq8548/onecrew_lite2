# 另一台电脑接手：onecrew_lite 迁移

本分支包含公司已发布 Mac 0.7.1 r4 对应的完整受版本管理源码，以及迁移到 TypeScript/Node.js 统一程序的详细分析和计划。迁移实现尚未开始。

**接手分支：`codex/typescript-migration-handoff`。不要直接克隆默认 main 后开始迁移；个人 GitHub main 与公司 main 的历史已经分开，本次保留原历史。**

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
- Windows线上仍是用户指定0.7.1 r3；其声明源码 `aef10db...` 尚未核实，不能声称本分支可重建该线上包。
- Library本来就是TypeScript/Node SEA，迁移会复用它。统一程序不要求同时合并SQLite与PGlite格式。
- Pro业务实现不复制到个人GitHub；文档提供固定来源与设计分析。
- 内附 `onecrew_lite-access/` 是旧快照。部署网站必须先取得公司当前Access源码，不能从旧快照直接部署。
- 线上当前保持Mac r4、Windows r3、Pro0.1.8。此分支仅交接，不会自动部署或升级客户电脑。

## 接手完成条件

新电脑成功克隆指定分支、运行源码交接验证、确认版本和缺口、建立独立开发分支，再依M00和M01开始。开发者持续把实际完成结果写入任务记录；不得把文档中的未来目标标成已实现。
