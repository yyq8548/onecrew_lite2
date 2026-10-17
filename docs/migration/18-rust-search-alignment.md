# Rust 搜索行为同步验收

2026-10-16 核对 GitHub Rust 已提交版本 `8d931ab08448bcded456c210ebc29ad9709a68a5`。比较未纳入 Rust 本地未提交修改。本轮修改及验收范围已由用户批准。

## 错误提示

TypeScript 现在区分真实 SQLite 锁冲突、计划缺失、数据库连接关闭和数据库操作失败。工作台新增中英文说明，并识别历史 Rust 仓储错误代码。原始异常继续报告，不自动重试，也不将失败记为零结果。

回归测试复制仓库内真实 SQLite，通过排他锁、缺失计划、关闭连接及表缺失触发实际仓储故障，没有模拟异常。

## 验收草稿

新计划：`mac-arm-all-platform-beauty-acceptance-20261016-v2`。

宿主模型先理解原始需求及批准的设置，再依据与 Rust 一致且经过 SHA-256 校验的指导生成分平台查询。独立模型审核全部 78 条，拒绝重复翻译后复核变更的账号查询。随后使用 `buildDiscoveryDraft` 和 `validateDiscoveryQueryAudit` 校验，通过实际 MCP 的 `intent.validate` 和 `campaign.create` 保存。未调用嵌套的工作台 Codex 模型桥接。

草稿包含 13 个来源，每来源 6 条查询；目标 130 人，最多读取 520 次，最低粉丝 2000，内容语言不限，公开邮箱作为排序偏好。Instagram、Pinterest 使用账号领域词，LinkedIn 使用人员角色词，视频、帖子、混合结果及网页入口分别规划。

brief 保留原始请求；随后批准的 130 人、520 次和最低粉丝 2000 作为明确设置覆盖早期 YouTube 10 人请求。旧计划及历史结果保留。该计划随后已由用户在工作台批准，实际搜索结果及当前限制见 [全平台搜索验收](19-platform-search-acceptance.md)。

## 实际验证

- 应用和工作台类型检查、工作台构建通过。
- 工作台完整回归：125 个文件、824 项通过。
- SQLite 故障、真实搜索调度和适配器安装回归：4 项通过。
- candidate-06 真实 SEA 启动、两个界面、SQLite/PGlite 和缓存完整性：1 项通过。
- 真实 Codex 安装、candidate-05 至 candidate-06 升级及上一插件恢复：3 项通过。
- 原生测试均为 0 项失败、0 项跳过。升级保持端口，切换实例，拒绝旧会话，使用新票据建立有效会话；缓存插件的 MCP 返回新实例及程序哈希，状态卡 HTML 可读取。该测试不证明宿主状态卡实际渲染。
- macOS ad-hoc 签名完整性及 `git diff --check` 通过。
- 当前用户工作台及资料库已恢复连接，原专用 Chrome 身份 `jga2zn4u` 已重新验证。2026-10-16 本地时间 15:20 至 15:21，TikTok、Instagram、X、LinkedIn、Facebook、Reddit、Bilibili、Douyin、Weibo 的真实登录复查均通过。登录通过不等于搜索成功。

日志位于 `.scratch/alignment-web-tests-final.log`、`.scratch/alignment-web-build.log`、`.scratch/alignment-discovery-tests.log`、`.scratch/alignment-sea-06.log` 和 `.scratch/alignment-codex-upgrade-06.log`。规划、独立审核和 MCP 回执保存在同一私有中间目录。

本地运行 candidate-06，原账号数据目录和 53400 端口保持不变，保留 candidate-05。许可全文仍缺 13 项，候选未正式发布。浏览器会话仍随服务退出失效，本次验证新票据恢复，未取消该会话边界。
