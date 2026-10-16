# 当前统一 HTTP 合同

本文件记录已实现的开发合同。服务仍返回 `production_ready: false`；完整旧操作目录、MCP 注册、统一网页和安装验收继续按迁移计划处理。

## 实例与身份

服务仅监听 `127.0.0.1`。私有 `server.json` 保存控制令牌、实例身份和固定端口。客户端读取后必须调用 `/_local/status` 比对实例身份，不能凭端口或文件存在报告在线。CLI 状态输出不含控制令牌。

管理接口使用 Bearer 控制令牌。浏览器使用独立会话：已经验证的账号生成 60 秒有效的一次性票据；网页通过同源 `POST /api/local/session` 兑换 HttpOnly、SameSite=Strict Cookie 和 CSRF。Cookie 名称包含绑定端口，多个数据目录的会话独立。

`GET /api/session` 从 Cookie 回读会话，返回原 `ok/result` 外层以及 `csrf`、`version`、`transport`、`protocol_version: 2`、`accountId` 和 `epoch`。版本 2 客户端必须随业务请求发送 `X-OneCrew-Account-Id`、`X-OneCrew-Auth-Epoch`；写请求还需要 `X-OneCrew-CSRF` 和相同 Origin。`GET /api/events` 使用 Cookie 中的账号上下文，适配 EventSource。

账号切换、重新授权或登出使旧 epoch 失效。操作执行前重新核对持久凭据与已验证身份。已经开始发送的回执仍写入原账号数据库。

未登录工作台使用独立的一次性 bootstrap 票据和二十分钟会话。握手返回 `protocol_version: 1, scope: bootstrap`，只允许九项启动状态、语言、组件检查及配对命令；业务命令、账号、表格和管理接口明确拒绝。只有实际发起 `creator_sync_pairing_begin` 的会话才绑定该 flow。真实 callback、enrollment 和身份验证完成后，`GET /api/session` 校验同一 flow 的私有完成证明及账号 epoch，再提升为版本 2；其他配对或账号变化不能被旧会话认领。

## 路由与结果

| 路由 | 已实现行为 |
| --- | --- |
| `GET /_local/status` | 回读实例、发现协议版本 1、开发状态和实际已注册操作名称 |
| `POST /_local/shutdown` | 返回 202 后停止接收工作，等待资源排空，再删除描述文件和释放实例锁 |
| `POST /_local/workspace` | 已登录时签发账号票据，未登录/配对中签发受限 bootstrap 票据；返回同一实例 URL，不启动系统浏览器 |
| `GET /_local/library/status`、`POST /_local/library/workspace` | 核验真实 Library 资源及其摘要，签发同一服务的资料库页面票据；资源缺失返回 503 |
| `GET /_local/service/status` | 原完整状态合同；安装、组件和数据检查缺少真实证据时明确显示不可用 |
| `GET /_local/access/status` | 回读主服务 Access 状态；尚未登录时可用 |
| `POST /_local/access/login`、`cancel`、`logout` | 主服务唯一配对、取消及退出入口，使用原 Access 地址和 PKCE |
| `POST /_local/mcp/session`、`session/close` | 创建或关闭绑定账号 epoch、独立来源及原七种 surface 的适配器会话 |
| `POST /_local/mcp/review/prepare` | 当前单个 Creator 批量创建、候选保存、Choice 或 SearchPlan 的密封准备入口 |
| `POST /_local/mcp/result/present`、`result/state` | 同一会话呈现、确认、取消、编辑和 continuation；不能自行指定 origin |
| `POST /api/operations/:name` | 返回操作结果本身；失败返回 `error.code` |
| `POST /api/commands/:name` | 保留 `ok/result` 成功外层；操作异常使用 `ok: false, error: code` |
| `/api/v1/dynamic-tables` 及其子路径 | 在同一服务进程内执行原 Library 表格路由，使用主服务验证的 tenant/base |
| `GET /api/events` | 原 `runtime://changed` 数据外层；新连接以 sequence=0、scopes=[all] 请求重新读取当前状态 |

退役名称保留明确拒绝，状态为 HTTP 410、错误码 `operation.retired`。未注册名称返回 HTTP 404。账号变化返回 409，身份缺失/撤销返回 401，远端身份验证不可用返回 503。浏览器 Origin 不符合要求返回 403，输入格式错误返回 400，请求过大返回 413。版本 2 客户端需要读取这些 HTTP 状态，旧客户端最低兼容版本仍待 M11/M14 验证。

