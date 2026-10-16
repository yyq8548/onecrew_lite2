# 开发执行记录

更新日期：2026-10-16。当前配套包为 candidate-52，最终主程序 545 项全部通过、0 失败、0 跳过，类型检查通过。正式插件安装和 Codex 完整重启后的 live MCP、工作台与当前表格回读一致；宿主状态卡视觉、Mac ARM / Intel 及发布门槛未完成。已验证计划修改、真实只读搜索和 Gmail 发件人门禁，没有发送邮件或消息。产品范围见 `08-current-scope.md`，上游迁入见 `12-upstream-sync-20261016.md`，精确结果见 `11-windows-verification.md`，客户安装约束见 `13-customer-installation-paths.md`。源码交付后按用户要求暂停，当前交接和下一步见 `14-handoff-status-and-next-plan.md`。下方为各阶段记录，以当前验收结果为准。

## 当前实现

- 新增 `onecrew_lite-app`，与工作台、MCP UI、Library、Teable 和浏览器共享合同使用根目录 pnpm 工作区及锁文件，固定 Node 26.7.0、pnpm 10.32.1。原网页直接依赖按原 npm 锁文件固定；TypeScript 版本按组件保留。工作区包仍为 private 0.0.0，构建产品版本为 `0.7.1`，尚未发布。
- 原生 Windows DPAPI、wreq-js 原生网络已实际执行。网络探针返回 HTTP/2，尚未完成真实 Chrome 身份恢复与浏览器指纹逐项比较。
- 新 Hono loopback HTTP 入口执行 Host、Origin、Bearer 校验和输入大小限制；HTTP 与兼容命令共用操作注册器。尚未注册的业务返回失败，不报告迁移完成。
- 实例锁使用操作系统原生文件锁。真实两个进程竞争与强制终止后的恢复通过；端口占用明确失败。私有 profile 保留已验证旧端口，拒绝冲突来源。
- 主服务取得实例锁后才执行资源装配；关闭先停止任务，再等待 HTTP 和原账号回执资源排空，最后释放实例锁。真实排空期间第二个 owner 拒绝启动，且不会执行其资源初始化。
- 新 CLI 的 start/serve/status/stop/workspace/mcp 已实现；workspace 返回一次性内置浏览器地址，不启动系统浏览器。真实 Windows 独立进程、重复启动复用、身份回读、强制终止恢复及排空关闭测试通过。MCP SDK 的五个启动工具、完整保留目录和实际卡片资源已通过真实 stdio 子进程验证。
- 浏览器使用一次性登录票据与独立 HttpOnly/SameSite 会话，写请求校验 Origin、CSRF 和账号 epoch。本地真实 HTTP 已验证票据重放、缺少 CSRF、跨来源、跨账号及切换后旧会话拒绝；管理接口继续要求服务控制令牌。
- 统一账号上下文保存 tenant、base、设备来源及授权 epoch；账号切换取消旧读取，旧 MCP 会话失效，已开始发送的回执归属独立保留。旧 Vault 凭据读取、Access 在线验证客户端和身份服务已编写，真实账号与当前公司服务兼容尚未验收。
- 账户数据库要求服务实例锁、私有目录、明确的账号/tenant/base 映射和完整 schema90 核验；批量邮件、邮箱映射和历史使用同一连接。真实 Windows 双账号隔离、并发连接复用、旧会话拒绝及切回账号测试通过。旧库归属准备及预先验证的 adoption 已实现；缺少映射明确失败，尚未迁移用户生产数据。
- GmailBulk 输入、指纹、状态及恢复规则已移植；SQLite 仓储使用原有表与原始 SQL 验证并发领取、跨批去重和永久尝试标记。整个 Creator schema90 已与 Rust 原构造器比较；27–89 所有原始版本逐项升级至 90 并核验通过。
- Vault v2/v3 隔离副本升级至 v4、完整冻结结构核验、AES-GCM 解密、私有密钥文件读取、SQLite 完整性检查和安全备份已实现。Windows 备份创建时设置私有 DACL，并回读目录和文件权限；不接管已有开放目录。
- Gmail MIME 使用 MailComposer 编译、MailParser 验证；Gmail 传输区分拒绝、未知、已接受待回读和已回读。Access 邮箱授权适配、worker、历史和本地线程维护已接入统一业务模块。邮件页确认或恢复后由服务持有任务，保持原两秒领取间隔；草稿、暂停、取消、失败控制及过期账号不触发执行，真实账号数据库测试通过。原 manual send 成功历史事务、永久发送标记及晚到回执恢复通过真实 SQLite 测试；实际提供方发送验收未完成。
- Creator CRUD、分组、阶段、归档、置顶、联系路由、删除影响、XLSX 导入导出、批量创建及候选保存已移植。完整 Creator 回归 52 项通过；原 Rust 导入和候选保存 oracle 验证 payload、指纹、收据和重放一致。普通 Creator 与候选按原合同保留完整整数精度；transfer bundle 和 XLSX 保留原 Rust 明确的安全整数上限，超限拒绝，不舍入原数据。
- Creator 三表及 Library 推荐 XLSX 的导入、导出、预览确认和桌面导出已实现；真实工作簿和 SQLite 测试通过。原二进制上传路径 `/api/creator-import`、`x-onecrew-file-name` 解码和 32 MiB 限制已接入 HTTP。批量创建接入 MCP 逐项密封确认，保留已完成收据，后续取消不回滚已完成创建。
- SQLite 恢复准备及 staging 使用实例锁、私有目录、真实备份和原子日志，比较完整整数、REAL 二进制、原始文本字节、空值、重复行、结构及版本；staging 验证全部 Vault 密文及密钥。Library 在原生锁内复制完整 PGlite 目录，验证源、恢复副本和独立 staging 的全部文件；只在 staging 读取账户。真实 Windows 测试通过，所有恢复记录明确保持未激活。
- Access 已实现原固定配对地址、PKCE、本地回调、取消、退出、私有 Vault 保存及统一账户 owner。服务控制接口可在尚未登录时发起配对；新安装仅初始化两个同时缺失的 Vault 文件。现有用户身份的迁移选择、来源指纹 CAS 和真实远端证明已实现；实际公司服务正面兼容验收未完成。
- Creator 密封确认、候选路由选择、逐步确认、已完成历史、取消、过期及 continuation 已实现；Choice 的私有单次 grant、答案排序、容量和账号隔离验证通过。MCP 目录、最多八调用的输入预检与顺序执行已接入真实账号会话协调器，搜索确认精确绑定 typed operation、请求、账号和 scope；仅列出已注册的实际业务操作。真实 SQLite、官方 SDK 和 HTTP 已验证确认、后续展示、取消和中途停止。
- Library 使用进程内表格入口和主服务的登录身份、HttpOnly 会话、CSRF及账号 epoch。实际统一前端构建、同一 HTTP 监听器的 HTML/JS/HEAD、来源拒绝及私有就绪状态通过验证。原 Rust 身份合并和联系信息 SQLite 数据已写入真实 PGlite，分页、原表格字段、共同来源/身份版本、重复同步的表格及记录编号、身份发布回读和旧账号拒绝通过测试；真实用户登录后的宿主界面验收尚未完成。
- 浏览器共享合同使用独立 `@onecrew/runtime-contracts`，严格 JSON 拒绝重复字段、无效 Unicode 和已舍入整数。工作台 823 项、MCP UI 406 项回归通过，并完成类型检查及构建；这些界面回归不能代替宿主实际挂载验收。
- Discovery 已实现原审核材料、资格规则、身份 ledger、分组审计、联系信息、Library 来源及联合版本 CAS。真实原 Rust SQLite oracle、双连接竞争、幂等重放、整事务回滚、任务调度、暂停恢复和 Library 发布已验证。实际 Codex CLI 两次结构化规划调用已通过，未执行平台搜索或发送。
- 语言证据补读、Web 来源的 YouTube/X/抖音原生核验和查询轮次已接实际账号仓储。原 Rust 冻结 SQLite 校验语言三阶段、六组 Web 来源和查询追加逐字段一致，包含完整文件 SHA-256、预算、永久尝试、来源变更与事务回滚。查询扩展另通过实际 Codex CLI 生成七条 YouTube 新查询，保留原批准身份；未执行平台搜索、对外发送或修改原数据库。
- 工作台活动、历史、心跳和执行指标读取实际 Discovery 仓储与同一任务监督器。MCP 卡片登记使用服务端验证的实际会话，逐次检查当前账号；真实官方 SDK、HTTP 和 stdio 测试通过。
- Windows 旧服务控制已验证真实原 CLI、原计划任务停用与 XML 备份、TCP 停机、历史保留、旧 origin 派生及原生锁。旧文件和目录仅按原 ACL 合同只读核验，私有副本继续使用新严格权限。候选服务可加载实际 Vault、Creator、PGlite 和工作台进行健康回读，来源变化、伪造授权和过期证明明确拒绝；真实远端证明后的生产激活尚未执行。
- 动态表格同步已实现固定端点传输、账号私有队列、完整整数持久化、检查点 CAS、实际 Library JSONL 捕获及分片/清单完整性校验。worker和原网页三项操作已接入统一服务，与 Discovery 使用同一准入锁和实际任务监督器；旧队列迁移保留完整原字节、未知结果及分账号归属并绑定候选指纹。传输合同、本地持久化及隔离分区已验证；真实远端上传及完整 Discovery 接入后的联合恢复验收尚未完成。原生产 Creator 云快照上传继续保持 `Retired`。
- ServiceShell 已接实际配对、账号证据、会话、数据状态与构建哈希。数据检查只读实际 Vault、账号清单和 SQLite 结构，需要迁移的旧数据明确受阻。未取得的更新、活动或组件证据明确显示 unavailable。
- Codex ClientAdapter 已在隔离 `CODEX_HOME` 使用真实 Codex CLI 完成 marketplace/plugin 安装、完整缓存哈希回读、状态检查和定向卸载，保留无关配置。SEA 服务已接状态检查；真实 SDK stdio 进程退出会撤销其控制会话。正式宿主未安装此候选，状态卡与动态表格的宿主验收仍未完成。
- 最终 Windows SEA 候选已从冻结源码完整构建，冷缓存提取、完整文件哈希、重复启动、同端口重启、SQLite/Vault/PGlite、Telegram Wasm、两套实际网页资源和 stdio MCP 的 2 项实际包验收通过；隔离 Codex 安装、核验和卸载的 1 项实际验收通过，均无跳过。候选包、精确哈希和验收边界见 `11-windows-verification.md`。16 项授权文本缺失继续保留发布门槛，未伪造许可文本。

