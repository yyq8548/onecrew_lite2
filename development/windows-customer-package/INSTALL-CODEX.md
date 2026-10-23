# OneCrew Lite 0.7.1 Windows x64 R8.1：Codex 安装指南

本指南用于用户明确授权的安装或升级。使用目标电脑上的原生 Windows 执行工具。通过 Access 下载时，要求 HTTP 200，禁止自动跟随重定向，并先按该发布记录核对 ZIP 的准确字节数和 SHA-256。收到用户提供的 ZIP 时，使用随包提供的发布校验值。校验失败立即停止。

本包主程序 SHA-256：`1238fa16671a7bf44c374fae9b8b55743ab6238aa84fe4c274cd01a885b2cb64`。主程序已经构建完成，无须安装 Node.js、Rust 或编译工具。ZIP 的大小和 SHA-256 见同名 .release.json 与 .sha256；它们校验 ZIP，包内 SHA256SUMS.json 校验解压后的全部文件。

给 Codex 的默认流程是依次直接执行下面的命令。无需执行包里的 CMD、下载另一个安装脚本或修改 PowerShell 执行策略。用户需要双击操作时仍可使用 Start-OneCrew.cmd。工具明确拒绝某项操作时报告原文与步骤，不换用其他入口绕过拒绝。

## 1. 确认解压目录和原数据目录

解压完整 ZIP，将 `$packageDir` 设置为包含 onecrew_lite.exe 的实际绝对目录。不要在 ZIP 预览里运行。先阅读 README.zh-CN.md 和 RELEASE-STATUS.md。

默认安装目录为当前 Windows 用户的 `%LOCALAPPDATA%\Programs\OneCrewLite`。程序自动检查该用户的服务登记、Codex 插件和历史默认目录；唯一旧安装继续使用原目录，首次安装使用当前用户的默认目录。多个数据目录时明确报错并列出候选，询问用户选择后向 installation-discover 传入 --data-dir，禁止猜测。不要读取或输出主密钥、Cookie 或服务控制令牌。

以下命令在同一个 PowerShell 会话执行。将路径变量替换成核实后的实际路径，再执行：

```powershell
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$packageDir = '解压后包含 onecrew_lite.exe 的完整路径'
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\OneCrewLite'
$sourceBinary = Join-Path $packageDir 'onecrew_lite.exe'
$binary = Join-Path $installDir 'onecrew_lite.exe'
function Get-OneCrewHash([string]$path) {
    $stream = [IO.File]::OpenRead($path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $algorithm.Dispose() }
}
$expectedBinaryHash = '1238fa16671a7bf44c374fae9b8b55743ab6238aa84fe4c274cd01a885b2cb64'
if ((Get-OneCrewHash $sourceBinary) -ne $expectedBinaryHash) { throw 'OneCrew binary checksum mismatch' }
$candidate = Get-Content -LiteralPath (Join-Path $packageDir 'candidate.json') -Raw | ConvertFrom-Json
if ($candidate.target -ne 'win32-x64' -or $candidate.productVersion -ne '0.7.1' -or $candidate.executableSha256 -ne $expectedBinaryHash) { throw 'OneCrew package identity mismatch' }
$packageRoot = [IO.Path]::GetFullPath($packageDir).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$checksums = Get-Content -LiteralPath (Join-Path $packageDir 'SHA256SUMS.json') -Raw | ConvertFrom-Json
foreach ($entry in $checksums.PSObject.Properties) {
    if ([IO.Path]::IsPathRooted($entry.Name) -or $entry.Name.Contains(':') -or $entry.Value -notmatch '^[a-f0-9]{64}$') { throw 'Invalid OneCrew file manifest' }
    $file = [IO.Path]::GetFullPath((Join-Path $packageRoot $entry.Name))
    if (-not $file.StartsWith($packageRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'OneCrew manifest path escapes package' }
    if ((Get-OneCrewHash $file) -ne $entry.Value) { throw "OneCrew file checksum mismatch: $($entry.Name)" }
}
```

上述哈希函数使用 Windows 内置 .NET，避免某些 PowerShell 环境缺少 Get-FileHash 命令。全部文件校验通过后才进入安装步骤；指南中的主程序哈希、candidate.json 和 SHA256SUMS.json 必须一致。

## 2. 检查状态、安装或更新

缺少数据目录表示首次安装。已有目录必须通过程序的归属和状态检查；状态检查失败不能当成“服务停止”。运行中的同一构建可以复用；更换构建时先停止，确认成功后再复制。程序文件已丢失而数据仍在时，也用已校验的新程序检查原服务。

