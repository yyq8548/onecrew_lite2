# Windows 验证记录

## 当前 candidate-52 验证（2026-10-16）

- candidate-51 完整套件最终为 544 项：542 通过、1 失败、1 跳过。失败的旧版升级测试目录保存端口 50986，服务日志为 EACCES；Windows 实际 TCP 排除范围包含 50908–51007。没有把该轮整套记为通过。
- 新安装改为由操作系统分配 loopback 端口并保存，已有 profile、明确升级端口及端口占用时的失败行为保留。端口与运行时 5 项专项、类型检查通过。旧版升级测试使用真实可绑定的明确来源端口，以验证替换过程保留同一端口。
- 唯一跳过项原来只寻找 `~/.opencli`，已补充显式实际安装验证开关，使用产品自身安装解析器。真实 OpenCLI 1.8.8 的 5 项检查通过，无跳过、无平台访问。
- 新包程序 SHA-256 `eb1b201cf9b525e91e325c2b7618c122190ddfa2eab767bea5ad8f1d911fb387`；payload `917fe18315f83a853dc270b39b1d81dfbc375e2159eee71e43091008a40c39c4`；源码 `581d8b75f2d4e1bfbeabdbc449fd63baa6b494e34e415cf906e79f43bfa17a44`。ZIP 57,133,674 字节、SHA-256 `80228acb9285206892508b9ff982b244cbc90b5b1485a7054c439f863e94460d`。
- 实际旧版替换复验通过，日志 `.scratch/windows52-upgrade-final.log`：旧程序创建实际数据库，运行中新旧构建混用被拒绝，明确停止后替换并保留原端口。292 字符中文客户路径安装也通过，原安装文件移走后服务继续可用。类型检查通过，日志 `.scratch/windows52-typecheck.log`。
- 官方插件安装后完整重启 Codex，实际插件为 `0.7.1+codex.87b56e411320e1199f4ab5da890d757a81b12b4d7cb2ab473b42e7d65a198d08`，installed、fresh、enabled、diagnostic_code null。正式 live MCP 回读程序哈希匹配本包；服务 PID 50488、instance `33f46102-0931-493e-a91b-00805fa77f03`，保留原端口 62462，Access signed_in、startup ready。日志 `.scratch/windows52-codex-restart.log`。
- 最终完整主程序套件 545 项全部通过，0 失败、0 跳过，退出码 0，耗时约 21 分钟。启用实际安装 OpenCLI、SEA、原 Rust CLI 和真实数据库副本，覆盖整包冷暖启动、原生数据库、两套网页、MCP、缓存损坏、长路径、升级、环境继承、父 Job 和原 TaskScheduler 注册。日志 `.scratch/windows52-full-main.log`、`.scratch/windows52-full-main.stderr.log`、`.scratch/windows52-full-main.exit`。
- 最后源码指纹及锁文件哈希与实际运行包一致。包在本次 Git 提交前构建，manifest 记录原基线 checkoutCommit 和 dirty true；其他电脑重新构建时须重新记录哈希。
- 重启后的工作台、专用 Chrome 和原表格回读通过，原计划 finished、stale false、合格 1、待核实 0、读取 2 / 8、错误 0；自动同步 28 条来源、新增 0。Library 显示 Brandon Melville、约 6,910 粉丝、推荐候选人、良好和原生频道入口，未取得邮箱。截图 `.scratch/windows52-workbench-verified.png`、`.scratch/windows52-library-verified.png`。没有新增搜索或发送。
- 正式 onecrew_web 回读 Library error null、activity error null；宿主 MCP Apps 没有可检查的展开卡片，状态卡视觉及三界面联合视觉验收保持未完成。Mac ARM / Intel 未完成。`productionReady: false`，16 项许可全文仍需补齐，未进行正式发布或客户生产数据迁移。
- 保留当前 candidate-52、上一版 51 和升级测试引用的 46；累计清理 32 份不用的旧候选及完成的本任务宿主任务，源码、账号、浏览器和必要日志保留。按用户要求，源码推送后暂停后续开发。

## candidate-51 已执行验证（2026-10-16）

