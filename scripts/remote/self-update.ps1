# Runs on the server, started by the engine's code_install through Task
# Scheduler (so it outlives the engine it replaces): installs the new build,
# waits for the new engine to answer, and puts the previous build back if it
# doesn't. Writes the outcome to -Result as JSON for the new (or old) engine.
param(
    [Parameter(Mandatory)] [string] $Installer,
    [string] $Previous,
    [Parameter(Mandatory)] [string] $Result,
    [int] $WaitSeconds = 120
)

$ErrorActionPreference = 'Continue'
$log = New-Object System.Collections.Generic.List[string]
function Note([string] $line) { $log.Add("$(Get-Date -Format HH:mm:ss) $line") }
$installScript = Join-Path $PSScriptRoot 'install-app.ps1'

function Install([string] $setup) {
    Note "installing $setup"
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $installScript -Installer $setup 2>&1 |
        ForEach-Object { Note "  $_" }
}

function Healthy {
    $deadline = (Get-Date).AddSeconds($WaitSeconds)
    while ((Get-Date) -lt $deadline) {
        try {
            $r = Invoke-RestMethod -Uri 'http://127.0.0.1:7700/health' -TimeoutSec 4
            if ($r.status -eq 'ok') { return $true }
        } catch { }
        Start-Sleep -Seconds 3
    }
    return $false
}

function Finish([bool] $ok, [bool] $rolledBack) {
    $out = [ordered]@{
        ok          = $ok
        rolled_back = $rolledBack
        log         = ($log -join "`n")
        finished_at = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    }
    $out | ConvertTo-Json | Set-Content -Path $Result -Encoding UTF8
    Unregister-ScheduledTask -TaskName 'IRIS Self-update' -Confirm:$false -ErrorAction SilentlyContinue
    exit 0
}

# Let the engine finish its reply ("installing…") before it's stopped.
Start-Sleep -Seconds 15
Install $Installer
if (Healthy) {
    Note 'the new version is up'
    Finish $true $false
}

Note "the new version didn't answer within $WaitSeconds s"
if ($Previous -and (Test-Path $Previous)) {
    Install $Previous
    if (Healthy) { Note 'the previous version is back' } else { Note 'the previous version did not answer either' }
    Finish $false $true
}
Note 'no previous installer to roll back to'
Finish $false $false
