param(
  [Parameter(Mandatory=$true)][ValidateSet('Create','CreateTree','Verify','VerifyTree')][string]$Operation,
  [Parameter(Mandatory=$true)][string]$Directory,
  [string]$File,
  [string]$TreeManifest
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$directoryPath = [System.IO.Path]::GetFullPath($Directory)
$sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$systemSid = New-Object System.Security.Principal.SecurityIdentifier('S-1-5-18')
$administratorOwner = 'S-1-5-32-544'
$isAdministrator = @([System.Security.Principal.WindowsIdentity]::GetCurrent().Claims | Where-Object { $_.Value -eq $administratorOwner -and $_.Type -in @([System.Security.Claims.ClaimTypes]::GroupSid, [System.Security.Claims.ClaimTypes]::DenyOnlySid) }).Count -gt 0

function Assert-NoReparse([string]$Path) {
  $item = New-Object System.IO.DirectoryInfo($Path)
  while ($null -ne $item) {
    if (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw 'Backup path contains a reparse point'
    }
    if (-not $item.Exists) { throw 'Backup parent does not exist' }
    $item = $item.Parent
  }
}

function Assert-PrivateAcl([string]$Path, [bool]$Protected) {
  if ($Protected) { $acl = [System.IO.Directory]::GetAccessControl($Path) }
  else { $acl = [System.IO.File]::GetAccessControl($Path) }
  if ($Protected -and -not $acl.AreAccessRulesProtected) { throw 'Backup directory DACL inherits permissions' }
  $owner = $acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Value
  # 提权令牌创建的文件默认归 Administrators；仍逐项检查当前用户和 SYSTEM 的精确 DACL。
  if ($owner -ne $sid.Value -and -not ($isAdministrator -and $owner -eq $administratorOwner)) { throw 'Backup owner differs from the current user' }
  $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
  if ($rules.Count -ne 2) { throw 'Backup DACL must contain exactly two rules' }
  $seen = @{}
  foreach ($rule in $rules) {
    $identity = $rule.IdentityReference.Value
    if ($identity -ne $sid.Value -and $identity -ne $systemSid.Value) { throw 'Backup DACL grants another identity' }
    if ($rule.AccessControlType -ne [System.Security.AccessControl.AccessControlType]::Allow -or
        $rule.FileSystemRights -ne [System.Security.AccessControl.FileSystemRights]::FullControl) {
      throw 'Backup DACL has unexpected access rights'
    }
    if ($seen.ContainsKey($identity)) { throw 'Backup DACL duplicates an identity' }
    $seen[$identity] = $true
    if ($Protected -and ($rule.IsInherited -or
        $rule.PropagationFlags -ne [System.Security.AccessControl.PropagationFlags]::None -or
        $rule.InheritanceFlags -ne ([System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit))) {
      throw 'Backup directory DACL has unexpected inheritance flags'
    }
  }
}

if ($Operation -eq 'Create' -or $Operation -eq 'CreateTree') {
  Assert-NoReparse ([System.IO.Path]::GetDirectoryName($directoryPath))
  # 使用 .NET 构造 DACL，并在原生目录创建调用中传入，避免开放权限窗口。
  $security = New-Object System.Security.AccessControl.DirectorySecurity
  $security.SetAccessRuleProtection($true, $false)
  $security.SetOwner($sid)
  $inheritance = [System.Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [System.Security.AccessControl.InheritanceFlags]::ObjectInherit
  foreach ($identity in @($sid, $systemSid)) {
    $security.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', $inheritance, 'None', 'Allow')))
  }
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PrivateBackupDirectory {
  [StructLayout(LayoutKind.Sequential)]
  public struct SecurityAttributes {
    public int Length;
    public IntPtr Descriptor;
    [MarshalAs(UnmanagedType.Bool)] public bool Inherit;
  }
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool CreateDirectory(string path, ref SecurityAttributes attributes);
}
'@
  $descriptor = $security.GetSecurityDescriptorBinaryForm()
  $memory = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($descriptor.Length)
  try {
    [System.Runtime.InteropServices.Marshal]::Copy($descriptor, 0, $memory, $descriptor.Length)
    $attributes = New-Object PrivateBackupDirectory+SecurityAttributes
    $attributes.Length = [System.Runtime.InteropServices.Marshal]::SizeOf($attributes)
    $attributes.Descriptor = $memory
    $attributes.Inherit = $false
    if (-not [PrivateBackupDirectory]::CreateDirectory($directoryPath, [ref]$attributes)) {
      throw (New-Object System.ComponentModel.Win32Exception([System.Runtime.InteropServices.Marshal]::GetLastWin32Error()))
    }
    if ($Operation -eq 'CreateTree') {
      if (-not $TreeManifest -or $File) { throw 'Private tree creation requires a directory manifest' }
      $children = ConvertFrom-Json -InputObject $TreeManifest
      $seen = @{}
      foreach ($child in $children) {
        if ($child -isnot [string] -or -not $child -or $child.Contains('\') -or $child.Contains(':') -or $child.StartsWith('/')) { throw 'Invalid private tree directory' }
        $parts = $child.Split('/')
        foreach ($part in $parts) {
          if (-not $part -or $part -eq '.' -or $part -eq '..' -or $part.EndsWith('.') -or $part.EndsWith(' ')) { throw 'Invalid private tree component' }
        }
        if ($seen.ContainsKey($child)) { throw 'Duplicate private tree directory' }
        $seen[$child] = $true
        $childPath = [System.IO.Path]::GetFullPath((Join-Path $directoryPath $child))
        if (-not $childPath.StartsWith($directoryPath + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Private tree escaped its root' }
        Assert-NoReparse ([System.IO.Path]::GetDirectoryName($childPath))
        if (-not [PrivateBackupDirectory]::CreateDirectory($childPath, [ref]$attributes)) {
          throw (New-Object System.ComponentModel.Win32Exception([System.Runtime.InteropServices.Marshal]::GetLastWin32Error()))
        }
        Assert-NoReparse $childPath
        Assert-PrivateAcl $childPath $true
      }
    }
  } finally {
    [System.Runtime.InteropServices.Marshal]::FreeHGlobal($memory)
  }
}
Assert-NoReparse $directoryPath
Assert-PrivateAcl $directoryPath $true
if ($Operation -eq 'VerifyTree') {
  if ($File) { throw 'Tree verification cannot select one file' }
  $pending = New-Object 'System.Collections.Generic.Stack[System.IO.DirectoryInfo]'
  $pending.Push((New-Object System.IO.DirectoryInfo($directoryPath)))
  while ($pending.Count -gt 0) {
    $current = $pending.Pop()
    foreach ($child in $current.GetFileSystemInfos()) {
      if (($child.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Backup tree contains a reparse point' }
      if ($child -is [System.IO.DirectoryInfo]) {
        Assert-PrivateAcl $child.FullName $true
        $pending.Push($child)
      } elseif ($child -is [System.IO.FileInfo]) {
        Assert-PrivateAcl $child.FullName $false
      } else { throw 'Backup tree contains a non-regular entry' }
    }
  }
}
if ($File) {
  $filePath = [System.IO.Path]::GetFullPath($File)
  if ([System.IO.Path]::GetDirectoryName($filePath) -ne $directoryPath) { throw 'Backup file must be a direct child of its private directory' }
  $item = Get-Item -LiteralPath $filePath -Force
  if ($item.PSIsContainer -or ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Backup file is not a regular file' }
  Assert-PrivateAcl $filePath $false
}
[Console]::Out.WriteLine('private-acl-verified')
