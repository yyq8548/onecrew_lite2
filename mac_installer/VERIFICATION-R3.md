# OneCrew Lite 0.9.0 · Mac ARM64 · R3 实机记录

日期：2026-10-19，America/Los_Angeles。

- 源码提交：`cb1caf3e8886f4653c83d294e65f0e19b93cb284`。
- 安装包：`OneCrew-Lite-0.9.0-Mac-arm64-R3.zip`，61,726,852 字节。
- ZIP SHA-256：`2eb0712b384c719a074dfe08a13248503cf00797f4d5cb2a4662d2a39b947195`。
- 主程序 SHA-256：`90b3cd5452a994afd92e1e5e96995fedcbe164e57a0db8ecd51f02ac7092bb70`。
- ZIP CRC、521个文件的 SHA-256 校验通过。使用 ditto 实际解压后，包内 SHA256SUMS 完整校验通过；Mac ARM64 ad-hoc 代码签名严格校验通过，未进行 Apple 公证。
- 原问题通过产品页面复现：Pinterest 页面具有当前账号导航，完整 auth status 仍返回未知。核实 OpenCLI 1.8.8 的 runFull 固定使用 windowMode=background，环境变量不能改变这一选项。
- Pinterest 改为直接调用原生 whoami，并显式传入 foreground、ephemeral、keep-tab=false；仅接受 verified=true 的单条完整回执。命令路由回归先失败后通过，登录相关共2项检查通过，应用类型检查通过。使用相同身份、真实安装适配器和生产参数的现场检查为 logged_in/full，耗时3.328秒。
- 从用户 ZIP 安装到本机 `~/Applications/OneCrewLite/releases/0.9.0-R3`，沿用原数据目录、vault、主密钥及端口52757；实际 PID55545，live MCP 核对版本0.9.0、主程序哈希一致、signed_in、data=ready，Library 和浏览器连接均为 ready。
- 本机配套插件 `0.9.0+codex.77b8aaa8b66cd65bdf4079f0a2c37d37457601ac393bcc5bfcdae2a97e5e1924` 已生成、安装并启用，CLI 检查为 fresh。其他电脑核对其自身生成的完整版本。
- 实际工作台显示 Pinterest 身份验证通过，并通过原任务的“继续搜索”恢复执行；任务和目标条件没有重建或修改。验收时已完成4次读取，原空表 `tbl88dYkjsKbFZ0rO4Y` 成功同步19条真实来源；实际作者列表已显示1位推荐、3位待核实。推荐作者 BePositive by Mima Castro 具有9322位粉丝，页面显示“粉丝数达标”、良好评分和原生主页。任务继续按原目标5位运行；此证据不描述为已完成5/5，也没有重建结果表。
- 没有重新验证其他已完成的平台，没有构建 Windows 包。原 `windows installer` 目录与初始提交 `d1dc266076631641c1ba4936308fd7d328cf4f4a` 一致。

R2 的空表和停止流程回归继续保留。首次安装的独立数据启动证据见 R1 的 VERIFICATION.md；本记录不将其描述为 R3 独立首次安装测试。产品已知限制和 `productionReady=false` 继续适用。
