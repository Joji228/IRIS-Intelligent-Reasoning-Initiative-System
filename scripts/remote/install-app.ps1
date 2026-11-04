# Runs on the server: installs (or updates) the I.R.I.S. desktop app from the
# freshly built NSIS installer and starts it in the logged-on user's session.
#
# First install only: retires the Phase 1 "IRIS Engine (dev)" task and moves its
# data (C:\IRIS\data) into the app's data dir so agents and history carry over.
param([string] $Installer)

$ErrorActionPreference = 'Stop'
$dataDir = Join-Path $env:LOCALAPPDATA 'dev.iris.desktop'

if (-not $Installer) {
    $Installer = (Get-ChildItem C:\IRIS\target\release\bundle\nsis\*-setup.exe |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
}
if (-not $Installer) { throw 'no installer found; build one first (scripts\build-windows.ps1)' }
"installer: $Installer"

# 1. Stop anything running: the app, its engine, the old dev task.
$devTask = Get-ScheduledTask -TaskName 'IRIS Engine (dev)' -ErrorAction SilentlyContinue
if ($devTask) {
    Stop-ScheduledTask -TaskName 'IRIS Engine (dev)' -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName 'IRIS Engine (dev)' -Confirm:$false
    'removed the dev engine task'
}
# Ask the running app to quit cleanly first (it runs on the user's desktop, so
# go through a task there); a killed app leaves a dead icon in the tray.
$running = Get-Process iris-desktop -ErrorAction SilentlyContinue | Select-Object -First 1
if ($running -and $running.Path) {
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $action = New-ScheduledTaskAction -Execute $running.Path -Argument '--quit'
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    Register-ScheduledTask -TaskName 'IRIS Quit (once)' -Action $action -Principal $principal -Force | Out-Null
    Start-ScheduledTask -TaskName 'IRIS Quit (once)'
    for ($i = 0; $i -lt 20 -and (Get-Process iris-desktop -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 500 }
    Unregister-ScheduledTask -TaskName 'IRIS Quit (once)' -Confirm:$false
}
Get-Process IRIS, iris-desktop, iris-engine -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 2

# 2. Carry the dev data over once (the engine is stopped, so the DB is consistent).
if (-not (Test-Path (Join-Path $dataDir 'iris.db')) -and (Test-Path C:\IRIS\data\iris.db)) {
    New-Item -ItemType Directory -Force $dataDir | Out-Null
    Copy-Item C:\IRIS\data\* $dataDir -Recurse -Force -Exclude logs
    "migrated data: C:\IRIS\data -> $dataDir"
}

# 3. Silent install (per-user, no admin prompt).
$p = Start-Process $Installer -ArgumentList '/S' -Wait -PassThru
"installer exit code: $($p.ExitCode)"
if ($p.ExitCode -ne 0) { exit $p.ExitCode }

$uninstall = Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall |
    ForEach-Object { Get-ItemProperty $_.PSPath } |
    Where-Object { $_.DisplayName -eq 'IRIS' } | Select-Object -First 1
$installDir = $uninstall.InstallLocation.Trim('"')
$exe = Get-ChildItem $installDir -Filter *.exe | Where-Object { $_.Name -notmatch 'uninstall|iris-engine' } | Select-Object -First 1
"installed: $($exe.FullName) (version $($uninstall.DisplayVersion))"

# 3b. Keep the elevated helper and the boot-time engine up to date.
$setup = 'C:\IRIS\src\scripts\remote\setup-host.ps1'
if (Test-Path $setup) {
    & $setup -InstallDir $installDir -HelperSource 'C:\IRIS\src\scripts\remote\iris-helper.ps1'
}

# 4. Start it on the real desktop (session 1), not inside this SSH session.
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$action = New-ScheduledTaskAction -Execute $exe.FullName
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'IRIS Launch (once)' -Action $action -Principal $principal -Force | Out-Null
Start-ScheduledTask -TaskName 'IRIS Launch (once)'
Start-Sleep 3
Unregister-ScheduledTask -TaskName 'IRIS Launch (once)' -Confirm:$false
'launched'

# 5. Point the shortcuts at a copy of the icon whose name changes with its
# content, so Explorer can't keep showing an icon it cached earlier.
$ico = 'C:\IRIS\src\app\src-tauri\icons\icon.ico'
if (Test-Path $ico) {
    $hash = (Get-FileHash $ico -Algorithm MD5).Hash.Substring(0, 8).ToLower()
    $target = Join-Path $installDir "iris-$hash.ico"
    Get-ChildItem $installDir -Filter 'iris-*.ico' | Where-Object { $_.FullName -ne $target } | Remove-Item -Force
    Copy-Item $ico $target -Force
    $shell = New-Object -ComObject WScript.Shell
    foreach ($lnk in @("$([Environment]::GetFolderPath('Desktop'))\IRIS.lnk", "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\IRIS.lnk")) {
        if (Test-Path $lnk) {
            $l = $shell.CreateShortcut($lnk)
            $l.IconLocation = "$target,0"
            $l.Save()
        }
    }
    'shortcut icons refreshed'
}
