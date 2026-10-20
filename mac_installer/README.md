# OneCrew Lite 0.9.0 · Mac ARM64 · R2

适用于 Apple Silicon（M 系列）Mac。程序已经构建完成，安装时不需要 Node.js、pnpm 或编译工具。

- [安装包](OneCrew-Lite-0.9.0-Mac-arm64-R2.zip)
- [大小、SHA-256、源码提交和构建记录](OneCrew-Lite-0.9.0-Mac-arm64-R2.zip.release.json)
- [ZIP SHA-256](OneCrew-Lite-0.9.0-Mac-arm64-R2.zip.sha256)
- [给 Codex 的安装提示词](CODEX-PROMPT.md)
- [完整安装指南](INSTALL-CODEX.md)
- [验证状态与限制](RELEASE-STATUS.md)
- [R2 实机验证](VERIFICATION-R2.md)
- [Windows 源码打包交接](WINDOWS-BUILD.zh-CN.md)

先核对 ZIP 大小和 SHA-256，再完整解压。解压后进入 `OneCrew-Lite`，运行 `shasum -a 256 -c SHA256SUMS` 校验所有文件，再按指南安装主程序和配套插件。

本包复用已验证的多平台优化代码，保留旧账号、表格、浏览器身份和授权。Mac 的 `installation-discover` 命令尚不支持；安装指南通过已有服务和客户端配置确认原数据目录。多个候选目录时需要用户选择。

这是开发候选包，使用 ad-hoc 签名，未进行 Apple 公证，`productionReady` 为 `false`。Intel Mac 和新版 Windows 原生验收不在本次 Mac 打包范围内。仓库原 `windows installer` 文件夹保持原内容。

R2 修正 Pinterest 登录检查的页面模式、所有平台被排除时的停止流程，以及零结果任务的表格创建。R1 历史文件和标签保留。
