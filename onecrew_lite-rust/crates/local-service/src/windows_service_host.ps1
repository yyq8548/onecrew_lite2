$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$servicePath = $env:ONECREW_HOST_BINARY
$packagePath = $env:ONECREW_HOST_PACKAGE
$dataPath = $env:ONECREW_HOST_DATA
$operation = $env:ONECREW_HOST_ACTION
$phase = 'validate_paths'
trap {
    # Preserve the original exception; diagnostics must never replace it.
    $failure = $_
    try {
        $chain = @()
        $exception = $failure.Exception
        while ($null -ne $exception) {
            $message = $exception.Message
            foreach ($privatePath in @($env:USERPROFILE, $servicePath, $packagePath, $dataPath)) {
                if ($privatePath) { $message = $message.Replace($privatePath, '<path>') }
            }
            $chain += @{ type = $exception.GetType().FullName; hresult = ('0x{0:X8}' -f $exception.HResult); message = $message }
            $exception = $exception.InnerException
        }
        $diagnostic = @{
            action = $operation; phase = $phase; errors = $chain
            windows = [Environment]::OSVersion.Version.ToString()
            powershell = $PSVersionTable.PSVersion.ToString()
            process_64_bit = [Environment]::Is64BitProcess
            elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
            logon_type = 3; run_level = 0
        } | ConvertTo-Json -Depth 6 -Compress
        [Console]::Error.WriteLine($diagnostic)
    } catch { [Console]::Error.WriteLine('service_host_diagnostics_unavailable') }
    throw $failure
}
if ($servicePath.Contains('"') -or $dataPath.Contains('"') -or
    -not [IO.Path]::IsPathRooted($servicePath) -or -not [IO.Path]::IsPathRooted($dataPath)) {
    throw 'service_host_invalid_path'
}
$sha = [Security.Cryptography.SHA256]::Create()
try { $digest = -join ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($dataPath.ToLowerInvariant())) | ForEach-Object { $_.ToString('x2') }) } finally { $sha.Dispose() }
$name = 'OneCrew-Service-' + $digest
$scheduler = New-Object -ComObject Schedule.Service
$phase = 'scheduler_connect'
$scheduler.Connect()
$folder = $scheduler.GetFolder('\')
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
function NewHostDefinition([string]$commandArguments) {
    if (-not [IO.File]::Exists($servicePath)) { throw 'service_host_binary_missing' }
    $definition = $scheduler.NewTask(0)
    $definition.RegistrationInfo.Description = 'onecrew_lite current-user service; started on demand by onecrew_lite clients.'
    $definition.Principal.UserId = $identity
    $definition.Principal.LogonType = 3
    $definition.Principal.RunLevel = 0
    $definition.Settings.DisallowStartIfOnBatteries = $false
    $definition.Settings.StopIfGoingOnBatteries = $false
    $definition.Settings.ExecutionTimeLimit = 'PT0S'
    $definition.Settings.MultipleInstances = 2
    $action = $definition.Actions.Create(0)
    $action.Path = $servicePath
    $action.Arguments = $commandArguments
    if (-not $packagePath -or -not [IO.Path]::IsPathRooted($packagePath) -or
        (GetBinaryDigest $packagePath) -ne (GetBinaryDigest $servicePath)) {
        throw 'service_host_package_mismatch'
    }
    $action.WorkingDirectory = [IO.Path]::GetDirectoryName($packagePath)
    return $definition
}
function RegisterHostDefinition($definition) {
    # Pass serialized XML rather than a live COM definition across the call.
    # Omit optional SDDL so the scheduler retains its normal inherited ACL.
    $script:phase = $operation + '_validate'
    $null = $folder.RegisterTask($name, $definition.XmlText, 1, $identity, $null, 3)
    $script:phase = $operation + '_register'
    $null = $folder.RegisterTask($name, $definition.XmlText, 6, $identity, $null, 3)
}
function GetBinaryDigest([string]$path) {
    $stream = [IO.File]::OpenRead($path)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try { return [Convert]::ToBase64String($algorithm.ComputeHash($stream)) }
    finally { $algorithm.Dispose(); $stream.Dispose() }
}
function AssertHostDefinition($definition, [string]$commandArguments) {
    if ($definition.Actions.Count -ne 1 -or
        $definition.Actions.Item(1).Arguments -ne $commandArguments -or
        $definition.Principal.RunLevel -ne 0 -or $definition.Principal.LogonType -ne 3 -or
        $definition.Settings.ExecutionTimeLimit -ne 'PT0S' -or
        $definition.Settings.MultipleInstances -ne 2) {
        throw 'service_host_registration_mismatch'
    }
    $installed = if ($operation -eq 'probe') { $packagePath } else { [IO.Path]::Combine($definition.Actions.Item(1).WorkingDirectory, 'onecrew_lite.exe') }
    if (-not [IO.Path]::IsPathRooted($installed) -or
        (GetBinaryDigest $installed) -ne (GetBinaryDigest $servicePath)) {
        throw 'service_host_package_mismatch: register the installed service again'
    }
    if ((GetBinaryDigest $definition.Actions.Item(1).Path) -ne (GetBinaryDigest $servicePath)) {
        throw 'service_host_binary_mismatch'
    }
}
if ($operation -eq 'probe') {
    # Identical host definition; only the bounded command is changed. The
    # polling deadline below owns the probe lifetime, not a different task type.
    $definition = NewHostDefinition '--version'
    $registered = $false
    $probeFailure = $null
    try {
        RegisterHostDefinition $definition
        $registered = $true
        AssertHostDefinition ($folder.GetTask($name).Definition) '--version'
        $phase = 'probe_run'
        $null = $folder.GetTask($name).Run($null)
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        while ($true) {
            $task = $folder.GetTask($name)
            if ($task.LastRunTime.Year -gt 2000 -and $task.State -ne 2 -and $task.State -ne 4) {
                if ($task.LastTaskResult -ne 0) { throw ('service_host_probe_execution_failed: ' + $task.LastTaskResult) }
                break
            }
            if ([DateTime]::UtcNow -ge $deadline) { throw 'service_host_probe_timeout' }
            Start-Sleep -Milliseconds 100
        }
        [IO.File]::WriteAllText([IO.Path]::Combine($dataPath, 'host-probe.json'), '{"executed":true}')
    } catch {
        $probeFailure = $_
        throw
    } finally {
        if ($registered) {
            try { $folder.GetTask($name).Stop(0); $folder.DeleteTask($name, 0) } catch {
                if ($null -eq $probeFailure) { throw }
                [Console]::Error.WriteLine('service_host_probe_cleanup_failed; original failure preserved')
            }
        }
    }
    Write-Output 'service_host_ok'
    return
}
$arguments = '__service-host --data-dir "' + $dataPath + '"'
if ($operation -eq 'install') {
    RegisterHostDefinition (NewHostDefinition $arguments)
} elseif ($operation -ne 'start' -and $operation -ne 'remove') {
    throw 'service_host_unknown_action'
}
$task = $folder.GetTask($name)
$phase = 'registration_readback'
$definition = $task.Definition
AssertHostDefinition $definition $arguments
if ($operation -eq 'start') { $null = $task.Run($null) }
if ($operation -eq 'remove') { $folder.DeleteTask($name, 0) }
Write-Output 'service_host_ok'
