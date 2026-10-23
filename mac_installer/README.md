# OneCrew Lite 0.9.1 · Mac ARM64 · R32

适用于 Apple Silicon（M 系列）Mac。程序内置运行环境，无须安装 Node.js、pnpm 或编译工具。

- [安装包](OneCrew-Lite-0.9.1-Mac-arm64-R32.zip)
- [大小、SHA-256 与构建身份](OneCrew-Lite-0.9.1-Mac-arm64-R32.zip.release.json)
- [ZIP SHA-256](OneCrew-Lite-0.9.1-Mac-arm64-R32.zip.sha256)
- [给本地 AI 的安装提示词](CODEX-PROMPT.md)
- [完整安装指南](INSTALL-CODEX.md)
- [版本范围与验证](RELEASE-STATUS.md)

将安装提示词发给目标 Mac 上的 AI 客户端，由 AI 下载、校验和安装；也可同时提供本目录的 ZIP 及其校验文件。遇到 sandbox 或目录、网络、本地端口权限限制，AI 使用客户端支持的审批流程申请必要权限，批准后从安装断点继续。

安装沿用已有账号、数据、主密钥、端口、Chrome 身份和搜索结果。主程序与配套插件分别核实，验收同时检查状态卡、工作台和原结果表；安装不恢复搜索。

本包为 GitHub 分发的开发候选，`productionReady=false`，使用 ad-hoc 签名，尚未经过 Apple 公证。Intel Mac 不适用。`windows_installer` 保持原样，公司服务及下载清单未在本次更新范围内。
