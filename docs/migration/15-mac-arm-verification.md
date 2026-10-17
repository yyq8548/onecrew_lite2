# Mac ARM 本机开发与验证

日期：2026-10-16。用户已恢复开发，按当前交接计划继续。工作目录为 `onecrew_lite2`，分支为 `codex/typescript-unified-gmail`，起始提交为 `08b990a22933b92bbbbd20ab2cc1d6242be41c18`。原 Rust 版工作目录未修改。

## 环境

本机为 macOS ARM64。项目固定工具链放在本仓库 `.scratch/toolchains/`，未替换系统工具：

- Node.js 26.7.0，官方 ARM64 压缩包 SHA-256 为 `7ee659a7768e641bbfd5360940660b8e8fd0052f77488f365562bac522fc15d4`，与官方下载清单一致。
- pnpm 10.32.1，使用根目录锁文件安装。
- Rust 1.98.0，用于实际原 Rust 文件锁对照程序；`RUSTUP_HOME` 和 `CARGO_HOME` 均在本仓库 `.scratch/toolchains/`。

运行检查前，为当前命令设置项目工具路径。使用 Rust 对照测试时同时指定上述 Rust 目录，不修改全局工具配置。

## 已完成

| 检查 | 结果 |
| --- | --- |
| Library 服务及 Teable 构建 | 通过 |
| 主程序类型检查 | 通过 |
| MCP UI 类型检查及构建 | 通过 |
| 工作台及 unified Library 网页构建 | 通过 |
| 工作台完整测试 | 824 项通过 |
| MCP UI 完整测试 | 406 项通过 |
| 只读适配器完整测试 | 348 项通过 |
| 主程序完整套件 | 546 项，其中 531 项通过、0 失败、15 项 Windows 专项跳过；完整运行使用 candidate-01，后续浏览器错误提示修改另作 candidate-02 回归 |
| 真实 Mac SEA 专项 | 1 项通过，0 跳过；覆盖冷启动、热启动、停止重启、SQLite/PGlite、两套网页、stdio MCP 资源及缓存损坏拒绝 |
| Library 投影及发现调度回归 | 4 项通过，0 跳过 |
| 原 Rust fslock 与迁移锁互斥 | 1 项通过，使用实际 Rust 对照程序 |
| Mac Chrome 独立私有目录启动与关闭 | 1 项通过；Windows 对应项按平台跳过 |
| Mac 原生网络探针 | HTTP/2 成功，浏览器配置为 `chrome_149`；实际 Chrome 指纹逐项对照仍未完成 |
| Rust current schema90 联系路由及 CRUD | 18 项通过，使用本仓库实际 Rust 构造器生成的数据库副本，0 跳过 |
| 修改后的 Mac 联合回归 | 7 项通过，0 失败；1 项 Windows 专项按平台跳过 |
| 显式 Cargo 路径的 staging 专项 | 1 项通过；只运行指定用例，不计为完整 staging 套件通过 |
| 本机程序签名完整性 | `codesign --verify --strict` 通过；当前为 ad-hoc 签名，尚无 Developer ID 签名或公证 |
| 最终类型检查及差异检查 | 通过 |
| candidate-02 浏览器修复回归 | 真实 HTTP、会话、浏览器挑战及 SEA 共 6 项通过，0 跳过；工作台重新运行 824 项通过 |
| candidate-03 搜索验收修复 | 实际缺失适配器的错误分类、真实安装与 SEA 冷热启动及缓存损坏拒绝共 2 项通过，0 跳过；主程序类型检查、工作台构建和签名完整性检查通过 |
| candidate-04 许可打包回归 | 许可全文、ZIP 与来源哈希及真实 SEA 共 4 项通过；真实 Codex 安装、升级、缓存 MCP 与恢复共 3 项通过，均无跳过；类型检查及 ad-hoc 签名完整性检查通过 |
| Mac ARM 原生 Codex ClientAdapter | 真实 Codex CLI 隔离安装、刷新校验、卸载、candidate-02 到 candidate-03 插件升级及恢复共 3 项通过，0 跳过；日志为 `.scratch/mac-arm-codex-native-upgrade.log` |
| 独立新版实际账号及浏览器连接 | 实际账号登录、MCP 程序哈希一致、schema90 数据 ready、专用 Chrome 归属验证及身份选择均通过 |
| 同目录候选升级 | candidate-01 正常停止，candidate-02 使用相同私有目录启动；端口 53400、实际账号和数据状态保留 |

SEA 测试在隔离的子进程环境中设置 Mac 用户目录，并预建 `Library/Caches`，因此冷启动、热缓存和损坏检查均在当前工作区内进行。Library 测试复制原始真实 SQLite 对照文件后，在 Unix 平台明确恢复副本的 `0600` 权限；生产权限验证保持严格。旧版文件锁及 SQLite staging 测试支持通过 `CARGO` 显式指定程序路径。

## 本机构建候选

本记录对应的历史构建目录为 `.scratch/mac-arm-candidate-06/`，产品版本为 `0.7.1`，目标为 `darwin-arm64`。最新运行与验收范围见[当前交接](14-handoff-status-and-next-plan.md)。

- 程序 SHA-256：`25ebe537d4cb25c0b38c1f4e392b5f9690bb3e5bc53e76265500c77030b821fa`。
- payload SHA-256：`cdae0072962407e8b1bf296fbd69b566d6d10d228ab8e32d43cda7775ab4c18c`。
- 源码指纹：`625a2f3cd2b1447d9c381dbe015a229a5d014d26a0b1cedef3bb8613e3bdff7d`。
- 程序大小：215294544 字节。
- `productionReady` 为 `false`；许可全文缺口为 13 项，来源和剩余材料见 [许可全文证据](16-license-evidence.md)。

