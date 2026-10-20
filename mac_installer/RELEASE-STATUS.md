# OneCrew Lite 0.9.0 · Mac ARM64 · R3 验证状态

该包为 GitHub 开发候选版本，保留 `productionReady=false`。没有切换 Access、R2 或其他生产下载清单。具体源码提交、字节大小、哈希和许可证缺项见同名 `.zip.release.json`，主程序及资源身份同时保存在包内 candidate.json 和 runtime-manifest.json。

版本统一为 0.9.0，Lite 主程序、工作台、Library、共享组件和新生成的 Codex 插件使用相同产品版本。历史迁移输入和第三方依赖仍保留各自真实版本。

0.9.0 检查通过：冻结锁文件依赖检查、Library 与主界面及 MCP 界面构建、应用类型检查、共享合同检查，以及版本、浏览器会话、配套插件和真实嵌入式表格启动相关的6项检查。此前完整应用检查为586项：559项通过、27项环境或平台条件跳过、零失败；本次版本更新未重复运行平台搜索。

R3 修复：OpenCLI 的 auth status 完整检测内部强制使用后台窗口；Pinterest 改为直接调用原生 whoami，显式使用 foreground、ephemeral 和 keep-tab=false，并仅接受 verified=true 的完整回执。账号绑定、适配器校验及明确未登录、超时和访问限制处理继续保留。同一检测页最多等待16秒，出现明确账号导航立即返回，不通过 Cookie 猜测登录。实际同一身份检查为 logged_in，耗时3.328秒；命令路由回归先失败后通过，应用类型检查通过。实机任务恢复与表格检查见 VERIFICATION-R3.md。

R2 的空表及停止流程修复继续保留：全部平台被排除时停止；已批准任务没有来源时可创建空表，后续来源写入同一张表。真实SQLite/PGlite、空表到真实来源、重复同步、原有结果发布及登录路由共6项检查通过。

已有多平台优化的验证结果沿用，不因构建此包重复搜索平台。Pinterest 月浏览量达标标注、medium 推荐、邮箱优先及全部联系方式保留、Instagram Computer Use 路径、Facebook/LinkedIn 资料读取和网页来源绑定均包含在本次源码中。

已知业务限制：LinkedIn 角色筛选仍存在制造服务账号进入推荐的问题；Google/Yahoo 没有完成各五位的最终推荐；Facebook 有两个已展示邮箱的归属仍待核实。隐藏 YouTube 邮箱依然受网站验证码和次数限制。上述问题不描述为已解决。

Mac 程序为 ad-hoc 签名，未进行 Apple 公证；系统阻止运行时由用户处理，禁止自动绕过保护。目标仅为 Apple Silicon，Intel Mac 未构建、未验收。

主程序升级、原账号数据保留、独立配套插件和 live MCP 的本机验收结果在交付时分别核实；首次安装的独立环境只验证程序启动及正常的未登录状态，不使用开发者账号。不能将一次 Mac 验收描述为所有用户环境都通过。

本次没有构建 Windows 包，仓库原 `windows installer` 文件夹保留原字节内容。Windows 0.9.0 应在原生 Windows 电脑从本分支构建并完成平台验收，操作入口见 WINDOWS-BUILD.zh-CN.md；通用发布规则见仓库 docs/release/合并与部署流程.md。
