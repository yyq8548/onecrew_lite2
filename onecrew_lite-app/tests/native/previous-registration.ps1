param([Parameter(Mandatory=$true)][ValidateSet('Install','Remove')][string]$Operation,
      [Parameter(Mandatory=$true)][string]$Directory,
      [Parameter(Mandatory=$true)][string]$Executable,
      [Parameter(Mandatory=$true)][string]$Package)
$ErrorActionPreference = 'Stop'
$hash = [Security.Cryptography.SHA256]::Create()
try { $suffix = -join ($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($Directory.ToLowerInvariant())) | ForEach-Object { $_.ToString('x2') }) }
finally { $hash.Dispose() }
$name = 'OneCrew-Service-' + $suffix
$scheduler = New-Object -ComObject Schedule.Service
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
if ($Operation -eq 'Remove') { $folder.DeleteTask($name,0); exit 0 }
if (-not [IO.File]::Exists($Executable) -or -not [IO.File]::Exists($Package)) { throw 'registration_test_executable_missing' }
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$definition = $scheduler.NewTask(0)
$definition.RegistrationInfo.Description = 'onecrew_lite current-user service; started on demand by onecrew_lite clients.'
$definition.Principal.UserId = $sid
$definition.Principal.LogonType = 3
$definition.Principal.RunLevel = 0
$definition.Settings.ExecutionTimeLimit = 'PT0S'
$definition.Settings.MultipleInstances = 2
$definition.Settings.DisallowStartIfOnBatteries = $false
$definition.Settings.StopIfGoingOnBatteries = $false
$action = $definition.Actions.Create(0)
$action.Path = $Executable
# 只注册真实任务验证历史元数据的读取和停用，不启动旧协议。
$action.Arguments = '__service-host --data-dir "' + $Directory + '"'
$action.WorkingDirectory = [IO.Path]::GetDirectoryName($Package)
$null = $folder.RegisterTaskDefinition($name,$definition,6,$sid,$null,3)