- 来源仍为 GitHub `8d931ab08448bcded456c210ebc29ad9709a68a5` 的批准范围。保留原修复及专用 Chrome，对外发送仅邮件页 GmailBulk；没有发送邮件或消息。
- Windows 程序 SHA-256 `e0b38be33f0b77a2baf3789edcd79c68d9944e2c1bcb33b5a1b88ae6b24a697a`；payload `e372677c66374b5dd6697b873892777080e500cd55bc6cc2aa50ee1dba560f35`；源码 `9257f7be1d1c183c82689e781cad89321eff06688b072bb5fdf741d76af49047`。ZIP 57,133,715 字节，SHA-256 `a0d961e852861d2749c408461d45a40eef87471af4b98913e1d9275b17125b6f`。
- 客户路径设计见 `13-customer-installation-paths.md`。真实桌面宿主文件重定向回归先失败后通过，物理根目录由完整性标记确定，并继续核验全部权限与文件哈希。没有放宽卡片的别名和文件身份检查。
- 真实 292 字符中文含空格程序路径先暴露运行时自身路径截断，现由 Windows QueryFullProcessImageName 返回实际进程映像路径，统一供服务、构建身份、插件和迁移指纹使用。未依赖 argv、当前目录或本机用户名猜测程序位置。
- 私有服务程序按完整 SHA-256 保存到数据目录 `.windows-host/bin`，原生安装锁串行化并发保存。实际移走原安装文件后，同一服务保持可用。不同构建明确返回 `service.build_update_required`，旧服务保留；明确停止后替换并保留原端口。
- 8 项真实专项全部通过，无跳过：缓存损坏拒绝、桌面宿主物理资源路径、整包冷暖启动和数据库/两套网页/MCP、292 字符客户路径及插件准备、旧版替换、并发安装、数据环境继承和父 Job 退出。日志 `.scratch/windows51-native-final.log`；类型检查 `.scratch/windows51-typecheck.log`。产品源码未发现本机用户名、工作区或预览目录硬编码。
- 服务使用原数据目录和端口 62462，PID 51564、instance `dea7c4cc-9cdf-42e6-819b-d794a24916ee`。官方安装插件 `0.7.1+codex.ae356579492c1964bd6bd91772e5a616672d7200447dbf24f8b7da4f9041fd9c` 后完整重启 Codex，服务进程和实例均保留。正式 live MCP 的程序、源码和依赖哈希匹配当前包，Access signed_in、数据 ready、startup ready，日志 `.scratch/windows51-codex-restart.log`。
- 最终完整套件在独立 Windows 原生任务中运行，启用实际 SEA、两个 Rust current 副本、旧原生 CLI 与 TaskScheduler 验证；日志 `.scratch/windows51-full-main.log`、`.scratch/windows51-full-main.stderr.log`，退出码 `.scratch/windows51-full-main.exit`。先前完整套件和专项结果分别保留，不将旧候选的通过数视为本次完整通过数。
- 重启后工作台恢复同一计划，合格 1、待核实 0、读取 2 / 8 次；自动同步 28 条线索、本次新增 0 条，结果表格链接指向 `tblPbF5GN9CfZQJ07Cf`。Library 实际行仍显示 Brandon Melville、约 6,910 粉丝、推荐候选人、良好及原生频道链接，截图 `.scratch/windows51-library-verified.png`。专用 Chrome `xrcq5ac3` 再次通过归属挑战和身份选择。
- 正式 MCP 状态和卡片数据回读为 finished、stale false、activity error null、Library error null；宿主 MCP Apps 接口仍为空标签列表，卡片视觉和三界面联合视觉验收未完成。Mac ARM / Intel 未完成。远端同步协议未自动接受；`productionReady: false`，16 项许可全文缺失仍是发布门槛。
- 按用户授权累计清理了 31 份不再使用的旧构建及两个已完成的本任务宿主任务。保留当前 51、上一版 50 和最终升级测试引用的 46；源码、账号、Chrome 身份和必要日志未清理。

## candidate-45 历史验证（2026-10-16）

