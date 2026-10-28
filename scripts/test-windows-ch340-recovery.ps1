# Exercise the production COMPort open path with a test-only CH340 driver fault.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$repoRoot = Split-Path $PSScriptRoot -Parent
$serialTestRoot = Join-Path ([IO.Path]::GetTempPath()) ('beambench-ch340-' + [Guid]::NewGuid().ToString('N'))
$serialTargetRoot = Join-Path $repoRoot 'target/serialport-regression'
Copy-Item (Join-Path $repoRoot 'vendor/serialport') $serialTestRoot -Recurse
Add-Content (Join-Path $serialTestRoot 'Cargo.toml') "`n[workspace]"
$manifest = Join-Path $serialTestRoot 'Cargo.toml'
$comPath = Join-Path $serialTestRoot 'src/windows/com.rs'
$fixed = [IO.File]::ReadAllText($comPath)
$recoveryCall = 'dcb::set_dcb_on_open(handle, dcb, &builder.path)?;'
if (-not $fixed.Contains($recoveryCall)) { throw 'CH340 opening recovery call was not found' }

try {
    # Keep the same Win32 fault and full open/I/O test; disable only recovery.
    [IO.File]::WriteAllText($comPath, $fixed.Replace($recoveryCall, 'dcb::set_dcb(handle, dcb)?;'))
    $before = (& cargo test --manifest-path $manifest --target-dir $serialTargetRoot --lib windows::com::tests::ch340_recovers_high_baud_open_and_exchanges_status_at_requested_settings -- --exact 2>&1 | Out-String)
    $beforeCode = $LASTEXITCODE
    Write-Output $before
    if ($beforeCode -eq 0 -or -not $before.Contains('(Windows error 31)') -or -not $before.Contains('FAILED')) {
        throw 'The original opening path did not reproduce the expected Windows error 31'
    }
} finally {
    [IO.File]::WriteAllText($comPath, $fixed)
}

& cargo test --manifest-path $manifest --target-dir $serialTargetRoot --lib
if ($LASTEXITCODE -ne 0) { throw 'Windows serial initialization regressions failed' }
Remove-Item $serialTestRoot -Recurse -Force
