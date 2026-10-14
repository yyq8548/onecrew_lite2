# OneCrew 数据表定义

本文件为数据结构设计说明；当前结构以数据库迁移和源码为准，接口参数由 Zod 定义。

## 1. 存储边界与当前现状

本地使用一个 PGlite 实例。Teable 管理动态表格元数据和实际记录；OneCrew 固定业务表管理搜索、邮件和租户归属。不得用动态表格存邮箱凭据、队列或模板。

当前代码证据：src/database.ts、src/database-schema.ts、src/bootstrap.ts、src/modules/tables/data.ts、src/modules/search/service.ts、src/modules/access/store.ts。本次未扫描运行数据库 catalog，因此以下是代码声明的结构，不是完整数据库反向工程。

| 当前对象 | 实际用途 | 状态 |
| --- | --- | --- |
| Teable space / base | 每个个人租户按需创建独立 space/base，不创建 collaborator 或演示用户 | 已有；产品暂不提供团队工作区 |
| Teable 表格、字段元数据及记录物理表 | 由 Teable v2 命令/查询管理 | 已有；不复制维护其内部 DDL |
| onecrew_field_types | 对外语义类型映射 | 已有 |
| onecrew_search_jobs | 搜索任务与结果 | 已有，tenant_id 非空并贯穿查询与后台任务 |
| onecrew_access_accounts | 本机 Access 账号及登录凭据 | 本机登录凭据，与 Teable 共用 PGlite；业务隔离另由 onecrew_tenants 管理 |
| onecrew_tenants | 已验证用户与个人租户、Teable space/base 的归属映射 | 本轮实现 |
| 邮件业务表 | 下面定义 | 待实现 |

### 初始化和连接生命周期

- `src/database.ts` 管理一个 PGlite 实例和 Kysely 连接，调用原生 Teable repository/core/import 注册函数；正式运行不依赖 `container-node-test`。
- `src/database-schema.ts` 提供 Kysely `Migrator` 的静态 `001_tenant_initial` migration。Teable 表由上游 `ensureV1MetaSchema`、`installUndoCaptureGlobals` 安装；四张 OneCrew 固定表在同一个 baseline 内创建。不在各业务模块启动时反复执行 DDL。
- Kysely 自动维护 `kysely_migration`（name、timestamp）和 `kysely_migration_lock`（id、is_locked）。这些是迁移工具的内部表，不是租户或业务表。
- 首次初始化不创建任何租户或默认 base。收到已认证请求时，在同一事务内创建个人租户和对应 Teable space/base，`created_by` 为真实用户 ID；并发首次请求通过 owner_user_id 唯一约束收敛到同一租户。再次登录或重启复用该用户的映射。
- 表格读写使用已登录 Access 用户的 ID 和邮箱作为 Teable 执行上下文；不使用 `onecrew-demo` 或固定演示身份。获取登录身份在进入数据库事务之前完成。
- 通过 `kysely-pglite-dialect` 管理单连接事务。本地数据库只保留新的 baseline；当前旧数据为可清除的测试数据，不增加旧 schema 兼容或数据迁移分支。
- macOS/Linux 权限在 PGlite 初始化之后设为 0700，避免初始化过程重设权限；关闭时先完成已调度事件，再释放依赖容器和数据库。

### onecrew_field_types（当前）

| 字段 | SQL 类型 | 约束 |
| --- | --- | --- |
| table_id | text | 非空，联合主键 |
| field_id | text | 非空，联合主键 |
| data_type | text | 非空 |

people、company、score 当前在 Teable singleLineText 中保存序列化 JSON，OneCrew 负责按 Zod 校验和编解码；并不是 Teable 自带的人物/评分字段。其余映射见 data.ts。表格记录继续通过 Teable API 操作。

### 动态表格容量与事务（已实现并通过回归）

每张表最多 10,000 行、100 列，统一常量为 `packages/contracts/src/tables.ts` 的 `TABLE_CAPACITY`。Zod 限制单次请求；`database.ts` 将累计行列容量传给 Teable 的 `tableDataSafetyLimits`。网页、HTTP、MCP、文件导入均受写入限制；导出读取全部记录，不另设 10,000 行截断。

新增记录批次在同一事务中完成。Teable 返回的创建记录数若少于请求数，整个批次回滚，避免只保存部分记录却返回成功。新增多列也在一个事务内完成。