- 来源仍为 GitHub `8d931ab08448bcded456c210ebc29ad9709a68a5` 的批准范围，保留专用 Chrome 与现有修复；对外发送仅邮件页 GmailBulk，没有发送真实邮件。
- Windows x64 程序 SHA-256 `f556fb0e0b92bbd6718789fa97dcc51198814579bde3469957f22e7ff7c3df1d`；payload `b8ca6d52a4fd7d84f880b612ca9f6a87a301ae3b50acb659d6ad6e0d7a61c754`；源码 `693ed796bdb8723c333e4f6dff142b1a8f5da1e5e090e0424c066d82053f39af`。ZIP 57,130,273 字节，SHA-256 `37bedc579ee7f5c1cba50dda59512a5ee452314844986266a1b047a775af59f8`。
- 完整主程序套件 539 项：534 通过、5 跳过、0 失败，日志 `.scratch/windows-host-full-main.log`。跳过项为两个真实 Rust current 数据库副本、SEA 整包、父 Job 生命周期、旧计划任务专项；分别进行显式环境验收，不将跳过计为通过。Discovery 129、工作台 824、适配器 348 项已通过。
- Windows 按需服务由当前用户、最低权限、无触发器的原生计划任务托管，避免随 Codex 的 kill-on-close Job 退出。任务仅绑定本数据目录、程序及脚本哈希，正常停止移除自身任务。逻辑缓存路径解析为物理路径后调用脚本。
- 真实 Job 测试确认 kill-on-close 标志、父进程确实入 Job，并检查服务在 Job 外；candidate-44 复验与独立复验均通过。一次并行验收记录 EACCES，追加子进程错误日志后并行复验未重现，保留原失败记录，不认定已定位其根因。
- 真实计划任务进一步复现 LOCALAPPDATA 未继承的问题。candidate-45 显式绑定四个数据路径变量的物理路径，未设置的保持未设置；不保存其他环境变量。真实任务回归修改前失败、修改后通过，日志 `.scratch/windows-host-environment-red.log`、`.scratch/windows-host-environment-green.log`；类型检查通过 `.scratch/windows45-typecheck.log`。
- candidate-44 的实际工作台、专用 Chrome 挑战与动态表格回读已通过。同一计划 `84cfef4f-dc4f-4137-957a-ad7540e1b941` 合格 1、待核实 0、使用 2 / 8 次读取、错误 0；表格 `tblPbF5GN9CfZQJ07Cf` 显示 Brandon Melville。重复同步 28 条来源、本次新增 0。live 状态为 finished、stale false、activity error null。
- candidate-45 的真实 CLI 生命周期、SEA 整包、父 Job 生命周期、环境路径及两个 Rust current 副本等 22 项全部通过，无跳过，日志 `.scratch/windows45-native-final.log`。修复版使用原数据目录和端口 62462，PID 14120、instance `ecef6e07-0565-472e-aae6-70fd7cb8232f`。最终源码完整套件正在独立原生任务中重跑，日志 `.scratch/windows45-full-main.log`；退出码写入同名前缀 `.exit` 文件。
- 配套插件的最终宿主重启验收正在执行。当前宿主仍加载 candidate-42 插件；没有将服务升级视为插件升级成功。
- 状态卡 MCP 数据回读通过，宿主 MCP Apps 接口无可读标签，视觉验收未完成。Mac ARM / Intel 验收保持未完成。远端同步协议未自动接受。`productionReady: false`，16 项许可全文缺失仍为发布门槛。

## candidate-42 历史验证（2026-10-16）

GitHub 同步来源为 `8d931ab08448bcded456c210ebc29ad9709a68a5`，具体范围见 `12-upstream-sync-20261016.md`。已有 Windows 修复保留，专用 Chrome 仍为 `xrcq5ac3`。没有发送邮件或其他消息。

- Windows x64 程序 SHA-256 `63defd161ccefee0bb02c2c90b1eea01217f529e7fbada79c01c2251c0ca18ac`；payload `375dc7d458d12a3cd35f0cea825de9218f890b1c417a68de907270c66b7bf9d9`；源码 `ace95aa8d145745e091528611f2d3ffd01df4a0edb154014a41f2ebc2aca79b3`。
- ZIP 57,126,405 字节，SHA-256 `886df6085e602e3bfadcb72c0566cabf1a83f3934035d5d5934b453cd8fcb7c5`。主程序、MCP 资源、工作台、Library 与只读适配器来自同一 manifest。
- 最终 Discovery 129 项、工作台 824 项、适配器 348 项通过。Unicode 原生权限 15 项、真实 Rust SQLite 快照 18 项、激活 12 项与 Windows 真实计划任务隔离验证通过。完整主程序套件早先失败已逐项修复和复验，不把早先整套记录标为全绿。
- candidate-42 整包冷启动、同实例重复启动、同端口重启、SQLite/Vault/PGlite、两套网页、stdio MCP 资源、ZIP 哈希与缓存损坏拒绝通过，耗时约 91 秒：`.scratch/windows42-sea-acceptance.log`。完整 Discovery 129 项最终日志 `.scratch/upstream-discovery-final-05.log`，类型检查 `.scratch/workspace-recovery-typecheck-02.log`。
- 实际 UI 已通过“备份并更新只读适配器”修复旧文件冲突，原文件保存在账号私有备份。13 个搜索入口恢复注册，Pinterest 为 `search-users`。同一专用 Chrome 再次通过真实绑定验证。
- candidate-40 的正式 UI 生成并批准新计划 `84cfef4f-dc4f-4137-957a-ad7540e1b941`，保留源计划引用，英文、目标 1 人、最多 8 次读取、无新增粉丝门槛、公开邮箱软偏好，且无错误的执行类作者条件。实际只读搜索使用 2 次读取，合格 1、待核实 0、读取错误 0、target_reached，28 条来源已同步。当前表格 `tblPbF5GN9CfZQJ07Cf` 实际显示 Brandon Melville、约 6,910 粉丝、推荐候选人、良好及原生频道 `UCsv6YMh5kLhixuepSEzHKzg`，27 条来源尚未绑定作者。仅取得原生频道联系入口。截图 `.scratch/windows40-library-verified.png`。
- Access 短暂 unavailable 后恢复同一账号时，发现指标缓存引用旧 epoch，状态卡持续报 access.account_changed。真实 SQLite / AccountContext 回归先复现失败，再通过将指标缓存绑定 epoch 修复；旧会话继续拒绝，新会话读取相同持久计数与结果。修复后的完整 Discovery 129 项及类型检查通过，candidate-42 包含此修复。
- Gmail 实际刷新后原 Gmail 账号出现在可用发件人列表；无收件人时下一步禁用。没有创建发送批次、确认发送或执行提供方发送验收。
- Codex 完整重启后，正式宿主 MCP 进程实际加载 `0.7.1+codex.ac97aee23f7981d8a866b1a09841c50faf2b5cce3748c4ef253ec5599131f35e`；安装路径的程序 SHA-256 与 candidate-42 完全一致。`codex-status --refresh` 返回 installed、fresh、enabled、diagnostic_code null，日志 `.scratch/windows42-postrestart-plugin-status.log`。正式 live MCP 再次回读同一程序及源码哈希，数据 ready、Access signed_in。重启后恢复同一服务目录，PID 40976、instance `aeb4355a-98e6-483e-8522-2ffaa3f496cc`、端口 62462。
- 重启后 `onecrew_status`、`onecrew_web` 已重新返回只读状态，Library error null、activity error null；当前没有页面心跳激活的计划，不将空 activity 说成结果丢失。MCP Apps 入口仍无可读卡片标签，状态卡视觉检查未完成。本次 MCP App 请求限定只刷新状态、不打开浏览器，重启后的三界面联合回读保持未完成；重启前 candidate-42 的工作台和同一动态表格回读均已通过。
- 最后一次状态刷新又观察到远端登录验证 unavailable，随后 `onecrew_web` 恢复返回工作台及 Library 地址；远端验证仍有间歇性故障，未据此宣布连接持续正常或全部 Windows 验收完成。
- Mac ARM、Mac Intel 验收保持未完成。远端表格同步仍需用户同意协议，未自动接受。`productionReady: false`，16 项授权全文缺失仍为发布门槛。

