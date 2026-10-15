# 交给另一台电脑 Codex 的接手提示词

请接手 onecrew_lite 的 TypeScript/Node.js 统一程序迁移开发。目标是保留当前全部业务能力、账号数据、Codex接入与官网升级兼容，借鉴Pro的统一服务和SEA发行方式。产品名称统一使用 onecrew_lite。

先确认你能操作这台电脑，并读取当前用户规则及仓库相关AGENTS.md。仓库为 https://github.com/yyq8548/onecrew_lite.git ，必须使用 `codex/typescript-migration-handoff` 分支；默认main是保留的旧历史。没有本地检出时先克隆该分支。不要覆盖已有未提交内容。

按顺序读取 docs/migration/README.md、01-lite-codebase-analysis.md、02-pro-reference-analysis.md、03-detailed-migration-plan.md、04-acceptance-and-release.md、05-new-computer-handoff.md 和source-ledger.json。先执行 `node docs/migration/verify-handoff.mjs` 验证原始交接源码，再建立 `migration/typescript-runtime` 开发分支；已经存在则核对状态后继续。

当前已知源码来自公司main 26eefcab4dab3b836937c2922ec33789c6c10975，与Mac 0.7.1 r4构建提交8cedccf2a29433728e9c81f052c9d485df39a1ad内容相同。线上Windows是用户指定0.7.1 r3，其源码aef10db3c16754d39a5f1d424dd88541b412ee55尚未核实，不能宣称本分支可以重建该包。Mac已包含登录预检、搜索错误、iPhone整体语义及安装说明修复，必须保留。

Library已经使用TypeScript/Hono/PGlite/Teable/Node SEA，优先复用。统一主服务和身份，首次迁移保持Creator SQLite与Library PGlite原格式，通过明确仓储和outbox协调；不要在改语言时同时重写所有存储。Pro完整业务实现不复制进个人GitHub。保留Lite的持久搜索、预算、暂停恢复、补词、证据、表格同步、邮件/私信幂等、Telegram和X Chat等全部能力。

从M00开始逐项执行计划，每项完成条件满足后更新记录。M01先验证Chrome系统凭据、X网络指纹、Telegram会话和X Chat加密兼容，再固定原生依赖和SEA组合。主业务目标是TypeScript，必要受控原生依赖明确列入包和维护责任，不能未经验证承诺纯JavaScript替代全部能力。

安装本体、接入Codex、语义provider是三个不同边界。WorkBuddy暂为未来adapter目标，不宣称已支持。保留现有CLI/Web/MCP契约、数据和账号隔离，两个不同旧账号不能自动合并。不要删除功能以完成迁移。

新建或调整验收时使用真实本地服务和数据库、授权测试账号，不用mock/伪造内容代替运行结果。未授权不得发送邮件、私信或发布内容。需要用户浏览器确认时明确说明具体步骤，其余可执行工作持续完成。自动检查失败应修复并复验，不要求用户代做你能执行的验证。

所有开发先在隔离目录完成，保留日常安装和账号数据。禁止把凭据、Cookie、客户数据库、私人安装链接提交Git。中间文件放仓库忽略目录，不使用/tmp。代码和文档使用文件编辑工具修改。

仓库中的onecrew_lite-access是旧快照；当前生产Access来源为source-ledger记录的公司整合分支。网站相关工作必须取得并核对公司当前源码，保留Pro。尚未取得资源时继续不依赖它的任务，明确记录阻塞。

本次授权为按计划开展迁移开发与验证。正式发布、替换日常安装、迁移真实用户数据或对外发送需有相应明确授权。持续更新任务状态、代码提交、验证结果和缺口，达到完整候选与验收门槛后提交可审核的发布材料。
