# OneCrew Lite 0.9.9 Mac 发布包验证

日期：2026-10-29，Apple Silicon Mac。Mac 同时提供 ZIP 和 npm，Windows 仅通过 npm 分发，旧 windows_installer 不变。

- Mac 构建提交：`beb75290403b55fc062b1052e3816bf18df3bcbb`，构建时工作区干净。
- ZIP：`OneCrew-Lite-0.9.9-Mac-arm64.zip`，61,650,764 字节。
- ZIP SHA-256：`18ea8820cfb58a89265d5f3641544e31d892de7cbcdeb38d144d45b233a28689`。
- 原生程序 SHA-256：`0d5821d1de5445eddb575fcb5332583952395bc0df05b0848b24d9855cf89467`。
- 523 项包内文件和校验清单通过；ad-hoc 签名，未公证，保留 productionReady=false 和原始许可证证据列表。
- 实际本地 npm 安装、入口解析、程序哈希、后台服务启动与停止、工作台、Library、stdio live MCP 和两个网页授权会话通过，合计 15.292 秒。
- 安装验证使用独立测试目录，没有操作客户账号、原表格、搜索结果或当前 Mac 安装，没有恢复搜索或读取创作者账号。
- 搜索和评分继续使用已验证的 0.9.9 实现；本次新增 Windows 运行库和 npm 分发支持。知乎活动搜索入口已移除，历史材料保留。

npm 主包与两个平台包版本均为 0.9.9，实际文件大小与 npm SRI 见 `npm_installer/release.json`。Windows 原生验收、公司 MR、网站部署和最终入口在 `docs/releases/lite-0.9.9.md` 单独记录，不能用本机 Mac 检查代替 Windows 运行结果。