## candidate-35 验收（2026-10-16）

GitHub 基线为 `efc3c6eb69507b9809bb30279d9af26156d075e3`。保留专用 Chrome，官方 OpenCLI 扩展为 1.0.24，daemon 为 1.8.8，实际身份为 `xrcq5ac3`。本轮没有发送邮件或其他对外消息。

- 程序 SHA-256：`e0cec8c6295d43f9cf065f6840378ef29fed92281bb3d03d9224007fcf3f11df`；payload：`0ccdd91eebc4ba27813a60b5e3fc3c0b6075cdda57254b5aa16bcd0000a79e58`。
- 实现源码 SHA-256：`3d4dbd0018664c6640674ec6aeb4636c7c837c290dd718d5d28cc61355137cfb`。ZIP 为 57,113,684 字节，SHA-256 `017c2f74d1409598b6b35a91f38c8588f641fae8fdf62293b8d60b87be778e75`。
- 预览数据仍使用 `.scratch/candidate-preview/data`。重启后 PID 12580，instance `cf457a08-78d2-4ee9-89bd-3034a27676f3`，端口 62462。实际 SDK 回读程序哈希匹配、Access signed_in、数据 ready；工作台显示对应程序哈希和“本地服务已连接”。
- 真实测试计划 `716b2050-97fb-45c4-a20c-8e59cd49e7bf` 在 YouTube 搜索 Playwright 教程：2 次实际读取，20 条来源，核实并评分 1 名作者 Testopic；合格 1、待核实 0、读取错误 0，停止原因为 `target_reached`。证据包括真实视频 `wGr5rz8WGCE` 与原生频道 `UCyyRxky_FxZ45H6B92Ttfpw`。仅取得原生频道联系入口，没有声称找到邮箱或已触达。
- 当前动态表格 `tblS9O15yCxDYEOqlb2` 在内置浏览器实际显示 Testopic、约 5870 粉丝、推荐候选人、良好及原生频道链接。20 条来源形成 1 个已核实作者分组及 19 条尚未绑定作者的来源；重复同步新增 0 条。服务重启后回读仍一致。
- 原失败计划 `9e28d770-9c16-4f26-b6ad-e123ab090b20` 的 11 次作者读取错误及预算耗尽结果保留。实际故障为 OpenCLI 子进程从用户全局 HOME 加载 1.1.0 适配器；子进程 HOME/USERPROFILE 现绑定账号目录，实际返回账号 1.0.0 适配器并通过原严格解码。没有覆盖用户全局适配器或放宽输出合同。
- 已修复 Windows Codex 现有目录权限核验、SEA 物理资源路径、MCP 可选分页游标、浏览器临时绑定会话释放。实际 Codex CLI 隔离安装/回读/卸载通过 1 项；长路径卡片资源与 ClientAdapter 通过 3 项；OpenCLI 与 MCP 相关回归通过 12 项；退役操作及 MCP 回归通过 3 项，均无跳过。
- 停机回归发现浏览器未收完整请求头的连接阻塞退出；真实 TCP 测试修改前超时，修改后通过。服务关闭只读事件流及未进入业务处理的连接，已接受的业务请求继续排空。实际打开工作台和该动态表格时，candidate-35 正常停机用时 6,954 毫秒，旧实例记录已删除，同端口重新启动成功。相关 HTTP/runtime 回归 5 项通过，类型检查通过。
- 重启后从工作台重新打开同一专用 Chrome，实际挑战再次显示“已验证并选择此身份”。已安装扩展与签名绑定保留。
- 当前候选插件已通过官方 `connect-codex` 安装，并通过 `codex-status --refresh` 完整回读：`0.7.1+codex.d7d04dab858c50f7683907e8def811d4d13ad77121d06781bf2c4901b97125e8`，enabled、verification fresh，程序哈希与 candidate-35 相同。用户完成 Codex 重启后，正式宿主 live MCP 已回读 candidate-35 的精确程序及源码哈希；预览服务重新启动于端口 62462，PID 2872，instance `128aa926-5f9e-49de-bdbc-3d34927f17bc`，Access signed_in、数据 ready。
- 宿主重启后，内置浏览器工作台及当前 Library 表格实际恢复；专用 Chrome 再次完成绑定验证。切回当前任务并激活工作台后，live `onecrew_status` 和 `onecrew_web` 均回读同一计划 `716b2050-97fb-45c4-a20c-8e59cd49e7bf`，stage finished、qualified 1、pending 0、used 2、read_errors 0、stale false、activity error null，Library error null。未激活的后台页面不会抢占状态卡当前计划。
- 用户确认已展开状态卡。宿主的 MCP Apps 自动化入口仍返回空标签列表，因此已验证正式 MCP 与卡片返回数据，卡片视觉检查仍未完成；不将数据回读或 SDK 资源读取视为视觉验收。
- 远端表格上传因 `table_sync_consent_required` 保持未完成；本地 Library 发布和回读已通过。Gmail 仅保留邮件页批量发送，没有进行真实发送验收。Mac ARM、Mac Intel 原生验收保持未完成。`productionReady` 为 false，16 项授权文本缺失仍是发布门槛。

