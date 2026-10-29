# OneCrew Lite 0.9.4 R1 · Mac 验收

验收日期：2026-10-27。适用平台：macOS Apple Silicon（darwin-arm64）。

## 发布文件

| 项目 | 结果 |
|---|---|
| 安装包 | `OneCrew-Lite-0.9.4-Mac-arm64-R1.zip` |
| ZIP 大小 | 61,865,248 字节 |
| ZIP SHA-256 | `d297f3d3adda03c22ae5c7e5e11299a6eaa65d1fadf55af7f7d8e2b27fd2b297` |
| 程序 SHA-256 | `75129c026b9a4bd004a4c575dda987b99c9e8b736997e407431a2dc983c1bb96` |
| 构建源码提交 | `3a3a744f83b3a554bb0b450ba6a4fbcecbbce78d` |
| 构建源码摘要 | `66aa9655e979f7036a0fb8a337b09ab5c1a4f66a05b15177ef3bcb4b745f3f34` |
| 源码状态 | 干净，`sourceDirty=false` |
| Windows 安装目录 tree | `62e1e60d58358a6def6e760eda08ea196d314d69`，保持原样 |

ZIP 的实际大小与 SHA-256 和外部发布记录一致；压缩完整性检查通过。解压后，`SHA256SUMS` 中的 521 个文件全部通过；另含两份文件校验清单。程序可执行权限为 755，`codesign --verify --strict` 通过。程序使用 ad-hoc 签名，尚未经过 Apple 公证。

## 源码检查

- Library 服务构建、全工作区类型检查、契约检查通过。
- 小红书、知乎和评分提交相关测试：10 项通过，0 项失败。
- 从上述干净源码重新构建候选并打包，源码及依赖锁摘要核对通过。
- npm 构建输入更新为本包；本次未发布 npm，Windows 平台输入保持原样。

## 本机升级验收

- 运行服务为 0.9.4，实际程序哈希与发布记录一致。
- Codex 配套插件为 `0.9.4+codex.<本机摘要>`，已安装、已启用，独立检查为 `fresh`。
- live MCP 账号状态正常，原 installation ID、数据目录、主密钥、端口和 Chrome 身份保持不变。
- 数据升级检查返回 `current`，业务数据库版本保持 97，Library 正常。
- 原工作台可读取，本轮小红书任务显示结束：推荐 10/10、待核实 0、读取次数 61。
- 原 Library 保留 30 个表格；本轮结果表显示 10 位推荐作者，140 条来源线索已保存。
- 升级验收没有启动或恢复搜索，没有重读成功账号。

本次仅更新个人 GitHub 的源码和 Mac 分发文件。公司 GitLab、官网、Cloudflare、Pro 和 `windows_installer` 均不在本次发布范围。