普通命令限制为 1 MiB，表格 JSON 为 2 MiB，表格导入为 10 MiB 加 64 KiB。表格路径只接受主服务验证后的账号归属，持续事件流也随账号 epoch 取消。

变更通知只包含顺序和集合名称，不承载数据、凭据、指令或持久成功回执。单个订阅缓冲为 256；订阅者落后时关闭连接，重连后重新读取。Library 持久同步与任务恢复使用各自仓储，不能以此通知代替。

## MCP 适配器

`mcp --data-dir <私有目录>` 使用固定版本 MCP SDK 的 stdio transport，仅连接已核验实例。五个启动工具、按实际已注册处理器过滤的原七种 surface 目录、工具搜索、合同查询、最多八调用执行及结果工具已接通。Creator、Choice、SearchPlan 使用密封授权及逐步确认；取消保留已完成步骤。尚未迁移的处理器不会出现在可执行目录中，账户连接清单需要实际读取器。MCP 不提供邮件或社交发送入口。

搜索保留原英语分词、词干、停用词、字段权重、召回阈值和工作流依赖规则。原 Rust 129 项评估查询的目标召回与首项均为 129/129；完整顺序一致为 81/129。MiniSearch 的字段长度与 Tantivy 的压缩 fieldnorm 存在差异，尚不声明所有结果排序相同。

ClientAdapter 已提供 Codex 检测、配置、内容摘要版本包、安装、状态回读和仅卸载自身注册。Windows 实际 Codex CLI 在隔离 `CODEX_HOME` 完成真实 SEA 包安装、缓存摘要回读、刷新状态和卸载测试，并保留无关 TOML 设置。生产服务在 SEA 中使用实际运行版本和二进制摘要报告插件状态；源码运行明确显示不可用。该测试没有修改正式 Codex 配置，也没有证明宿主已经挂载新插件。

MCP Apps 资源提供实际构建的操作卡片，URI 包含内容 SHA256 的前 16 个十六进制字符；符合旧 URI 形状的过期卡片只返回停用提示。资源读取拒绝其他路径。SDK 与真实统一服务的进程内传输和实际 stdio 子进程测试已通过；Codex 宿主实际挂载尚未验收。

完整 u64 在共享合同中保留数字语义。输出使用精确 `content[0].text`，卡片初始元数据使用 `onecrew/result-json`。普通 Host JSON 无法无损表示时，`structuredContent` 使用严格的 `{application_json: string}` 整体封装，同时标记 `onecrew/lossless-required: true`；不提供会舍入的对象镜像。MCP SDK 在工具声明了 `outputSchema` 时要求 `structuredContent`，因此公开工具 schema 明确包含该封装分支。普通对象分支只接受安全整数；这属于 SDK JSON 传输约束，原 Rust 业务合同的 u64 范围保持不变。

适配器发送封装前，使用成熟 AJV 2020 校验工具输出及每项已完成操作的当前输出合同。全部批量调用在第一步执行前逐项校验当前输入合同，再执行各业务的严格输入和授权检查。原 Rust 的 `uint8/16/32/64`、`uint`、`int32/64` 数值格式使用精确整数规则，完整 bigint 同样检查实际上下界；其他 JSON Schema 结构由 AJV 校验。不能用任意字符串封装替代该校验。输入通过整个 `application_json` 文本保留精度，拒绝重复字段、无效 Unicode 和已经舍入的整数。浏览器从共享 lossless/Zod 边界解码。

原注册器快照和摘要保留用于历史验证。当前 X、Instagram、Telegram 连接合同移除 X Chat 密码、偏好及状态字段；发生变化的输入与输出分别计算新 revision，合同查询同步返回当前 schema。未改变的业务合同继续使用原 revision。X Chat 设置、解密和发送均不注册。

候选运行可在安装者仍持有实例锁时启动真实 HTTP、Vault、Creator 和 PGlite，并核验网页内容及实际资源路径。候选运行不会写正式发现文件或切换当前 generation。当前构建身份明确标记 `source_development`，不代表 SEA 发行验收。

## 当前验证范围

真实本地 HTTP 已验证管理鉴权、同源要求、一次性票据、重放拒绝、会话回读、CSRF、账号切换、退役命令和命令结果外层。真实 PGlite 已完成独立租户隔离及统一服务启动关闭验证。

真实账号的浏览器登录、表格端到端读写、MCP 宿主接入和安装后的三个界面回读尚未完成。上述验证不替代这些验收。