证据保存在忽略的 `.scratch`：`windows35-shutdown-tests.log`、`windows35-typecheck.log`、`windows35-live-plugin-install.log`、`windows-opencli-mcp-final-regression.log`、`windows34-mcp-retired.log`、`windows-live-codex-native.log`、`candidate-preview/data/windows34-candidate-evidence.json`。重启后的界面证据为 `windows35-library-verified.png`。

更新日期：2026-10-16。源码基线、保留范围和构建来源分别见 `source-ledger.json`、`08-current-scope.md` 和 `09-implementation-status.md`。各候选的结果保留其实际版本；当前结果见本文件末尾的 candidate-35 记录。

## 当前通过

环境为 Windows x64、Node 26.7.0、pnpm 10.32.1、Rust 1.98.0 MSVC。

- 新应用 `tsc --noEmit` 通过。
- 工作台 124 个测试文件、823 项测试通过；MCP UI 48 个测试文件、406 项测试通过。
- Discovery 完整回归 120 项通过，无跳过。包括真实 SQLite、PGlite、任务准入、重复启动、排空、指标和工作台活动。
- 根模块联合检查 24 项通过，无跳过。包括语言证据、Web 来源、自动查询扩展、数据状态、退役操作、Gmail 确认后唤醒及实际 MCP SDK。
- 语言证据使用原 Rust 三阶段数据库；Web 来源使用两种搜索引擎与三种原生平台的十八个数据库；查询扩展使用原 Rust 追加前后数据库。测试先验证文件 SHA-256，再比较业务字段和原子事务结果。
- 实际 Codex CLI 在只读原 Rust 搜索耗尽数据上生成七条 YouTube 新查询，保留批准平台和身份。没有执行平台搜索或对外发送。
- 官方 MCP SDK 实际连接统一服务，读取状态、卡片资源和两套网页地址；真实 stdio 子进程退出后撤销控制会话。未登录卡片登记正常返回无待发送消息，未认证管理请求返回 401。
- 冻结源码后，实际 SQLite/PGlite 激活候选 12 项通过，原 Rust schema90 数据库的 Creator CRUD 与联系路由 18 项通过，原生旧服务锁 1 项通过，Discovery runtime、Workspace 与任务监督器联合回归 10 项通过，均无跳过。
- 原 CLI 的真实 Windows 计划任务安装、私有 XML 备份、停用、停机、旧权限核验及恢复 staging 用例通过，耗时 50.2 秒。该用例使用独立目录和任务，执行后清理，不涉及用户原服务。
- 最终插件兼容合同精确限定产品 `0.7.1`，旧候选及其他版本继续拒绝。插件生成持有真实操作系统锁直到实际生成结束，成功与拒绝后均能重新加锁；相关 2 项回归通过。修正后的最终源码再次通过类型检查和 12 项激活候选检查，后者耗时 200.2 秒。
- 扩展应用回归实际执行 495 项。首次运行的 487 项通过、4 项失败、4 项条件跳过不作为全绿结果。失败分别涉及真实任务超过固定 30 秒轮询、Cargo 环境未包含 MSVC 工具链、构建期间源码变更触发 CAS；相关固定轮询已改为等待实际任务，随后在正确工具链与冻结源码下逐项复验通过。条件用例独立记录实际执行证据。

