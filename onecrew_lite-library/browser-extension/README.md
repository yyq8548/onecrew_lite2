# OneCrew 浏览器扩展

需要 Chrome 120+ 和已登录 OneCrew 的本地服务。扩展没有单独的登录流程，不需要安装 OpenCLI。界面使用 Chrome 原生 i18n（English 与简体中文），支持在弹窗内切换语言。

1. 用安装后的 `onecrew start` 启动服务（源码开发使用 `make start`）。
2. Chrome 打开 `chrome://extensions`，启用开发者模式，选择“加载已解压的扩展程序”。
3. 选择项目内的 `browser-extension` 目录。
4. 打开 `onecrew status` 显示的地址，完成 OneCrew 登录后进入 `/connections`。扩展会自动发现 8091–8110 端口中的服务。
5. 点击 Chrome 的 OneCrew 扩展图标查看状态；服务重启后扩展会自动恢复。
6. 通过 MCP 的
   `platform_search_create` → `platform_search_status` → `platform_search_fetch` 使用。

扩展只处理它创建的 YouTube
标签页。查询完成后关闭搜索标签页。YouTube 的登录、同意页或验证需要用户在浏览器
中处理。YouTube 登录由浏览器保留。

本版按视频内容寻找创作者：视频搜索 → 作者 channelId 去重 → 频道资料及 Videos 页近期内容。返回的视频证据供调用方 AI 评价，不提供私信、全球订阅数榜单或自动补全邮箱。

搜索后端独立保存任务结果；默认流程由 agent 继续调用表格工具，创建独立的三列结果表（创作者、频道链接、匹配评价）。用户明确要求仅预览时跳过保存。原始 Cookie 不发送到 OneCrew 服务，仅在 YouTube 页面中用于请求签名。

自动测试：`node --test browser-extension/tests/*.test.js`。
测试使用固定响应验证执行逻辑；真实 YouTube 搜索需要加载扩展后验收。

## 目录职责

- `background.js`：服务连接、命令收发与结果回传。
- `browser.js`：扩展拥有的标签页与快照。
- `discovery.js`：本地服务发现。
- `platforms/index.js`：平台注册和操作路由。
- `platforms/youtube/`：YouTube 视频搜索、作者去重、Cookie 签名、分页及频道 Videos 内容读取；`utils.js`、`search.js`、`video.js`、`channel.js` 为 Apache-2.0 来源适配，`index.js` 负责 OneCrew 流程编排。
- `_locales/`：Chrome 原生翻译目录；`bridge*` 为桥接错误，`youtube*` 为平台错误。

新增平台时增加独立适配器、注册操作并配置所需域名权限，无需修改桥接收发流程。目前只启用 YouTube，通用桥接不意味着开放任意网页执行。
