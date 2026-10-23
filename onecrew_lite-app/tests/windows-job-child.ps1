param([Parameter(Mandatory=$true)][string]$Configuration)
$ErrorActionPreference = 'Stop'
$config = [IO.File]::ReadAllText($Configuration) | ConvertFrom-Json
while (-not [IO.File]::Exists($config.gate)) { Start-Sleep -Milliseconds 20 }
$launchArguments = [string[]]@($config.arguments)
$result = & $config.executable @launchArguments
if ($LASTEXITCODE -ne 0) { throw 'Real service launch failed inside Windows Job' }
[IO.File]::WriteAllText($config.ready, ($result -join "`n"), [Text.UTF8Encoding]::new($false))
Start-Sleep -Seconds 120