Teable 表格写操作还包含提交后将表结构标记为 ready 的步骤，因此写入并发控制覆盖整个公共写操作，不能只覆盖 BEGIN/COMMIT。使用现成 p-limit 将同一本地实例的表格写操作依次执行；读查询仍直接交给 Teable/Kysely。容量、回滚和并发边界由 `tests/integration/table-files.ts` 验证。

### onecrew_search_jobs（当前）

| 字段 | SQL 类型 | 约束 |
| --- | --- | --- |
| id | text | 主键 |
| tenant_id | text | 非空，无 FK |
| platform / query / status | text | 非空 |
| limit | integer | 非空 |
| error / result_json | text | 可空 |
| created_at / updated_at | text | 非空，当前为 ISO 时间文本 |

索引 `(tenant_id, created_at)`。读取状态/结果均使用 `tenant_id + id`；后台执行捕获创建时 tenant_id 和浏览器连接，账号切换不会改变归属。重启将 queued/running 标记 failed，不自动认领或迁移旧测试数据。

### onecrew_access_accounts（本轮）

使用现有 PGlite 的固定业务表，不新建数据库实例、不使用 Teable 动态表格保存凭据，不改 Teable 上游表结构。

| 字段 | SQL 类型 | 约束与含义 |
| --- | --- | --- |
| access_user_id | text | 主键；远端验证后的 `installation.access_user_id`，不接受客户端自由指定 |
| credential | jsonb | 可空；明文保存由 `credentialSchema` 校验的完整 Credential |
| is_active | boolean | 非空，默认 false；当前本机 profile 正在使用的账号 |
| created_at | text | 非空；首次保存账号的 ISO 时间文本 |
| updated_at | text | 非空；最近保存或退出的 ISO 时间文本 |

约束：主键保证同一 Access 用户只有一行；`is_active = true` 的部分唯一索引保证同一个 profile 最多一个激活账号；CHECK 保证激活账号的 credential 非 SQL NULL。不设 FK，不添加 version。

`credential` 包含 `bearer` 和远端返回的 `installation`：安装 ID、凭据 ID、Access 用户/设备/配对 ID、源实例 ID、平台、应用版本、状态、修订号及时间。这里的 `installation.revision` 是既有远端响应字段，不是新增本地乐观锁。加载和保存都按现有 Zod 合同验证，加载时再检查嵌入的 Access 用户 ID 与行主键相同。邮箱由 `/me` 获取供账号 UI 显示，本表不保存用户密码或头像。

操作语义：

- 保存登录：验证凭据；在同一事务中取消旧账号的激活状态，并按 Access 用户 ID upsert 新凭据和激活状态。失败时整体回滚。
- 恢复登录：只读取激活账号；结构错误或用户 ID 不匹配则报错，不跳过坏数据再使用其他账号。远端状态校验继续由 AccessService 执行。
- 切换账号：保存新账号的凭据并激活；其他账号行保留，但不自动启用其旧凭据。
- 退出登录：仅将当前激活行的 credential 置 NULL、is_active 置 false；保留账号行和所有业务数据。不代表远端凭据撤销，不清除其他账号行。

存储与隔离边界：

- 不使用系统钥匙串、不做应用层加密、不生成 `access.key`，不迁移或读取旧凭据存储。安装新版后重新登录。
- macOS/Linux 的 PGlite 目录权限为 0700；Windows 使用当前用户 LocalAppData 的继承 ACL。自定义数据目录须由用户选择私人目录，不能以 Node chmod 冒充 Windows ACL 隔离。
- 能读取数据库或完整数据库备份的人可以读取凭据。Web/MCP、表格导出、日志和诊断报告不包含凭据。
- `dataDir` 是本机实例边界，不是租户 ID。Access 当前只提供个人用户身份；每个用户映射到独立个人租户及 Teable base，搜索历史按 tenant_id 隔离。

## 2. 后续业务表统一约定（待实现）

以下邮件表为后续设计；账号隔离按第 3 节实际代码定义。未接入团队工作区，不强行把当前表改成邮件表的字段形式。