当前候选包含浏览器连接错误提示修复：只向客户端返回明确列出的安全错误代码；网页保留服务端错误，并提示切回实际连接标签页、重新连接已关闭浏览器或处理过期连接。绑定仍核对真实专用 Chrome、扩展身份、随机码和账号归属。实际连接已在新版工作台完成验证。

搜索验收修复将缺少只读命令或配套适配器的错误归为 `opencli_command_unavailable`。工作台在没有任务运行且持久化推荐数达到目标时显示完成，并优先显示晚于旧执行记录的手动同步回执。candidate-06 使用原私有目录运行，MCP 核对实际程序哈希一致，登录和 schema90 数据保持就绪。

升级后已重新打开并验证原专用 Chrome 身份；实际工作台显示“本轮搜索结束”、10/10、读取 23 次及已同步 37 条线索。对应 Library 刷新后仍显示相同的 10 个推荐作者分组。

原生 ClientAdapter 验收使用独立 `CODEX_HOME` 和真实 candidate-02、candidate-03，运行打包程序的插件准备、安装、卸载命令。升级保持原数据目录及服务端口，服务实例切换为新程序；从 Codex 实际缓存目录启动的 MCP 返回同一新实例和正确程序哈希，并成功读取状态卡 HTML 资源。另行验证旧插件更新提示、无关设置保留及重新安装上一版。隔离目录在测试后清理，现有 Rust 插件与当前登录数据保持原归属。此项验证插件传输和资源读取，宿主实际渲染仍待验收。

构建基于起始提交及本轮未提交修改；不宣称候选来自一个尚不存在的提交。重新构建须使用新目录，重新记录来源及哈希。

构建、探针及测试日志保存在 `.scratch/`，不提交账号数据或运行缓存。用户授权开发期间自动清理本任务的旧构建，保留当前版本、上一版本及仍被进程、插件、验收或恢复操作引用的产物；清理前核对工作区内的实际路径和引用，不删除源码、账号、浏览器身份及必要日志。

已检查旧候选的实际路径、打开文件及验收脚本引用，删除 candidate-01 至 candidate-04 构建目录。当前保留 candidate-06 和 candidate-05，历史测试日志保留。

## YouTube 真实搜索验收

用户指定“youtube 美妆博主 10人”，已在工作台批准计划。通过新版 live MCP 执行实际搜索、读取作者原视频资料、提交基于保存材料的引用和评分，并在对应 Library 表核对推荐结果。

- 仅使用 YouTube，目标 10 人，不添加粉丝量、语言或地域条件。
- 查询词为 `makeup tutorial`、`skincare routine`、`化妆教程`、`护肤分享`。
- 沿用产品新计划的不限读取默认值，工作台明确展示该预算；达到合格目标后停止。
- Campaign 编号为 `mac-arm-youtube-beauty-10-20261016-v1`，批准修订为 1。实际使用 23 次平台读取，其中 2 次搜索；最终返回 `target_reached`，推荐 10、待核实 1、拒绝 1。
- 实际账号隔离目录起初缺少配套只读适配器；安装仓库随附适配器后恢复证据读取。无法取得简介的视频保留真实停止记录，不计入推荐。
- Library 表为 `tblzewdBMX2SISd539u`，工作台返回地址为 `http://127.0.0.1:53400/library/tblzewdBMX2SISd539u`。实际页面显示 10 个作者分组、评分及 4 个公开邮箱；24 条来源尚未绑定可核实作者。
- 推荐账号为 Christen Dominique、Doctorly、Jooshica、kayli boyle、Embreigh Courtlyn、Slaybyjess、Alxandra、Samantha Harvey、KKAÖ by KKB、Emily Heal。Doctorly 按频道账号计一位，嘉宾不另计人数。
- 未创建 Creator，未发送邮件或消息。旧 Rust 插件继续保持原配置；新版宿主 MCP 状态卡的视觉验收仍未完成，SDK 状态核对不能代替该项。

## 未完成与继续条件

主程序完整套件、后续修复专项和各前端套件分别记录，不重复相加。15 项完整套件跳过项均依赖 Windows。本机已取得实际 Rust 构造器生成的 schema90 数据库，并完成依赖该数据库的 18 项专项；当前实际 OpenCLI 1.8.8 和扩展连接已回读。Mac 本轮同目录候选替换不代替旧 Rust 客户端完整升级验收。

以下项目继续保留未完成：

1. 正式 Codex 宿主状态卡视觉验收，以及同账号、同计划的工作台和动态表格联合验收。本轮使用独立 live MCP 客户端核对新版程序，未替换已有 Rust 插件或操作其账号数据。
2. 各平台真实账号、原生身份恢复及批准范围内的搜索验收。测试对照材料不代替真实平台验证。
3. Mac ARM 的真实旧版升级、完整客户安装和宿主重启验收，以及 Mac Intel 全部原生验收。
4. 许可全文、Windows r3 来源、公司当前 Access 源码及发行接口核实。
5. 三平台最终候选、签名、公证、发布及升级后验收。没有进行生产部署、下载切换或客户数据迁移。

本机代码签名身份检查返回 0 个有效身份；`notarytool` 可用，正式 Developer ID 签名及公证仍未完成。

Gmail 真实发送仍需具体发件人、收件人和内容授权。本轮未发送邮件或消息。
