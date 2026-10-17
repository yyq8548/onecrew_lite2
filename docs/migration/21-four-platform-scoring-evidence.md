# 四个平台的搜索字段与评分证据

2026-10-16，本地开发验证。沿用已批准的美妆博主 Campaign、原浏览器身份 `jga2zn4u`，最低粉丝条件 2000。成功标准为真实来源足够支持当前条件判断，包括能够确定排除的作者；成功后停止重复验收。本文没有提交作者评分，也没有将平台验收成功计为合格作者。

## 实际搜索覆盖

| 平台 | 已实际执行的关键词 | 返回情况 | 搜索直接提供的有效字段 |
| --- | --- | --- | --- |
| Instagram | makeup artist；beauty blogger | 两次各 5 条 | 原生 author_id、username、显示名称、主页 URL |
| LinkedIn | beauty content writer；makeup blogger | 两次各 10 条 | 姓名、公开主页 URL、handle；同卡片职业描述的现有提取逻辑在这两次结果中仍未取得值 |
| Facebook | eye makeup tutorial；foundation application | 6 条；1 条 | 名称、主页 URL；数值主页地址包含可绑定 author_id，vanity 地址需要后续原生身份核实 |
| Pinterest 账号搜索 | makeup artist；beauty blogger | 两次各 20 条 | 原生 author_id、username、显示名称、主页 URL |
| Pinterest 原创 Pin 搜索 | eye makeup tutorial | 8 条 | Pin ID、原生作者 ID、username、作者名称、主页及 Pin 地址、标题和说明预览 |

以上数量为各次返回量，没有将不同查询的结果相加作为独立作者数。关键词命中或名称含有 beauty 只能初筛，不能单独证明持续创作能力。

## Instagram

已经取得 maribo003、delinamedhin 的原生身份绑定和主页预览。主页 DOM 读取的粉丝量为未知，简介为空。帖子地址可以作为下一步定位信息；返回的图片 alt 明确属于预览，不能作为已经核实的原创正文。

第二次搜索返回 eliirax、tssitlidze_sofia、beautybloggerinnalove 等真实账号，名称含 beauty blogger、UGC 等线索，可以用于选择需要进一步核实的作者。

对 delinamedhin 的最新一次用户授权重试明确返回 `Instagram HTTP 429 rate limit`。未取得新的粉丝量或正文，已停止该平台进一步读取，未更换身份或改用另一入口规避限制。真实回执 `.scratch/four-platform-instagram-native-2cb7a15b-6e4a-486c-966e-b1e2f901dcf5.json`，日志 `.scratch/instagram-retry-native.log`。

可用来评分的范围：账号身份和来源可核实；名称提供领域初筛线索。粉丝门槛、真实简介和原创正文仍未知，保留待核实。

## LinkedIn

主页实际提供原生 account_id、姓名、职业标题、简介和公开 Contact Info 入口。没有取得邮箱，不把 Contact Info 入口称为邮箱证据。简介标记为截断，可引用其中明确文字，不能当作完整履历。

新样本 Rachel Martino：职业标题为 “Fashion, Beauty and Lifestyle Blogger at RachMartino.com”，简介明确描述美妆、时尚、社交媒体和营销经历。真实页面的 Activity 区域显示 `734 followers`。此前读取器只在顶部资料区寻找数量，已补充读取该作者 Activity 区域的粉丝文字；同一区域数量冲突时明确失败。

该作者没有可读取的原创动态卡片，诊断明确 `listitems=0`、`posts=[]`。空动态不能证明不创作。当前 734 粉丝已足以判定低于 2000 门槛，因此无须继续读取其动态。

关键回执：`.scratch/four-platform-linkedin-profile-84769421-d461-4b53-8bdd-cc023d482929.json`、`.scratch/four-platform-linkedin-diagnostic-b360a6cc-8fe8-4656-8670-2d0219a2744c.json`、`.scratch/four-platform-linkedin-content-fdc47239-79c2-4477-bcfd-a56744b83437.json`。

修复后真实主页读取通过严格解码，返回 `follower_count=734`、`follower_text="734 followers"`；回执 `.scratch/four-platform-linkedin-profile-66c37590-d50d-4220-bd99-58a1429e21ce.json`。该平台评分证据验收成功，停止重复验证。