- 主键 id 为服务端生成的不透明 text；时间为 timestamptz，UTC；下表 `?` 表示可空，其他字段非空。
- 所有租户业务主表含 id、tenant_id、created_at、updated_at，并建立 UNIQUE(tenant_id,id)。关联表在各节列明例外。
- tenant_id 来源为验证后的会话工作区；user_id 是操作者，不是租户。创建人需要追溯的对象增加 created_by text。
- OneCrew 自有表不建立数据库外键（FK/REFERENCES），包括 tenant_id 和各业务关联 ID；Teable 上游表结构不改动。关联 ID 是普通字段，由服务层验证对象存在、状态可用且属于当前租户，不能只按 ID 查询。
- 保留主键、必要的唯一约束、CHECK 和查询索引。关联检查与写入在适用的事务中完成；允许硬删除的对象，其删除和新增引用必须遵循一致的并发控制，避免检查后对象被删除。不得依赖数据库级联。
- 状态/provider 使用 text + CHECK；接口 Zod 使用对应枚举。不添加乐观锁 version 字段。
- 默认不做硬删除历史邮件；禁用连接保留历史。实际清除凭据不级联删除消息。
- JSONB 用于正文结构、提供方专有设置和快照；常用筛选键、状态、邮箱、ID 单独成列。

## 3. 个人租户与请求边界（已实现）

### onecrew_tenants

| 字段 | SQL 类型 | 约束与含义 |
| --- | --- | --- |
| id | text | 主键，服务端生成的不透明 tenant ID |
| owner_user_id | text | 非空且唯一，Access 验证后的用户 ID |
| space_id | text | 非空且唯一，Teable space |
| base_id | text | 非空且唯一，Teable base |
| created_at / updated_at | text | 非空，ISO 时间文本 |

无 FK。首期一个账号一个个人租户，一个租户一个 base；两种身份仍分开建模，以后引入团队成员关系时业务表无需从 user_id 改成 tenant_id。不单独拆一张只有 base_id 的映射表。

`onecrew_field_types` 的 table_id 全局唯一并从 Teable base 继承归属；字段语义查询之前先校验表在当前 base，不再冗余增加第二份 tenant_id。表格 CRUD、导入/导出、字段修改、行查询/更新/删除都经过同一 data/service 入口，使用原生 Teable GetTableByIdQuery(baseId,tableId) 验证，再验证字段/行属于该表。

### 认证与业务职责

- 认证适配器 `PrincipalResolver(request)` 返回已验证的 user.id/email。本地实现读取 Access 登录状态；HTTP 与 MCP 复用一个租户解析器。未来 SaaS 必须替换为逐请求验证会话或令牌的适配器，不能把本机全局激活账号当远端用户会话。
- `TenantContext={tenantId,baseId,user}` 在入口确定，作为业务方法的必需首参显式传递；业务模块不读取全局当前账号、环境变量默认租户或客户端提交的 tenant_id。
- 同一个服务实例可处理多个可信 principal 的并发请求；本机 PGlite 只是数据库连接方式。浏览器桥会话/令牌/命令队列按 tenantId 分开，搜索记录与后台回调始终使用原创建租户。
- 网页按 user.id 重建业务页面；陈旧请求/下载被丢弃，跨标签只广播失效通知，不广播可信身份。`X-OneCrew-Account-Id` 仅用于比较客户端已知账号，不能作为认证输入；与已验证用户不一致返回 409。
- 本地 MCP stdio 首次业务调用绑定用户 ID，端口变化可重连同账号；账号改变后拒绝业务操作，需重新连接 MCP。公开 healthz 不包含 base ID。
- 自有表的关联检查由服务层和事务维护，不添加 FK，不修改 Teable 上游。

### 部署边界与验收

- 仍只运行 loopback 本地服务。当前个人租户隔离不等于 SaaS 已可公开部署：远端部署还要接服务端 session/OAuth、HTTPS/CSRF策略、独立 PostgreSQL 连接与连接池及浏览器代理的远程传输。
- pooled PostgreSQL 上线时，应为自有租户表增加 RLS 作为纵深隔离，并使用非超级用户/非表 owner 的应用角色，在事务内设置租户上下文；不能把租户 session 变量遗留给连接池下一个请求。Teable 动态表保持上游管理，继续按 base 校验。
- `tests/integration/tenancy.ts` 覆盖并发身份、跨账号 table/field/row ID、导出导入、HTTP/MCP、搜索任务归属；stdio 和 web-client 回归覆盖旧连接与迟到响应。自有表 FK 数量须为零。
- 用户已明确允许清空本地测试业务数据；本轮重置开发数据库并保留有效登录凭据。产品不加入旧库认领、数据兼容或破坏性自动迁移；已有旧 baseline 会要求明确重建。