测试产生的私有数据库和密钥保存在忽略的 `.scratch` 目录。Windows 浏览器目录测试使用用户目录下按完整账号路径摘要隔离的 `.onecrew-browsers`，测试结束后清理其独立目录；真实预览的浏览器和诊断记录保留。测试未修改用户生产数据库或日常插件安装，也未发送邮件。

## 保留的验收边界

专用 Chrome 恢复、YouTube 实际搜索与公开作者证据读取已完成正面验收，见末尾记录。其他平台及 Gmail 提供方发送未完成正面验收。隔离数据库、输入拒绝和回执持久化测试不能替代这些证据。

## 最终 Windows 候选

本节记录 candidate-18 的整包隔离验收，产物目录为工作区 `.scratch/unified-sea-candidate-18/`，产品版本 `0.7.1`，目标 `win32-x64`。这是未发布候选，`productionReady: false`，源码修改尚未提交。当前运行预览为下方的 candidate-29，各候选验收证据分别记录。

- ZIP：`OneCrew-Lite-0.7.1-win32-x64-candidate.zip`，57,658,787 字节，SHA-256 `ac2ca43fa02b9835f6ad2fea1aad5d3f4d5f25cf51e417ff12888ce91551e6af`。
- 程序：`onecrew_lite.exe`，179,353,416 字节，SHA-256 `23384b35de5b9975676e13d5ccf46093819bff910f41c248b4f000f2435c2e78`。
- Payload SHA-256：`e3efb5eaf52360270ce06126c0bf1fb035dec74c7e46c66eb53492d95d4913ef`。
- 实现源码 SHA-256：`7654eab21d514c318df4efb594b0089fe5d8ca7c18653a5991ba41a1ac064d0c`；锁文件 SHA-256：`8fc24c3e75c91145277e177179d4c060cadda2835c993ffe368c634f0ac6e9d4`。

源码基线、dirty 状态、组件版本、资源文件哈希和授权证据保存在 `runtime-manifest.json`，候选及归档摘要分别在 `candidate.json`、`archive.json`。ZIP 哈希已从实际文件独立重算核对。最终包启动及插件验收结果在下方记录。

最终包使用实际 Codex CLI 在隔离 `CODEX_HOME` 完成插件安装、状态和文件哈希回读、卸载，1 项通过，无跳过，耗时 12.63 秒。此证据不代表日常 Codex 宿主已安装或显示候选卡片。

最终包冷启动、同实例重复启动、同端口重启、1,088 个资源文件的完整性、SQLite/Vault/PGlite、Telegram Wasm 初始化、两套网页资源、真实 stdio MCP 卡片、ZIP 大小及哈希、损坏缓存拒绝验收 2 项通过，无跳过，总耗时 51.58 秒。实际就绪耗时分别为冷启动 16.516 秒、同实例重复启动 3.529 秒、热缓存重启 9.391 秒。进程 working set 为 684,412,928 字节，private bytes 为 622,944,256 字节。没有测量原版同数据性能，不作性能等价结论。

最终证据文件为 `.scratch/sea-acceptance-e3efb5eaf5236027-203571bf.json`；最终类型检查、激活、原构造器数据库、原生锁与计划任务结果分别见 `.scratch/typecheck-final-20261015.log`、`.scratch/activation-final-20261015.log`、`.scratch/creator-original-final-20261015.log`、`.scratch/native-quiescence-final.log` 和 `.scratch/optional-native-final-20261015.log`。最后一份日志同时包含使用未合并 WAL 数据库导致的两项输入错误；这两项已改用正确独立原构造器数据库，在 `creator-original-final-20261015.log` 中全部通过。

候选清单报告 16 项 `licenseTextMissing`，包含内部 workspace 包与 `kysely-pglite-dialect` 等，完整名单以候选 `candidate.json` 为准。`kysely-pglite-dialect` 1.3.1 包含 MIT 声明，npm 包与 2026-10-15 检查的上游 GitHub 主分支均未提供授权全文。发布门槛继续保留 `license_text_missing`，没有补造授权文本。

