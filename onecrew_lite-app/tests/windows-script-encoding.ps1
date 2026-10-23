param([Parameter(Mandatory=$true)][string]$Verifier, [Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# 按中文 Windows 的默认编码读取，保留 PowerShell 的 BOM 自动检测行为。
$reader = New-Object System.IO.StreamReader($Verifier, [System.Text.Encoding]::GetEncoding(936), $true)
try { $source = $reader.ReadToEnd() } finally { $reader.Dispose() }
$script = [scriptblock]::Create($source)
& $script -Operation CreateTree -Directory $Directory -TreeManifest '["child"]'
& $script -Operation VerifyTree -Directory $Directory