## 已执行验证

新应用运行 `tsc --noEmit` 和 Node 原生测试。验证包含真实 SQLite/WAL、Windows DPAPI、Windows ACL/Junction、真实本地 HTTP、真实进程文件锁；没有发送真实邮件，也没有读取用户生产数据库。

测试覆盖和生产验收分别记录。单元测试、子 schema、随机加密字节与只读公共网络探针不能代替真实账号、整个旧数据副本、Codex 宿主或安装验收。

## 尚未完成的门槛

保留操作已完成逐项注册；实际服务在启动时检查完整目录，遗漏 handler 明确失败。当前输入与输出合同已编译验证，X Chat 字段从当前合同移除，原始合同快照保留用于溯源。源码冻结后的真实 SQLite/PGlite 激活候选验证 12 项通过。真实 YouTube 搜索、作者证据、Codex 评分、Library 发布与重启恢复通过：20 条来源、1 名合格作者、0 次读取错误。远端上传等待协议同意，其他平台实际流程仍需验收。Creator 生产云快照上传保持退役状态。

专用 Chrome 的官方扩展安装、签名绑定和重启后实际挑战验证通过。Telegram 属于用户要求删除的范围，不作为保留功能验收。Mac ARM、Mac Intel 原生验收保持未完成。

当前 Windows 候选 candidate-35 使用产品版本 `0.7.1`，manifest 保留候选状态、源码哈希、dirty 状态和 `productionReady: false`。配套插件已安装并完成 fresh 回读；宿主重启后的正式 live MCP 精确程序哈希匹配，Access signed_in、数据 ready。工作台、动态表格与最新卡片返回数据均回读同一已完成计划，数据未过期。用户已展开卡片，宿主自动化接口未暴露其页面，卡片视觉检查仍未完成。对外发送仅保留邮件页 Gmail 批量发送，未发送真实邮件。生产升级、线上发布及旧版退役尚未执行。