参考：[AWS 租户隔离](https://docs.aws.amazon.com/whitepapers/latest/saas-architecture-fundamentals/tenant-isolation.html)、[Hono 类型化 Context](https://hono.dev/docs/guides/middleware)、[AWS PostgreSQL RLS](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-managed-postgresql/rls.html)。

## 4. mail_connections

除统一字段外：

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| provider | text | resend / google / outlook / smtp_imap |
| display_name | text | 连接名称 |
| status | text | pending / ready / needs_auth / disabled / error |
| provider_account_id | text? | 提供方稳定账号标识，非邮箱主键 |
| config | jsonb | 非敏感配置，如 SMTP/IMAP host、port、TLS、sent-copy 策略 |
| credential_ref | text? | 后续本地邮箱凭据记录的引用；仅服务端使用，不是系统钥匙串引用 |
| last_checked_at | timestamptz? | 验证时间 |
| last_error_code | text? | 脱敏错误码 |
| sync_cursor | jsonb? | 提供方历史同步续点，仅 provider adapter 解释 |
| last_synced_at | timestamptz? | 历史同步时间 |

索引 (tenant_id,provider,status)。配置 JSON 必须由 provider 专用 Zod 校验，拒绝凭据混入会返回前端的 config。本地邮箱凭据表在邮件实现时单独定义，不能复用 Access 账号凭据行；沿用本地私有目录存储策略，不引入系统钥匙串或配置文件回退。

## 5. mail_senders

字段：connection_id text、email text、display_name text、status text（pending/ready/disabled）、is_default boolean 默认 false、provider_identity_id text?，以及统一字段。

UNIQUE(tenant_id,connection_id,email)；每租户默认身份用部分唯一索引约束 is_default=true。邮箱规范化规则固定且保留原始地址，不对 local-part 擅自改写。From 必须由服务端 sender_id 解析，并验证提供方授权。

## 6. mail_templates

字段：name text、category text、subject text、body_json jsonb、created_by text，以及统一字段。

body_json 为受限 Tiptap 文档；服务端统一渲染 HTML/plaintext。占位符只有 name/platform/profile_url/email/sender_name。模板名不做唯一约束，允许重复标题；列表索引 (tenant_id,updated_at,id)。

## 7. mail_drafts

字段：sender_id text?、recipient_email text?、subject text、body_json jsonb、variables jsonb、source_table_id text?、source_record_id text?、source_snapshot jsonb?、template_id text?、created_by text，以及统一字段。

草稿保存正文副本，template_id 仅追溯来源；模板变化不改变草稿。一个草稿对应一个收件人；批量共用模板发送不必先创建 N 个草稿。来源表格引用通过 Teable 适配层校验；不建立会导致删除源记录时删除草稿/历史的级联。

## 8. mail_batches

字段：request_id text、request_hash text、status text（queued/running/completed/partial/failed/paused）、created_by text、started_at timestamptz?、finished_at timestamptz?，以及统一字段。

UNIQUE(tenant_id,request_id)。相同 key+相同规范化载荷返回原批次，相同 key+不同载荷拒绝。统计优先从 messages 聚合，暂不冗余保存一套计数。首期最多 100 个收件人。

## 9. mail_messages

| 字段 | 类型 | 含义 |
| --- | --- | --- |
| batch_id | text? | 外部同步历史为空 |
| item_key | text? | 批次内稳定项目标识 |
| connection_id | text | 提供方归属 |
| sender_id | text? | 外部历史可能没有本地发件身份 |
| origin | text | onecrew / provider_sync |
| from_email / recipient_email | text | 实际发件与收件地址 |
| sender_snapshot | jsonb | 当时的发件名称等 |
| subject | text | 渲染后的主题 |
| body_html / body_text | text? | 渲染快照；历史尚未获取可为空 |
| source_table_id / source_record_id | text? | 来源引用，不能当授权依据 |
| source_snapshot | jsonb? | 当时姓名、平台、邮箱、变量等 |
| submission_status | text | queued/submitting/accepted/failed/unknown |
| delivery_status | text | unknown/delivered/bounced/complained |
| provider_message_id | text? | 提供方返回的稳定 ID |
| internet_message_id | text? | MIME Message-ID，不能保证提供方幂等 |
| error_code / error_message | text? | 脱敏错误 |
| attempt_count | integer | 默认 0，非负 |
| started_at / accepted_at | timestamptz? | 提交时间 |
| provider_occurred_at | timestamptz? | 外部历史实际发送时间 |

另含统一字段。UNIQUE(tenant_id,batch_id,item_key)、UNIQUE(tenant_id,connection_id,provider_message_id)。索引 (tenant_id,created_at,id)、(tenant_id,batch_id)、(tenant_id,submission_status)。

本地 queued 邮件必须有 batch_id、item_key、完整渲染正文；历史数据不完整时不伪造发送成功。provider_sync 的状态按真实证据映射。accepted 只表示提供方接受，不是 delivered。超时为 unknown，不盲目自动重发。快照在提交后不可编辑。

## 10. mail_attachments 与 mail_attachment_links

mail_attachments 除统一字段外：filename text、content_type text、size bigint CHECK >=0、storage_key text?、provider_ref jsonb?、sha256 text?、status text（pending/ready/failed）。storage_key/provider_ref 至少一项存在；远端引用由 adapter 校验，禁止任意 URL 代理。

storage_key 为 dataDir 相对路径 tenants/<tenantID>/mail/attachments/<文件ID>。文件落盘成功后才标记 ready；未 ready 不允许发送。所有下载重新鉴权。未知远端元数据先查询取得，不用 0 大小占位为已就绪文件。

mail_attachment_links 字段：id text PK、tenant_id text、attachment_id text、template_id text?、draft_id text?、message_id text?、disposition text（attachment/inline）、content_id text?、position integer、created_at timestamptz。

CHECK 三个 owner ID 必须且只能一个非空；各 owner 建 (tenant_id,owner_id,attachment_id) 部分唯一索引。inline 必须有 content_id，同一邮件内 CID 唯一。保留显式的 template_id/draft_id/message_id 关联字段，不建立外键；服务层验证 owner 和附件存在并属于同一租户，不使用任意 owner_type/owner_id。删除链接不自动删除文件；只有无引用文件可由清理任务回收。

## 11. mail_table_bindings

字段：tenant_id text、table_id text、contact_column_id text、email_column_id text、created_at timestamptz、updated_at timestamptz。主键 (tenant_id,table_id)。

绑定列与表必须同租户，contact 列必须 people/company，email 列必须 email。列删除后绑定报明确失效，不猜列名、不新增隐藏评价/抓取信息列。姓名和平台从富文本联系人值读取，平台 ID 可保留在其 identity 中；不增加跨表去重。

## 12. 写入事务、文件与迁移

- 一个事务创建 batch 与所有已校验的 recipient messages，再进行外部发送；网络发送不能包在长期数据库事务里。
- 提交前持久化 submitting，结果持久化 accepted/failed/unknown；数据库事务不能提供跨邮件提供方的 exactly-once。
- 队列任务绑定创建时 tenant_id，不读全局当前用户。退出登录暂停未提交任务；不删除历史，不认为网络中的邮件已撤回。
- 文件与数据库不能原子提交：上传使用临时文件→落盘→记录 ready；失败清理孤儿文件，历史引用必须保留。
- 应用数据在系统 dataDir；只有开发使用项目 .data。程序安装目录只读。一个 dataDir 只能一个进程打开 PGlite。
- OneCrew 当前业务表已由 Kysely 静态 migration 管理；后续邮件表添加独立编号 migration。待实现部分不提前创建空表。不在 Teable 上游容器添加兼容分支。
- 正式用户数据投入使用后，结构升级应明确数据处理策略；不能默认把历史数据归给首次登录者。当前授权清理范围仅限本地测试 profile。

## 13. 首期验收

1. 换租户后，表格/行/邮箱/模板/附件 ID 均不能越权访问。
2. 重复 request_id 不重复排队，同 key 不同内容拒绝。
3. 删除表格行、修改模板不改变历史邮件。
4. Resend 重复同步不新增重复记录，历史 from 不自动成为启用 sender。
5. unknown 消息重启不自动重发；accepted 不显示为已投递。
6. 附件跨租户下载失败，删除草稿不删除历史邮件引用的文件。
7. Win/macOS 升级程序不丢数据库/附件，MCP 子进程不直接打开数据库。

暂不定义：云端成员管理 DDL、收件箱、私聊、CRM、跨设备同步、跨表达人去重。这些不属于当前邮件实现范围。
