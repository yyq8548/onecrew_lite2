# 全平台搜索及平台登录检查

2026-10-16，candidate-10 阶段的 TypeScript 开发验证记录；没有发布到客户下载入口。后续运行版本与搜索复验见 [全平台搜索优化与复验](20-platform-search-optimization.md)。

## 搜索记录

计划 `mac-arm-all-platform-beauty-acceptance-20261016-v2` 已经批准并执行。已计费搜索 34 次，随后读取抖音原生作者 2 次，总读取 36 次。新计划目前有 2 条待审核记录、0 位合格创作者。查询结果数量没有经过跨查询去重，也没有完成最低粉丝量和作者内容核验。

| 来源 | 各次搜索返回数量 | 当前结果 |
| --- | --- | --- |
| YouTube | 20、失败 | 第二次 `opencli_command_failed`，原因待定位 |
| TikTok | 20、20、20 | 搜索完成，仍需审核作者 |
| Instagram | 失败 | `opencli_timeout`；实际中文页面输入标签为“搜索输入” |
| X | 20、20、20 | 混入讨论及转发，仍需审核 |
| LinkedIn | 9、10、10 | 仍需读取人员资料 |
| Facebook | 4、0、3 | 返回数量偏低，混入地点及品牌页面 |
| Pinterest | 失败 | `opencli_invalid_output`；另已观察到未登录首页 |
| Reddit | 20、20、20 | 混入一般讨论，创作者筛选待改善 |
| Bilibili | 20、20、20 | 主题相关，仍需排除搬运和合集作者 |
| Douyin | 16、16、16 | 已通过原生读取补充两位作者身份 |
| Weibo | 20、20、20 | 混入转发和推广，仍需审核 |
| Google | 7、7、7 | 部分查询混入工具、目录及学校 |
| Yahoo | 7、7、7 | 首次查询工具结果较多，仍需审核 |

LinkedIn 首次参数超过来源上限，被调用前校验拒绝，没有计费；表中仅列随后三次有效调用。全部回执及停止状态保存在 `.scratch/platform-search-receipt-*.json`，汇总为 `.scratch/platform-acceptance-sample-results.json`。当前证据尚不足以确认全平台稳定或创作者命中率达标。

Instagram 查询输入检测已增加实际观察到的中文标签，并在真实页面确认输入就绪。完整搜索仍待重新验收，原超时检查点没有重置或重放。

## Pinterest 登录要求

按用户要求，Pinterest 进入必须登录的平台列表，包含人员、Pin 和画板搜索入口。工作台排序、状态显示和搜索前检查共用该规则；没有新鲜完整登录凭据时，不将平台显示为可直接搜索。

TypeScript 登录检测使用 `onecrew-pinterest-auth/whoami`，复用受管理浏览器身份。检测只接受当前账号的个人主页导航和账号菜单共同出现；登录入口和匿名首页返回未登录；信号缺失或冲突返回未知。Cookie 本身不能证明登录，不输出账号标识或 Cookie。

修正后的探针已在实际 Pinterest 首页返回 `authenticated: true`。页面同时具备本人资料导航和账号菜单，未显示登录入口。常驻 reCAPTCHA anchor 只作为页面标识处理；实际验证挑战仍停止检测。本轮没有重试业务搜索。

## YouTube 登录要求

按用户最新要求，YouTube 也进入必须登录的平台列表。工作台显示登录检测入口，搜索前要求新鲜、完整的账号登录验证；单独连接浏览器不会将该平台显示为就绪。

原生检测使用 `onecrew-youtube-auth/whoami`。固定 YouTube 首页上的当前访问者 `ytcfg.LOGGED_IN=true`、顶部账号按钮可见且登录入口不可见时，才返回已登录；明确匿名配置和登录入口共同出现时返回未登录；缺失或冲突返回未知。没有输出账号标识或 Cookie。探针已在原专用浏览器的真实页面返回 `authenticated: true`。

本次没有重新执行业务搜索，原搜索错误仍待独立验证。

## 本轮验证

- 登录探针契约和真实适配器隔离安装：2 项通过。
- 搜索登录要求测试：2 项通过。
- 应用、工作台类型检查及工作台构建通过。
- 原生探针两个文件的 JavaScript 语法检查通过。
- candidate-08 SEA 冷启动、再次启动、两个界面及真实数据库检查：1 项通过。
- Codex 真实安装、candidate-06 至 candidate-08 升级及上一插件恢复：3 项通过，0 项失败、0 项跳过。
- candidate-09 包含实际验证标识识别修正，同一 SEA 验收再次执行，1 项通过、0 项失败、0 项跳过。
- YouTube 登录要求更新后，登录探针及真实安装 2 项、工作台登录规则及连接页面 12 项通过；应用、工作台类型检查及工作台构建通过。
- candidate-10 同一 SEA 原生验收通过，1 项通过、0 项失败、0 项跳过。
- macOS ad-hoc 签名完整性检查通过。

当前候选 executable SHA-256：`ca25f3e3286722528d51a5e4611bbd44cec2555b4a7b802fe7e776735a81509d`；payload SHA-256：`4c36affdeaccfd47fdc8488fb245874b334ab3e4e12f1ce36608972da6aef8da`。许可全文仍缺 13 项，`productionReady=false`。

日志：`.scratch/pinterest-web-build.log`、`.scratch/pinterest-sea-08.log`、`.scratch/pinterest-sea-09.log`、`.scratch/pinterest-codex-upgrade-08.log`、`.scratch/youtube-login-web-build.log`、`.scratch/youtube-login-sea-10.log`。

本地当前运行 candidate-10，实例 `00559123-8b82-4883-900c-ea4742a1b3b2`，端口仍为 53400，账号数据目录保持不变。

最终工作台显示“本地服务已连接”和哈希前缀 `ca25f3e32867`。2026-10-16 本地时间 15:51 实际点击 Pinterest 登录检测，工作台显示“身份验证通过”。

candidate-10 的新版适配器已通过实际安装文件与源码逐字节核对。原浏览器身份 `jga2zn4u` 已恢复验证；15:59 实际点击 YouTube 登录检测，工作台显示“身份验证通过”。资料库会话已恢复。

工作台已显示 YouTube 和 Pinterest 登录检测入口。保留当前 candidate-10 和上一运行版本 candidate-09；已检查无打开文件和辅助脚本引用后删除 candidate-05、candidate-06、candidate-08 及未启用的 candidate-07，保留验收日志和全部账号数据。
