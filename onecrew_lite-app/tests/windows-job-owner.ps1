param([Parameter(Mandatory=$true)][string]$Configuration)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ParentJob {
  [StructLayout(LayoutKind.Sequential)] public struct Basic {
    public long ProcessTime, JobTime; public uint Flags; public UIntPtr Minimum, Maximum;
    public uint Active; public UIntPtr Affinity; public uint Priority, Scheduling;
  }
  [StructLayout(LayoutKind.Sequential)] public struct Counters { public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes; }
  [StructLayout(LayoutKind.Sequential)] public struct Extended { public Basic Basic; public Counters Io; public UIntPtr ProcessMemory, JobMemory, PeakProcess, PeakJob; }
  public static Extended KillOnClose() { var value = new Extended(); value.Basic.Flags = 0x2000; return value; }
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr CreateJobObject(IntPtr security, string name);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool SetInformationJobObject(IntPtr job, int kind, ref Extended data, uint size);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool QueryInformationJobObject(IntPtr job, int kind, out Extended data, uint size, IntPtr returned);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool IsProcessInJob(IntPtr process, IntPtr job, out bool result);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr handle);
}
'@
$config = [IO.File]::ReadAllText($Configuration) | ConvertFrom-Json
$job = [ParentJob]::CreateJobObject([IntPtr]::Zero, $null)
if ($job -eq [IntPtr]::Zero) { throw 'CreateJobObject failed' }
try {
    $limits = [ParentJob]::KillOnClose()
    if (-not [ParentJob]::SetInformationJobObject($job, 9, [ref]$limits, [Runtime.InteropServices.Marshal]::SizeOf($limits))) { throw 'SetInformationJobObject failed' }
    $readback = [ParentJob+Extended]::new()
    if (-not [ParentJob]::QueryInformationJobObject($job, 9, [ref]$readback, [Runtime.InteropServices.Marshal]::SizeOf($readback), [IntPtr]::Zero) -or ($readback.Basic.Flags -band 0x2000) -eq 0) { throw 'Native kill-on-close flag readback failed' }
    $runner = Join-Path $PSScriptRoot 'windows-job-child.ps1'
    $childError = $Configuration + '.stderr.log'
    $child = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList ('-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $runner + '" -Configuration "' + $Configuration + '"') -RedirectStandardError $childError -RedirectStandardOutput ($Configuration + '.stdout.log') -WindowStyle Hidden -PassThru
    if (-not [ParentJob]::AssignProcessToJobObject($job, $child.Handle)) { throw 'AssignProcessToJobObject failed' }
    $assigned = $false
    if (-not [ParentJob]::IsProcessInJob($child.Handle, $job, [ref]$assigned) -or -not $assigned) { throw 'Launcher did not enter the real parent Job' }
    [IO.File]::WriteAllText($config.gate, 'start')
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    while (-not [IO.File]::Exists($config.ready)) {
        if ($child.HasExited) { throw ('Job launcher exited before readiness: ' + [IO.File]::ReadAllText($childError)) }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Job launcher readiness timeout' }
        Start-Sleep -Milliseconds 50
    }
    $started = [IO.File]::ReadAllText($config.ready) | ConvertFrom-Json
    $service = Get-Process -Id $started.pid
    $inside = $false
    if (-not [ParentJob]::IsProcessInJob($service.Handle, $job, [ref]$inside)) { throw 'Service Job inspection failed' }
    [Console]::WriteLine('service_in_parent_job=' + $inside)
} finally {
    if (-not [ParentJob]::CloseHandle($job)) { throw 'CloseHandle failed' }
}
[Console]::WriteLine('parent_job_closed')
