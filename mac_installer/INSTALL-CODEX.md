# OneCrew Lite 0.9.9 · Mac ARM64 安装指南

由用户 Mac 上的当前 AI 客户端执行。目标为 Apple Silicon Mac，程序已包含运行环境。安装验收保留账号、数据、主密钥、端口、原 Chrome 身份、证据、评分和结果，搜索保持停止。

## 1. 目标、授权与权限

使用 `uname -s` 确认 Darwin，并结合 `uname -m`、`sysctl -n hw.optional.arm64` 确认 Apple Silicon。Rosetta 下使用原生 arm64 运行环境；Intel Mac 停止，因为本包只支持 ARM64。

安装请求授权 AI 下载和校验本包、写入用户自己的安装目录、升级原数据、启动或停止已核实归属的 Lite 服务、配置所选客户端的 Lite 插件或 MCP 连接，并检查本地连接。命令由 AI 执行；用户完成登录、浏览器授权、必要的安全同意和客户端重启。

当 sandbox、`Operation not permitted`、`EPERM`、`EACCES`、网络限制或本地端口绑定限制阻止必要操作时，使用客户端支持的权限审批流程，说明具体命令、目录和用途。Codex 的命令工具提供 `sandbox_permissions` 时，可为该命令申请 `require_escalated`；其他客户端使用自己的同等流程。批准后重试受阻步骤并继续，不把 sandbox 拒绝直接判为程序或数据故障。仅在 macOS 明确要求时申请管理员权限。保持客户端安全配置和系统保护原状。

审批被拒、不可用，或批准后仍失败时，保存该命令、路径、退出码和错误，报告仍需用户完成的权限操作。继续执行不依赖该步骤的必要检查；保持原服务和数据可恢复。进程枚举不是安装必需步骤，已核实的服务和配套客户端元数据足够时直接复用。

在用户选定的工作目录保存非敏感安装断点，记录输入哈希、已完成检查、已确认路径、服务与插件状态、受阻步骤和下一步。继续时复用仍然有效的检查。主密钥、Cookie、控制令牌及私有服务描述文件内容保持私密。

