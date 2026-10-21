# OneCrew Lite

TypeScript 本地程序：生成搜索计划，读取创作者资料和内容，按证据评分，在动态表格整理推荐，通过邮件页 Gmail 批量发送邮件。

当前源码版本为 **0.9.0**，默认分支为 `codex/typescript-unified-gmail`。

## Mac installer

当前 Mac 包：**OneCrew Lite 0.9.0 · Apple Silicon ARM64 · R4**。

- [Mac 安装包、校验记录和安装提示词](mac_installer/)
- [给 Codex 的安装提示词](mac_installer/CODEX-PROMPT.md)
- [完整 Mac 安装指南](mac_installer/INSTALL-CODEX.md)
- [Windows 电脑拉取源码与构建步骤](mac_installer/WINDOWS-BUILD.zh-CN.md)
- [合并与部署流程](docs/release/合并与部署流程.md)

Mac 包为独立程序，使用 ad-hoc 签名，未进行 Apple 公证，保持 `productionReady: false`。R4 包含 LinkedIn 条件识别、联系页、评分队列和断点恢复修复，以及权限预检和断点续装提示词。实测同一任务完成 30 人并保留原结果。安装时保留原数据和登录，主程序和配套插件分别核实版本。

## Windows installer

当前安装包：**OneCrew Lite 0.9.0 · Windows x64**。

- [下载 Windows 安装包 ZIP](windows_installer/OneCrew-Lite-0.9.0-Windows-x64.zip)
- [仓库中的 windows_installer 文件夹](windows_installer/)
- [发布校验记录](windows_installer/OneCrew-Lite-0.9.0-Windows-x64.zip.release.json)
- [SHA-256 文件](windows_installer/OneCrew-Lite-0.9.0-Windows-x64.zip.sha256)
- [通用 Codex 安装提示词](windows_installer/OneCrew-Lite-0.9.0-Windows-x64-CODEX-PROMPT.zh-CN.md)

完整解压 ZIP，按包内 INSTALL-CODEX.md 先校验再分步安装。自动识别首次安装和升级，保留已有账号、主密钥、表格、端口和浏览器授权。

搜索使用日常 Chrome 的官方 OpenCLI 扩展；工作台和动态表格使用 Codex 内置浏览器。对外发送只保留邮件页 Gmail 批量发送，需要用户确认。后台表格同步保留。

Windows 文件沿用远端已发布内容，本次未重新构建或更改 `windows_installer`。其验收及许可证记录见原发布记录。新版 Mac 的验收范围和限制见 [RELEASE-STATUS.md](mac_installer/RELEASE-STATUS.md)；Mac Intel 尚未构建和验收。

## 源码与构建

业务程序、工作台、MCP 页面、适配器、共享合同和动态表格分别位于 onecrew_lite-app、onecrew_lite-web、onecrew_lite-mcp-ui、onecrew_lite-opencli、onecrew_lite-contracts、onecrew_lite-library。

使用 Node.js **26.7.0**、pnpm **10.32.1** 和根目录锁文件。Windows 原生依赖需要对应 C++ 构建工具。

```powershell
pnpm install --frozen-lockfile
pnpm check:independence
pnpm build:tables
pnpm typecheck
pnpm check:contracts
pnpm build:ui
pnpm test:app
pnpm test:opencli
```

配套构建、安装验证和清理要求见 [development/README.md](development/README.md)。账号凭据、客户数据、浏览器配置和依赖缓存不提交仓库。

本仓库尚未对项目整体授予开源许可。第三方许可证和声明保留在所属目录中。
