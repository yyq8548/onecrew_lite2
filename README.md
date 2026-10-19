# OneCrew Lite · TypeScript 统一程序

从一句话需求生成搜索计划，读取创作者资料和内容，结合证据评分，在动态表格整理推荐，再通过邮件页 Gmail 批量联系。

当前产品版本 **0.7.1**，Windows 配套构建 **candidate-87 / R8.1**。主程序、工作台、Codex 插件和 Library 配套构建。Mac ARM、Mac Intel、宿主状态卡实际按钮验收和 11 项许可全文缺口仍未完成，`productionReady: false`。

## 当前源码与文档

仓库为 [yyq8548/onecrew_lite2](https://github.com/yyq8548/onecrew_lite2)，默认分支为 `codex/typescript-unified-gmail`。

- [当前交接与下一步](docs/migration/14-handoff-status-and-next-plan.md)
- [迁移计划](docs/migration/03-detailed-migration-plan.md)与[产品范围](docs/migration/08-current-scope.md)
- [R8.1 Windows 验证](docs/migration/28-sync-and-table-loading-windows.md)
- [当前源码交付与检查](docs/migration/29-current-source-delivery.md)
- [搜索、评分与连接行为](docs/migration/current-platforms.md)
- [独立开发与配套验收](development/README.md)

| 目录 | 内容 |
| --- | --- |
| onecrew_lite-app | TypeScript CLI、本地服务、搜索、评分、Gmail、数据兼容、安装与 SEA 打包 |
| onecrew_lite-web | 工作台、邮件页和日常 Chrome 身份授权 |
| onecrew_lite-mcp-ui | Codex MCP 状态卡与操作卡 |
| onecrew_lite-opencli | 只读平台适配器及测试 |
| onecrew_lite-contracts | 共享类型、校验合同和自有 schema |
| onecrew_lite-library | 动态表格服务、网页、同步、推荐视图及必要的 Teable 依赖 |
| development | 契约生成、源码独立性检查和当前 Windows 客户安装资料 |

仓库只维护当前实现。构建和测试不引用旧工程；第三方原生扩展、许可证、数据库兼容样本和必要测试仍保留。账号凭据、客户数据、浏览器配置、依赖缓存和安装包不提交 Git。

## 使用与安装

搜索使用用户日常 Chrome 中的官方 OpenCLI 扩展，明确选择并授权浏览器身份，不创建独立浏览器。工作台和 Library 默认复用 Codex 内置浏览器。已有账号、数据库、主密钥、端口和浏览器授权在升级时保留。

Windows 采用预构建 ZIP、外部大小与 SHA-256 校验、包内逐文件校验，再分步安装。唯一维护的安装入口位于 [windows-customer-package](development/windows-customer-package/)；[安装指南](development/windows-customer-package/INSTALL-CODEX.md)和[通用 Codex 提示词](development/windows-customer-package/CODEX-PROMPT.zh-CN.md)随包交付，首次安装与旧版升级自动识别。多数据目录时由用户选择，不猜测或清空。

对外发送仅保留邮件页 Gmail 批量发送，需用户具体确认；MCP 不提供发送操作。后台表格同步继续保留授权、账号隔离和失败恢复。同步仅由新增推荐、暂停、结束或用户手动操作触发，普通读取和重启不重新发布已完成表格。

## 构建与检查

固定使用 Node.js **26.7.0**、pnpm **10.32.1** 和根目录锁文件。Windows 第三方原生依赖需要对应 C++ 构建工具。业务代码、网页、适配器、契约和测试均由当前项目独立管理。

```powershell
pnpm install --frozen-lockfile
pnpm check:independence
pnpm build:tables
pnpm typecheck
pnpm check:contracts
pnpm build:ui
pnpm test:app
pnpm test:opencli
node docs/migration/verify-handoff.mjs
```

原生安装和真实平台检查需要明确提供测试环境与真实候选。测试跳过不计为通过；进程存在、端口在线或返回 URL 不能代替状态卡、工作台和动态表格验收。构建参数与完成条件见 development/README.md。

本仓库尚未对项目整体授予开源许可。第三方许可证与声明保留在所属目录中；源码推送不代表切换生产下载或云端部署。
