# OneCrew Lite 0.9.0 · Mac ARM64 · R1 实机记录

日期：2026-10-19，America/Los_Angeles。

- 源码提交：`018e86c8acc851bfadc542949e3cbf13658d7909`。
- 安装包：`OneCrew-Lite-0.9.0-Mac-arm64-R1.zip`，61,726,008 字节。
- ZIP SHA-256：`63f9cadc1c647a6ffe6fda636d66f9ecb901451deab35ecf5bce1e31ea63b90e`。
- 主程序 SHA-256：`6baa061b17add0dd983a93f03f1c7d5f7060959be337b171a2723232f81596e2`。
- 源码和依赖锁文件身份与候选清单一致；ZIP 重新读取、CRC 和521个文件的 SHA-256 校验通过。实际使用 ditto 解压后，再次使用包内 SHA256SUMS 完整校验通过。
- Mac ARM64 ad-hoc 代码签名严格校验通过，未进行 Apple 公证。
- 独立新数据目录真实启动成功；live MCP 返回版本0.9.0、正确二进制哈希、startup=ready、signed_out、pairing_required。该首次启动没有使用开发者账号，检查后已正常停止。
- 从本用户 ZIP 安装到当前 Mac 的 `~/Applications/OneCrewLite/releases/0.9.0-R1`，使用原数据目录、vault、主密钥和原端口52757升级启动成功。原账号保持 signed_in，数据状态 ready，原有搜索及结果数据保留；未重新执行平台搜索。
- 本机实际生成、安装并启用的插件为 `0.9.0+codex.b416c9b36a39bcc1c8deca6235af8502871a80aaa7c02ba7d046c7a5e3a4147e`，CLI 安装检查为 fresh。其他电脑应核对其自身 codex-package 生成的完整版本。
- 本次聊天中的 live MCP 已回读到0.9.0服务、正确主程序哈希和原账号登录状态。已有客户端窗口仍需要按实际宿主提示重新加载插件或页面，不能将注册结果替代每个宿主的加载确认。
- 没有构建 Windows 包，原 `windows installer` 目录与初始提交 `d1dc266076631641c1ba4936308fd7d328cf4f4a` 保持一致。Intel Mac、Windows 0.9.0 原生验收未执行。

该记录仅描述本次真实检查；产品已知限制及 `productionReady=false` 继续适用。
