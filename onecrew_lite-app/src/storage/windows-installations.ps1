param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$paths = @()
foreach ($task in $scheduler.GetFolder('\').GetTasks(1)) {
    if ($task.Name -notlike 'OneCrewLite-Unified-*') { continue }
    $definition = $task.Definition
    $principal = $definition.Principal.UserId
    $principalSid = if ($principal -like 'S-1-*') { $principal } else { ([Security.Principal.NTAccount]::new($principal)).Translate([Security.Principal.SecurityIdentifier]).Value }
    if ($principalSid -ne $identity.User.Value) { continue }
    $description = $definition.RegistrationInfo.Description
    if (-not $description.StartsWith('OneCrewLiteUnified:')) { throw 'installation.registration_invalid' }
    $paths += $description.Substring('OneCrewLiteUnified:'.Length)
}
ConvertTo-Json -InputObject @($paths) -Compress
