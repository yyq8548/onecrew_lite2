# 全平台搜索优化与验收

2026-10-16，本地 TypeScript 开发验证。本次搜索验收使用 candidate-17，端口 53400，实例 `aeffc01f-4a76-4b28-8b2b-b866b8c5592b`；最新连接候选 candidate-22 见 [连接修复记录](22-connection-stability.md)。数据目录 `.scratch/mac-arm-live-data`、原浏览器身份 `jga2zn4u` 和 binding `724e8b59-dfd1-4f62-a949-a16e26da073f` 保持不变。历史首轮记录见 [首次搜索及登录检查](19-platform-search-acceptance.md)。四个平台本轮字段与证据详见 [四个平台的评分证据](21-four-platform-scoring-evidence.md)。

## 完成条件与停止边界

真实搜索及必要原生读取稳定完成，保存的来源、作者和内容证据足以支持当前条件的评分，即标记成功，停止该平台的验收读取。只补充缺失证据；出现实际失败或后续修改影响该平台时才重新验证。成功表示平台能提供评分材料，不表示全部返回作者符合条件。

本文件记录评分证据验收状态；Campaign 的 completed 仅表示一次操作完成。没有修改数据库状态以伪造成功。

## 连接与界面

工作台已实际显示“本地服务已连接”“扩展已连接”，原身份已验证并选中。资料库已实际显示 3 个表格。服务切换使旧内存会话失效，已通过正式新票据恢复两个现有页面，并检查页面文字确认。

专用浏览器界面已增加中英文说明：退出或重启 OneCrew 会关闭专用浏览器；使用原身份的“重新打开”，再完成“验证并连接此浏览器”。仅重启 OpenCLI daemon 时，保持浏览器打开，扩展尝试自动重新连接。当前 OpenCLI 1.8.8，daemon 端口 19825。

同账号、同认证代次的有效会话复用 Cookie 和 CSRF；一次性票据仍消费，退出、账号切换和服务重启仍使旧会话失效。双界面真实 HTTP 检查通过，回执 `.scratch/workspace-session-reuse-browser-help.json`。candidate-17 原身份恢复回执 `.scratch/connection-repair-ce3232f3-cb4a-4530-b293-fbe230643339.json`；切换后已用新票据恢复两个现有页面，通过页面文字核实服务、扩展及原身份已连接、资料库仍有 3 个表格。

## 已实现修复

- YouTube 独立搜索页面按来源、路径、查询参数及原生初始化数据判断就绪，区分超时、跳转、HTTP、结构、授权及风控错误。修复后真实搜索约 12 秒返回 20 条，随后原生视频读取成功。
- Instagram 中文输入标签修复后搜索完成，返回 5 条账号结果。
- Pinterest 账号搜索有效；默认创作者入口改为已有原创 Pin 搜索，正式 Campaign 返回 10 条含原作者与正文预览的结果，避免从转存列表寻找原创。
- daemon、扩展、原身份不可用三类连接停止支持明确检查点恢复。恢复前真实检查原身份；页面未读取的连接失败恢复同一查询，其他可恢复解析错误推进下一已批准查询。停止原因、时间、账号、版本和预算校验保留。
- LinkedIn 搜索卡片副标题仍未取得；原生主页已有职业和简介。已修复只在顶部找粉丝量的问题，实际从同一作者 Activity 区域取得 734 followers，并通过严格解码。
- Bilibili 视频读取改用已有独立页面读取器，校验视频和作者身份。真实读取完成：知日wit，390078 粉丝，视频 BV1Df4y1C7Vg；付费状态未核实，保持未知。
- Facebook 原生正文末尾实际含有“ … See more”，新读取与已保存回执的评分材料均保守标记为截断；缩写粉丝显示进入材料，以保守范围判断门槛，精确值保持未知。
- 本地验收脚本拒绝对已成功的 12 个平台重复搜索或原生读取；读取已保存的评分材料不受影响。

连接恢复测试先复现失败，修复后 SQLite 检查点、原生搜索及 MCP 上游共 3 项测试通过；应用类型检查通过。Web 类型检查、生产构建通过，实际 HTTP 资源包含中英文说明。candidate-14 SEA 验证及 macOS 签名完整性检查通过。

candidate-16 构建、SEA 测试和 macOS 签名完整性检查通过；Facebook 相关 5 项测试通过。`.scratch/verify-facebook-truncation.ts` 使用 TypeScript AST 定位实际截断表达式，以已保存真实回执正文验证 `text_truncated=true`，新增外部访问为 0。当前账号的只读适配器已更新，并检查安装文件确实包含该修复。构建及 SEA 日志为 `.scratch/mac-arm-candidate-16-build.log`、`.scratch/platform-optimization-sea-16.log`。

candidate-17 构建、SEA、签名检查通过；本轮资格与搜索相关 18 项、LinkedIn 与审核相关 26 项检查通过。实际评分材料回读验证 Facebook 估计范围及历史正文截断，实际主页读取验证 LinkedIn 734 粉丝，正式 Campaign 验证 Pinterest 新入口。当前适配器已更新。