```powershell
$discoveryText = & $sourceBinary installation-discover
if ($LASTEXITCODE -ne 0) { throw "OneCrew installation discovery failed: $discoveryText" }
$installation = $discoveryText | ConvertFrom-Json
$dataDir = $installation.data_directory
if ($installation.state -eq 'existing') {
    $statusText = & $sourceBinary status --data-dir $dataDir
    if ($LASTEXITCODE -ne 0) { throw "OneCrew status failed: $statusText" }
    $status = $statusText | ConvertFrom-Json
    if ($status.state -notin @('running', 'stopped')) { throw 'Unknown OneCrew service state' }
    if ($status.state -eq 'running') {
        $runningBinary = (Get-Process -Id $status.pid -ErrorAction Stop).Path
        $sameBinary = (Get-OneCrewHash $runningBinary) -eq (Get-OneCrewHash $sourceBinary)
    }
    if ($status.state -eq 'running' -and -not $sameBinary) {
        $stopText = & $sourceBinary stop --data-dir $dataDir
        if ($LASTEXITCODE -ne 0) { throw "OneCrew stop failed: $stopText" }
        if (($stopText | ConvertFrom-Json).state -ne 'stopped') { throw 'OneCrew stop not confirmed' }
    }
}
$null = [IO.Directory]::CreateDirectory($installDir)
& robocopy.exe $packageDir $installDir /E /COPY:DAT /R:1 /W:1
if ($LASTEXITCODE -ge 8) { throw 'OneCrew copy failed' }
if ((Get-OneCrewHash $binary) -ne (Get-OneCrewHash $sourceBinary)) { throw 'Installed binary checksum mismatch' }
$null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($dataDir))
$startedText = & $binary start --data-dir $dataDir --vault $installation.vault_path --master-key $installation.master_key_path
if ($LASTEXITCODE -ne 0) { throw "OneCrew start failed: $startedText" }
if (($startedText | ConvertFrom-Json).state -ne 'running') { throw 'OneCrew start not confirmed' }
$statusText = & $binary status --data-dir $dataDir
if ($LASTEXITCODE -ne 0) { throw "OneCrew status failed: $statusText" }
if (($statusText | ConvertFrom-Json).state -ne 'running') { throw 'OneCrew service not running' }
$statusText
```

如果返回 `service.build_update_required`，报告实际运行构建不一致，核对原安装归属后执行明确的停止和更新流程。不要清理数据或杀死其他程序。不要把系统拒绝、文件占用、归属错误与正常停止混为一谈。

## 3. 连接 Codex

Lite 使用自己的配套插件；不要安装 Pro 的插件或套用 Pro 的版本、端口和命令。本包 `connect-codex` 通过官方 Codex 插件管理入口安装并核验配套插件，插件版本包含构建身份，不能只比较 0.7.1。

```powershell
$pluginText = & $binary connect-codex --data-dir $dataDir
if ($LASTEXITCODE -ne 0) { throw "OneCrew Codex connection failed: $pluginText" }
$plugin = $pluginText | ConvertFrom-Json
if ($plugin.state -ne 'installed' -or -not $plugin.enabled -or $plugin.verification -ne 'fresh') { throw 'OneCrew companion plugin not verified' }
$pluginText
```

找不到 Codex CLI 时先定位已安装的官方 Codex CLI，再使用本程序的 `--codex-executable` 参数指定；不要安装其他来源的同名软件。遇到插件缓存文件占用，保存安装步骤，提示完整退出并重启 Codex 后继续这一阶段，不结束 OneCrew 服务或改动其他插件。

## 4. 重启后的实际验收

按宿主提示重新打开 Codex。调用 live `onecrew_status`，确认主程序哈希等于本指南记录值，服务正常，并核对配套插件。使用 `onecrew_web` 在 Codex 内置浏览器复用工作台和资料库，检查状态卡、页面及已有动态表格实际可读。

明确显示右侧浏览器面板（`visibility.set(true)`），依次选中工作台和当前计划的动态表格标签，读取实际页面内容；没有计划时读取资料库。最后将工作台标签置于前台，保留表格标签。`markDeliverable()` 仅保留标签，面板可见性和当前选中标签必须分别确认。工具不可用、仅排队或页面读取失败时报告实际状态，显示完成以面板可见且目标页面实际可读为准。

登录、验证码和浏览器授权由用户完成。默认在用户日常 Chrome 中连接官方 OpenCLI 扩展，然后在工作台选择并授权该身份。保持已有登录、浏览器身份和后台同步授权。未经用户确认不开始搜索或发送 Gmail。若还需要重启、登录或界面检查，逐项报告，不能仅凭返回 URL 宣称全部完成。Mac 验收保持未完成。
