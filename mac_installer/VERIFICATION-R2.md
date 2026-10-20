# OneCrew Lite 0.9.0 · Mac ARM64 · R2 实机记录

日期：2026-10-19，America/Los_Angeles。

- 源码提交：`12fa98ea460a3c33457647157f1245e61db1ce9c`。
- 安装包：`OneCrew-Lite-0.9.0-Mac-arm64-R2.zip`，61,726,587 字节。
- ZIP SHA-256：`ce6b9ef0d70f0cd1260fc888146ad38790d67007835be7f35574412d41eb89a3`。
- 主程序 SHA-256：`80222155b1d186b553a80d60e0006757b7f03daa4e84a3d6c1e6407e8ac55095`。
- ZIP CRC、521个文件的 SHA-256 校验通过。实际使用 ditto 解压，再次执行包内 SHA256SUMS 完整校验通过；主程序 ad-hoc 代码签名严格校验通过，未进行 Apple 公证。
- 从该 ZIP 安装到当前 Mac 的 `~/Applications/OneCrewLite/releases/0.9.0-R2-12fa98e`，沿用原数据、vault、主密钥及端口52757。实际服务版本0.9.0、PID48153、startup=ready、signed_in、data=ready；live MCP 回读主程序哈希与本记录一致，Library 和浏览器连接均为 ready。
- 本机生成并安装启用的配套插件为 `0.9.0+codex.aec2a478eb676e8defe050bc9bc6a7dd19c484296cdc4b8e89970aca6c6545f0`，CLI 检查为 fresh。其他电脑需核对其自身生成的完整版本。
- 真实 SQLite/PGlite 运行时回归共6项通过，包括全部平台被排除时停止、零来源创建空表、已有真实来源写入同一表、重复同步、原有结果发布和 Pinterest 登录路由。应用类型检查通过；后续 Pinterest 检测页等待调整的 JavaScript 语法检查通过。
- 本次新 Pinterest 任务没有重建，关键词、筛选条件和目标人数保持原值。其0条来源状态已在真实页面同步成功，结果表 `tbl88dYkjsKbFZ0rO4Y` 可以实际打开，表头及空结果正常显示。页面不再把零结果报告为表格连接失败。
- Pinterest 检测使用同一身份的前台页面和完整登录证明。检查期间曾返回未获得明确身份信号，也实测返回 logged_in；R2 将同页等待时间扩展至最多16秒。最终用户任务恢复和来源写入的实机验收仍待完成；Chrome 扩展界面占用曾阻止页面检查，不能把独立检测通过描述为本轮搜索完成。
- 没有重新验证其他已完成的平台。没有构建 Windows 包，原 `windows installer` 目录与初始提交 `d1dc266076631641c1ba4936308fd7d328cf4f4a` 一致。

首次安装的独立数据启动证据见 R1 的 VERIFICATION.md；本记录不将该证据描述为 R2 独立首次安装测试。产品已知限制和 `productionReady=false` 继续适用。
