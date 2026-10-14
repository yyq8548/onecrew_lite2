import { resolveEmailLocale } from "../auth/email-locale";

export type InstallLocale = ReturnType<typeof resolveEmailLocale>;

export function resolveInstallLocale(request: Request): InstallLocale {
  const language = new URL(request.url).searchParams.get("lng");
  return language === "zh-CN" || language === "en-US" ? language : resolveEmailLocale(request);
}

export function installPrompt(url: string, locale: InstallLocale): string {
  // Carry the page language to the agent's request, which may have no language header.
  const localizedUrl = `${url}?lng=${locale}`;
  return locale === "en-US"
    ? `Install or update OneCrew from this link:
${localizedUrl}
First confirm the user's target computer and whether your tools can operate it; uname or PowerShell results from a cloud sandbox describe only that sandbox, not the user's computer. Reuse the user's stated target OS; ask only if the target is unknown. Identify your current AI client and follow the returned instructions for that client: use the OneCrew plugin in Codex, or use setup to configure MCP in other supported clients. If the client is unclear, ask which one to connect. Use your available execution tools to complete installation and client setup. When a sandbox or tool permission blocks an authorized step on the target computer, request the minimum required permission through your execution tool and wait for approval; do not silently retry, bypass restrictions, or immediately hand the task back. If the user denies approval, stop that step. Ask the user for required permission approval, browser authorization, client trust, or a specific action your environment cannot perform. Preserve existing login; run login only when required by the installer.`
    : `请通过以下链接安装或更新 OneCrew：
${localizedUrl}
先确认用户要安装到哪台电脑，以及你的工具能否操作该电脑；云端沙箱里的 uname 或 PowerShell 结果只代表沙箱，不代表用户电脑。用户已说明目标系统时直接采用，目标不明时再询问。先识别你当前所在的 AI 客户端，再按照链接返回的说明选择对应的接入方式：Codex 使用 OneCrew 插件，其他支持 MCP 的客户端使用 setup 生成配置。如果无法确定客户端，请询问要接入哪个客户端。请使用你可用的执行工具完成安装与客户端配置。目标电脑上已获任务授权的步骤被沙盒或工具权限阻止时，通过执行工具主动申请最小必要权限并等待批准，不要闷头重试、绕过限制或立即把任务交回用户；用户拒绝批准时停止该步骤。需要用户参与的事项包括必要的权限批准、浏览器授权、客户端信任或环境明确无法执行的具体操作。保留已有登录状态，仅在安装流程要求时登录。`;
}

