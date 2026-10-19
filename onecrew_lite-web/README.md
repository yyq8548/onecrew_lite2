# OneCrew Lite 工作台

当前 TypeScript 项目的 React 网页，独立维护源码、浏览器绑定页面和本地字体资源。后端通过经过账号会话与 CSRF 校验的 HTTP 接口提供业务；网页不调用 loopback MCP。

从仓库根目录执行 pnpm --filter onecrew-web typecheck、pnpm --filter onecrew-web contracts:check、pnpm --filter onecrew-web build。构建产物为 onecrew_lite-web/dist，由统一程序内嵌提供。共享 schema 位于 onecrew_lite-contracts/schemas，生成与检查工具位于 development/schema-codegen。

搜索使用已验证的专用 Chrome 身份。工作台和 Library 默认复用 Codex 内置浏览器。缺失、断线和数据过期必须明确显示；页面心跳和本地业务读取保持同一服务实例与账号。

对外发送仅保留邮件页 Gmail 批量发送，执行前经过具体收件人、内容和发送授权确认。历史私信记录相关类型不代表当前版本提供私信发送能力。

React Query 只保存当前页面缓存；本地变更、MCP 变更和连接恢复必须刷新相关权威查询。固定文案使用 zh-CN 与 en-US 的 i18next 资源，协议标识不翻译。

完整安装和三界面验收要求见 development/README.md。
