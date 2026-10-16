# 交给新电脑 Codex 的接手提示词

请接手 `https://github.com/yyq8548/onecrew_lite2` 的 `codex/typescript-unified-gmail` 分支。当前已经实现 TypeScript/Node.js 统一程序，先读取用户规则、相关 AGENTS.md，以及 `docs/migration/14-handoff-status-and-next-plan.md`、`11-windows-verification.md`、`13-customer-installation-paths.md` 和 `source-ledger.json`。仅在需要核对设计时读取 M00–M17 详细计划，不重新通读或迁移已经完成的部分。

本次交付后用户要求暂停。先说明实际提交和当前状态；只有用户明确恢复后，才从交接记录的未完成项继续实现。不要自动启动平台搜索、发送、生产迁移、网站部署或客户升级。

源码基线为 `efc3c6eb69507b9809bb30279d9af26156d075e3`；最新批准迁入的上游为 `8d931ab08448bcded456c210ebc29ad9709a68a5`。保留现有 Windows、权限、身份、专用 Chrome、数据和恢复修复。不要覆盖本地未提交修改或用旧 Rust 实现替换当前 TypeScript 代码。

当前产品范围见 `08-current-scope.md`：对外发送仅邮件页 GmailBulk。X Chat、Instagram/Telegram 私信、关注、回复、Resend、单封用户邮件发送和专用发送审核均退役；历史记录继续可读。依赖包和原始合同不代表产品功能。保留计划确认、Creator 保存等非发送审核及必要的认证和公开读取共享逻辑。

主程序、工作台、MCP UI、Library 必须配套构建、安装和核对哈希。使用固定 Node 26.7.0、pnpm 10.32.1 和根锁文件。Windows 新安装端口由操作系统分配，已有数据目录和端口不自动替换；账号或路径错误必须明确失败。客户路径和清理约束按 docs13 执行，不硬编码开发电脑用户名、工作区和预览目录。

验收使用真实服务、原生能力、数据库及授权平台，不用 mock 或伪造提供方结果替代。缺失、失败、跳过、零结果和未知状态分别记录。正式宿主重启后回读 live MCP、状态卡、工作台及当前表格；只能验证数据而不能观察卡片视觉时保持视觉未完成。Mac ARM / Intel 需要各自真实设备，Windows 结果不能替代。

不把账号数据、密钥、Cookie、浏览器身份、私人安装链接、本机日志和构建缓存提交 GitHub。原 `onecrew_lite-access` 是旧快照，取得并核对当前生产源码及权限前不得部署。`productionReady: false` 和许可全文缺失等发布门槛以实际最新记录为准。

实现、修复、检查结果和下一步均更新到当前交接记录。改代码前梳理逻辑，精准修改，完成相关测试；不要求用户代做可自动执行的检查。未经具体授权不发送真实邮件或消息。
