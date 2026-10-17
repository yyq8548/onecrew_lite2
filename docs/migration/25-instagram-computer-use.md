# Instagram 网页读取验收与后续方式

2026-10-17，用户明确要求 Instagram 后续使用 Computer Use。使用 Codex 内置浏览器现有 Instagram 登录页，通过可见搜索框、主页和帖子详情读取。未调用资料接口、隐藏页面状态或网络接口；没有关注、发消息、保存 Creator 或发送邮件。

## 后续执行方式

Instagram 的模型辅助搜索和补证优先使用 Computer Use。输入具体创作者职业关键词，等待实际搜索结果；通过返回的真实账号链接打开主页。简介折叠时点击“更多”，帖子必须打开详情核实作者、正文和时间。明确私密账号直接跳过；登录、验证或页面限流时停止，不推断成功。粉丝缩写保留约数，未公布邮箱、隐藏互动量及未加载数据保持未知。

这是 Codex 网页操作路径的真实验收，不代表 OneCrew 应用内的 OpenCLI 自动读取已经改用 Computer Use。资料 API 仍有 HTTP 429，不能将网页成功写成 API 恢复。

## 实测证据

网页搜索 `bridal makeup artist` 返回 beautyby_kk、makeupbyalessandra_、zaynura.salon、rawans_makeup、js.makeup.events。makeupbyalessandra_ 的搜索卡直接显示约 13.7 万粉丝；其余结果未显示粉丝，不能补成零。

| 字段 | makeupbyalessandra_ | beautyby_kk |
| --- | --- | --- |
| 主页 | https://www.instagram.com/makeupbyalessandra_/ | https://www.instagram.com/beautyby_kk/ |
| 粉丝 | 页面显示 13.7 万，为约数 | 2827，页面整数 |
| 帖子数 | 1204 | 4140 |
| 关注数 | 2843 | 2619 |
| 简介 | Luxury Destination weddings；Your Big Sister in Makeup | Karina Belisario；SoCal Artist, Moreno valley；Licensed hairstylist, Certified Makeup Artist；For availability/bookings/rates please hit that link（已展开） |
| 公开邮箱 | contact@makeupbyalessandra.com | 已读简介未公布，不推断其他页面没有 |
| 内容领域 | 原创帖子详情列出化妆产品与色号 | 主页有婚礼妆发作品及作者自述，仍属主页预览 |

原创帖子详情 https://www.instagram.com/p/DefeF4fSnjZ/ 明确显示作者 makeupbyalessandra_、完整正文“The usual suspects”及 PRODUCTS 产品清单、468 次赞。正文包含 CC Cream - Medium Neutral、All Hours Concealer - LC2、Bronzing Balm - Leo 等具体产品。页面可见时间为 2026 年 10 月 14 日，作者正文时间元素 datetime 为 `2026-10-14T22:38:38.000Z`；已区分评论自己的时间。返回主页后该帖子卡显示互动数字 468、18，18 为卡片评论数，未把实际加载评论条数当成总评论量。

## 结论与停止边界

Computer Use 的关键词搜索、两个公开主页和一个真实原创帖子详情均读取成功。已取得足够证据支持样本的最低 2000 粉丝及美妆领域初步资格判断，Instagram 网页人工辅助读取验收通过，停止重复验收。不是所有账号都提供邮箱、精确粉丝和全部互动量；没有证明历史内容全为原创或满足未核实的其他条件，没有提交正式评分。
