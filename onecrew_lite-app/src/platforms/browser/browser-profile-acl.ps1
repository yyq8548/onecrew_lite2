param([Parameter(Mandatory=$true)][string]$Root)
$ErrorActionPreference = 'Stop'
$rootPath = [System.IO.Path]::GetFullPath($Root)
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$systemSid = 'S-1-5-18'
$isAdministrator = @([Security.Principal.WindowsIdentity]::GetCurrent().Claims | Where-Object { $_.Value -eq 'S-1-5-32-544' -and $_.Type -in @([Security.Claims.ClaimTypes]::GroupSid, [Security.Claims.ClaimTypes]::DenyOnlySid) }).Count -gt 0
$ancestor = New-Object System.IO.DirectoryInfo($rootPath)
while ($null -ne $ancestor) {
  if (-not $ancestor.Exists -or ($ancestor.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Browser profile ancestor is missing or a reparse point' }
  $ancestor = $ancestor.Parent
}
$pending = New-Object 'System.Collections.Generic.Stack[System.IO.FileSystemInfo]'
$pending.Push((New-Object System.IO.DirectoryInfo($rootPath)))
while ($pending.Count -gt 0) {
  $entry = $pending.Pop()
  if (($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Browser profile contains a reparse point' }
  $directory = $entry -is [System.IO.DirectoryInfo]
  if ($directory) { $acl = [System.IO.Directory]::GetAccessControl($entry.FullName) }
  elseif ($entry -is [System.IO.FileInfo]) { $acl = [System.IO.File]::GetAccessControl($entry.FullName) }
  else { throw 'Browser profile contains a special entry' }
  if ($entry.FullName -eq $rootPath -and -not $acl.AreAccessRulesProtected) { throw 'Browser profile root inherits permissions' }
  $owner = $acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value
  if ($owner -ne $sid -and -not ($isAdministrator -and $owner -eq 'S-1-5-32-544')) { throw 'Browser profile has another owner' }
  $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
  if ($rules.Count -ne 2) { throw 'Browser profile requires exactly two access rules' }
  $seen = @{}
  foreach ($rule in $rules) {
    $identity = $rule.IdentityReference.Value
    if (($identity -ne $sid -and $identity -ne $systemSid) -or $seen.ContainsKey($identity)) { throw 'Browser profile grants another or duplicate identity' }
    $seen[$identity] = $true
    if ($rule.AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or $rule.FileSystemRights -ne [System.Security.AccessControl.FileSystemRights]::FullControl -or $rule.PropagationFlags -ne [System.Security.AccessControl.PropagationFlags]::None) { throw 'Browser profile has unexpected rights' }
    if (-not $acl.AreAccessRulesProtected -and -not $rule.IsInherited) { throw 'Browser profile has explicit unprotected rules' }
    if ($directory -and $rule.InheritanceFlags -ne ([System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit)) { throw 'Browser directory has unexpected inheritance flags' }
    if (-not $directory -and $rule.InheritanceFlags -ne [System.Security.AccessControl.InheritanceFlags]::None) { throw 'Browser file has unexpected inheritance flags' }
  }
  if ($directory) { foreach ($child in $entry.GetFileSystemInfos()) { $pending.Push($child) } }
}
[Console]::Out.WriteLine('browser-private-tree-verified')
