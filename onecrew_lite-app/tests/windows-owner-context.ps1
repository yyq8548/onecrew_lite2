param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = Join-Path $PSScriptRoot '..\src'
$verifier = Join-Path $source 'storage\private-backup.ps1'
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$administrator = ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$null = & $verifier -Operation Create -Directory $Directory
$file = Join-Path $Directory 'created-by-current-token'
[IO.File]::WriteAllBytes($file, [IO.File]::ReadAllBytes($PSCommandPath))
$owner = ([IO.File]::GetAccessControl($file)).GetOwner([Security.Principal.SecurityIdentifier]).Value
if ($owner -ne $identity.User.Value -and $owner -ne 'S-1-5-32-544') { throw 'Unexpected default owner in native test' }
$null = & $verifier -Operation VerifyTree -Directory $Directory
$null = & (Join-Path $source 'platforms\browser\browser-profile-acl.ps1') -Root $Directory
$null = & (Join-Path $source 'storage\legacy-private-source.ps1') -Path $file
$null = & (Join-Path $source 'storage\client-home.ps1') -Directory $Directory
# 真实修改本测试文件的权限，验证所有者兼容不会放行其他用户。
$acl = [IO.File]::GetAccessControl($file)
$everyone = [Security.Principal.SecurityIdentifier]::new('S-1-1-0')
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($everyone,'Read','Allow'))
[IO.File]::SetAccessControl($file, $acl)
[Console]::WriteLine((@{ administrator = $administrator; owner = $owner; user = $identity.User.Value; file = $file } | ConvertTo-Json -Compress))
