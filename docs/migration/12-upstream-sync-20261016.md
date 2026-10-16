# GitHub 改动迁入记录

用户已批准将 GitHub `8d931ab08448bcded456c210ebc29ad9709a68a5` 的平台搜索、适配器、计划调整、资格证据、执行恢复及 Gmail/Library 相关修复迁入当前 TypeScript 版本。保留已有 Windows、浏览器、身份和权限修复。对外发送仅保留邮件页 Gmail 批量发送；Mac ARM/Intel 验收保持未完成。当前实施尚未完成，不作为新版本验收记录。

## 来源与上下文

- 原迁移基线：`efc3c6eb69507b9809bb30279d9af26156d075e3`；工作分支 `codex/typescript-unified-gmail`。
- `git fsck --full --no-dangling` 通过；GitHub 主分支差异涉及 177 个路径。
- 独立上游源码副本：`.scratch/upstream-8d931ab`，只读核对，不将其 Rust 实现直接覆盖 TypeScript 工作区。
- 本对话持续使用 `context-mode` 技能：大段源码分析、差异和日志通过 `ctx_execute`、`ctx_execute_file`、按来源限定的 `ctx_search` 管理；小范围编辑使用文件编辑工具。约定已按用户要求记录到记忆更新目录。该工具不保证无限上下文，也不修改宿主历史。

## 实施检查点

- 已迁入 Instagram 原生 GraphQL 搜索及主页名称/粉丝读取修正，Pinterest 主搜索改为 search-users，YouTube 细分失败代码。完整适配器 348 项回归通过；Instagram 测试采用上游脱敏真实响应。
- 查询指导已更新为上游内容；规范化 SHA-256 `da1a52eefe1a34dbe0aad19e0d12e55bd32f518c391457117dee21267ce28a39`，与独立上游副本一致。
- 已添加独立查询审查及扩词审查；计划修改携带源计划及版本；额外条件保存 requirements，默认公开邮箱软偏好。
- 已添加逐项 requirement 审核合同、引用核验、匹配模型输出和资格合并；缺少证据保持 pending，明确违反为 rejected，匹配分不能覆盖额外条件。
- 已迁入计划修改的源版本核对、精确搜索资料复用、额外条件审核证明审计、联系人有界补证和公开邮箱排序；旧计划及旧资格回执保留。
- MCP 当前合同和网页共享审核合同已支持逐项条件与恢复检查点；冻结的 Rust 历史合同作为原始对照保存。网页展示原文条件、缺失证据及细分读取失败；旧发送入口不恢复。
- 评分规则增加原生作者是否确实创作相关内容的核验，与 GitHub 原文一致；SHA-256 为 `694f0cfbee3ebe89df2cdb44b93aca6bce31076048a6eec83651ee265c9a6a67`。规则变化生成不同评分幂等键。
- 持久待评分队列、并发门禁和有限预读使用现有 TypeScript 等价实现；没有替换已验证的调度器。Rust 专用测试反序列化辅助、Rust dispatcher 线程池和 Rust 单组件安装器不作为 TypeScript 运行依赖迁入。
- 上游 Resend 删除与本分支的更严格发送退役范围一致；邮件页仅 GmailBulk 可发送。刷新时停用遗留 Resend 授权，保留历史数据。
- Windows 中文权限检查管道已明确使用 UTF-8；中文路径检查与账号迁移回归 15 项通过，8 次真实权限检查约 77 毫秒。原权限及重解析点检查保留。
- 网页完整回归 824 项通过；全部适配器回归 348 项通过。真实 SQLite 计划修改、恢复检查点、额外条件 ledger、MCP 新合同及当前评分规则均已单独验证。
- 工作台、Library 服务及网页已重新构建。Windows 候选包使用 `VITE_ONECREW_UNIFIED=1` 的 Library；打包入口会拒绝错误的前端模式。最终配套包及 live 验收尚在执行。

## 当前 Windows 验证

