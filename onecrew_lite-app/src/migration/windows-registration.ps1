param(
    [Parameter(Mandatory=$true)][ValidateSet('Inspect','Disable','VerifyDisabled','VerifyProcess')][string]$Operation,
    [Parameter(Mandatory=$true)][string]$DataDirectory,
    [Parameter(Mandatory=$true)][string]$ServiceExecutable,
    [Parameter(Mandatory=$true)][string]$PackageExecutable,
    [Parameter(Mandatory=$true)][string]$ExpectedSha256,
    [string]$BackupFile,
    [ValidateRange(1,2147483647)][int]$ProcessId = 1
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
foreach ($path in @($DataDirectory, $ServiceExecutable, $PackageExecutable)) {
    if (-not [IO.Path]::IsPathRooted($path) -or $path.Contains('"')) { throw 'migration.registration_path_invalid' }
}
if ($ExpectedSha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'migration.registration_digest_invalid' }
function FileDigest([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $hash = [Security.Cryptography.SHA256]::Create()
    try { return -join ($hash.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) }
    finally { $hash.Dispose(); $stream.Dispose() }
}
if ((FileDigest $ServiceExecutable) -cne $ExpectedSha256 -or (FileDigest $PackageExecutable) -cne $ExpectedSha256) {
    throw 'migration.registration_executable_changed'
}
$algorithm = [Security.Cryptography.SHA256]::Create()
try { $suffix = -join ($algorithm.ComputeHash([Text.Encoding]::UTF8.GetBytes($DataDirectory.ToLowerInvariant())) | ForEach-Object { $_.ToString('x2') }) }
finally { $algorithm.Dispose() }
$name = 'OneCrew-Service-' + $suffix
$scheduler = New-Object -ComObject Schedule.Service
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
$matches = @($folder.GetTasks(1) | Where-Object { $_.Name -ceq $name })
if ($matches.Count -ne 1) { throw 'migration.registration_not_found' }
$task = $matches[0]
$definition = $task.Definition
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$principal = [Security.Principal.NTAccount]::new($definition.Principal.UserId).Translate([Security.Principal.SecurityIdentifier]).Value
$arguments = '__service-host --data-dir "' + $DataDirectory + '"'
if ($definition.Actions.Count -ne 1 -or $definition.Actions.Item(1).Type -ne 0 -or
    $definition.Actions.Item(1).Path -ine $ServiceExecutable -or
    $definition.Actions.Item(1).Arguments -cne $arguments -or
    $definition.Actions.Item(1).WorkingDirectory -ine [IO.Path]::GetDirectoryName($PackageExecutable) -or
    $principal -cne $sid -or $definition.Principal.LogonType -ne 3 -or $definition.Principal.RunLevel -ne 0 -or
    $definition.Triggers.Count -ne 0 -or $definition.Settings.ExecutionTimeLimit -cne 'PT0S' -or
    $definition.Settings.MultipleInstances -ne 2 -or $definition.Settings.DisallowStartIfOnBatteries -or
    $definition.Settings.StopIfGoingOnBatteries -or
    $definition.RegistrationInfo.Description -cne 'onecrew_lite current-user service; started on demand by onecrew_lite clients.') {
    throw 'migration.registration_definition_unsupported'
}
$xml = $definition.XmlText
if ($Operation -eq 'VerifyProcess') {
    $process = [Diagnostics.Process]::GetProcessById($ProcessId)
    try {
        if ($process.HasExited -or $process.MainModule.FileName -ine $ServiceExecutable) { throw 'migration.legacy_process_identity_changed' }
        if ((FileDigest $process.MainModule.FileName) -cne $ExpectedSha256) { throw 'migration.legacy_process_identity_changed' }
    } finally { $process.Dispose() }
}
if ($Operation -eq 'Disable') {
    if (-not $BackupFile -or -not [IO.Path]::IsPathRooted($BackupFile)) { throw 'migration.registration_backup_required' }
    # 父目录已由调用者创建私有 DACL；CreateNew 保留首次注册原文供回退。
    $stream = [IO.File]::Open($BackupFile, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($xml)
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally { $stream.Dispose() }
    $task.Enabled = $false
    $task = $folder.GetTask($name)
}
if ($Operation -eq 'VerifyDisabled' -or $Operation -eq 'Disable') {
    if ($task.Enabled) { throw 'migration.registration_still_enabled' }
}
@{ taskName = $name; enabled = [bool]$task.Enabled; running = ($task.GetInstances(0).Count -ne 0); executableSha256 = $ExpectedSha256 } | ConvertTo-Json -Compress
