param([Parameter(Mandatory=$true)][string]$Executable,
      [Parameter(Mandatory=$true)][string]$RuntimeLocal)
$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
if (-not [IO.Path]::IsPathRooted($RuntimeLocal)) { throw 'absolute RuntimeLocal required' }
$probeRoot = Join-Path $env:TEMP ('onecrew-codex-host-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $probeRoot
$resultPath = Join-Path $probeRoot 'result.json'
$script = @'
$env:LOCALAPPDATA='__LOCAL__'
$env:PATH=$env:SystemRoot+'\System32;'+$env:SystemRoot
$output = & '__EXE__' plugin status --refresh --json 2>&1
$code = $LASTEXITCODE
[IO.File]::WriteAllText('__RESULT__.tmp', (@{exit_code=$code;output=($output -join "`n")} | ConvertTo-Json -Compress))
[IO.File]::Move('__RESULT__.tmp','__RESULT__')
'@
$script=$script.Replace('__LOCAL__',$RuntimeLocal.Replace("'","''")).Replace('__EXE__',$Executable.Replace("'","''")).Replace('__RESULT__',$resultPath.Replace("'","''"))
$scheduler=New-Object -ComObject Schedule.Service
$scheduler.Connect()
$folder=$scheduler.GetFolder('\')
$name='OneCrew-CodexProbe-'+[guid]::NewGuid().ToString('N')
$identity=[Security.Principal.WindowsIdentity]::GetCurrent().Name
$definition=$scheduler.NewTask(0)
$definition.Principal.UserId=$identity
$definition.Principal.LogonType=3
$definition.Principal.RunLevel=0
$definition.Settings.ExecutionTimeLimit='PT2M'
$definition.Settings.DisallowStartIfOnBatteries=$false
$definition.Settings.StopIfGoingOnBatteries=$false
$action=$definition.Actions.Create(0)
$action.Path=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$action.Arguments='-NoProfile -NonInteractive -WindowStyle Hidden -EncodedCommand '+[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($script))
$registered=$false
try {
    $null=$folder.RegisterTaskDefinition($name,$definition,6,$identity,$null,3,$null)
    $registered=$true
    $null=$folder.GetTask($name).Run($null)
    $deadline=[DateTime]::UtcNow.AddSeconds(90)
    while (-not [IO.File]::Exists($resultPath)) {
        if ([DateTime]::UtcNow -ge $deadline) { throw 'independent Codex probe timed out' }
        Start-Sleep -Milliseconds 100
    }
    $result=Get-Content -LiteralPath $resultPath -Raw | ConvertFrom-Json
    if ($result.exit_code -ne 0) { throw $result.output }
    $status=$result.output | ConvertFrom-Json
    if ($status.state -ne 'installed' -or $status.verification -ne 'fresh' -or $status.enabled -ne $true) { throw 'independent plugin readback failed' }
    $status | Select-Object state,verification,installed_version,enabled,diagnostic_code | ConvertTo-Json -Compress
} finally {
    if ($registered) { $folder.GetTask($name).Stop(0); $folder.DeleteTask($name,0) }
}
