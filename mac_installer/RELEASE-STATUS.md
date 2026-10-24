# OneCrew Lite 0.9.2 · Mac ARM64 · R1

新增 `library-migrate`，支持两种已有安装：

- 已有 `account-*` 目录：保持账号库、Vault、主密钥及端口，迁入完整独立 Library。
- 旧 Rust 根目录：备份和升级原 Creator、Vault，完整迁入 Library，验证原账号与安装身份后原子发布统一数据指针；旧安装保留为恢复来源。

迁移验证全部原表格和行的内容摘要、tenant/base 归属及原安装的在线身份。独立服务或数据库仍有 writer 时拒绝迁移；重复执行已完成迁移返回 `current`。候选健康检查不会修改原 Library 的排队搜索记录。

安装指南适用于 ZIP 与 npm 的数据切换；客户端连接采用迁移返回的实际数据、Vault 和密钥路径。所有安装验收均保持搜索停止。

版本、源码提交、源码摘要、依赖锁摘要、程序和 ZIP 的大小及 SHA-256 以同名 `.release.json` 和包内 `runtime-manifest.json` 为准。Mac 候选为 `productionReady=false`，使用 ad-hoc 签名，未经过 Apple 公证。

Windows 继续使用 0.9.1，未更改 Windows 安装包、Pro、公司其他功能或配置。正式发布和下载入口切换在审批后执行。
