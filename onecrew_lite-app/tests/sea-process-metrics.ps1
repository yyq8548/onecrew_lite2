param([Parameter(Mandatory=$true)][int]$ProcessId)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$selected = Get-Process -Id $ProcessId -ErrorAction Stop
[PSCustomObject]@{ workingSetBytes = $selected.WorkingSet64; privateBytes = $selected.PrivateMemorySize64; processorMilliseconds = $selected.TotalProcessorTime.TotalMilliseconds } | ConvertTo-Json -Compress
