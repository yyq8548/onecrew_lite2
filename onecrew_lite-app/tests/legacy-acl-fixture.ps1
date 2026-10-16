param([Parameter(Mandatory=$true)][string]$Directory, [Parameter(Mandatory=$true)][ValidateSet('Administrators','Everyone')][string]$Grant)
$ErrorActionPreference = 'Stop'
$acl = [IO.Directory]::GetAccessControl($Directory)
$identity = [Security.Principal.SecurityIdentifier]::new($(if ($Grant -eq 'Administrators') { 'S-1-5-32-544' } else { 'S-1-1-0' }))
$flags = [Security.AccessControl.InheritanceFlags]::ContainerInherit -bor [Security.AccessControl.InheritanceFlags]::ObjectInherit
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity, [Security.AccessControl.FileSystemRights]::FullControl, $flags, [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow))
[IO.Directory]::SetAccessControl($Directory, $acl)