正式 Codex 宿主安装、三个界面同时回读、当前公司 Access 服务兼容、生产数据迁移和发布尚未完成。Mac ARM 与 Mac Intel 均未验收；用户确认暂无可用设备，Windows 结果不替代 Mac 结果。

## 当前运行预览

当前运行 candidate-29。正常未登录状态保留 `pairing_required`，`issue` 为 null；已登录状态使用预览目录中现有的真实授权。Windows 权限检查复用短时存活的解释器，每次请求仍执行原始完整 ACL、owner 和 reparse 检查，不缓存权限结果。连续八次实际文件检查从 11,631 毫秒降为 86 毫秒；权限更改后的拒绝、恢复和链接路径拒绝均通过。打包显式采用 Library 编译配置，避免供应商目录的源码路径映射将同一依赖打包两次；实际表格列表已无依赖注册错误。

- 程序目录：`.scratch/unified-sea-candidate-29/`，程序 SHA-256 `f3a8e7be9ddca2497982320b176d5a87eb65502c6b3b08171e6d655821087e7a`。
- ZIP：57,112,725 字节，SHA-256 `43026cd358eb1fb85c3f1e8b16cb43b1db00809adc164efa0036ee2cf6b33aa8`。
- 实现源码 SHA-256：`931b4bd6f5ad0c6c81e2b637978ff00c66a8464c329f2e7023f16e1f86055b37`。
- 独立数据目录：`.scratch/candidate-preview/data/`；工作台地址 `http://127.0.0.1:62462/`，Library 为同源 `/library`。
- 实际进程版本为 `0.7.1`，本地数据 ready，账号 signed_in，Library 资源 ready。内置浏览器已回读工作台的“本地服务已连接”和 Library 的真实“0 个表格”结果。当前预览账号没有计划和表格；动态表格与候选 MCP 状态卡未完成正式宿主挂载验收。
- 实际热刷新测量：工作台主界面出现用时 2,822 毫秒，Library 表格列表出现用时 403 毫秒。实际完整状态接口从 16,287 毫秒降为 225 毫秒，登录状态从 12,070 毫秒降为 126 毫秒；重启后的首次登录核验另测为 2,350 毫秒。
- 相关权限、原 Vault、升级和账号隔离回归 20 项通过；最终类型检查通过。candidate-22 的冷/热启动、SQLite/PGlite、Wasm、两套网页、MCP 与缓存完整性实际包验收 2 项通过，无跳过。日志为 `.scratch/preview-performance-regression.log`、`.scratch/preview-performance-security.log`、`.scratch/preview-performance-sea-final-22.log`。

没有发送邮件、迁移生产数据或替换日常插件。预览进程保持运行，两个内置浏览器标签页保持打开。当前 OpenCLI 和浏览器绑定状态见下方记录；没有以页面加载完成推断搜索能力已就绪。

candidate-23 使用 .NET SHA-256 校验实际 Chrome 程序，保留 PID、程序路径、程序哈希和启动时间检查，避免依赖宿主缺失的 `Get-FileHash` 命令。真实 Chrome 启动、正常关闭和错误哈希拒绝，以及既有浏览器归属、挑战检查共 5 项回归通过，无跳过；类型检查通过。实际候选进程哈希已回读匹配；内置浏览器真实发起连接后，专用 Chrome 保持运行，连接流程进入等待连接扩展状态，没有启动错误。日志为 `.scratch/browser-connect-fix-regression.log`。

candidate-24 保留专用浏览器，恢复后台网络、组件更新及 phishing detection，不再使用禁止这些功能的启动参数；仅在显式 loopback 代理场景禁用 QUIC。实际 Chrome 进程参数检查、启动关闭、错误哈希拒绝、代理和浏览器归属共 4 项回归通过，类型检查通过。运行包哈希已回读匹配，新专用 Chrome 的实际进程参数中上述禁用项均不存在。独立 HTTP 探针从 Google 官方取得 48,465 字节 OpenCLI 扩展包，用时 651 毫秒；该探针不安装扩展，也不能替代 Chrome 商店完整安装验收。日志为 `.scratch/browser-interactive-arguments-after.log`。

candidate-26 根据实际 Chrome 日志修复 Windows 浏览器目录过长问题。原目录为 202 字符；商店下载附加随机文件名后达到 264 字符。使用未声明长路径支持的实际 Win32 文件进程复现错误码 3；Chrome 自身日志进一步记录缓存目录 `0xCE` 和扩展语言文件读取失败。新建专用浏览器使用用户目录 `.onecrew-browsers`，按账号数据目录完整 SHA-256 隔离，以 80 位随机目录名创建私有身份，并限制实际规范路径长度不超过 140 字符。避开商店宿主对 LocalAppData 的路径重定向，原目录及旧绑定继续保留，签名、文件身份、私有权限和进程归属检查继续执行。

