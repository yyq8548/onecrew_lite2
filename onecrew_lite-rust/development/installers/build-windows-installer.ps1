param(
    [switch]$UnsignedDevelopment,
    [string]$SigningThumbprint = '',
    [string]$SignToolPath = ''
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$versionFile = Join-Path $root 'development\release\version.json'
$version = (Get-Content -LiteralPath $versionFile -Raw | ConvertFrom-Json).cliPluginBase
if ($version -cnotmatch '^\d+\.\d+\.\d+$') { throw 'Invalid onecrew_lite version' }
$artifactDir = Join-Path $root "dist\onecrew_lite\$version\x86_64-pc-windows-msvc"
$manifestFile = Join-Path $artifactDir 'manifest.json'
$cli = Join-Path $artifactDir 'onecrew_lite.exe'
if (-not (Test-Path -LiteralPath $manifestFile -PathType Leaf) -or -not (Test-Path -LiteralPath $cli -PathType Leaf)) {
    throw 'Build the Windows onecrew_lite release artifact and manifest first.'
}
$manifest = Get-Content -LiteralPath $manifestFile -Raw | ConvertFrom-Json
if ($manifest.schema_version -ne 1 -or $manifest.product -cne 'onecrew_lite-cli' -or
    $manifest.target -cne 'x86_64-pc-windows-msvc' -or $manifest.version -cne $version -or
    $manifest.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $manifest.size -le 0) {
    throw 'Invalid onecrew_lite Windows manifest.'
}
if ((Get-Item -LiteralPath $cli).Length -ne [long]$manifest.size -or
    (Get-FileHash -LiteralPath $cli -Algorithm SHA256).Hash.ToLowerInvariant() -cne $manifest.sha256) {
    throw 'onecrew_lite Windows artifact does not match its manifest.'
}
$reported = @(& $cli --version 2>$null)
if ($LASTEXITCODE -ne 0 -or $reported.Count -ne 1 -or $reported[0] -cne "onecrew_lite $version") {
    throw 'onecrew_lite Windows artifact did not execute with the expected version.'
}
if (-not $UnsignedDevelopment) {
    if ($SigningThumbprint -cnotmatch '^[0-9A-Fa-f]{40}$') {
        throw 'A customer installer requires a 40-character signing certificate thumbprint.'
    }
    if (-not $SignToolPath -or -not (Test-Path -LiteralPath $SignToolPath -PathType Leaf)) {
        throw 'A customer installer requires an explicit SignToolPath.'
    }
    $certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$SigningThumbprint" -ErrorAction SilentlyContinue
    if (-not $certificate -or -not $certificate.HasPrivateKey -or
        $certificate.NotBefore -gt (Get-Date) -or $certificate.NotAfter -lt (Get-Date) -or
        -not ($certificate.EnhancedKeyUsageList | Where-Object { $_.Value -eq '1.3.6.1.5.5.7.3.3' })) {
        throw 'A valid code-signing certificate with a private key is required.'
    }
    $binarySignature = Get-AuthenticodeSignature -LiteralPath $cli
    if ($binarySignature.Status -ne 'Valid' -or
        $binarySignature.SignerCertificate.Thumbprint -cne $SigningThumbprint.ToUpperInvariant()) {
        throw 'The release executable must already be signed by the selected certificate before its manifest is generated.'
    }
}
$installRecord = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
    Where-Object DisplayName -Like 'Nullsoft Install System*' | Select-Object -First 1
$compiler = if ($installRecord -and $installRecord.InstallLocation) {
    Join-Path $installRecord.InstallLocation 'makensis.exe'
} else { $null }
if (-not $compiler -or -not (Test-Path -LiteralPath $compiler -PathType Leaf)) {
    throw 'NSIS compiler is required on the build machine.'
}
$outputDir = Join-Path $root "dist\onecrew_lite\$version\windows-installer"
[IO.Directory]::CreateDirectory($outputDir) | Out-Null
$script = Join-Path $PSScriptRoot 'onecrew-windows.nsi'
& $compiler "/DOneCrewVersion=$version" "/DOneCrewExe=$cli" "/DOneCrewOutput=$outputDir" $script
if ($LASTEXITCODE -ne 0) { throw 'NSIS compilation failed.' }
$installer = Join-Path $outputDir "onecrew_lite-Setup-$version-windows-x64.exe"
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) { throw 'Installer output is missing.' }
if (-not $UnsignedDevelopment) {
    & $SignToolPath sign /sha1 $SigningThumbprint /fd SHA256 /tr https://timestamp.digicert.com /td SHA256 $installer
    if ($LASTEXITCODE -ne 0) { throw 'Installer signing failed.' }
    $signature = Get-AuthenticodeSignature -LiteralPath $installer
    if ($signature.Status -ne 'Valid' -or
        $signature.SignerCertificate.Thumbprint -cne $SigningThumbprint.ToUpperInvariant()) {
        throw 'Installer signature verification failed.'
    }
}
Write-Output "Installer: $installer"
Write-Output "Signed: $(-not $UnsignedDevelopment)"
Write-Output "SHA-256: $((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant())"
