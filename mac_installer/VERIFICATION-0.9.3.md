# Mac 与 npm 0.9.3 发布验证

任务 QB-3448。将同事已验收的 0.9.2 R2 修复发布为 0.9.3，保留旧版独立 Library 迁移及原账号、表格、搜索结果。Windows 继续使用 0.9.1，windows_installer 不变。

源码分支 feat/mac-npm-093-20261025，目标为公司 main。Access 配套分支 feat/lite-mac-npm-093-20261025，目标为 onecrew。本次用户明确授权先部署、不等 MR 合并；已上线，MR 尚待合并。

## 产物

- 原生构建提交：e019846de83ab2c17a8196f3535ec97e1ef68aab，工作树干净。
- npm 清单提交：56aec2d579a019a8ca1be37f2f89a7fe547b251c。
- Mac：OneCrew-Lite-0.9.3-Mac-arm64-R1.zip，61,828,504 字节。
- ZIP SHA-256：f9a72b56847eac89584c0146fc076a3d202ccf143e06310218291d43f5f3597a。
- 程序 SHA-256：56e9f098c39174b44d3a84693425e9da876cb5dccad789ed9755677c1b4d256a。
- macOS ARM64，ad-hoc 签名，未公证。
- npm 发布 @onecrew/lite-darwin-arm64@0.9.3 和 @onecrew/lite@0.9.3；主包继续引用已发布的 Windows 0.9.1。

## 验证

- 类型检查、契约检查、工作台和表格构建通过。
- Codex 适配器回归 3 项通过；真实原生 Codex 安装及 R2 升级测试 2 项通过。
- 原生服务、真实数据库、stdio MCP、工作台和 Library 验收通过。
- 最终 ZIP 523 个文件校验及 codesign --verify --strict 通过。
- 两个本地 npm tarball 实际安装通过；程序哈希、服务启动、MCP 账号状态及工作台和 Library 授权会话通过，耗时 15.231 秒。
- 验证使用隔离数据，不启动或恢复用户搜索。

执行日志位于发布工作区 outputs/lite-093-publish-20261025。合并提交、上传回执、线上验收和生产版本记录在关联发布 MR。

## 发布与恢复

已发布并完整下载核验 npm 两个包，上传独立 Mac ZIP 和版本清单，部署 Access 将 Mac 指向 0.9.3。Pro、Windows 和其他公司配置保持原值，无云端数据库迁移。官网复用 Access 指引，官网服务无改动。

2026-10-26 07:19 UTC 线上验收通过：16 组安装指引、Mac/Windows 下载 HEAD 与完整 GET、文件哈希、账号页面及 Pro 保持性检查通过。公开 npm 实际安装及服务/MCP/表格验证通过，耗时 10.662 秒。Access 版本为 9c91172a-424a-4611-8c63-086a99a37030，源码提交 4114138a1103a627915e35800734cb6f49f4cd53；Access MR 为 https://git.digifinex.org/onecrew/onecrew-access/-/merge_requests/29 。

Mac ZIP 入口：https://access.onecrewai.io/cli/install/lite?lng=zh-CN 。npm 入口：https://access.onecrewai.io/cli/install/lite/npm?lng=zh-CN 。后续合并发布 MR 后核对主分支与实际产物一致。

异常时通过 Access 发布配置恢复 Mac 0.9.2 指向，保留旧产物和原数据；npm 已发布版本不能覆盖。恢复前检查生产变更，保留同事已发布功能。
