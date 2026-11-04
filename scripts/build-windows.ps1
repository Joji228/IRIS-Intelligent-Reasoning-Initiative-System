# Builds the I.R.I.S. desktop installer on this Windows PC:
# release engine -> bundled as the app's sidecar -> UI -> Tauri NSIS installer.
# Output: target\release\bundle\nsis\IRIS_<version>_x64-setup.exe (or under CARGO_TARGET_DIR)
# Needs Git, Node.js LTS, Rust (stable, MSVC) and the VS 2022 C++ Build Tools
# (scripts\server-setup.ps1 installs them).
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }

function Run([string] $what, [scriptblock] $cmd) {
    Write-Host "== $what"
    & $cmd
    if ($LASTEXITCODE) { throw "$what failed (exit $LASTEXITCODE)" }
}

Run 'engine (release)' { cargo build --release -p iris-engine }
New-Item -ItemType Directory -Force app\src-tauri\binaries | Out-Null
Copy-Item (Join-Path $target 'release\iris-engine.exe') app\src-tauri\binaries\iris-engine-x86_64-pc-windows-msvc.exe -Force

# npm/npx resolve to .ps1 shims that execution policy may block; the .cmd ones always work.
Push-Location app
try {
    Run 'npm ci' { npm.cmd ci --no-audit --no-fund }
    Run 'tauri build' { npx.cmd tauri build }
} finally {
    Pop-Location
}
Get-ChildItem (Join-Path $target 'release\bundle\nsis\*.exe') | ForEach-Object {
    Write-Host ("installer: {0} ({1} MB)" -f $_.FullName, [math]::Round($_.Length / 1MB, 1))
}
