[CmdletBinding()]
param(
    [ValidateSet('Start', 'Stop', 'Check', 'Upgrade')]
    [string]$Action = 'Start',
    [string]$DataDirectory,
    [switch]$SkipCodex,
    [string]$CodexHome,
    [string]$CodexExecutable
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$program = Join-Path $PSScriptRoot 'onecrew_lite.exe'
$receipt = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'candidate.json') -Raw | ConvertFrom-Json
if ($receipt.target -ne 'win32-x64') { throw '安装包的平台不正确。' }
$programStream = [IO.File]::OpenRead($program)
$algorithm = [Security.Cryptography.SHA256]::Create()
try { $programHash = [BitConverter]::ToString($algorithm.ComputeHash($programStream)).Replace('-', '').ToLowerInvariant() }
finally { $programStream.Dispose(); $algorithm.Dispose() }
if ($programHash -ne $receipt.executableSha256) {
    throw '主程序校验失败，请重新获取完整安装包。'
}
function Invoke-OneCrew {
    param([string[]]$Arguments)
    $raw = & $program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "OneCrew 命令失败：$($Arguments[0])，退出码 $LASTEXITCODE。$([Environment]::NewLine)$($raw -join [Environment]::NewLine)"
    }
    return ($raw | ConvertFrom-Json)
}

$discoveryArguments = @('installation-discover')
if ($DataDirectory) { $discoveryArguments += @('--data-dir', [IO.Path]::GetFullPath($DataDirectory)) }
if ($CodexHome) { $discoveryArguments += @('--codex-home', [IO.Path]::GetFullPath($CodexHome)) }
$installation = Invoke-OneCrew -Arguments $discoveryArguments
$DataDirectory = $installation.data_directory

if ($Action -ne 'Check') {
    if ($installation.state -eq 'existing') {
        $existing = Invoke-OneCrew -Arguments @('status', '--data-dir', $DataDirectory)
        if ($existing.state -eq 'running') {
            $runningPath = (Get-Process -Id $existing.pid -ErrorAction Stop).Path
            $stream = [IO.File]::OpenRead($runningPath)
            $hash = [Security.Cryptography.SHA256]::Create()
            try { $runningHash = [BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
            finally { $stream.Dispose(); $hash.Dispose() }
            if ($Action -eq 'Stop' -or $Action -eq 'Upgrade' -or $runningHash -ne $programHash) {
                $stopped = Invoke-OneCrew -Arguments @('stop', '--data-dir', $DataDirectory)
                if ($stopped.state -ne 'stopped') { throw '尚未确认旧服务停止。' }
            }
        } elseif ($existing.state -ne 'stopped') {
            throw "无法识别旧服务状态：$($existing.state)"
        }
    }
    if ($Action -eq 'Stop') { Write-Host '本地服务已停止，账号、主密钥、表格和浏览器身份保留。'; return }
}
if ($Action -eq 'Check') {
    if ($installation.state -eq 'new') {
        @{ service = 'onecrew_lite'; state = 'not_installed' } | ConvertTo-Json
        return
    }
    $status = Invoke-OneCrew -Arguments @('status', '--data-dir', $DataDirectory)
    $status | ConvertTo-Json -Depth 5
    return
}

$parent = [IO.Path]::GetDirectoryName($DataDirectory)
if (-not $parent) { throw '请使用独立的数据目录。' }
$null = [IO.Directory]::CreateDirectory($parent)
Write-Host '正在启动 OneCrew 本地服务，首次启动需要解包资源。'
$service = Invoke-OneCrew -Arguments @('start', '--data-dir', $DataDirectory, '--vault', $installation.vault_path, '--master-key', $installation.master_key_path)
if ($service.state -ne 'running') { throw '尚未确认本地服务启动。' }
Write-Host "本地服务已启动。数据目录：$DataDirectory"
if (-not $SkipCodex) {
    $clientArguments = @('--data-dir', $DataDirectory)
    if ($CodexHome) { $clientArguments += @('--codex-home', [IO.Path]::GetFullPath($CodexHome)) }
    if ($CodexExecutable) { $clientArguments += @('--codex-executable', [IO.Path]::GetFullPath($CodexExecutable)) }
    Write-Host '正在安装并核对配套 Codex 插件。'
    $plugin = Invoke-OneCrew -Arguments (@('connect-codex') + $clientArguments)
    if ($plugin.state -ne 'installed' -or -not $plugin.enabled -or $plugin.verification -ne 'fresh') {
        throw '尚未确认 Codex 插件安装并启用。'
    }
    Write-Host "配套 Codex 插件已安装：$($plugin.installed_version)"
    Write-Host '如果 Codex 已经打开，请完整退出后重新打开，在对话中输入：打开 OneCrew Lite 工作台和资料库。'
}
