# OneCrew Lite Windows npm 安装提示词

交付时填入该次主包版本、Windows 平台版本与校验值。只使用经过发布验收的版本；候选 `.tgz` 的本地安装命令随候选记录提供，不能把未发布版本当成 npm registry 已有版本。

## 交给用户本地 AI 的提示词

请在我的 Windows x64 电脑安装或升级 OneCrew Lite，使用本次提供的 npm 版本及下述步骤。我授权必要的本地安装、校验、原服务停止与启动、原数据升级、OneCrew 配套插件或 MCP 配置和本地网页检查。请直接执行终端操作，保留原账号、installation ID、数据目录、表格、搜索结果、评分、浏览器身份、主密钥和原端口，保持搜索暂停。其他应用、插件和客户端配置保持原状。登录、浏览器授权、安全确认和必要的客户端重启由我完成。

遇到终端沙箱拒绝时，通过客户端支持的审批流程申请该命令及相关路径的权限，获准后从该步骤继续。提示词本身不能改变系统权限。保留 Windows 执行策略、访问控制和证书验证设置。

按以下顺序完成安装：

1. 确认原生 Windows x64、当前用户和 Node.js 20 或更高版本的 x64 环境，定位 `npm.cmd` 的绝对路径。后续使用 `npm.cmd`、`onecrew-lite.cmd` 和已核实的程序绝对路径。每条原生命令执行后检查 `$LASTEXITCODE`。
2. 更新前先从已安装 OneCrew 的配置、服务登记或进程命令行核实原程序、数据目录、vault、主密钥与端口，保存这些路径，不输出密钥内容。已有 npm 安装先用旧入口的 `--print-binary-path` 定位程序；旧 ZIP 安装使用已核实的原程序。停止这份安装的服务并确认状态为 `stopped`。如果进程或 MCP 客户端占用将要更新的文件，只结束已核实属于该安装的连接；需要用户退出客户端时保留步骤并从此处继续。
3. 正式 npm 发布使用下列命令，将版本替换成本次已验收的精确主包版本：

   ```powershell
   & $npm install --global '@onecrew/lite@<精确主包版本>' --registry=https://registry.npmjs.org/ --include=optional --ignore-scripts
   if ($LASTEXITCODE -ne 0) { throw 'OneCrew npm 安装失败' }
   ```

   保持 optional dependencies 开启。主包选择精确版本的 `@onecrew/lite-win32-x64`；安装不运行生命周期脚本，也不另行下载程序。Windows 后续只使用 npm，原 ZIP 用户沿用原账号与数据。
4. 使用新 `onecrew-lite.cmd --print-binary-path` 获取原生程序路径，读取旁边的 `candidate.json`、`runtime-manifest.json`，核对本次 Windows 产品版本、`win32-x64`、原生程序 SHA-256 和发布记录。程序不支持 `--version` 或 `--help`，不要用这两个参数验收。主包与平台包版本按发布记录分别核对。
5. 运行新程序 `installation-discover`。已有数据必须与第二步一致；多份目录时让用户选择后传入 `--data-dir`，不能选择空目录掩盖失败。首次安装使用发现结果返回的默认路径。原有统一数据继续复用；需要 Creator SQLite 升级时，在原服务已停止后运行 `data-upgrade --data-dir <原目录>`。已启用 `current.json` 的统一 generation 不在根目录再次执行升级。Mac 的 `library-migrate` 流程不适用于 Windows。发现未受支持的独立 Library 时报告具体布局，保留原目录，不手工复制或创建空表。
6. 使用发现结果和已核实的原路径执行：

   ```powershell
   & $binary start --data-dir $dataDir --vault $vault --master-key $masterKey --legacy-port $originalPort
   if ($LASTEXITCODE -ne 0) { throw 'OneCrew 服务启动失败' }
   & $binary status --data-dir $dataDir
   if ($LASTEXITCODE -ne 0) { throw 'OneCrew 服务状态检查失败' }
   ```

   变量均使用已核实的绝对路径。首次安装没有原端口时省略 `--legacy-port`。要求两次结果均为 `running`，并核对实际运行程序路径、版本及哈希。服务停止则启动，不能只报告 npm 安装成功。
7. Codex 使用 Lite 自带的配套插件，依次执行 `codex-package --data-dir <原目录>`、`connect-codex --data-dir <原目录> --refresh`、`codex-status --data-dir <原目录> --refresh`。核对完整 `<原生版本>+codex.<digest>`、已启用和 `fresh`。不要安装 Pro 插件。其他 MCP 客户端配置绝对原生路径与 `mcp --stdio --data-dir <原目录>`，保留已有无关配置。需要重启客户端时说明具体原因，重启后只继续剩余检查。
8. 调用实际客户端中的 live `onecrew_status`，核对原账号、数据、Library 和浏览器连接。使用 `onecrew_web` 显示状态卡，打开并读取工作台和原结果表；无计划时读取 Library。用户在原日常浏览器完成必要登录和授权。安装、服务、客户端和页面分别报告结果，不自动开始或恢复搜索，不发送邮件。

出现失败时，报告具体命令、退出码和错误，在发生处修复可处理的问题；保存原数据和升级断点。用户说“一键修复”时，调用可用的 `onecrew_repair`；MCP 不可用时，使用已验证原生程序的 `repair --data-dir <原目录> --client codex`，其他 MCP 客户端使用 `--client mcp`。修复后完成相同的实际连接验收，继续保持搜索暂停。
