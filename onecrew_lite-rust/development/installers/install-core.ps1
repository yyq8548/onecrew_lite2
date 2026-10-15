$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# No execution-policy change, client registration, network or business-data writes.
$Version = '__VERSION__'
$CliSha = '__CLI_SHA__'
$CliSize = [long]'__CLI_SIZE__'
$LibSha = '__LIB_SHA__'
$LibSize = [long]'__LIB_SIZE__'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or [Environment]::OSVersion.Version.Major -lt 10) { throw 'Windows 10 or newer required.' }
if (-not ('OnecrewLiteNativeArchitecture' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OnecrewLiteNativeArchitecture {
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool IsWow64Process2(IntPtr process, out ushort processMachine, out ushort nativeMachine);
}
'@
}
[ushort]$ProcessMachine = 0
[ushort]$NativeMachine = 0
if (-not [OnecrewLiteNativeArchitecture]::IsWow64Process2([Diagnostics.Process]::GetCurrentProcess().Handle, [ref]$ProcessMachine, [ref]$NativeMachine) -or $NativeMachine -ne 0x8664) { throw 'Native Windows x64 required.' }
if (-not $env:LOCALAPPDATA -or -not [IO.Path]::IsPathRooted($env:LOCALAPPDATA)) { throw 'Local application data directory unavailable.' }
$Bin = Join-Path $env:LOCALAPPDATA 'onecrew_lite\bin'
$Cli = Join-Path $Bin 'onecrew_lite.exe'
$Library = Join-Path $Bin 'onecrew_lite_library'
$Tx = Join-Path $Bin '.onecrew_lite-install'
$SourceCli = Join-Path $PSScriptRoot 'onecrew_lite.exe'
$SourceLibrary = Join-Path $PSScriptRoot 'onecrew_lite_library\onecrew_lite.exe'
function Assert-Regular([string]$Path) {
    $File = Get-Item -LiteralPath $Path -Force
    if ($File.PSIsContainer -or ($File.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Non-regular executable rejected.' }
}
function Assert-Digest([string]$Path, [string]$Hash, [long]$Size) {
    Assert-Regular $Path
    if ((Get-Item -LiteralPath $Path).Length -ne $Size -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Hash) { throw 'Component integrity mismatch.' }
}
function Run-Cli([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'onecrew_lite command failed.' }
}
foreach ($Path in @($env:LOCALAPPDATA, (Join-Path $env:LOCALAPPDATA 'onecrew_lite'), $Bin, $Cli, $Library, $Tx)) {
    if ((Test-Path -LiteralPath $Path) -and ((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse-point installation path rejected.' }
}
Assert-Digest $SourceCli $CliSha $CliSize
Assert-Digest $SourceLibrary $LibSha $LibSize
$CandidateVersion = & $SourceCli --version
if ($LASTEXITCODE -ne 0 -or $CandidateVersion -ne "onecrew_lite $Version") { throw 'CLI version mismatch.' }
[IO.Directory]::CreateDirectory($Bin) | Out-Null
$LockPath = Join-Path $Bin '.onecrew_lite-install.lock'
$Lock = $null
$Prepared = $false
$Success = $false
$Restored = $true
$CliChanged = $false
$LibraryChanged = $false
$HadLibrary = $false
$Previous = Join-Path $Tx 'previous-cli.exe'
$PreviousLibrary = Join-Path $Tx 'previous-library'
$OldVersion = ''; $OldHash = ''; $OldSize = 0
try {
    if ((Test-Path -LiteralPath $LockPath) -and ((Get-Item -LiteralPath $LockPath -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse-point lock rejected.' }
    $Lock = [IO.File]::Open($LockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $HadLibrary = Test-Path -LiteralPath $Library
    if (Test-Path -LiteralPath $Tx) { throw 'Interrupted installation: preserve .onecrew_lite-install for recovery.' }
    [IO.Directory]::CreateDirectory($Tx) | Out-Null
    $Prepared = $true
    if (Test-Path -LiteralPath $Cli) {
        Assert-Regular $Cli
        Copy-Item -LiteralPath $Cli -Destination $Previous
        $OldVersion = & $Previous --version
        if ($LASTEXITCODE -ne 0 -or $OldVersion -notmatch '^onecrew_lite (\d+\.\d+\.\d+)$') { throw 'Unexpected existing CLI identity.' }
        $OldVersion = $Matches[1]
        $OldHash = (Get-FileHash -LiteralPath $Previous -Algorithm SHA256).Hash.ToLowerInvariant()
        $OldSize = (Get-Item -LiteralPath $Previous).Length
    }
    $NextLibrary = Join-Path $Tx 'next-library'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'onecrew_lite_library') -Destination $NextLibrary -Recurse
    Assert-Digest (Join-Path $NextLibrary 'onecrew_lite.exe') $LibSha $LibSize
    'stopping_library' | Set-Content -LiteralPath (Join-Path $Tx 'phase')
    if (Test-Path -LiteralPath $Library) {
        Assert-Regular (Join-Path $Library 'onecrew_lite.exe')
        Run-Cli (Join-Path $Library 'onecrew_lite.exe') @('stop')
    }
    'replacing_cli' | Set-Content -LiteralPath (Join-Path $Tx 'phase')
    $CliChanged = $true
    Run-Cli $SourceCli @('__bootstrap-update', '--expected-version', $Version, '--expected-sha256', $CliSha, '--expected-size', "$CliSize", '--install-path', $Cli, '--json')
    'replacing_library' | Set-Content -LiteralPath (Join-Path $Tx 'phase')
    $LibraryChanged = $true
    if ($HadLibrary) { Move-Item -LiteralPath $Library -Destination $PreviousLibrary }
    Move-Item -LiteralPath $NextLibrary -Destination $Library
    Assert-Digest $Cli $CliSha $CliSize
    Assert-Digest (Join-Path $Library 'onecrew_lite.exe') $LibSha $LibSize
    $InstalledVersion = & $Cli --version
    if ($LASTEXITCODE -ne 0 -or $InstalledVersion -ne "onecrew_lite $Version") { throw 'Installed version mismatch.' }
    'complete' | Set-Content -LiteralPath (Join-Path $Tx 'phase')
    $Success = $true
} catch {
    if ($Prepared) {
        try {
            if ($CliChanged) {
                if (Test-Path -LiteralPath $Previous) {
                    Run-Cli $Previous @('__bootstrap-update', '--expected-version', $OldVersion, '--expected-sha256', $OldHash, '--expected-size', "$OldSize", '--install-path', $Cli, '--json')
                } elseif (Test-Path -LiteralPath $Cli) { Remove-Item -LiteralPath $Cli }
            }
            if ($LibraryChanged -and ((Test-Path -LiteralPath $PreviousLibrary) -or -not $HadLibrary)) {
                if (Test-Path -LiteralPath $Library) { Move-Item -LiteralPath $Library -Destination (Join-Path $Tx 'failed-library') }
                if (Test-Path -LiteralPath $PreviousLibrary) { Move-Item -LiteralPath $PreviousLibrary -Destination $Library }
            }
        } catch { $Restored = $false }
    }
    if (-not $Restored) { throw "Recovery required. Keep $Tx and its backups." }
    throw
} finally {
    if ($Prepared -and ($Success -or $Restored)) { Remove-Item -LiteralPath $Tx -Recurse -Force }
    if ($null -ne $Lock) { $Lock.Dispose() }
}
Write-Output "onecrew_lite $Version core installed: $Cli"
Write-Output 'Client connection, dependencies, login and browser readiness have not been verified.'
