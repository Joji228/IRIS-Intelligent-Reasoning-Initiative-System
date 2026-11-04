# One-time dev toolchain setup for the IRIS build server (run elevated).
# Installs: long-path support, Git, Node LTS, Rust (MSVC), VS 2022 C++ Build Tools.
$ErrorActionPreference = 'Continue'
$log = 'C:\IRIS\setup.log'
New-Item -ItemType Directory -Force C:\IRIS | Out-Null
function Log($m) { $line = "$(Get-Date -Format 'HH:mm:ss') $m"; $line; Add-Content $log $line }

Log 'enable long paths'
Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem -Name LongPathsEnabled -Value 1

$wg = '--silent --accept-source-agreements --accept-package-agreements --disable-interactivity'
foreach ($id in 'Git.Git', 'OpenJS.NodeJS.LTS') {
    Log "winget install $id"
    $p = Start-Process winget -ArgumentList "install --id $id -e $wg" -Wait -PassThru -NoNewWindow
    Log "  exit $($p.ExitCode)"
}

Log 'install rustup (stable-msvc, minimal profile)'
$ri = "$env:TEMP\rustup-init.exe"
curl.exe -sSfL -o $ri https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
$p = Start-Process $ri -ArgumentList '-y --default-toolchain stable --profile minimal --default-host x86_64-pc-windows-msvc' -Wait -PassThru -NoNewWindow
Log "  exit $($p.ExitCode)"
Remove-Item $ri -Force

Log 'winget install VS 2022 Build Tools (C++ workload); this takes a while'
$vs = '--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
$p = Start-Process winget -ArgumentList "install --id Microsoft.VisualStudio.2022.BuildTools -e $wg --override `"$vs`"" -Wait -PassThru -NoNewWindow
Log "  exit $($p.ExitCode)"

Log 'done'
