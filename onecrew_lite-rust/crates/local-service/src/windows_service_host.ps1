$ErrorActionPreference = 'Stop'
$servicePath = $env:ONECREW_HOST_BINARY
$dataPath = $env:ONECREW_HOST_DATA
$operation = $env:ONECREW_HOST_ACTION
if ($servicePath.Contains('"') -or $dataPath.Contains('"') -or
    -not [IO.Path]::IsPathRooted($servicePath) -or -not [IO.Path]::IsPathRooted($dataPath)) {
    throw 'service_host_invalid_path'
}
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = -join ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($dataPath.ToLowerInvariant())) | ForEach-Object { $_.ToString('x2') }) } finally { $sha.Dispose() }
$name = 'OneCrew-Service-' + $digest
$scheduler = New-Object -ComObject Schedule.Service
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
if ($operation -eq 'probe') {
    $receipt = [IO.Path]::Combine($dataPath, 'host-probe.json')
    $probe = @'
$ErrorActionPreference='Stop'
try {
    $version = & '__EXE__' --version
    if ($LASTEXITCODE -ne 0 -or $version -notmatch '^onecrew_lite [0-9]+\.[0-9]+\.[0-9]+$') { throw 'candidate_execution_failed' }
    [IO.File]::WriteAllText('__RECEIPT__.tmp', '{"executed":true}')
    [IO.File]::Move('__RECEIPT__.tmp', '__RECEIPT__')
} catch { [IO.File]::WriteAllText('__RECEIPT__.tmp', '{"executed":false}'); [IO.File]::Move('__RECEIPT__.tmp', '__RECEIPT__'); exit 1 }
'@
    $probe = $probe.Replace('__EXE__', $servicePath.Replace("'", "''")).Replace('__RECEIPT__', $receipt.Replace("'", "''"))
    $definition = $scheduler.NewTask(0)
    $definition.Principal.UserId = $identity
    $definition.Principal.LogonType = 3
    $definition.Principal.RunLevel = 0
    $definition.Settings.DisallowStartIfOnBatteries = $false
    $definition.Settings.StopIfGoingOnBatteries = $false
    $definition.Settings.ExecutionTimeLimit = 'PT1M'
    $action = $definition.Actions.Create(0)
    $action.Path = [IO.Path]::Combine($env:SystemRoot, 'System32\WindowsPowerShell\v1.0\powershell.exe')
    $action.Arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probe))
    try {
        $null = $folder.RegisterTaskDefinition($name, $definition, 6, $identity, $null, 3, $null)
        $null = $folder.GetTask($name).Run($null)
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not [IO.File]::Exists($receipt)) {
            if ([DateTime]::UtcNow -ge $deadline) { throw 'service_host_probe_timeout' }
            Start-Sleep -Milliseconds 100
        }
        if (-not (([IO.File]::ReadAllText($receipt) | ConvertFrom-Json).executed)) { throw 'service_host_probe_execution_failed' }
    } finally {
        try { $folder.GetTask($name).Stop(0); $folder.DeleteTask($name, 0) } catch { throw 'service_host_probe_cleanup_failed' }
    }
    Write-Output 'service_host_ok'
    return
}
$arguments = '__service-host --data-dir "' + $dataPath + '"'
if ($operation -eq 'install') {
    if (-not [IO.File]::Exists($servicePath)) { throw 'service_host_binary_missing' }
    $definition = $scheduler.NewTask(0)
    $definition.RegistrationInfo.Description = 'OneCrew current-user shared service; started on demand by OneCrew clients.'
    $definition.Principal.UserId = $identity
    $definition.Principal.LogonType = 3
    $definition.Principal.RunLevel = 0
    $definition.Settings.ExecutionTimeLimit = 'PT0S'
    $definition.Settings.DisallowStartIfOnBatteries = $false
    $definition.Settings.StopIfGoingOnBatteries = $false
    $definition.Settings.MultipleInstances = 2
    $action = $definition.Actions.Create(0)
    $action.Path = $servicePath
    $action.Arguments = $arguments
    $action.WorkingDirectory = [IO.Path]::GetDirectoryName($servicePath)
    $null = $folder.RegisterTaskDefinition($name, $definition, 6, $identity, $null, 3, $null)
} elseif ($operation -ne 'start' -and $operation -ne 'remove') {
    throw 'service_host_unknown_action'
}
$task = $folder.GetTask($name)
$definition = $task.Definition
if ($definition.Actions.Count -ne 1 -or
    $definition.Actions.Item(1).Arguments -ne $arguments -or $definition.Principal.RunLevel -ne 0 -or
    $definition.Principal.LogonType -ne 3) { throw 'service_host_registration_mismatch' }
function GetBinaryDigest([string]$path) {
    $stream = [IO.File]::OpenRead($path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [Convert]::ToBase64String($algorithm.ComputeHash($stream)) }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}
if ((GetBinaryDigest $definition.Actions.Item(1).Path) -ne
    (GetBinaryDigest $servicePath)) { throw 'service_host_binary_mismatch' }
if ($operation -eq 'start') { $null = $task.Run($null) }
if ($operation -eq 'remove') { $folder.DeleteTask($name, 0) }
Write-Output 'service_host_ok'
