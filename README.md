# onecrew_lite

从一句话需求出发，寻找合适的创作者，整理资料，再通过 Gmail 联系。

**当前版本：0.7.0** · macOS Apple Silicon · 中文 / English

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

1. 解压完整安装包，双击 `安装或更新.command`。
2. 安装完成后重启 Codex，输入「使用 onecrew_lite」。
3. 按工作台提示完成登录、Chrome 扩展连接和浏览器身份选择。
4. 输入需求，确认计划，开始搜索并查看结果表格。

之后可使用 `打开 onecrew_lite.command` 启动。当前测试包未经 Apple 签名和公证。

## 完整源码

| 目录 | 内容 |
| --- | --- |
| [onecrew_lite-rust](onecrew_lite-rust/) | Rust CLI、本地服务、搜索与评分、平台适配器、React 工作台、Codex 插件 |
| [onecrew_lite-library](onecrew_lite-library/) | 动态表格服务、结果同步、创作者视图与导出 |
| [onecrew_lite-access](onecrew_lite-access/) | 账号授权与发行服务 |

仓库保留源码、测试、构建脚本、依赖锁文件和必要的技术说明。登录凭据、客户数据库、运行缓存和安装包不进入源码仓库。

## 构建

准备项目固定版本的 Rust 工具链、Node.js，以及 Library 使用的 pnpm。各子项目的环境和依赖要求见其 README。

构建 Mac 主程序（包含工作台与插件资源）：

```sh
cd onecrew_lite-rust
bash development/scripts/build.sh --release --target aarch64-apple-darwin
```

完整安装包还需包含 Library。其构建入口是 `onecrew_lite-library/scripts/build-release.mjs`，组合打包入口是 `onecrew_lite-rust/development/scripts/package-mac-trial.mjs`。打包脚本校验两个程序的清单、目标平台和文件摘要。

进一步阅读：[主程序开发](onecrew_lite-rust/development/README.md) · [运行时架构](onecrew_lite-rust/ARCHITECTURE.md) · [动态表格](onecrew_lite-library/README.md) · [授权服务](onecrew_lite-access/README.md)

## 项目状态

本仓库目前保持私有，尚未对项目整体授予开源许可。第三方组件的许可证和声明保留在相应目录中。
