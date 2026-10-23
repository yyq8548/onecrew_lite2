param([Parameter(Mandatory=$true)][string]$Verifier)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::InputEncoding = [System.Text.UTF8Encoding]::new($false, $true)
trap { [Console]::Error.WriteLine($_.Exception.Message); exit 1 }
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $request = ConvertFrom-Json -InputObject $line
  if ($request.operation -ne 'Verify' -and $request.operation -ne 'VerifyTree') { throw 'Unsupported private verification operation' }
  if ($request.file) { & $Verifier -Operation $request.operation -Directory $request.directory -File $request.file }
  else { & $Verifier -Operation $request.operation -Directory $request.directory }
  [Console]::Out.Flush()
}
