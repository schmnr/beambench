param(
    [Parameter(Mandatory = $true)][string]$Cli,
    [Parameter(Mandatory = $true)][string]$Desktop,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $env:RUNNER_TEMP -or $env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this runtime isolation test only on a disposable GitHub Actions Windows runner.'
}
$cliPath = (Resolve-Path $Cli).Path
$desktopPath = (Resolve-Path $Desktop).Path
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$output = (Resolve-Path $OutputDirectory).Path
$testRoot = Join-Path $env:RUNNER_TEMP ('Beam Bench runtime isolation ' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null

# Invalid application-local DLLs take precedence over the runner's installed VC
# runtime. Never rename/remove system DLLs or alter either release executable.
function Block-VCRuntime([string]$directory) {
    foreach ($name in @('MSVCP140.dll', 'VCRUNTIME140.dll', 'VCRUNTIME140_1.dll')) {
        [IO.File]::WriteAllText((Join-Path $directory $name), 'Deliberately invalid runtime DLL for Beam Bench CI')
    }
}
function Invoke-Probe([string]$executable, [string[]]$arguments, [string]$name) {
    $processArgs = @{
        FilePath = $executable
        PassThru = $true
        RedirectStandardOutput = Join-Path $output "$name.stdout.log"
        RedirectStandardError = Join-Path $output "$name.stderr.log"
    }
    if ($arguments.Count) { $processArgs.ArgumentList = $arguments }
    $process = Start-Process @processArgs
    try {
        if (-not $process.WaitForExit(30000)) { throw "$name did not exit within 30 seconds." }
        return $process.ExitCode
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
        $process.Dispose()
    }
}
Add-Type @'
using System.Runtime.InteropServices;
public static class BeamBenchRuntimeErrorMode {
    [DllImport("kernel32.dll")] public static extern uint GetErrorMode();
    [DllImport("kernel32.dll")] public static extern uint SetErrorMode(uint mode);
}
'@
$previousMode = [BeamBenchRuntimeErrorMode]::GetErrorMode()
[BeamBenchRuntimeErrorMode]::SetErrorMode($previousMode -bor 0x8003) | Out-Null
try {
    # A dynamically linked native positive control must work normally and fail
    # with the same blocked DLLs. This proves the runner cannot mask the test.
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (-not $vs) { throw 'MSVC is required for the dynamic-runtime positive control.' }
    $vcvars = Join-Path $vs 'VC/Auxiliary/Build/vcvars64.bat'
    $controlDir = Join-Path $testRoot 'control'
    New-Item -ItemType Directory -Path $controlDir | Out-Null
    $source = Join-Path $controlDir 'control.cpp'
    $control = Join-Path $controlDir 'control.exe'
    [IO.File]::WriteAllText($source, @'
#include <stdexcept>
#include <vector>
int main() {
    try { std::vector<int> v; v.reserve(v.max_size() + 1); }
    catch (const std::length_error&) { return 0; }
    return 1;
}
'@)
    $buildScript = Join-Path $controlDir 'build.cmd'
    @(
        '@echo off',
        "call `"$vcvars`" >nul",
        'if errorlevel 1 exit /b 1',
        "cl.exe /nologo /EHsc /MD `"$source`" /Fe:`"$control`" /Fo:`"$controlDir/control.obj`" /link /INCREMENTAL:NO"
    ) | Set-Content $buildScript -Encoding ascii
    & $env:ComSpec /d /c "`"$buildScript`"" 2>&1 | Set-Content (Join-Path $output 'control-build.log')
    if ($LASTEXITCODE -ne 0) { throw 'Could not build the dynamic-runtime positive control.' }
    if ((Invoke-Probe -executable $control -arguments @() -name 'control-normal') -ne 0) { throw 'The dynamic positive control must start normally.' }
    & node (Join-Path $PSScriptRoot 'check-windows-runtime.mjs') $control 2>&1 | Set-Content (Join-Path $output 'control-audit.log')
    if ($LASTEXITCODE -eq 0 -or -not (Select-String -Path (Join-Path $output 'control-audit.log') -SimpleMatch 'msvcp140.dll' -Quiet)) {
        throw 'The audit failed to detect MSVCP140.dll in the dynamic positive control.'
    }
    Block-VCRuntime $controlDir
    $blocked = $false
    try {
        $code = Invoke-Probe -executable $control -arguments @() -name 'control-blocked'
        $blocked = $code -eq -1073741701 # STATUS_INVALID_IMAGE_FORMAT, 0xC000007B
    } catch {
        $cause = $_.Exception
        while ($cause.InnerException) { $cause = $cause.InnerException }
        if ($cause -isnot [System.ComponentModel.Win32Exception] -or $cause.NativeErrorCode -ne 193) { throw }
        $blocked = $true # ERROR_BAD_EXE_FORMAT
    }
    if (-not $blocked) { throw 'The invalid local DLLs did not block the dynamic positive control.' }

    & node (Join-Path $PSScriptRoot 'check-windows-runtime.mjs') $cliPath $desktopPath
    if ($LASTEXITCODE -ne 0) { throw 'A release executable imports the external VC runtime.' }
    $cliDir = Join-Path $testRoot 'cli'
    $desktopDir = Join-Path $testRoot 'desktop'
    New-Item -ItemType Directory -Path $cliDir, $desktopDir | Out-Null
    $isolatedCli = Join-Path $cliDir 'beambench-cli.exe'
    $isolatedDesktop = Join-Path $desktopDir 'beambench-tauri.exe'
    Copy-Item $cliPath $isolatedCli
    Copy-Item $desktopPath $isolatedDesktop
    Block-VCRuntime $cliDir
    Block-VCRuntime $desktopDir
    if ((Invoke-Probe -executable $isolatedCli -arguments @('version', '--json') -name 'cli-blocked-runtime') -ne 0) {
        throw 'The standalone CLI did not start with the VC runtime blocked.'
    }
    $version = Get-Content (Join-Path $PSScriptRoot '../tauri-app/src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
    $reported = Get-Content (Join-Path $output 'cli-blocked-runtime.stdout.log') -Raw | ConvertFrom-Json
    if ($reported.version -ne $version.version) { throw 'The standalone CLI reported the wrong version.' }
    & (Join-Path $PSScriptRoot 'test-windows-installed-package.ps1') `
        -DesktopExecutable $isolatedDesktop -OutputDirectory (Join-Path $output 'desktop')
    @{
        dynamicControlBlocked = $blocked
        cliStartedWithRuntimeBlocked = $true
        desktopStartedTwiceWithRuntimeBlocked = $true
        cliSha256 = (Get-FileHash $isolatedCli -Algorithm SHA256).Hash.ToLowerInvariant()
        desktopSha256 = (Get-FileHash $isolatedDesktop -Algorithm SHA256).Hash.ToLowerInvariant()
        scope = 'Actual release executables with invalid local VC runtime DLLs; system DLLs unchanged. No physical hardware.'
    } | ConvertTo-Json | Set-Content (Join-Path $output 'runtime-isolation.json') -Encoding utf8
} finally {
    [BeamBenchRuntimeErrorMode]::SetErrorMode($previousMode) | Out-Null
    Remove-Item $testRoot -Recurse -Force
}
