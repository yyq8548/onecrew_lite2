# OneCrew Lite 0.9.9 npm 分发

三个 `.tgz` 为实际 `npm pack` 成品，主包精确选择当前系统的 0.9.9 平台包。Mac 另保留 `mac_installer` 中的 ZIP，Windows 新版仅通过 npm 安装。

发布顺序为两个平台包、主包，实际安装验收后再更新 latest。不可覆盖 npm 已发布的同名同版本。来源、大小、SHA-512 integrity 与 SHA-1 见 `release.json`；原生程序与 Mac ZIP 校验值见 `release-inputs.json`。文件存在不代表已发布。

正式安装：`npm install --global @onecrew/lite@0.9.9 --include=optional --ignore-scripts`。Windows 使用 `npm.cmd` 与 `onecrew-lite.cmd`。完整用户提示词由 Access 的 Lite 安装入口提供，必须保留旧 ZIP/npm 用户的数据、账号、表格和搜索结果，安装不启动搜索。

发布与验收记录见 `docs/releases/lite-0.9.9.md`。遵循仓库 `docs/release/合并与部署流程.md`，不得只凭跨平台构建判定 Windows 已完成原生验收。