权限机制参考 [OpenAI 官方说明](https://learn.chatgpt.com/docs/agent-approvals-security)。安装提示词授权具体工作，实际权限由客户端和操作系统授予。

## 2. 获取并验证安装包

使用本目录或用户提供的以下同一组文件：

- `OneCrew-Lite-0.9.9-Mac-arm64.zip`
- `OneCrew-Lite-0.9.9-Mac-arm64.zip.release.json`
- `OneCrew-Lite-0.9.9-Mac-arm64.zip.sha256`

本版使用本目录或用户提供的安装包与同名校验文件，尚未发布到 npm 或官网；安装提示词见 [CODEX-PROMPT.md](CODEX-PROMPT.md)。已通过 ZIP 或 npm 安装的用户均按下文识别原安装并升级，保留原数据。

用 `plutil -p` 读取发布记录，确认 `product=onecrew_lite`、`version=0.9.9`、`target=darwin-arm64`。记录 ZIP 的 `bytes`、`sha256`、程序的 `executableSha256`、源码提交和摘要。以 `stat -f '%z'` 和 `shasum -a 256` 核对 ZIP 的确切大小和 SHA-256。缺少记录或校验不符时停止。

在用户工作目录中用 `/usr/bin/ditto -x -k` 解压到一个新的目录。将 `packageDir` 设置为其中 `OneCrew-Lite` 文件夹的实际绝对路径。执行：

```sh
cd "$packageDir"
shasum -a 256 -c SHA256SUMS
plutil -p candidate.json
shasum -a 256 onecrew_lite
chmod 755 "$packageDir/onecrew_lite"
/usr/bin/codesign --verify --strict "$packageDir/onecrew_lite"
```

要求所有文件通过校验，`candidate.json` 的版本、架构及程序哈希与外部记录和实际文件一致。本 CLI 使用命令和 JSON 状态输出，不支持 `--help` 或 `--version`。

## 3. 下载隔离标记

首次执行前，用 `xattr` 检查已校验程序的属性名称。此包为 ad-hoc 签名且未经过 Apple 公证，签名校验通过只证明签名完整性。

存在 `com.apple.quarantine` 且全部完整性与签名检查通过时，显示已核实的程序绝对路径，并询问一次：

> 安装包校验已通过，当前为未公证版本。是否同意仅移除这个 OneCrew Lite 程序的下载隔离标记，并继续安装？回复“同意安装”即可。

当前用户明确同意后，使用安全引用的绝对路径对该程序执行 `xattr -d com.apple.quarantine`，确认属性已消失，并重新核对哈希和签名，再继续断点。不递归处理其他文件；保持 Gatekeeper、SIP 及其他系统保护。没有该属性时直接继续，无须询问这一项。工具仍要求批准时，按第 1 节处理。

退出码 137 或无输出被终止不足以证明隔离或数据不兼容；记录真实错误和检查结果，保留正常旧服务，避免重复启动。具体安全操作完成后仅重试受阻的只读状态检查一次。

## 4. 确认原安装和数据

从已授权的 Lite 服务、启动记录、客户端配置或配套插件 `.mcp.json` 核实原程序、数据目录、vault、主密钥路径和端口。配套插件的 `ONECREW_LITE_MCP_DATA_DIR` 可提供原数据目录。只记录非敏感路径及状态，保留原账号和浏览器身份。多个真实候选且现有证据无法确定时才让用户选择。Mac 不调用仅支持 Windows 的 `installation-discover`。

识别原数据布局：已有 `current.json` 或完整 `tables/pglite` 的统一安装直接沿用；已有 `account-*` 账号目录但表格仍在独立 Library 的安装，按第 5 节迁入表格；旧 Rust 安装的 `crm.db`、`agent-platform.db`、`vault.key` 和独立 Library 则按第 5 节完整迁移。历史文件名以已核实配置为准。`data-upgrade` 只处理账号目录中的 Creator SQLite，返回 `current` 不能证明 Library 已迁移。

从独立 Library 的原服务记录核实 `libraryDataDir`。旧 Mac 常见路径为 `~/Library/Application Support/io.onecrewai.onecrew_lite` 和 `~/Library/Application Support/onecrew_lite_tables`，只用于查找，不能据此替代实际核实。数据、密钥缺失或归属冲突时停止该步骤；没有原安装或数据时才使用默认目录 `~/Library/Application Support/OneCrewLite/TypeScript`，由启动流程初始化。

将 `dataDir`、`vaultPath`、`masterKeyPath`、`installDir` 设为已核实的绝对路径。首次安装默认使用 `~/Applications/OneCrewLite/releases/0.9.9`；已有安装使用新的独立版本目录，保留旧程序直到验收完成。新数据目录使用 `install -d -m 700 "$dataDir"`，已有目录保持原样。

## 5. 安装、升级和启动

对已有统一安装执行新程序的只读状态检查：

```sh
"$packageDir/onecrew_lite" status --data-dir "$dataDir"
```

原程序及运行服务的哈希已等于本发布记录，且数据正常时，复用它们并继续客户端验收。相同版本号但不同哈希仍按升级处理。

返回 `cli.failed` 时，对同一目录用 `ONECREW_LITE_DIAGNOSTICS=1` 重试该只读命令一次，保留真实诊断及断点。权限错误按第 1 节审批；其他错误只依据实际证据处理，保留旧服务。文件名差异或通用错误码不能证明数据不兼容。

确认需要升级且原服务归属明确后，用原安装支持的控制命令停止原主服务和独立 Library。新统一安装可以执行以下 `stop`；旧 Rust 服务使用原安装的已核实命令。不得用新 CLI 对旧目录返回的通用错误推断旧服务已经停止。实际迁移还会持有原服务与数据库的原生锁，活动 writer 会明确拒绝迁移。

```sh
"$packageDir/onecrew_lite" stop --data-dir "$dataDir"
mkdir -p "$installDir"
/usr/bin/ditto "$packageDir" "$installDir"
chmod 755 "$installDir/onecrew_lite"
shasum -a 256 "$installDir/onecrew_lite"
/usr/bin/codesign --verify --strict "$installDir/onecrew_lite"
```

再次确认安装后哈希。已有 `account-*` 账号目录、没有 `current.json` 的安装，在服务停止时执行：

```sh
"$installDir/onecrew_lite" data-upgrade --data-dir "$dataDir"
```

要求 `state=upgraded` 或 `state=current`，保留自动备份。若表格仍在独立 Library，继续执行：

```sh
"$installDir/onecrew_lite" library-migrate --data-dir "$dataDir" --library-data-dir "$libraryDataDir" --vault "$vaultPath" --master-key "$masterKeyPath"
```

旧 Rust 根目录布局使用完整迁移：将 `legacyDataDir`、`creatorDb`、`vaultPath`、`masterKeyPath`、`libraryDataDir` 和 `legacyPort` 设为已核实原值；`dataDir` 为与旧主目录和 Library 均不嵌套的新私有统一目录。这个目录只接收通过验证的完整旧数据副本。执行：

```sh
"$installDir/onecrew_lite" library-migrate --data-dir "$dataDir" --legacy-data-dir "$legacyDataDir" --creator-db "$creatorDb" --library-data-dir "$libraryDataDir" --vault "$vaultPath" --master-key "$masterKeyPath" --legacy-port "$legacyPort"
```

包内程序自动使用内置网页资源。源代码开发运行需额外提供 `--web-assets` 和 `--library-web-assets`，用户安装不需要这些参数。要求迁移返回 `state=migrated` 或 `state=current`、`source_preserved=true`、`search_resumed=false`。保存 `recovery_directory`、表格数量和原安装身份检查结果。将后续启动和客户端连接使用的 `dataDir`、`vaultPath`、`masterKeyPath` 更新为迁移返回的 `data_directory`、`vault_path`、`master_key_path`，不继续使用旧根目录路径。

迁移会验证全部原表格和行的内容摘要、账号与 tenant/base 的归属、原登录和 installation ID。原主库、独立 Library 和密钥保留；重复执行同一迁移返回 `current`。活动服务、数据变更、身份冲突、未知目标库或在线账号验证失败时停止切换，保留原安装和恢复副本，报告具体错误。不要手工复制 PGlite、伪造 `current.json`、创建空 `tables` 或重新注册账号绕过失败。首次安装跳过升级与迁移。

```sh
"$installDir/onecrew_lite" start --data-dir "$dataDir" --vault "$vaultPath" --master-key "$masterKeyPath"
"$installDir/onecrew_lite" status --data-dir "$dataDir"
```

已有端口时，`start` 增加 `--legacy-port` 和该实际端口；首次安装由程序分配。要求 `state=running`，服务身份中的版本和哈希与发布记录一致。保留原账号、数据、主密钥、端口及浏览器身份，禁止换空目录绕过失败。

## 6. 连接所选 AI 客户端

Codex 执行：

```sh
"$installDir/onecrew_lite" codex-package --data-dir "$dataDir"
"$installDir/onecrew_lite" connect-codex --data-dir "$dataDir" --refresh
"$installDir/onecrew_lite" codex-status --data-dir "$dataDir" --refresh
```

要求配套插件为本机生成的完整 `0.9.9+codex.<64 位摘要>`，已安装、已启用并完成新鲜核实。完整摘要与本机程序和数据路径相关，不套用打包电脑的摘要。找不到已安装的 Codex CLI 时定位当前客户端附带的 CLI，通过 `--codex-executable` 指定绝对路径。保留其他插件和配置。

完成注册后，若宿主仍加载旧配套进程，集中请求一次重载或重启；之后只执行尚未完成的 live MCP 和页面验收，复用已完成安装和授权。`mcp_operation_output_contract_mismatch` 需要核对运行中配套进程，不能当成平台读取失败。

其他 MCP 客户端使用已安装程序的绝对路径，参数为 `mcp --stdio --data-dir <原数据目录的绝对路径>`，保留同一 Lite 连接。客户端管理 stdio 桥接，主服务仍需独立运行。规划与评分使用当前对话的可用模型，不要求常驻 App Server 或额外模型 API Key。以实际客户端工具能力判断可用性。

## 7. 一键修复和验收

用户说“一键修复”时，直接调用 `onecrew_repair`，主 HTTP 服务停止时仍可调用。复用一个 `request_id` 和所选客户端，等待同次修复的结果；根据返回的诊断执行可用且针对已发现问题的修复操作，最后调用完成检查。不得重复执行已经完成的修复通知。

没有可用 MCP 时，由 AI 对已核实程序执行 `repair --data-dir "$dataDir" --client codex`；其他客户端使用 `--client mcp`。程序缺失或没有原启动记录时，从本指南实际安装断点继续，保持原数据和路径。权限仍按第 1 节审批，登录及浏览器授权由用户完成。

通过 live `onecrew_status` 核实 0.9.9、实际程序哈希、原账号和安装身份，以及当前客户端连接。确认 `discovery.host_work.claim/get/submit` 与 `discovery.host_operation.get` 可发现，安装验收只检查能力，不领取或执行暂停任务。使用原表格链接和已保存任务结果验证历史数据仍可读。

调用 `onecrew_web`，使用可用 UI 工具打开并读取只读状态卡、可见工作台及当前任务原结果表；没有任务时显示 Library。保持工作台选中并保留表格标签页。仅返回 URL 或排队打开不算页面验收完成。客户端缺少相应能力时如实列为待完成，不宣称通过。

保持日常 Chrome 和原网站登录。组件缺失时使用工作台内置安装和修复入口；用户完成必要浏览器授权。登录 Lite 后检查当前账号自己的适配器清单，按界面更新缺失或不匹配的组件；全局适配器检查不能替代当前账号检查，不使用过时的固定组件数量。

报告版本、哈希、服务、配套插件或 MCP、浏览器和表格连接结果，以及真正待完成的登录、批准或重启。保留所有已有结果和安装断点，不启动或恢复搜索，不发送消息或邮件。
