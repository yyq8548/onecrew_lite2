# onecrew_lite

从一句话需求出发，寻找合适的创作者，整理资料，再通过 Gmail 联系。

**当前版本：0.9.1** · macOS Apple Silicon / Windows x64 · 中文 / English

当前实现为 TypeScript，源码与构建入口位于仓库根目录，对应 GitHub 提交 `1ac72b654eaf823df6c14ce512e34c6c43ddd8f2`。旧 Lite Rust 实现已移除，Library 使用新版配套实现；公司独立 Access 目录保持原样。详见 [0.9.1 源码与成品记录](docs/verification/onecrew_lite-0.9.1-source-sync.md)。

## 从需求到合作

输入「帮我在 TikTok 找 10 位耳机测评博主，粉丝不少于 5000」，在工作台确认搜索计划后开始查找。

- **理解需求**：提取商品、创作者类别、平台、人数与粉丝要求，生成对应搜索词；也可使用高级设置或手动输入。
- **查找与筛选**：读取平台内容和作者资料，结合证据判断匹配程度，支持暂停、继续和调整计划。
- **整理结果**：在动态表格查看主页、粉丝量、核实状态、匹配评分及公开联系方式，按粉丝量排序。
- **邮件联系**：连接 Gmail，支持单封和批量发送，集中查看通过 onecrew_lite 发出的邮件及其回复。
- **工作台与对话**：在浏览器操作，也可通过 Codex 插件查看进度和调用工具。

平台可读数据受登录状态、页面变化和公开范围影响。缺失信息保留为未知；搜索结果仍需结合证据判断。

## 安装与使用

使用 [Mac 安装指南](https://access.onecrewai.io/cli/install/lite?lng=zh-CN) 或 [Windows 安装指南](https://access.onecrewai.io/cli/install/lite/windows?lng=zh-CN)，由目标电脑上的 AI 客户端校验并安装预构建程序。已安装用户保留原账号、数据、密钥、端口、Chrome 身份和结果。

程序与客户端配套插件分别核实。用户完成必要权限、登录、浏览器授权和客户端重启；最后确认状态卡、工作台与原结果表。Mac ARM64 成品使用 ad-hoc 签名，未经过 Apple 公证。安装验收保持搜索停止。

## 完整源码

| 目录 | 内容 |
| --- | --- |
| [onecrew_lite-app](onecrew_lite-app/) | CLI、本地服务、平台读取、当前对话模型规划与评分、配套插件与打包 |
| [onecrew_lite-web](onecrew_lite-web/) | 工作台和浏览器授权界面 |
| [onecrew_lite-mcp-ui](onecrew_lite-mcp-ui/) | 对话状态卡和操作卡 |
| [onecrew_lite-opencli](onecrew_lite-opencli/) | 已审核的只读平台适配器 |
| [onecrew_lite-contracts](onecrew_lite-contracts/) | 网页、MCP 与业务共享接口 |
| [onecrew_lite-library](onecrew_lite-library/) | 动态表格服务、结果同步、创作者视图与导出 |
| [onecrew_lite-access](onecrew_lite-access/) | 账号授权与发行服务 |

仓库保留源码、测试、构建脚本、依赖锁文件和必要的技术说明。登录凭据、客户数据库、运行缓存和安装包不进入源码仓库。

## 构建

使用 Node.js 26.7.0 和 pnpm 10.32.1，在仓库根目录执行：

```sh
pnpm install --frozen-lockfile
pnpm build:tables
pnpm typecheck
pnpm check:contracts
pnpm build:ui
```

原生程序使用 `pnpm build:candidate`，明确指定版本、目标架构、对应 Node 程序及两份网页资源。具体参数及验证要求见 [开发说明](development/README.md)。Mac 和 Windows 分别保留原生验收记录，成品从网站分发。

进一步阅读：[动态表格](onecrew_lite-library/README.md) · [独立授权服务](onecrew_lite-access/README.md) · [合并与部署流程](docs/release/合并与部署流程.md)

## 项目状态

本仓库目前保持私有，尚未对项目整体授予开源许可。第三方组件的许可证和声明保留在相应目录中。
