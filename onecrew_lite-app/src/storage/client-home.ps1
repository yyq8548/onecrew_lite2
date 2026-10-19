param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
$item = New-Object IO.DirectoryInfo([IO.Path]::GetFullPath($Directory))
$current = $item
while ($null -ne $current) {
  if (-not $current.Exists -or ($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'client_home_path_invalid' }
  $current = $current.Parent
}
$acl = [IO.Directory]::GetAccessControl($item.FullName)
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$isAdministrator = @([Security.Principal.WindowsIdentity]::GetCurrent().Claims | Where-Object { $_.Value -eq 'S-1-5-32-544' -and $_.Type -in @([Security.Claims.ClaimTypes]::GroupSid, [Security.Claims.ClaimTypes]::DenyOnlySid) }).Count -gt 0
$owner = $acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
if ($owner -ne $sid -and -not ($isAdministrator -and $owner -eq 'S-1-5-32-544')) { throw 'client_home_owner_invalid' }
$trusted = @($sid, 'S-1-5-18', 'S-1-5-32-544')
$writes = [Security.AccessControl.FileSystemRights]::Write -bor [Security.AccessControl.FileSystemRights]::Delete -bor [Security.AccessControl.FileSystemRights]::DeleteSubdirectoriesAndFiles -bor [Security.AccessControl.FileSystemRights]::ChangePermissions -bor [Security.AccessControl.FileSystemRights]::TakeOwnership
foreach ($rule in $acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
  if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and $trusted -notcontains $rule.IdentityReference.Value -and ($rule.FileSystemRights -band $writes) -ne 0) { throw 'client_home_untrusted_write_access' }
}
Write-Output 'client-home-verified'
