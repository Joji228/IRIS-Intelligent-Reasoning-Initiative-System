# Runs on the server with admin rights (over SSH, from install-app.ps1). Sets
# up what the per-user app can't do itself:
#   - "IRIS Engine (boot)": starts the engine when Windows boots, before
#     anyone logs in, so agents and routines keep working after a restart.
#     It runs as the user without storing a password (S4U); the desktop app
#     picks up this engine when the user logs in.
#   - "IRIS Helper": the elevated helper (iris-helper.ps1) the engine starts
#     for app updates, uninstalls and Windows Update installs.
param([Parameter(Mandatory)] [string] $InstallDir, [Parameter(Mandatory)] [string] $HelperSource)

$ErrorActionPreference = 'Stop'
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name

# The helper's folder: admins write the script, the user may only drop requests.
$helperDir = 'C:\ProgramData\IRIS\helper'
$queue = Join-Path $helperDir 'queue'
New-Item -ItemType Directory -Force $queue | Out-Null
Copy-Item $HelperSource (Join-Path $helperDir 'iris-helper.ps1') -Force
icacls $helperDir /inheritance:r /grant:r 'Administrators:(OI)(CI)F' 'SYSTEM:(OI)(CI)F' "${user}:(OI)(CI)RX" | Out-Null
icacls $queue /grant:r "${user}:(OI)(CI)M" | Out-Null

$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew

# Elevated helper, started on demand by the engine (schtasks /run).
$helperAction = New-ScheduledTaskAction -Execute 'powershell.exe' `
    -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$helperDir\iris-helper.ps1`""
$helperPrincipal = New-ScheduledTaskPrincipal -UserId $user -LogonType S4U -RunLevel Highest
Register-ScheduledTask -TaskName 'IRIS Helper' -Action $helperAction -Principal $helperPrincipal -Settings $settings -Force | Out-Null
# Let the (non-elevated) user start it: read + execute on the task.
$svc = New-Object -ComObject 'Schedule.Service'; $svc.Connect()
$task = $svc.GetFolder('\').GetTask('IRIS Helper')
$sid = (New-Object Security.Principal.NTAccount($user)).Translate([Security.Principal.SecurityIdentifier]).Value
$sddl = $task.GetSecurityDescriptor(4) + "(A;;GRGX;;;$sid)"
$task.SetSecurityDescriptor($sddl, 0)

# The engine at boot, logging next to the app's own engine log.
$dataDir = Join-Path $env:LOCALAPPDATA 'dev.iris.desktop'
$engine = Join-Path $InstallDir 'iris-engine.exe'
$bootAction = New-ScheduledTaskAction -Execute 'cmd.exe' `
    -Argument "/c `"`"$engine`" >> `"$dataDir\logs\engine-boot.log`" 2>&1`"" -WorkingDirectory $InstallDir
$bootTrigger = New-ScheduledTaskTrigger -AtStartup
$bootPrincipal = New-ScheduledTaskPrincipal -UserId $user -LogonType S4U -RunLevel Limited
$bootSettings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 5 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName 'IRIS Engine (boot)' -Action $bootAction -Trigger $bootTrigger `
    -Principal $bootPrincipal -Settings $bootSettings -Force | Out-Null

'host tasks ready'