candidate-52 已包含新安装端口分配修复，最终主程序完整套件 545 项全部通过、0 失败、0 跳过，含真实长中文路径、旧版替换及原生验收。官方插件安装及完整 Codex 重启后，正式 live MCP 程序哈希匹配，工作台和原表格回读一致。状态卡视觉、Mac ARM / Intel 和发布门槛未完成；精确结果见 `11-windows-verification.md` 首节。按用户要求推送源码后暂停。下列 candidate-51 记录为此前证据。

当前 candidate-51：程序 SHA-256 `e0b38be33f0b77a2baf3789edcd79c68d9944e2c1bcb33b5a1b88ae6b24a697a`；payload `e372677c66374b5dd6697b873892777080e500cd55bc6cc2aa50ee1dba560f35`；源码 `9257f7be1d1c183c82689e781cad89321eff06688b072bb5fdf741d76af49047`。ZIP 57,133,715 字节，SHA-256 `a0d961e852861d2749c408461d45a40eef87471af4b98913e1d9275b17125b6f`。

- 8 项真实 Windows 专项全部通过，无跳过，覆盖长中文安装路径、原安装文件移走、不同构建替换、并发保存、环境继承、父 Job 退出、原生数据库、两套网页、MCP 及缓存损坏拒绝。客户路径约束见 `13-customer-installation-paths.md`。
- 已执行官方插件安装和完整 Codex 重启。当前插件为 `0.7.1+codex.ae356579492c1964bd6bd91772e5a616672d7200447dbf24f8b7da4f9041fd9c`；正式 live MCP 回读的程序与源码哈希均匹配 candidate-51。服务保留重启前 PID 51564、instance `dea7c4cc-9cdf-42e6-819b-d794a24916ee` 和端口 62462，Access 为 signed_in、数据为 ready。
- Library 已重新授权并打开当前表格，Brandon Melville、约 6,910 粉丝、推荐候选人、良好和原生频道链接均与搜索结果一致。专用 Chrome `xrcq5ac3` 再次显示“已验证并选择此身份”。
- 最终主程序完整套件仍在独立 Windows 任务中运行，日志 `.scratch/windows51-full-main.log`；未将此前候选的完整测试记录作为本次通过证据。
- 状态卡视觉检查仍受宿主 MCP Apps 空标签列表限制。Mac ARM、Mac Intel 验收未完成。远端同步协议未接受，`productionReady: false`，16 项许可全文缺失仍为发布门槛。

实际 UI 已修复并重新构建：输入修改说明保留源计划；默认读取预算只在原文未指定时使用；查询审查检查最终合并后的计划，并拒绝将“不发送消息”等执行规则作为作者资格条件。修改计划的未指定人数、预算、语言、品类和粉丝门槛继承可信源计划；缺少原粉丝门槛保留为无限制。新建计划默认规则明确提供给审查模型。源码完整 Discovery 129 项、工作台 824 项通过，真实模型三阶段诊断通过；未批准此前失败草稿。

真实新计划 `84cfef4f-dc4f-4137-957a-ad7540e1b941` 使用 2 / 8 次读取，合格 1、待核实 0、读取错误 0，target_reached。表格 `tblPbF5GN9CfZQJ07Cf` 28 条来源，实际显示 Brandon Melville、约 6,910 粉丝、推荐候选人、良好及原生频道链接；27 条来源未绑定作者。candidate-51 正式宿主重启后的同表内容回读一致。没有发送邮件或其他消息。

状态卡的同账号恢复故障已用真实 SQLite / AccountContext 回归复现并修复：指标缓存绑定账号 epoch，避免重新验证后调用旧会话 owner。新旧会话隔离和持久指标读取均通过；最终 Discovery 完整日志为 `.scratch/upstream-discovery-final-05.log`。

通过日志：`.scratch/upstream-all-adapters-final.log`（348 项）；`.scratch/upstream-full-web-02.log`（824 项）；`.scratch/upstream-requirement-ledger-tests.log`（10 项）；`.scratch/upstream-unicode-private-tests.log`（15 项）；`.scratch/upstream-rust-snapshot-tests.log`（18 项）；`.scratch/upstream-activation-recheck.log`（12 项）；`.scratch/upstream-library-access-validation.log`（授权验证）；`.scratch/upstream-final-app-typecheck.log`（主程序类型检查）。
