param([Parameter(Mandatory=$true)][ValidateSet('Start','Run','Remove','Abort')][string]$Operation,
      [Parameter(Mandatory=$true)][string]$Directory, [string]$Configuration)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$accountSid = $identity.User.Value
$physicalDirectory = [IO.Path]::GetFullPath($Directory).TrimEnd('\')
function Hash-Bytes([byte[]]$Bytes) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($Bytes)).Replace('-','').ToLowerInvariant() }
    finally { $algorithm.Dispose() }
}
function Hash-File([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}
function Assert-Private([string]$Path) {
    $item = Get-Item -LiteralPath $Path
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'service.host_reparse_path' }
    $acl = $item.GetAccessControl()
    if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $accountSid) { throw 'service.host_owner_changed' }
    foreach ($entry in $acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])) {
        if ($entry.AccessControlType -eq 'Allow' -and $entry.IdentityReference.Value -notin @($accountSid,'S-1-5-18','S-1-5-32-544')) { throw 'service.host_acl_invalid' }
    }
}
Assert-Private $physicalDirectory
$taskName = 'OneCrewLite-Unified-' + (Hash-Bytes ([Text.Encoding]::UTF8.GetBytes($accountSid + "`n" + $physicalDirectory.ToUpperInvariant())))
if ($Operation -eq 'Run' -or $Operation -eq 'Start') {
    $hostDirectory = Join-Path $physicalDirectory '.windows-host'
    Assert-Private $hostDirectory
    if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($Configuration)) -ine $hostDirectory) { throw 'service.host_configuration_path_invalid' }
    Assert-Private $Configuration
    $config = [IO.File]::ReadAllText($Configuration) | ConvertFrom-Json
    if ($config.format -ne 2 -or $config.directory -ine $physicalDirectory -or $config.host_script -ine $PSCommandPath -or
        $config.service_log -ine (Join-Path $physicalDirectory 'service.log') -or
        (Hash-File $config.executable) -cne $config.executable_sha256 -or (Hash-File $PSCommandPath) -cne $config.host_script_sha256) { throw 'service.host_configuration_changed' }
    $environmentNames = @('LOCALAPPDATA','APPDATA','USERPROFILE','HOME')
    if (-not $config.environment -or @($config.environment.PSObject.Properties).Count -ne $environmentNames.Count) { throw 'service.host_environment_invalid' }
    foreach ($entry in $config.environment.PSObject.Properties) {
        if ($entry.Name -cnotin $environmentNames -or ($null -ne $entry.Value -and ($entry.Value -isnot [string] -or -not [IO.Path]::IsPathRooted($entry.Value) -or -not [IO.Directory]::Exists($entry.Value)))) { throw 'service.host_environment_invalid' }
    }
}
if ($Operation -eq 'Run') {
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Collections;
using System.IO;
using System.Threading.Tasks;
public static class UnifiedServiceHost {
  private static void Copy(Stream source, Stream target) { var buffer = new byte[8192]; int count; while ((count = source.Read(buffer, 0, buffer.Length)) > 0) { lock(target) { target.Write(buffer,0,count); target.Flush(); } } }
  public static int Run(string executable, string arguments, string directory, string logPath, IDictionary environment) {
    using (var log = new FileStream(logPath,FileMode.Append,FileAccess.Write,FileShare.ReadWrite))
    using (var process = new Process()) {
      process.StartInfo = new ProcessStartInfo(executable,arguments) { WorkingDirectory=directory, UseShellExecute=false, CreateNoWindow=true, RedirectStandardOutput=true, RedirectStandardError=true };
      foreach (DictionaryEntry entry in environment) { if (entry.Value == null) process.StartInfo.EnvironmentVariables.Remove((string)entry.Key); else process.StartInfo.EnvironmentVariables[(string)entry.Key] = (string)entry.Value; }
      if (!process.Start()) throw new InvalidOperationException("service.host_process_start_failed");
      var output = Task.Run(() => Copy(process.StandardOutput.BaseStream,log));
      var error = Task.Run(() => Copy(process.StandardError.BaseStream,log));
      process.WaitForExit(); Task.WaitAll(output,error); return process.ExitCode;
    }
  }
}
'@
    $environment = @{}
    foreach ($entry in $config.environment.PSObject.Properties) { $environment[$entry.Name] = $entry.Value }
    exit [UnifiedServiceHost]::Run($config.executable,$config.arguments,$config.working_directory,$config.service_log,$environment)
}
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
$task = @($folder.GetTasks(1) | Where-Object { $_.Name -ceq $taskName })
if ($task.Count -gt 1) { throw 'service.host_task_ambiguous' }
if ($task.Count -eq 1) {
    $security = [Security.AccessControl.CommonSecurityDescriptor]::new($false,$false,$task[0].GetSecurityDescriptor(7))
    $principal = $task[0].Definition.Principal.UserId
    $principalSid = if ($principal -like 'S-1-*') { $principal } else { ([Security.Principal.NTAccount]::new($principal)).Translate([Security.Principal.SecurityIdentifier]).Value }
    if ($security.Owner.Value -ne $accountSid -or $principalSid -ne $accountSid -or $task[0].Definition.Principal.LogonType -ne 3 -or
        $task[0].Definition.Triggers.Count -ne 0 -or $task[0].Definition.RegistrationInfo.Description -cne ('OneCrewLiteUnified:' + $physicalDirectory) -or $task[0].Definition.Principal.RunLevel -ne 0) { throw 'service.host_task_owner_changed' }
    foreach ($entry in $security.DiscretionaryAcl) {
        if ($entry.AceQualifier -eq 'AccessAllowed' -and $entry.SecurityIdentifier.Value -notin @($accountSid,'S-1-5-18','S-1-5-32-544')) { throw 'service.host_task_acl_invalid' }
    }
}
if ($Operation -eq 'Remove' -or $Operation -eq 'Abort') {
    if ($task.Count -eq 1) {
        if ($Operation -eq 'Abort' -and $task[0].State -eq 4) { $task[0].Stop(0) }
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while ($task[0].State -eq 4 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
        if ($task[0].State -eq 4) { throw 'service.host_task_still_running' }
        $folder.DeleteTask($taskName,0)
    }
    [Console]::WriteLine('service_host_removed')
    exit 0
}
foreach ($path in @($PSCommandPath,$Configuration,$physicalDirectory)) { if ($path.Contains('"') -or $path -match '[\x00-\x1f]') { throw 'service.host_argument_invalid' } }
$powerShell = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
$arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "' + $PSCommandPath + '" -Operation Run -Directory "' + $physicalDirectory + '" -Configuration "' + $Configuration + '"'
if ($task.Count -eq 1 -and $task[0].State -eq 4) {
    if ($task[0].Definition.Actions.Count -ne 1 -or $task[0].Definition.Actions.Item(1).Path -ine $powerShell -or $task[0].Definition.Actions.Item(1).Arguments -cne $arguments) { throw 'service.host_task_running_different_build' }
    [Console]::WriteLine('service_host_already_starting')
    exit 0
}
$definition = $scheduler.NewTask(0)
$definition.RegistrationInfo.Description = 'OneCrewLiteUnified:' + $physicalDirectory
$definition.Principal.UserId = $accountSid
$definition.Principal.LogonType = 3
$definition.Principal.RunLevel = 0
$definition.Settings.Enabled = $true
$definition.Settings.Hidden = $true
$definition.Settings.MultipleInstances = 2
$definition.Settings.ExecutionTimeLimit = 'PT0S'
$definition.Settings.DisallowStartIfOnBatteries = $false
$definition.Settings.StopIfGoingOnBatteries = $false
$action = $definition.Actions.Create(0)
$action.Path = $powerShell
$action.Arguments = $arguments
$action.WorkingDirectory = $config.working_directory
$registered = $folder.RegisterTaskDefinition($taskName,$definition,6,$accountSid,$null,3,('O:' + $accountSid + 'G:' + $accountSid + 'D:P(A;;GA;;;SY)(A;;GA;;;' + $accountSid + ')'))
$null = $registered.Run($null)
[Console]::WriteLine('service_host_started')
