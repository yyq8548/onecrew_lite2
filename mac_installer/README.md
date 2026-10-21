# OneCrew Lite 0.9.0 · Mac ARM64 · R4

适用于 Apple Silicon（M 系列）Mac。程序已构建完成，安装时不需要 Node.js、pnpm 或编译工具。

- [安装包](OneCrew-Lite-0.9.0-Mac-arm64-R4.zip)
- [大小、SHA-256、源码提交和构建记录](OneCrew-Lite-0.9.0-Mac-arm64-R4.zip.release.json)
- [ZIP SHA-256](OneCrew-Lite-0.9.0-Mac-arm64-R4.zip.sha256)
- [给 Codex 的安装提示词](CODEX-PROMPT.md)
- [完整安装指南](INSTALL-CODEX.md)
- [验证范围与限制](RELEASE-STATUS.md)
- [R4 实机验证](VERIFICATION-R4.md)
- [Windows 源码打包交接](WINDOWS-BUILD.zh-CN.md)

先核对 ZIP 大小和 SHA-256，再解压进入 `OneCrew-Lite`，运行 `shasum -a 256 -c SHA256SUMS` 校验所有文件，随后按指南安装主程序和配套插件。

R4 包含 LinkedIn 默认不限粉丝、主页/当前职位/帖子任选充分证据、自然语言 AND/OR 条件识别、合法空联系页、网站展示、评分队列容量和失败续跑修复。保存原结果的实测任务已完成 30 人，覆盖币安、OKX、Bitget、Bybit 和 Gate.io。全部联系方式保留，邮箱优先。

安装提示词增加权限预检、`cli.failed` 只读诊断、已确认账号及目录复用、安装断点，以及新版插件注册完成后的单次重载检查。安装保留原账号、数据、主密钥、端口和浏览器身份。

本包为 GitHub 开发候选，`productionReady=false`，使用 ad-hoc 签名，未进行 Apple 公证。系统阻止运行时由用户处理。Intel Mac 未构建。`windows_installer` 沿用远端原内容。

当前目录以 R4 替换旧 Mac 包；历史包和记录可从 Git 历史及原标签读取。
