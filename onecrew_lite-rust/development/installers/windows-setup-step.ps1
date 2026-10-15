param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('verify', 'service', 'start', 'opencli', 'codex', 'library')]
    [string]$Step
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $env:LOCALAPPDATA) { throw 'Local application data directory unavailable.' }
$Bin = Join-Path $env:LOCALAPPDATA 'onecrew_lite\bin'
$Cli = Join-Path $Bin 'onecrew_lite.exe'
$Library = Join-Path $Bin 'onecrew_lite_library\onecrew_lite.exe'
$LogDir = Join-Path $Bin 'installation-logs'
[IO.Directory]::CreateDirectory($LogDir) | Out-Null
$Log = Join-Path $LogDir ($Step + '-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N'))
$Lock = $null
$ExitCode = 1
function Assert-Digest([string]$Path, [string]$Hash, [long]$Size) {
    $File = Get-Item -LiteralPath $Path -Force
    if ($File.PSIsContainer -or ($File.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $File.Length -ne $Size -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Hash) {
        throw 'Installed component integrity mismatch. Stop and inspect the installation.'
    }
}
function Run-Cli([string]$Program, [string[]]$Arguments) {
    # Use native file handles, not PowerShell's output pipes: a detached
    # OpenCLI daemon may retain inherited handles after the CLI has exited.
    foreach ($Value in @($Program, $Log) + $Arguments) {
        if ($Value -match '["%\r\n]') { throw 'Unsupported command path or argument.' }
    }
    $QuotedArguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $Command = '""' + $Program + '" ' + $QuotedArguments + ' 1>>"' + $Log + '.stdout.log" 2>>"' + $Log + '.stderr.log""'
    $Process = Start-Process -FilePath $env:ComSpec -ArgumentList @('/d', '/s', '/c', $Command) -WindowStyle Hidden -PassThru
    try {
        $Process.WaitForExit()
        if ($Process.ExitCode -ne 0) { throw "onecrew_lite step failed with exit code $($Process.ExitCode). Stop at this step." }
    } finally { $Process.Dispose() }
}
try {
    $LockPath = Join-Path $Bin '.onecrew_lite-install.lock'
    if ((Test-Path -LiteralPath $LockPath) -and ((Get-Item -LiteralPath $LockPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse-point lock rejected.' }
    $Lock = [IO.File]::Open($LockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    if (Test-Path -LiteralPath (Join-Path $Bin '.onecrew_lite-install')) { throw 'Core installation or recovery is pending.' }
    # Verify both components before executing any installed code, on every step.
    Assert-Digest $Cli '__CLI_SHA__' __CLI_SIZE__
    Assert-Digest $Library '__LIB_SHA__' __LIB_SIZE__
    switch ($Step) {
        'verify' { Run-Cli $Cli @('--version'); Run-Cli $Library @('--version') }
        'service' { Run-Cli $Cli @('service', 'host', 'install') }
        'start' { Run-Cli $Cli @('web', '--no-browser') }
        'opencli' { Run-Cli $Cli @('opencli', 'repair') }
        'codex' { Run-Cli $Cli @('plugin', 'install') }
        'library' { Run-Cli $Cli @('workspace', '--no-browser') }
    }
    $ExitCode = 0
} finally {
    # A daemon may retain stdout. Keep completion evidence in a separate file.
    @{ step = $Step; exit_code = $ExitCode; finished_at = [DateTime]::UtcNow.ToString('o') } |
        ConvertTo-Json -Compress | Set-Content -LiteralPath "$Log.result.json" -Encoding UTF8
    if ($null -ne $Lock) { $Lock.Dispose() }
    Write-Output "Step logs: $Log"
}
Write-Output 'Command completed. This does not establish client connection or full acceptance.'
if ($Step -eq 'codex') {
    Write-Output 'Verify an actual onecrew_lite tool call in Codex. If unavailable, save progress, quit and reopen the host app, then resume verification. Do not reinstall or terminate onecrew_lite services.'
}
if ($Step -eq 'library') {
    Write-Output 'Open both URLs from the stdout log in Codex and verify the workbench and Library. Account login, Chrome authorization and real search require separate checks.'
}
