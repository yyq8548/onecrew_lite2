# OneCrew Lite 只读适配器

这些适配器归当前 TypeScript 项目独立维护，使用 OpenCLI 1.8.8 的页面 API、连接服务和扩展。安装清单以 onecrew_lite-app/src/platforms/opencli/addons.ts 的 OPENCLI_ADDON_FILES 为准；只复制清单中的运行文件，测试文件不进入客户适配器目录。

适配器用于公开搜索、登录状态检测、作者身份核实、帖子与评论证据读取，以及公开联系方式提取。它们不提供私信、发布、回复或邮件发送。对外发送仅由邮件页 Gmail 流程负责。

运行前检查指定浏览器身份、注册的只读能力、OpenCLI 版本和实际安装字节。网页返回值必须校验身份、来源、大小及读取范围；缺少数据、登录要求、限制和读取失败分别保留，不能构造成功结果。

在根目录执行 pnpm test:opencli。TypeScript 业务集成测试位于 onecrew_lite-app/tests，源码自带的公开页面材料位于该目录的 fixtures。实际平台、专用 Chrome 和安装验证按 development/README.md 执行；解析测试通过不等于平台实时可用。
