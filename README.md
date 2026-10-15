# onecrew_lite

从一句话需求出发，寻找合适的创作者，整理资料，再通过 Gmail 联系。

**当前版本：0.7.1** · Mac Apple Silicon r4 / Windows x64 r3 · 中文 / English

## 迁移开发接手

另一台电脑请克隆 `codex/typescript-migration-handoff` 分支，从 [完整分析、迁移计划与接手流程](docs/migration/README.md) 开始。该分支保留公司已发布 Mac r4 对应的全部受版本管理源码；TypeScript 统一程序迁移尚未实施。Windows r3 对应完整源码尚未核实，不能把本仓库描述为已能重建该包。

## 从需求到合作

输入「帮我在 TikTok 找 10 位耳机测评博主，粉丝不少于 5000」，在工作台确认搜索计划后开始查找。

- **理解需求**：提取商品、创作者类别、平台、人数与粉丝要求，生成对应搜索词；也可使用高级设置或手动输入。
- **查找与筛选**：读取平台内容和作者资料，结合证据判断匹配程度，支持暂停、继续和调整计划。
- **整理结果**：在动态表格查看主页、粉丝量、核实状态、匹配评分及公开联系方式，按粉丝量排序。
- **邮件联系**：连接 Gmail，支持单封和批量发送，集中查看通过 onecrew_lite 发出的邮件及其回复。
- **工作台与对话**：在浏览器操作，也可通过 Codex 插件查看进度和调用工具。

平台可读数据受登录状态、页面变化和公开范围影响。缺失信息保留为未知；搜索结果仍需结合证据判断。

## 安装与使用

当前 Mac 安装包适用于 **Apple 芯片、macOS 13.5 及以上**。

1. 从官网获取私人安装指引，核对 ZIP 大小、SHA256 和包内清单后完整解压。
2. 在包目录执行 `sh install-core.sh` 安装本体，再通过已安装的 `onecrew_lite opencli repair` 准备搜索依赖。
3. 执行包内 `sh connect-codex.sh` 接入 Codex，按提示重启 Codex 并验证真实 MCP 调用。
4. 使用 `open-onecrew_lite.command` 打开工作台和 Library，完成两侧授权、Chrome 扩展连接及浏览器身份选择，确认同一账号且组件状态 ready。
5. 输入需求，确认计划，开始搜索并查看结果表格。

以上为 Mac r4 包的实际文件布局；Windows r3 按包内 PowerShell 分步说明安装。Mac 包使用 ad-hoc 签名，尚无 Developer ID 签名和公证。

## 完整源码

| 目录 | 内容 |
| --- | --- |
| [onecrew_lite-rust](onecrew_lite-rust/) | Rust CLI、本地服务、搜索与评分、平台适配器、React 工作台、Codex 插件 |
| [onecrew_lite-library](onecrew_lite-library/) | 动态表格服务、结果同步、创作者视图与导出 |
| [onecrew_lite-access](onecrew_lite-access/) | 账号授权与发行服务旧快照；当前生产来源见交接记录，勿直接部署 |

仓库保留源码、测试、构建脚本、依赖锁文件和必要的技术说明。登录凭据、客户数据库、运行缓存和安装包不进入源码仓库。

## 构建

准备项目固定版本的 Rust 工具链、Node.js，以及 Library 使用的 pnpm。各子项目的环境和依赖要求见其 README。

构建 Mac 主程序（包含工作台与插件资源）：

```sh
cd onecrew_lite-rust
bash development/scripts/build.sh --release --target aarch64-apple-darwin
```

完整安装包还需包含 Library。其构建入口是 `onecrew_lite-library/scripts/build-release.mjs`，组合打包入口是 `onecrew_lite-rust/development/scripts/package-workspace.mjs`。原生配套候选使用 `development/scripts/workspace-release.mjs`，打包脚本校验两个程序的清单、目标平台和文件摘要。

进一步阅读：[主程序开发](onecrew_lite-rust/development/README.md) · [运行时架构](onecrew_lite-rust/ARCHITECTURE.md) · [动态表格](onecrew_lite-library/README.md) · [授权服务](onecrew_lite-access/README.md)

## 项目状态

本仓库目前保持私有，尚未对项目整体授予开源许可。第三方组件的许可证和声明保留在相应目录中。
