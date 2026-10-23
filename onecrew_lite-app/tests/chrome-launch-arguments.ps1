param([Parameter(Mandatory=$true)][string]$ProfileDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$expected = '--user-data-dir=' + [System.IO.Path]::GetFullPath($ProfileDirectory)
$selected = @(Get-CimInstance Win32_Process -Filter "Name='chrome.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains($expected) -and -not $_.CommandLine.Contains('--type=') })
if ($selected.Count -ne 1) { throw 'Expected one actual owned Chrome process' }
@{ commandLine=$selected[0].CommandLine } | ConvertTo-Json -Compress
