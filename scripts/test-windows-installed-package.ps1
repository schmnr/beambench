param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
if (-not $env:RUNNER_TEMP -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installer smoke only on a disposable GitHub Actions Windows runner.'
}
$installerPath = (Resolve-Path $Installer).Path
if ((Get-AuthenticodeSignature $installerPath).Status -ne 'Valid') {
    throw 'The installer must have a valid Authenticode signature.'
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = (Resolve-Path $OutputDirectory).Path
$install = Join-Path $env:RUNNER_TEMP ('beambench-installed-' + [guid]::NewGuid().ToString('N'))
$setup = Start-Process -FilePath $installerPath -ArgumentList @('/S', "/D=$install") -Wait -PassThru
if ($setup.ExitCode -notin @(0, 3010)) {
    throw "Installer failed with exit code $($setup.ExitCode)."
}
$desktop = Join-Path $install 'beambench-tauri.exe'
if (-not (Test-Path $desktop -PathType Leaf)) {
    throw 'The installer did not create the desktop executable.'
}
$signature = Get-AuthenticodeSignature $desktop
if ($signature.Status -ne 'Valid') {
    throw 'The installed desktop signature is invalid.'
}

$results = @()
foreach ($launch in 1..2) {
    $stdout = Join-Path $output "launch-$launch.stdout.log"
    $stderr = Join-Path $output "launch-$launch.stderr.log"
    $app = Start-Process -FilePath $desktop -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $ready = $false
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(60)
        while ([DateTime]::UtcNow -lt $deadline) {
            $app.Refresh()
            if ($app.HasExited) { throw 'Installed desktop exited before readiness.' }
            if ((Test-Path $stderr) -and (Select-String -Path $stderr -SimpleMatch 'Frontend ready' -Quiet)) {
                Start-Sleep -Seconds 2
                $app.Refresh()
                if ($app.HasExited) { throw 'Installed desktop exited immediately after readiness.' }
                $ready = $true
                break
            }
            Start-Sleep -Milliseconds 200
        }
        if (-not $ready) { throw 'Installed desktop did not report Frontend ready.' }
        $results += @{ launch = $launch; frontendReady = $ready; aliveAfterReady = $true }
    } finally {
        if (-not $app.HasExited) {
            $app.CloseMainWindow() | Out-Null
            if (-not $app.WaitForExit(5000)) { Stop-Process -Id $app.Id -Force }
        }
    }
}

@{
    installerSha256 = (Get-FileHash $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    installedSha256 = (Get-FileHash $desktop -Algorithm SHA256).Hash.ToLowerInvariant()
    installedSignature = $signature.Status.ToString()
    installedVersion = (Get-Item $desktop).VersionInfo.ProductVersion
    launches = $results
    paidAccountAndCredentialRestorationTested = $false
    scope = 'Disposable Windows runner; signed installer and desktop startup/restart. No physical hardware.'
} | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $output 'installed-package.json') -Encoding utf8
Write-Output 'Signed Windows installer and two actual desktop launches passed.'
