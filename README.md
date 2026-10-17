# OneCrew Lite · TypeScript 统一程序

从一句话需求出发，寻找合适的创作者，整理资料，再通过 Gmail 联系。

**产品版本：0.7.1，当前为开发候选**。主程序、本地服务、工作台、Codex 插件和 Library 配套构建。Windows 实际验证与 Mac 未完成项分开记录，尚未达到客户正式发布条件。

## 当前状态与接手

新版仓库：`https://github.com/yyq8548/onecrew_lite2`。开发分支为 `codex/typescript-unified-gmail`。先读 [当前交接、验证和下一步计划](docs/migration/14-handoff-status-and-next-plan.md)，再按需查看 [完整迁移计划](docs/migration/03-detailed-migration-plan.md)、[Windows 验证记录](docs/migration/11-windows-verification.md)和[客户安装约束](docs/migration/13-customer-installation-paths.md)。

来源基线为 `efc3c6eb69507b9809bb30279d9af26156d075e3`；本轮批准迁入的 GitHub 改动固定为 `8d931ab08448bcded456c210ebc29ad9709a68a5`。保留已有 Windows、专用 Chrome、身份、权限和数据恢复修复。

## 从需求到合作

输入「帮我在 TikTok 找 10 位耳机测评博主，粉丝不少于 5000」，在工作台确认搜索计划后开始查找。

- **理解需求**：提取商品、创作者类别、平台、人数与粉丝要求，生成对应搜索词；也可使用高级设置或手动输入。
- **查找与筛选**：读取平台内容和作者资料，结合证据判断匹配程度，支持暂停、继续和调整计划。
- **整理结果**：在动态表格查看主页、粉丝量、核实状态、匹配评分及公开联系方式，按粉丝量排序。
- **邮件联系**：对外发送仅保留邮件页 Gmail 批量发送，需真实用户确认；集中查看 OneCrew 邮件历史和回复。MCP 不提供发送操作。
- **工作台与对话**：在浏览器操作，也可通过 Codex 插件查看进度和调用工具。

平台可读数据受登录状态、页面变化和公开范围影响。缺失信息保留为未知；搜索结果仍需结合证据判断。

## 安装与使用

当前候选的 Windows 程序使用明确的数据目录、当前用户私有权限和按需计划任务；服务程序按完整 SHA-256 保存，原安装文件移走后仍可运行。新安装由操作系统分配端口，后续启动和升级保留端口。中文、空格和超过 260 字符的安装路径已进行真实验证。

打开工作台和 Library 默认复用 Codex 内置浏览器；搜索使用已验证的专用 Chrome 身份。先确认账号和平台连接，再确认搜索计划。缺少信息、登录失败、风控停止与有效零结果分别显示。

Mac ARM 已完成本机配套构建及部分真实原生验收，详情见 [Mac ARM 验证记录](docs/migration/15-mac-arm-verification.md)。Mac ARM 完整客户安装、升级及宿主验收，以及 Mac Intel 原生验收仍未完成。当前候选保持 `productionReady: false`。

## 完整源码

| 目录 | 内容 |
| --- | --- |
| [onecrew_lite-app](onecrew_lite-app/) | 当前 TypeScript CLI、本地服务、业务、存储、迁移、原生能力、安装与 SEA 打包 |
| [onecrew_lite-contracts](onecrew_lite-contracts/) | 当前共享接口与校验合同 |
| [onecrew_lite-rust/apps](onecrew_lite-rust/apps/) | 复用的 React 工作台和 MCP 卡片 |
| [onecrew_lite-rust](onecrew_lite-rust/) | 来源实现、冻结历史合同、Rust 对照测试及只读适配器；不是新版业务运行入口 |
| [onecrew_lite-library](onecrew_lite-library/) | 动态表格服务、结果同步、创作者视图与导出 |
| [onecrew_lite-access](onecrew_lite-access/) | 账号授权与发行服务旧快照；当前生产来源见交接记录，勿直接部署 |

仓库保留源码、测试、构建脚本、依赖锁文件和必要的技术说明。登录凭据、客户数据库、运行缓存和安装包不进入源码仓库。

## 构建

固定使用 Node.js **26.7.0**、pnpm **10.32.1** 和根目录锁文件。Windows 原生依赖需要对应 C++ 构建工具；重新生成 Rust 对照材料时另需项目固定 Rust 工具链。具体命令、候选打包及完整验收环境见 [交接说明](docs/migration/14-handoff-status-and-next-plan.md)。

```powershell
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test:app
```

原生和实际安装测试需要显式提供真实候选、旧版本、数据库对照及已安装组件。测试跳过不计为通过。工作台、MCP UI、Library 和主程序必须配套构建，不能只替换一个可执行文件。

## 项目状态

本仓库目前保持私有，尚未对项目整体授予开源许可。第三方组件的许可证和声明保留在相应目录中。