## 平台验收状态

以下是已批准 v3 计划的单次真实搜索，数量不代表合格作者数或命中率。

| 平台 | 返回条数 | 搜索耗时 | 评分证据验收 |
| --- | ---: | ---: | --- |
| YouTube | 20 | 12.0 秒 | **成功**：原作者、完整说明、152 万订阅量 |
| TikTok | 20 | 7.1 秒 | **成功**：正文、原生作者、主页简介及粉丝量已取得；停止重复验收 |
| Instagram | 5 | 18.0 秒 | 待核实：第二查询也返回 5 个账号；原生资料诊断遇风控，停止进一步读取 |
| X | 20 | 5.4 秒 | **成功**：tutatulalit 原帖、主页及 4502 粉丝 |
| LinkedIn | 10 | 12.1 秒 | **成功**：原生职业、简介和 734 粉丝足以按门槛排除该样本 |
| Facebook | 6 | 11.4 秒 | **成功**：原生作者、美妆正文节选及约 28.9 万粉丝，截断与近似限制保留 |
| Pinterest | 10 | 6.5 秒 | **成功**：正式原创 Pin 搜索；独立原生主页样本返回 12 粉丝，可按门槛排除 |
| Reddit | 20 | 4.0 秒 | **成功**：原作者、原帖及 0 粉丝已取得，可据此排除该候选 |
| Bilibili | 20 | 4.0 秒 | **成功**：原视频、作者、简介及 390078 粉丝 |
| Douyin | 16 | 6.8 秒 | **成功**：原生正文、作者、主页及 1.8 万粉丝显示 |
| Weibo | 20 | 5.8 秒 | **成功**：原创正文、作者及 553051 粉丝 |
| Google | 7 | 3.0 秒 | **成功**：首条视频复用已保存 YouTube 同来源原生证据 |
| Yahoo | 7 | 3.6 秒 | **成功**：首条视频独立原生读取，完整说明、作者及 44.3 万订阅显示 |

以上 12 个成功平台停止重复验收。Google、Yahoo 的验收仅证明已核实来源可评分；没有声称全部返回结果均已核实，也没有声称已持久化为 Campaign web_source。平台成功与作者符合筛选条件分别判断，尚未提交评分。

关键回执：Bilibili `platform-evidence-bilibili-a83da0a6-fe10-4611-b79a-09eb416f11e1.json`；X `platform-evidence-twitter-606c3c72-f4be-4d37-8d3d-d7aa9e90578a.json`；Reddit `platform-evidence-reddit-276eadcf-a626-4b5a-a387-a22c952201a4.json`；Weibo `platform-evidence-weibo-77030469-9fc0-4ab8-8c4b-c9a7c8d835dd.json`；Yahoo `content-diagnostic-yahoo-1792196555131.json`。均保存在 `.scratch`。

剩余 Instagram 缺真实粉丝量和原创正文，最新一次用户授权重试明确返回 HTTP 429 限流，保持停止。Facebook 保留折叠正文和近似数量的限制；LinkedIn 无动态不能推断不创作；Pinterest 转存不能作为原创证据，现已从原创 Pin 搜索取得内容来源。缺失粉丝量不填成 0。

TikTok 实例 eyeslipslace：来源视频 `7686252379590708511`，作者 `6765623526144017413`，真实主页粉丝量 6085；简介描述 bridal makeup 业务，搜索正文包含 matte eye 教程、技巧和产品说明。另一个作者主页读取也完成。回执 `.scratch/platform-evidence-tiktok-1b38cdf2-ffb6-4838-8721-366bf6c1aa8d.json`、`.scratch/platform-evidence-tiktok-3b17b8b4-f466-4cb2-bdf2-b7fd92c07fae.json`。后续不再为 TikTok 重复验收搜索或主页。

YouTube 恢复回执 `.scratch/platform-search-receipt-b5326753-fa4f-470b-ac87-8c016ed607a3.json`。其他搜索回执见 `.scratch/platform-optimization-resume-<platform>.log` 引用的 `platform-search-receipt-*.json`；新增原生读取为 `.scratch/platform-evidence-*.json`。均为实际调用，没有使用模拟结果。

## 批准计划与预算

计划 `mac-arm-all-platform-beauty-optimization-20261016-v3`，revision 1，批准时间戳 `1792193865`，状态 active。13 个来源各 6 组关键词；目标 130 人、最低粉丝量 2000、语言不限、公开邮箱软偏好保持有效。v3 预算 464 次；旧 v2 已消耗 56 次；合计总额度 520 次。最新消耗从 Campaign 真实回读取得，不按脚本启动次数估算。

当前 Campaign v3 已用 48 次，剩余 416 次；加旧 v2 共 104 次。独立原生诊断调用另外保存，不包含在 Campaign 的 104 次计数中。

## 交付边界

本轮为本地开发验收，未发布客户版本。许可全文仍缺 13 项，productionReady=false。尚缺证据的平台保留待验收状态。
