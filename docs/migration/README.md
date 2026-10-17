# 开发与交接文档

当前仓库：`yyq8548/onecrew_lite2`，默认开发分支：`codex/typescript-unified-gmail`。产品0.7.1保持开发候选状态。

## 当前入口

- [当前交接与构建](14-handoff-status-and-next-plan.md)：环境、配套构建、测试与未完成验收。
- [搜索、评分与连接现状](current-platforms.md)：Instagram 读取方式、评分、同步与连接恢复。
- [产品范围](08-current-scope.md)和[统一 HTTP 合同](10-unified-http-contract.md)。
- [客户安装约束](13-customer-installation-paths.md)、[Windows 验证](11-windows-verification.md)、[Mac ARM 验证](15-mac-arm-verification.md)和[许可证据](16-license-evidence.md)。平台验证记录只代表当时版本。

## 架构与来源

- [Lite 分析](01-lite-codebase-analysis.md)、[Pro 参考](02-pro-reference-analysis.md)和[迁移计划](03-detailed-migration-plan.md)保留设计来源，当前行为以源码及现状说明为准。
- [验收矩阵](04-acceptance-and-release.md)和[新电脑接手](05-new-computer-handoff.md)用于准备独立环境。
- [来源清单](source-ledger.json)、[发布基线](07-published-baseline.md)及[上游同步](12-upstream-sync-20261016.md)保留来源证据，旧记录不表示生产当前状态。

源码不包含凭据、运行数据库、Chrome 身份、依赖缓存和安装包。`onecrew_lite-access/` 为旧快照。跨电脑交接同时携带[合并与部署流程](../release/合并与部署流程.md)。
