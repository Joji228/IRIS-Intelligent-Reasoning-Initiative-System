# Runs on the server: (re)registers the "IRIS Engine (dev)" scheduled task and
# the firewall rule for the API port. The task runs in the interactive session
# of the logged-on user with a hidden window and appends output to a log.
# Temporary: from Phase 2 the desktop app owns starting the engine.
param([switch] $Register)

$name = 'IRIS Engine (dev)'
$exe = 'C:\IRIS\bin\iris-engine.exe'
$log = 'C:\IRIS\logs\engine.log'

if ($Register) {
    $cmd = "`$env:IRIS_DATA_DIR = 'C:\IRIS\data'; & '$exe' *>> '$log'"
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' `
        -Argument "-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -Command `"$cmd`""
    # $env:USERNAME can be empty in SSH sessions; ask Windows for the identity.
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Limited
    $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
        -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $name -Action $action -Principal $principal -Settings $settings -Force | Out-Null

    if (-not (Get-NetFirewallRule -DisplayName 'IRIS Engine API' -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -DisplayName 'IRIS Engine API' -Direction Inbound -Protocol TCP `
            -LocalPort 7700 -Action Allow -Profile Private | Out-Null
    }
    "task + firewall rule ready"
}
