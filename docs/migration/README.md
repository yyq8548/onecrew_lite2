# 开发与交接文档

当前仓库为 `yyq8548/onecrew_lite2`，默认分支为 `codex/typescript-unified-gmail`。产品 0.7.1，Windows 当前构建 candidate-87 / R8.1。

- [当前交接与下一步](14-handoff-status-and-next-plan.md)：当前实现、已验证内容和未完成验收。
- [迁移计划](03-detailed-migration-plan.md)、[验收要求](04-acceptance-and-release.md)和[产品范围](08-current-scope.md)。
- [统一 HTTP 合同](10-unified-http-contract.md)与[客户安装约束](13-customer-installation-paths.md)。
- [独立开发与配套验收](../../development/README.md)。
- [搜索、评分与连接行为](current-platforms.md)。
- [R8.1 同步、表格及安装验证](28-sync-and-table-loading-windows.md)。
- [当前源码交付与清理检查](29-current-source-delivery.md)。
- [许可全文证据与缺口](16-license-evidence.md)。
- [来源记录](source-ledger.json)仅用于来源追溯，不是构建依赖或当前线上发布证明。

历史实现、旧内测安装入口和过期交接记录已从当前源码移除。当前必要的测试、数据库兼容样本与第三方许可证继续保留。账号数据、密钥、Chrome 身份、依赖缓存和安装包不进入 Git。

涉及共享服务或生产发布时，继续遵守[合并与部署流程](../release/合并与部署流程.md)。本次源码推送不包含生产部署或下载切换。
