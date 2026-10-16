param([Parameter(Mandatory=$true)][string]$Path, [ValidateSet('File','Directory')][string]$Kind = 'File')
$ErrorActionPreference = 'Stop'
$item = Get-Item -LiteralPath $Path -Force
if (($Kind -eq 'File' -and $item -isnot [IO.FileInfo]) -or ($Kind -eq 'Directory' -and $item -isnot [IO.DirectoryInfo])) { throw 'migration.legacy_source_kind_invalid' }
$current = $item
while ($null -ne $current) {
    if (($current.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'migration.legacy_descriptor_reparse' }
    $current = if ($current -is [IO.FileInfo]) { $current.Directory } else { $current.Parent }
}
$acl = if ($Kind -eq 'Directory') { [IO.Directory]::GetAccessControl($item.FullName) } else { [IO.File]::GetAccessControl($item.FullName) }
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $sid) { throw 'migration.legacy_descriptor_owner_invalid' }
$rules = @($acl.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]))
# 原 Rust private_file.rs 使用 OW、SYSTEM、Administrators，仅读取该精确旧格式。
if ($Kind -eq 'Directory' -and $rules.Count -eq 3) { $allowed = @($sid, 'S-1-5-18', 'S-1-5-32-544'); $native = $false }
elseif ($Kind -eq 'File' -and $rules.Count -eq 3 -and -not $acl.AreAccessRulesProtected -and @($rules | Where-Object { -not $_.IsInherited }).Count -eq 0) { $allowed = @($sid, 'S-1-5-18', 'S-1-5-32-544'); $native = $false }
elseif ($rules.Count -eq 3 -and $acl.AreAccessRulesProtected) { $allowed = @('S-1-3-4', 'S-1-5-18', 'S-1-5-32-544'); $native = $true }
elseif ($rules.Count -eq 2) { $allowed = @($sid, 'S-1-5-18'); $native = $false }
else { throw 'migration.legacy_descriptor_acl_invalid' }
$seen = @{}
foreach ($rule in $rules) {
    $identity = $rule.IdentityReference.Value
    if ($allowed -cnotcontains $identity -or $seen.ContainsKey($identity) -or ($native -and $rule.IsInherited) -or
        $rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow -or
        $rule.FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl -or
        ($native -and $rule.InheritanceFlags -ne [Security.AccessControl.InheritanceFlags]::None) -or
        ($native -and $rule.PropagationFlags -ne [Security.AccessControl.PropagationFlags]::None)) { throw 'migration.legacy_descriptor_acl_invalid' }
    $seen[$identity] = $true
}
Write-Output 'legacy-source-acl-verified'
