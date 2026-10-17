# Instagram 程序内网页读取

2026-10-17。范围：将已验证的正常网页读取方式加入 OneCrew，只读取粉丝量、简介、公开邮箱和一篇原创正文用于评分。

## 当前实现

- 搜索沿用已登录网页的关键词搜索和原生账号身份。主页使用搜索返回的用户名及账号标识绑定；私密账号跳过。
- 程序通过现有 OpenCLI 专用浏览器读取正常网页 DOM。Codex Computer Use 用于人工式网页验收，其工具本身没有嵌入客户端。
- 展开简介，提取其中唯一明确公开的邮箱。没有公开邮箱记录未知；不影响其他条件的资格判断。
- 读取一个可见的非置顶原创帖子，并核对正文所在容器的作者链接。支持桌面无标题结构、桌面标题结构和移动版正文展开。图片替代文字和推荐卡片不能充当正文。
- 不请求点赞、评论、互动或发布时间；语言判断复用已经保存的正文，不额外打开评论窗口。
- 精确粉丝存储整数；`13.7万粉丝` 等缩写保留原文和估计区间，初评、审查材料及复评一致使用区间。接近门槛时保留待核实。
- 页面登录、验证、风险提示和读取失败明确停止，不切换身份、不以其他数据源填补证据。

## 真实程序路径验收

使用 OpenCLI 1.8.8、原专用身份 `jga2zn4u` 和正式适配器安装器。原身份重新打开并验证后连接成功。

| 样本 | 程序读取结果 |
|---|---|
| js.makeup.events | 2061 粉丝；完整展开简介；未发现公开邮箱 |
| makeupbyalessandra_ | 页面约 13.7 万粉丝，整数保持未知；完整简介；公开邮箱 contact@makeupbyalessandra.com |
| js.makeup.events / Ddj0tw5GDBr | 完整原创正文，包含 Bridal hair & makeup team、Orange County / LA / San Diego 和预约信息；未截断 |

主页与正文通过实际 OpenCLI 取得。真实回执再通过正式主页合同、正文解码、内容保存校验、无评论语言证据校验及粉丝范围评分。没有创建虚假作者或发送外部消息。

本地回执：`.scratch/four-platform-instagram-web-b485b544-41f6-4811-a6ce-7ebc2fe2aba0.json`、`.scratch/four-platform-instagram-web-00f74864-e3b9-4422-920f-909c9b5ed2d8.json`、`.scratch/four-platform-instagram-content-fbfe7679-466b-4f3f-809e-8becd16f28ea.json`。回执及账号环境不上传 GitHub。

相关类型检查通过；资格、语言、网页读取合同 11 项，适配器安装与合同 6 项，已有 Instagram 适配器回归 22 项全部通过。真实资料另行通过程序读取及回执校验。仅验收本次 Instagram 路径，其他成功平台未重复访问。私密页本轮未新增真实样本，不宣称完成私密页实测。

## 安装范围

本次交付为当前 Mac ARM 本地候选版和 GitHub 源码同步，不切换客户下载或云端服务。Windows 实机验收仍待完成。

已构建并启动 `.scratch/mac-arm-candidate-24/onecrew_lite`，版本 0.7.1，程序 SHA-256 `afabbd3e927a988c358c6aae9015ee37a6155a47fed48cf8ccb59e2880b542ec`，payload SHA-256 `7d90829ee150e840cdef4e94579305c5c259c187b345865c0c63f98aafd699c3`。严格 codesign 校验通过，实际 SEA 冷热启动和许可专项共 4 项通过，没有跳过。

配套 Codex 插件已安装 `0.7.1+codex.61977a35e321612edd0bd3ef5795a64f1db4a46b48305136986e274ee7788717`，安装器返回 fresh 验证。实际服务实例 `b7d00be7-597d-4553-8bbd-f0153b822ef7`，端口 53400；HTTP 验收为 startup ready、Access signed_in、OpenCLI daemon_running 和 extension_connected，issue 为 null。通过服务正式连接操作恢复原绑定及身份 `jga2zn4u`，结果 completed。

本轮服务切换后，Codex 网页控制工具报告宿主授权不可用；工作台和资料库的重新打开请求已排队，尚未取得新版页面可见状态。此限制不影响已经独立完成的 HTTP、实际安装包及 OpenCLI 读取验收，不能把排队结果当成页面已恢复。

构建日志及验收日志位于 `.scratch/instagram-visible-candidate24-*`。candidate-23 保留作上一版恢复入口；账号数据和专用浏览器身份保持原目录。

交接须携带工作区 `AGENTS.md` 与 `docs/release/合并与部署流程.md`；该流程位于上一级 OneCrew 工作区。
