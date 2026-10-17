# 低命中平台查询优化

2026-10-17。范围：根据 Tesla、粉丝不少于 1000 的上一轮实测，改善查询规划与噪声识别。粉丝阈值、平台覆盖、读取预算和评分条件均由现有结构化条件继续执行。

## 程序修改

新增共享的平台检索指导 `onecrew_lite-app/src/discovery/query-retrieval-guidance.ts`，用于首次规划、独立审核和后续补词。

- 按实际入口区分账号搜索与内容搜索。Pinterest 当前搜索 Pin；Facebook 当前保留账号链接，长篇帖子查询容易缺少账号卡片。
- LinkedIn 根据领域采用评测、记者、编辑等发布角色，识别企业内部制作人员与公开内容作者之间的证据差异。
- 视频平台采用明确的使用、评测、比较或教程角度。X 的原创内容路线使用原生回复和转发排除语法，仍需检查正文是否只是转贴链接。
- Google 使用可核验的视频路径；Yahoo 使用站点域名和精确主题，避免照搬 Google 路径限制语法。网页引擎仅采用当前作者解析支持的 YouTube 视频、X 帖子和抖音视频路线；其他平台使用其原生入口。
- 低粉账号尽早结束；同作者复用已有证据。Reddit 的 karma、社区人数、点赞和其他热度不能替代粉丝数。连续低产出时更换检索角度，仍遵守已批准预算。
- 保留原始回执，区分无关结果、形态不适合、低粉、资料不足和技术错误，不以删掉原始结果制造高命中率。

## 真实对比

使用同一原专用身份、正式搜索与读取计划；每次搜索最多 5 条，按作者去重后最多读取 3 个。以下查询是按新指导选择的人工验收输入，用于验证检索策略；不代表每次模型都会生成相同查询。成功平台没有重复进行本轮之外的全平台验收。

| 平台 | 本轮查询与结果 | 达标和评分证据结论 |
| --- | --- | --- |
| Google | `site:youtube.com/watch Tesla review -inurl:playlist`，5/5 为具体视频，替代上轮 3/3 播放列表；读取 3/3 成功，均值 16.35 秒 | 2/3 超过 1000 订阅，CarExpert 约 68.7 万、Richard M. 约 1.5 万；有简介及评测说明，可初评；另一个账号仅 183 订阅 |
| Yahoo | `site:youtube.com "Tesla" review`，5/5 为相关视频；读取 3/3 成功，均值 9.07 秒 | 3/3 达标：Edmunds Cars 约 64.6 万、Ryan Shaw 约 34.7 万、Chasing Cars 约 24.7 万；简介和评测说明足够初评；未证明上一轮特定失败视频已修复 |
| TikTok | `Tesla review`，5 条均为汽车主题配文，读取 3/3 成功，均值 3.71 秒；本次前 5 条未见上一轮 AI 飞行汽车及餐馆段子 | 前 3 个中 1 个达标：Chrisncars 244,000 粉丝；其余 974、121；配文可初筛，不能代替视频逐字稿 |
| X / Twitter | `Tesla review -filter:replies -filter:retweets`，5 条，读取 3/3 成功，均值 2.90 秒 | 2/3 达粉丝门槛，仍有分享链接和未来评测预告，不能全部算原创深度体验作者 |
| LinkedIn | `Tesla content creator`，读取 3/3 成功，均值 3.82 秒；随后采用不同角色词 `Tesla journalist`，读取 2/3 成功 | 第一组仅 1 个达标，但为公司视频制作人员、正文为招聘；第二组仍偏员工资料，第三个资料结构校验失败即停止。没有验收为高命中 |
| Facebook | `Tesla Model Y review` 返回 0 个账号；随后短词 `Tesla reviews` 返回 2 个，读取 1/2 成功，均值 13.20 秒 | 成功账号可见约 3900 粉丝，但当次正文没有 Tesla 评分证据；另一个身份读取超时即停止。仍待改善 |
| Pinterest | `Tesla Model Y review` 返回 4 个汽车主题 Pin，3 个作者均低粉；不同角度 `Tesla accessories` 返回 2 个配件 Pin，作者均低粉 | 已减少上一轮的投资推广误命中，但没有取得达标候选，不计为符合条件的成功 |
| Reddit | `"Tesla" AND ("ownership experience" OR "long term review")` 返回车主体验帖子，读取 3/3 成功，均值 1.92 秒 | 内容角度更贴近使用体验，但 3 个作者均返回 0 粉丝；在 1000 粉丝条件下仍属低产出渠道，停止重复相似查询 |

Yahoo 曾试用 Google 的路径限制查询，返回了无关页面，因此最终指导明确区分两个引擎。所有中间尝试均保留在本地回执中；表内没有把某次好结果当成长期成功率。账号平均读取包含低粉早停和真实失败；Google 与 Yahoo 的时间变化不表示读取性能获得提升。

## 本机安装与验证

本轮类型检查通过；规划、补词、条件及网页来源回归 19 项，OpenCLI 合同及实际安装检查 5 项，最终候选程序的实际 SEA 冷热启动及许可检查 4 项通过，均未跳过。严格 codesign 校验通过。

实际调用 Codex 语义规划器完成需求理解、自动查询生成和独立审核。最终结果覆盖 13 个平台注册来源，`criteria.min_subscribers=1000`；Google、Yahoo 的全部查询均采用当前支持的 YouTube、X 或抖音路线。没有建立或批准虚假搜索任务。回执位于 `.scratch/tesla-query-planning/result.json` 及同目录阶段记录；独立验证进程采用私有文件权限，与本机服务的启动权限一致。

本机 Mac ARM 候选版：`.scratch/mac-arm-candidate-26/onecrew_lite`，版本 0.7.1。程序 SHA-256：`f6575fa5ede26ba3dae105b85f7bdb6d07dc1b11d0688101ab78b0ce00513136`；payload SHA-256：`4b25ab41d6112a366fb9c5d86dd6c778da5682b640e7cdd11da97755422e8621`。candidate-24 保留为上一版。

已切换本机服务到实例 `a03f64dc-61c4-4279-af74-426f4594a816`，端口 53400。HTTP 验收为 startup ready、Access signed_in、OpenCLI daemon_running、extension_connected，issue 为 null。原账号数据及浏览器身份保留。Codex 插件安装返回 fresh，版本 `0.7.1+codex.63af6c463e5820c66845643342be818124b9e788f70914a2f8dab62de4c83121`。最终安装与连接日志为 `.scratch/tesla-optimized-candidate26-plugin-install.log`、`.scratch/tesla-optimized-candidate26-connection.log`。

本轮为本机开发候选版与 GitHub 源码同步，不切换客户下载或云端服务；Windows 尚未实机验收。

回执目录：`.scratch/tesla-optimized-20261017/`、`.scratch/tesla-optimized-second-20261017/`。本地日志前缀 `.scratch/tesla-optimized-`。这些账号资料与环境日志不上传。
