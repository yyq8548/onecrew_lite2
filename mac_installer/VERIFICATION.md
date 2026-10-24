# OneCrew Lite 0.9.2 R1 · Mac 迁移验证

真实旧 Mac 安装数据在隔离副本上完成验证，实际使用的服务和搜索保持原状。

- 旧 Creator schema 90 升级至 96；79 张原 SQLite 表及已保存结果完成保留核对。
- 独立 Library 的 87 张表、17,244 行原数据通过逐表、逐行内容摘要比较。
- 原账号及 installation ID 保留，使用真实官方 Access 完成在线身份验证。
- 完整根目录迁移约 45 秒；由真实迁移数据形成的兼容账号目录，Library 迁入约 16 秒。
- 活动旧服务锁和真实 tenant 归属冲突均在发布前拒绝；原库保留。成功后重复执行返回 `current`。
- 7 项存储回归、27 项账号与候选发布回归、1 项 CLI 启停回归通过。
- 类型检查、共享接口检查、网页构建通过；安装过程不恢复搜索。

最终 ZIP 已解压、核对全部文件和签名，并由包内原生程序重新执行真实旧库副本的完整迁移。新服务返回 `version=0.9.2`、`access.state=signed_in`、`data.state=ready`、`workspace_connections.library=ready`；通过原账号会话读取了 20 张历史表格的列表及首张表的 20 行数据。验收服务已停止，搜索未恢复。

npm 候选已真实安装至隔离目录；原生服务、MCP 账号状态、工作台、Library 及两处网页授权会话检查通过，约 13 秒。Access 使用真实 Mac ZIP 和只读取得的线上 Windows 0.9.1 清单，在本地完成 8 组双语 npm/ZIP 指南和下载哈希验证。

- 构建提交：`d4fadd8cd4bc36a2ce38403805533ccb903064ca`，`sourceDirty=false`。
- ZIP：`OneCrew-Lite-0.9.2-Mac-arm64-R1.zip`，61,825,625 字节。
- ZIP SHA-256：`18ce04649311a7fe9c3641e7685d8cc46397cfea391b770cdc995d689acdbc41`。
- 程序 SHA-256：`cec04fd700e57d37973dab78970194b9a6346937f2839ac50fb1f28af1b0b781`。

运行资源及依赖锁摘要以同名 `.release.json` 和包内 `runtime-manifest.json` 为准。

2026-10-23 经用户批准发布 Mac ZIP、`@onecrew/lite-darwin-arm64@0.9.2` 和 `@onecrew/lite@0.9.2`。Mac 官网入口已更新至 0.9.2；Windows 安装包及平台 npm 包保持 0.9.1，Pro 和公司其他功能配置保持原状。两个 MR 的合并由 River 后续处理，用户明确允许先行发布。

- 官网验证通过 16 组 Lite／Pro、Mac／Windows、中文／英文、ZIP／npm 指南，网页安装提示词与指南一致。
- Mac 和 Windows ZIP 均通过公开下载入口的完整大小及 SHA-256 校验。
- 从公开 npm 仓库全新安装 0.9.2 后，服务、MCP 账号状态、工作台和 Library 检查通过，验收耗时 11.65 秒；隔离验收服务已停止。
- Mac ZIP 安装指南明确使用官网提供的版本、准确字节数和 SHA-256 作为发布记录，继续核对包内清单；无须另取独立校验文件。
- 原账号、历史表格和搜索结果已在真实旧版 Library 迁移验收中保留，未恢复搜索。

下载与安装入口：[Mac npm 安装指南](https://access.onecrewai.io/cli/install/lite/npm?lng=zh-CN)、[Mac ZIP 安装指南](https://access.onecrewai.io/cli/install/lite?lng=zh-CN)、[npm 主包](https://www.npmjs.com/package/@onecrew/lite)。
