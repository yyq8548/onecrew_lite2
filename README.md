# OneCrew Lite

TypeScript 本地程序：生成搜索计划，读取创作者资料和内容，按证据评分，在动态表格整理推荐，通过邮件页 Gmail 批量发送邮件。

## Windows installer

当前安装包：**OneCrew Lite 0.7.1 · Windows x64 · R8.3**。

- [下载 Windows 安装包 ZIP](https://github.com/yyq8548/onecrew_lite2/releases/download/windows-installer-r8.3/OneCrew-Lite-0.7.1-Windows-x64-R8.3.zip)
- [仓库中的 windows installer 文件夹](windows%20installer/)
- [发布校验记录](windows%20installer/OneCrew-Lite-0.7.1-Windows-x64-R8.3.zip.release.json)
- [SHA-256 文件](windows%20installer/OneCrew-Lite-0.7.1-Windows-x64-R8.3.zip.sha256)
- [通用 Codex 安装提示词](windows%20installer/OneCrew-Lite-0.7.1-Windows-x64-R8.3-CODEX-PROMPT.zh-CN.md)
- [GitHub Release：windows installer](https://github.com/yyq8548/onecrew_lite2/releases/tag/windows-installer-r8.3)

完整解压 ZIP，按包内 INSTALL-CODEX.md 先校验再分步安装。自动识别首次安装和升级，保留已有账号、主密钥、表格、端口和浏览器授权。

搜索使用日常 Chrome 的官方 OpenCLI 扩展；工作台和动态表格使用 Codex 内置浏览器。对外发送只保留邮件页 Gmail 批量发送，需要用户确认。后台表格同步保留。

Mac ARM、Mac Intel 原生验收保持未完成。其他未完成验收及许可证文本缺项见发布校验记录，构建状态仍为 `productionReady: false`。

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