export function installInstructions(locale: InstallLocale, version: string, expiresAt: number, baseUrl: string, supportsBootstrap: boolean): string {
  const expiry = new Date(expiresAt).toISOString();
  const scripts = `macScriptUrl: ${baseUrl}/install.sh\nwindowsScriptUrl: ${baseUrl}/install.ps1`;
  const windowsCommands = `\`\`\`powershell
$installerPath = Join-Path $env:TEMP ('onecrew-install-' + [Guid]::NewGuid().ToString('N') + '.ps1')
Invoke-WebRequest -UseBasicParsing -Uri '${baseUrl}/install.ps1' -OutFile $installerPath -MaximumRedirection 0 -TimeoutSec 60 -ErrorAction Stop
Get-Content -LiteralPath $installerPath
\`\`\``;
  const windowsRun = `\`\`\`powershell
$powerShellPath = Join-Path $env:SystemRoot 'System32\\WindowsPowerShell\\v1.0\\powershell.exe'
& $powerShellPath -NoProfile -NonInteractive -File $installerPath
$installerExitCode = $LASTEXITCODE
\`\`\``;
  if (locale === "en-US") {
    const lifecycle = supportsBootstrap
      ? "Follow the installer result. For an existing installation, the installer updates the CLI and preserves login; do not run login again unless explicitly required. For a first installation, run the returned login command."
      : "This older release uses the legacy installer. Run the returned login command.";
    return `# OneCrew installation and client configuration

Install or update OneCrew CLI to ${version}. The download URLs in this guide expire at ${expiry}.

## Identify the environment
Establish three separate facts before downloading a platform script or running installation:

1. Target computer: the machine where the user wants OneCrew installed and its operating system. Reuse the user's explicit target or reliable device information; ask only when it is unknown. A cloud Ubuntu/Linux sandbox does not establish that the user's computer is Linux.
2. Execution access: whether an available local tool or explicitly connected remote tool can operate that target computer. uname, $PSVersionTable and filesystem checks describe the machine where that command runs. They are target evidence only after the tool's connection to the target is established. A desktop client can still execute commands in a cloud container, VM or WSL environment. Do not install into that environment unless the user actually chose it as the target.
3. Client capability: identify the intended AI client and confirm its actual plugin or local stdio MCP support. A product name or desktop window alone does not establish local command access or MCP support. Use the client the user explicitly names; merely finding Codex installed does not select the Codex route.

If only cloud/sandbox execution is available, look for an existing supported connection to the target computer. If none exists, explain that local execution access is unavailable, not that the user's OS is unsupported. Offer the target-specific local installation step or a local agent that can run it, and report that installation has not been performed on the user's computer. Do not download the macOS script to test Linux compatibility, seek a Linux build for the sandbox, or fabricate a successful local setup. Installing the CLI on the computer also does not make it reachable from a cloud-only MCP client.

## Sandbox and tool permissions

For an authorized step on the confirmed target, if the tool reports a sandbox or permission restriction (or explicitly requires approval before execution), proactively use that tool's supported permission request mechanism. State the exact command, reason, and minimum permission scope; redact credentials and private installation URLs from the explanation. Wait for the actual approval result before executing. The installation request does not waive tool approval, and saying that approval is needed does not submit a request.

Do not classify every network error or nonzero exit as a sandbox denial; distinguish tool permission failures from expired links, access denial, and installer or service errors. Do not blindly rerun a command whose effects are unknown: inspect the existing result and state before resuming only the blocked step. Do not loop on an unchanged denied command, switch shells or launchers to evade restrictions, disable security checks, or request blanket administrator access. If the user denies approval, stop that step and report the blocker without routing it through another tool or agent. If no supported approval mechanism is available, explain that limitation and provide only the necessary manual step; do not claim permission was requested or granted. Approval does not connect a cloud sandbox to the target computer.

## Install the CLI

Select the script for the confirmed target computer, and run it only through a tool connected to that computer. On macOS download and inspect macScriptUrl, then run it with /bin/sh; it selects Apple Silicon or Intel automatically. On Windows x64 download and inspect windowsScriptUrl, then run it with native PowerShell. Only a confirmed target outside macOS and Windows x64 is unsupported by these installers.

\`\`\`text
${scripts}
\`\`\`

### Windows execution

Use a native PowerShell tool connected to the target Windows computer, Windows PowerShell 5.1 or later, and PowerShell environment syntax ($env:LOCALAPPDATA). Download and inspect the script first:

${windowsCommands}

After inspection, execute the saved script in the same session (or use its actual saved path):

${windowsRun}

Inspect stdout, stderr and installerExitCode before continuing. Do not use Bash, WSL, Start-Job or a custom process launcher to work around a denied command. If execution policy or client policy blocks the script, report the actual restriction and request the specific approval required; do not change machine-wide policy or add a blanket Bypass flag. Use the returned installed_path with PowerShell's call operator (&) for later CLI commands. The agent should run required login and setup commands using its available tools; the user completes browser authorization and client trust prompts. A manual terminal command is a fallback only when the supported execution tool or approval mechanism is demonstrably unavailable, never a way around the user's denial.

${lifecycle} Complete browser login when required. Read the returned next_action, next_actions and connection_options as applicable guidance or alternative routes, not a list to execute in full. After the CLI succeeds and any required first-install login completes, use the executable at installed_path for the selected connection route below.

## Client configuration

### Codex only:
Automatically install or reinstall the OneCrew Codex plugin. Always run plugin install with the installed CLI executable at installed_path, even when cached plugin status reports installed or current; do not stop after a status check. If the CLI reports plugin_identity_change_required, remove only the old OneCrew plugin identities named in its instructions using Codex's plugin removal command, then retry plugin install. Require state installed, enabled true, and verification fresh before reporting plugin installation success. Ask the user to fully quit and restart Codex and continue in a new task, then verify the live OneCrew connection. Codex starts the plugin's MCP automatically; do not add a separate MCP entry or manually launch its process.

### Other MCP clients:
The current plugin install command supports Codex only. Run setup with the executable at installed_path to obtain the standalone MCP configuration. Add or update only the OneCrew entry in the target client's configuration, using that client's supported format and scope. Preserve the returned absolute command path and arguments, including mcp --stdio and any data-directory options. The client starts and manages this stdio process. Do not run Codex plugin commands or ask the user to restart Codex for this route. Reload or restart the target client as it requires, then verify that its OneCrew MCP connection is available. serve starts the local service and opens the webpage; it is not the MCP launch command. If the client cannot launch a local stdio process, explain that limitation instead of inventing an HTTP endpoint.

### Unknown client:
Only after confirming the target computer and execution access, complete CLI installation and any required login, and obtain the setup configuration if useful. Ask which AI client to connect before making client-specific changes. Report CLI installation and client connection separately; configuration saved or plugin installed does not by itself prove a live connection.

## Recovery

- download_refresh_required: reopen the original installation link to get fresh script and download URLs, then download and inspect the new script. If the original link has also expired, obtain a new installation prompt from Access. Do not keep retrying an expired script or change its embedded URLs.
- download_access_denied: return to Access to check your account and installation access.
- download_retryable: check the network or wait before retrying; refresh the guide if the download URLs have expired.
- Windows failures report stage, error, native_exit_code and cli_installed on stderr. A bootstrap failure is not a completed installation or a service-verification failure. Preserve these fields; do not clear bin, move the installed executable, manually place a downloaded binary, or repeat an unchanged failing command. A command_timeout means the final state needs inspection, not that rollback succeeded.
- The Windows CLI data directory defaults to $env:LOCALAPPDATA\\io.onecrewai.onecrew, separate from the installation directory. Do not infer login state from an absent process or a guessed directory. If service verification fails or times out, report that stage and its error; do not start another unbounded status command.

Current installers keep the CLI result on stdout and diagnostics on stderr; older installers may return key/value text. Preserve the returned result and distinguish CLI success from client connection success. The scripts install the CLI only; choose the client configuration route above afterward.
`;
  }
  const lifecycle = supportsBootstrap
    ? "按照安装器返回的结果继续操作。如果已有安装，安装器会自动更新 CLI 并保留登录状态；除非明确要求，否则不要重新登录。如果是首次安装，请执行返回的登录命令。"
    : "此旧版本使用旧版安装器。请执行返回的登录命令。";
  return `# OneCrew 安装与客户端配置

请将 OneCrew CLI 安装或更新至 ${version}。本指南中的下载地址有效期至 ${expiry}。

## 识别运行环境
下载平台脚本或执行安装之前，分别确认以下三件事：

1. 目标电脑：用户要把 OneCrew 安装到哪台电脑、该电脑是什么系统。优先采用用户明确说明的目标或可靠的设备信息；只有目标未知时才询问。云端 Ubuntu/Linux 沙箱不代表用户电脑是 Linux。
2. 执行能力：现有本地工具或明确连接的远程工具能否操作这台目标电脑。uname、$PSVersionTable 和文件系统检查只说明命令执行所在的机器；先确认工具连接到目标电脑后，这些结果才可用于判断目标系统。即使使用桌面客户端，命令也可能运行在云端容器、虚拟机或 WSL 中。除非用户明确将其选为目标，否则不要把 OneCrew 安装到这些环境。
3. 客户端能力：识别目标 AI 客户端，并确认它实际支持的插件或本地 stdio MCP 接入。仅凭产品名称或桌面窗口，不能认定它能执行本机命令或支持 MCP。用户已指定客户端时直接采用；不能仅因电脑上装有 Codex 就选择 Codex 流程。

如果只有云端或沙箱执行工具，先检查是否已有受支持的目标电脑连接；没有时，应说明缺少本机执行能力，而不是说用户的操作系统不受支持。提供适用于目标电脑的本地安装步骤，或交给能操作该电脑的本地 agent 执行，并明确尚未在用户电脑完成安装。不要下载 macOS 脚本来测试 Linux 兼容性，不要为沙箱寻找 Linux 安装包，也不要虚报本机安装成功。即使 CLI 已安装到电脑，也不代表仅支持云端连接的 MCP 客户端能够访问它。

## 沙盒与工具权限

在已确认的目标电脑上执行已获任务授权的步骤时，如果工具明确报告沙盒或权限限制，或明确要求执行前批准，应主动调用该工具支持的权限申请机制。说明具体命令、原因和最小权限范围，说明中隐去凭据和私有安装链接；等待实际批准结果后再执行。安装请求不等于免除工具审批，只说“需要批准”不等于已发起申请。

不要把所有网络错误或非零退出都归因于沙盒；区分工具权限失败、链接过期、访问被拒绝以及安装器或服务错误。效果未知的命令不得盲目重跑：先检查已有结果与状态，再仅恢复被阻止的步骤。不要反复执行未改变的被拒命令、换 shell 或启动器绕过限制、关闭安全检查或申请宽泛的管理员权限。用户拒绝批准时停止该步骤并报告阻塞，不得改走其他工具或 agent。如果没有受支持的审批机制，说明限制并仅提供必要的手动步骤，不要声称已申请或已获批准。批准不会把云端沙箱连接到目标电脑。

## 安装 CLI

按已确认的目标电脑选择脚本，并且只通过连接到该电脑的工具执行。macOS：下载并检查 macScriptUrl 对应的脚本，再使用 /bin/sh 运行；脚本会自动选择 Apple Silicon 或 Intel 版本。Windows x64：下载并检查 windowsScriptUrl 对应的脚本，再使用原生 PowerShell 运行。仅当已确认的目标系统不属于 macOS 或 Windows x64 时，才说明当前安装器不支持该目标。

\`\`\`text
${scripts}
\`\`\`

### Windows 执行方式

使用已连接到目标 Windows 电脑的原生 PowerShell 工具，要求 Windows PowerShell 5.1 或以上版本；环境变量使用 PowerShell 写法（$env:LOCALAPPDATA）。先下载并检查脚本：

${windowsCommands}

检查后，在同一会话中执行保存的脚本；如果会话已切换，请使用实际保存的路径：

${windowsRun}

检查标准输出、标准错误和 installerExitCode，再继续下一步。不要通过 Bash、WSL、Start-Job 或自建进程启动器绕过被拒绝的命令。如果执行策略或客户端策略阻止脚本，报告具体限制，并请求所需的具体批准；不要修改全局执行策略或统一添加 Bypass 参数。后续 CLI 命令使用返回的 installed_path，配合 PowerShell 调用运算符 & 执行。agent 应使用现有工具运行必要的 login 和 setup 命令，用户完成浏览器授权和客户端信任确认。只有受支持的执行工具或审批机制确实不可用时，才退回到用户手动运行终端命令；不得以此绕过用户拒绝。

${lifecycle} 需要时完成浏览器登录。返回的 next_action、next_actions 和 connection_options 是有适用条件的后续指引或可选接入方式，不要全部依次执行。CLI 安装成功且所需的首次登录完成后，使用 installed_path 指向的可执行文件，选择以下对应流程。

## 客户端配置

### 仅限 Codex：
自动安装或重新安装 OneCrew Codex 插件。必须使用 installed_path 指向的已安装 CLI 可执行文件运行 plugin install；即使缓存中的插件状态显示 installed 或 current，也要执行，不能仅检查状态就结束。如果 CLI 返回 plugin_identity_change_required，请使用 Codex 的插件移除命令，仅移除返回说明中列出的旧版 OneCrew 插件标识，然后重试 plugin install。只有 state 为 installed、enabled 为 true 且 verification 为 fresh 时，才能报告插件安装成功。请用户完全退出并重新启动 Codex，在新任务中继续，然后验证 OneCrew 的实际连接。Codex 会自动启动插件中的 MCP，无需另加 MCP 配置或手动启动进程。

### 其他 MCP 客户端：
当前 plugin install 命令仅支持 Codex。使用 installed_path 指向的可执行文件运行 setup，获取独立 MCP 配置。按照目标客户端支持的配置格式和作用范围，仅添加或更新其中的 OneCrew 配置项。保留返回的绝对命令路径及参数，包括 mcp --stdio 和可能存在的数据目录选项，由客户端启动并管理这个 stdio 进程。此流程不执行 Codex 插件命令，也不要求用户重启 Codex。按目标客户端的要求刷新或重启，并验证其 OneCrew MCP 连接是否可用。serve 用于启动本地服务并打开网页，不是 MCP 启动命令。如果客户端无法启动本地 stdio 进程，请说明这一限制，不要编造 HTTP 接入地址。

### 无法确定客户端：
只有确认目标电脑及执行能力后，才先完成 CLI 安装及必要登录，需要时生成 setup 配置。在修改客户端配置之前，询问用户要接入哪个 AI 客户端。分别说明 CLI 安装状态和客户端接入状态；保存配置或安装插件本身不代表实际连接已成功。

## 失败恢复

- download_refresh_required：重新打开原始安装链接，获取新的脚本和下载地址，再重新下载并检查脚本。如果原始链接也已过期，请回到 Access 获取新的安装提示词。不要反复运行已过期的脚本或自行替换其中的下载地址。
- download_access_denied：回到 Access 检查账号及安装权限。
- download_retryable：检查网络或稍后重试；如果下载地址已过期，先重新获取本指南。
- Windows 失败时会在标准错误中返回 stage、error、native_exit_code 和 cli_installed。bootstrap 失败不代表安装完成，也不等于服务校验失败。保留这些字段，不要清空 bin、搬走已安装程序、手动放置下载的二进制或反复执行同一失败命令。command_timeout 表示最终状态待核实，不代表回滚成功。
- Windows CLI 默认数据目录是 $env:LOCALAPPDATA\\io.onecrewai.onecrew，与安装目录分开。不要根据没有进程或猜测的目录判断登录状态。服务校验失败或超时时，报告该阶段及错误，不要再启动一个无超时限制的 status 命令。

当前安装器将 CLI 结果保留在标准输出，将诊断信息写入标准错误；旧版安装器可能返回键值文本。保留返回结果，区分 CLI 安装成功与客户端接入成功。脚本仅安装 CLI，之后按上面的分支完成客户端配置。
`;
}