最终浏览器、Vault、账号隔离、代理、HTTP 会话和权限回归 25 项通过，无跳过；类型检查通过。真实有授权预览经过超过 30 秒的在线核验后，原浏览器会话仍有效，独立实际测试 1 项通过。原生文件回归覆盖商店下载、临时后缀、Zone.Identifier、扩展语言文件和 GPU 缓存；修改前失败，修改后通过。日志为 `.scratch/browser-download-path-before.log`、`.scratch/browser-short-root-regression-final.log`、`.scratch/browser-short-root-typecheck-final.log` 和 `.scratch/access-session-refresh-before.log`。

实际 Chrome 在 112 字符规范路径下启动，12 秒原生启动探针正常关闭后读取完整日志，路径过长及扩展语言文件错误为零，偏好文件成功保存。最终运行包实际发起的新专用 Chrome 也使用 112 字符目录，父进程为当前 candidate-26，启动日志同类错误为零。工作台已连接，Library 真实返回 0 个表格。诊断由 `ONECREW_BROWSER_DIAGNOSTICS=1` 显式启用，日志仅写入该专用私有目录，默认不启用。

candidate-27 在 Windows 显式诊断模式中启用 Chromium 官方 `--force-renderer-accessibility=complete`，用于向原生辅助功能接口提供真实页面与网址信息；普通启动不增加该参数。实际 Chrome 参数、正常关闭、程序哈希拒绝及既有目录归属与挑战回归 4 项通过，类型检查通过。原生窗口控制工具已读取实际验证页与 Chrome 地址栏，通过其网址安全检查，没有改写、跳过或代替该检查。用户明确批准安装及所列扩展权限后，通过 Chrome 商店正常安装官方 OpenCLI 1.0.24，商店显示 Remove from Chrome，扩展弹窗显示 Connected to daemon、daemon 1.8.8、专用身份 xrcq5ac3。证据包括 `.scratch/opencli-dedicated-connected.png` 和原生参数测试日志。

candidate-28 修复实际绑定失败：OpenCLI 1.8.8 的 tab list 仅列出该会话的标签页，新建会话返回空列表；验证先使用正式 browser bind 绑定当前验证页，再执行原有精确 URL、一次性随机码、顶层页面、身份路由和进程归属检查，最后 unbind 释放临时会话。实际探针修改前报 browser.managed_challenge_tab_required，修改后通过。安装后未完成的专用目录仅在同一账号私有根目录内恢复，多个候选明确拒绝；扩展文件存在不视为所有权证明，登记仍要求完整随机码验证。验证失败时保留仍有效的窗口供重试；账号取消和登记失败继续清理。

candidate-28 已通过实际工作台操作登记格式 2 的绑定，身份为 xrcq5ac3，规范目录长度 112 字符。界面显示“浏览器归属已验证”和“已验证并选择此身份”。随后正常停止并启动 candidate-29，保留安装与签名绑定记录。candidate-29 移除 Chrome 会提示不支持的网络日志参数，保留私有普通诊断日志和辅助功能参数。

最终浏览器进程、目录、代理、会话、账号归属等 10 项回归通过，无跳过；类型检查和前端构建通过。真实已安装官方扩展的独立测试 1 项通过，用时 11.49 秒：错误挑战被拒绝，同一窗口继续有效，正确挑战通过，证明消费后和随机码重复使用均被拒绝。真实授权会话跨三次在线核验的测试通过，用时 103.58 秒。日志分别为 `.scratch/browser-binding-final-regression.log`、`.scratch/browser-binding-final-typecheck.log`、`.scratch/browser-bind-web-build.log`、`.scratch/browser-extension-live-final.log` 和 `.scratch/access-session-refresh-repeated-before.log`。临时 DEBUG 日志已从源码删除。

candidate-29 正常重启后，实际程序路径、版本与 binary_sha256 回读匹配候选清单，数据 ready，账号 signed_in。内置工作台读取原绑定并实际执行“重新打开 xrcq5ac3”和“验证并连接此浏览器”，再次显示“浏览器归属已验证”及“已验证并选择此身份”；绑定记录仍只有一个。专用 Chrome 保持运行，扩展弹窗再次显示 OpenCLI 1.0.24、Connected to daemon、daemon 1.8.8、Profile xrcq5ac3，截图为 `.scratch/opencli-restart-verified.png`。Library 重启后读取实际账号并返回 0 个表格。此结果完成专用浏览器扩展安装与绑定验证，不替代正式插件宿主状态卡、当前计划动态表格、Mac 原生及 Gmail 发送验收。
