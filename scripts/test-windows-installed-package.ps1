[CmdletBinding(DefaultParameterSetName = 'Installed')]
param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Installed')][string]$Installer,
    [Parameter(Mandatory = $true, ParameterSetName = 'Standalone')][string]$DesktopExecutable,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
if (-not $env:RUNNER_TEMP -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installer smoke only on a disposable GitHub Actions Windows runner.'
}
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = (Resolve-Path $OutputDirectory).Path
$installerHash = $null
if ($PSCmdlet.ParameterSetName -eq 'Installed') {
    $installerPath = (Resolve-Path $Installer).Path
    if ((Get-AuthenticodeSignature $installerPath).Status -ne 'Valid') {
        throw 'The installer must have a valid Authenticode signature.'
    }
    $installerHash = (Get-FileHash $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $install = Join-Path $env:RUNNER_TEMP ('beambench-installed-' + [guid]::NewGuid().ToString('N'))
    $setup = Start-Process -FilePath $installerPath -ArgumentList @('/S', "/D=$install") -Wait -PassThru
    if ($setup.ExitCode -notin @(0, 3010)) {
        throw "Installer failed with exit code $($setup.ExitCode)."
    }
    $desktop = Join-Path $install 'beambench-tauri.exe'
} else {
    # PR CI exercises the actual release binary without release-signing secrets.
    # The default installer path above still requires both signatures.
    $desktop = (Resolve-Path $DesktopExecutable).Path
}
if (-not (Test-Path $desktop -PathType Leaf)) {
    throw 'The installer did not create the desktop executable.'
}
$signature = Get-AuthenticodeSignature $desktop
if ($PSCmdlet.ParameterSetName -eq 'Installed' -and $signature.Status -ne 'Valid') {
    throw 'The installed desktop signature is invalid.'
}
& node (Join-Path $PSScriptRoot 'check-windows-runtime.mjs') $desktop
if ($LASTEXITCODE -ne 0) { throw 'The desktop requires an external Visual C++ runtime.' }

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
    installerSha256 = $installerHash
    installedSha256 = (Get-FileHash $desktop -Algorithm SHA256).Hash.ToLowerInvariant()
    installedSignature = $signature.Status.ToString()
    installedVersion = (Get-Item $desktop).VersionInfo.ProductVersion
    mode = $PSCmdlet.ParameterSetName
    externalVCRuntimeImports = @()
    launches = $results
    paidAccountAndCredentialRestorationTested = $false
    scope = 'Disposable Windows runner; actual desktop startup/restart. Installer mode requires Authenticode signatures. No physical hardware.'
} | ConvertTo-Json -Depth 5 | Set-Content -Path (Join-Path $output 'installed-package.json') -Encoding utf8
Write-Output "$($PSCmdlet.ParameterSetName) Windows desktop and two actual launches passed."
