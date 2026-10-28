# OneCrew Lite 0.9.4 · Mac ARM64 · R1

适用于 Apple Silicon Mac，程序内置运行环境。

- [Mac ZIP](OneCrew-Lite-0.9.4-Mac-arm64-R1.zip)
- [发布记录与哈希](OneCrew-Lite-0.9.4-Mac-arm64-R1.zip.release.json)
- [ZIP SHA-256](OneCrew-Lite-0.9.4-Mac-arm64-R1.zip.sha256)
- [安装提示词](CODEX-PROMPT.md)
- [完整安装与迁移指南](INSTALL-CODEX.md)
- [版本说明](RELEASE-STATUS.md)

本版提供 Mac 旧版独立 Library 的正式迁移命令，支持 ZIP 安装升级到 npm。迁移保留原账号、安装身份、表格、搜索结果和旧库恢复副本；安装不启动或恢复搜索。

本版包含配套插件修复，生成和校验 `0.9.4+codex.<摘要>`，保留插件、主程序和数据目录的一致性检查。保留知乎和小红书 OpenCLI 搜索、主页和正文读取，复用 Chrome 登录；沿用现有搜索、评分和证据校验，未知粉丝数保留未知。

0.9.4 包含原 R4 的小红书搜索跳转路径、原生粉丝最终推荐判断和公开主页联系方式表格同步修复。已完成10位AI Agent相关博主的实际搜索验证，保留原有结果并复用成功读取。

本版本通过本目录 ZIP 交付，尚未发布到 npm 或公司网站。请把 ZIP、同名发布记录、SHA-256 和安装提示词一起交给目标 Mac 上的 AI，按指南升级到 0.9.4。

Windows 分发目录保持原样，npm 主包为 Windows 选择已发布的 0.9.1 平台包。本包使用 ad-hoc 签名，未经过 Apple 公证。