## Facebook

Andriiana 的原生身份 `61566847065209`、主页数量文字 `289K followers` 和原生 reel `1114808264370130` 已取得。正文明确讨论 contour、concealer 和 blush placement，可以支持美妆内容相关性。末尾 `… See more` 表示正文折叠，评分材料必须保留截断标记；旧回执也在材料生成时保守标记截断，原始回执保持原样。

已修复缩写数量未进入评分材料的问题。保留原始显示 `289K followers`，名义值约 289000，以保守范围 288000—290000 判断条件。最低 2000 可以确定满足；门槛落在范围内时保留未知，不能把名义值当作精确粉丝数。资料精确计数为空仍保持为空。

Jessica Creates 的实际精确粉丝数为 2，可以直接排除。新关键词返回 jessejaggers 的数值主页，不因平台已有足够验收材料继续读取该账号。

可用来评分的范围：原生身份、来源、规模门槛及已显示的美妆正文。互动率、受众地域、持续质量不从现有材料推断。

修复后的正式评分材料回读已验证估计范围和 `text_truncated=true`，没有重新访问 Facebook；回执 `.scratch/platform-evidence-facebook-e93f5978-71be-4095-b54a-35a4f66f9a29.json`。该平台评分证据验收成功，停止重复验证。

## Pinterest

账号搜索能找到相关用户名，但用户最近内容可能全是转存。Vivian 有 58313 粉丝、相关简介，其检查的 4 条均为转存；Krity 有 7554 粉丝，其检查的 5 条也均为转存。这些数量和简介有领域与规模价值，转存不能证明原创能力。

原创 Pin 搜索已真实返回 8 条，绑定 Pin 与原始作者，并保存标题和说明预览。程序默认创作者搜索已切换至现有 `search-pins` 入口，随后可以对具体 Pin 定点读取，不再默认从账号的转存列表寻找正文。原账号搜索入口仍可单独使用。

样本 anushyyykhan：原生作者 `1117878076165258711`，原创 Pin `1117877938791441396`，标题 “Brown & Gold Eye Makeup Tutorial | Soft Glam Look”，说明包含暖棕、金色眼影及适用场景。主页实际返回 12 粉丝，足以按 2000 门槛排除；不再为该作者追加正文读取。

关键回执：`.scratch/four-platform-pinterest-pins-0b309975-2d5a-410d-83fa-e16ae7879c32.json`、`.scratch/four-platform-pinterest-profile-d14a08c0-6430-4d3a-a365-41a279df80ef.json`。

切换默认入口后，在正式 Campaign 下一组已批准关键词 `skincare blogger` 上取得 10 条原创 Pin 结果，约 6.5 秒；每条包含内容来源及原生作者绑定。回执 `.scratch/platform-search-receipt-285c408d-5582-4558-8ee2-596325e1789f.json`。该平台评分证据验收成功，停止重复验证。

## 验证与停止边界

代码修复覆盖 Facebook 缩写数量保留、估计范围与审核提交、旧折叠正文的材料标记，LinkedIn Activity 粉丝提取，以及 Pinterest 默认原创 Pin 搜索。原有 9 个成功平台没有重新搜索或读取。独立诊断调用单独保存，不混入 Campaign 计数。

满足当前评分证据条件的平台停止验收；Instagram 的原生读取遇到风控，停止进一步尝试并保留未知。后续新增候选可以按业务计划评分，平台成功不代表每个候选都具备全部字段。

当前运行 candidate-17，实例 `aeffc01f-4a76-4b28-8b2b-b866b8c5592b`，端口 53400。工作台、资料库和原身份连接均已通过页面文字确认；原身份恢复回执 `.scratch/connection-repair-ce3232f3-cb4a-4530-b293-fbe230643339.json`。本次涉及的资格、搜索、审核材料与 LinkedIn 检查分别为 18 项和 26 项全部通过，应用类型检查、SEA 和签名检查通过。临时页面结构诊断命令已删除，真实回执保留。当前仅作本地开发验证，未发布客户版本。
