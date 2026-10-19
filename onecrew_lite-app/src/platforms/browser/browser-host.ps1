param(
  [Parameter(Mandatory=$true)][ValidateSet('Detect','Inspect','Close')][string]$Operation,
  [int]$ProcessId,
  [string]$Executable,
  [string]$ExecutableSha256,
  [string]$StartTimeTicks
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($Operation -eq 'Detect') {
  $candidates = New-Object 'System.Collections.Generic.List[object]'
  if ($env:CHROME_BIN) { $candidates.Add(@{ path=$env:CHROME_BIN; scope='custom' }) }
  foreach ($entry in @(@{ hive=[Microsoft.Win32.Registry]::CurrentUser; scope='user' }, @{ hive=[Microsoft.Win32.Registry]::LocalMachine; scope='machine' })) {
    $key = $entry.hive.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\App Paths\chrome.exe')
    if ($null -ne $key) {
      try { $registered = $key.GetValue(''); if ($registered -is [string] -and $registered) { $candidates.Add(@{ path=$registered.Trim('"'); scope=$entry.scope }) } }
      finally { $key.Dispose() }
    }
  }
  foreach ($entry in @(@{ root=$env:LOCALAPPDATA; scope='user' }, @{ root=$env:ProgramFiles; scope='machine' }, @{ root=${env:ProgramFiles(x86)}; scope='machine' })) {
    if ($entry.root) { $candidates.Add(@{ path=(Join-Path $entry.root 'Google\Chrome\Application\chrome.exe'); scope=$entry.scope }) }
  }
  foreach ($entry in $candidates) {
    if ([System.IO.File]::Exists($entry.path)) {
      $version = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($entry.path)
      $major = $null
      if ($version.FileVersion) { $major = [string]$version.FileMajorPart }
      @{ path=[System.IO.Path]::GetFullPath($entry.path); version=$major; install_scope=$entry.scope } | ConvertTo-Json -Compress
      exit 0
    }
  }
  [Console]::Out.WriteLine('null')
  exit 0
}
if ($ProcessId -lt 1 -or -not $Executable -or $ExecutableSha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid owned browser process' }
$selected = Get-Process -Id $ProcessId -ErrorAction Stop
$executableStream = [System.IO.File]::OpenRead($Executable)
$executableHasher = [System.Security.Cryptography.SHA256]::Create()
try { $actualExecutableSha256 = [System.BitConverter]::ToString($executableHasher.ComputeHash($executableStream)).Replace('-', '').ToLowerInvariant() }
finally { $executableHasher.Dispose(); $executableStream.Dispose() }
if (-not [System.String]::Equals($selected.MainModule.FileName, [System.IO.Path]::GetFullPath($Executable), [System.StringComparison]::OrdinalIgnoreCase) -or
    $actualExecutableSha256 -ne $ExecutableSha256) { throw 'Owned browser executable changed' }
$actualStart = $selected.StartTime.ToUniversalTime().Ticks.ToString([System.Globalization.CultureInfo]::InvariantCulture)
if ($Operation -eq 'Inspect') { [Console]::Out.WriteLine($actualStart); exit 0 }
if ($StartTimeTicks -notmatch '^[0-9]+$' -or $actualStart -ne $StartTimeTicks) { throw 'Owned browser process identity changed' }
# 等待实际启动的浏览器出现可见窗口，仅向该 PID 的可见窗口发送 WM_CLOSE。
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OneCrewOwnedBrowserClose {
  public delegate bool WindowCallback(IntPtr window, IntPtr parameter);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool EnumWindows(WindowCallback callback, IntPtr parameter);
  [DllImport("user32.dll", SetLastError=true)] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr window, uint message, IntPtr first, IntPtr second);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  private static readonly System.Collections.Generic.HashSet<IntPtr> Posted = new System.Collections.Generic.HashSet<IntPtr>();
  public static void Close(uint expected) {
    int queryError=0; int postError=0;
    WindowCallback callback=(window, parameter)=> {
      uint owner; if (GetWindowThreadProcessId(window, out owner)==0) { queryError=Marshal.GetLastWin32Error(); return true; }
      if (owner==expected && IsWindowVisible(window) && Posted.Add(window)) { if (!PostMessage(window, 0x0010, IntPtr.Zero, IntPtr.Zero)) postError=Marshal.GetLastWin32Error(); }
      return true;
    };
    if (!EnumWindows(callback, IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
    if (queryError!=0) throw new System.ComponentModel.Win32Exception(queryError);
    if (postError!=0) throw new System.ComponentModel.Win32Exception(postError);
    GC.KeepAlive(callback);
  }
}
'@
$closing = [System.Diagnostics.Stopwatch]::StartNew()
while (-not $selected.HasExited -and $closing.ElapsedMilliseconds -lt 15000) {
  [OneCrewOwnedBrowserClose]::Close([uint32]$ProcessId)
  [void]$selected.WaitForExit([Math]::Min(100, [Math]::Max(0, 15000 - [int]$closing.ElapsedMilliseconds)))
}
if (-not $selected.HasExited) { throw 'Owned browser graceful close timed out' }
[Console]::Out.WriteLine('owned-browser-closed')
