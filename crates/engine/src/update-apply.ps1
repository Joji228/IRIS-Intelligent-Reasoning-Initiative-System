# Runs after the engine has downloaded and checked a new installer (see update.rs).
# The engine starts it detached, because it has to outlive the engine and the app it
# replaces: it closes I.R.I.S., runs the installer quietly, starts the app again, and
# writes how it went to -Result. Your agents, chats and settings are not touched: they
# live in a separate data folder that the installer never opens.
param(
    [Parameter(Mandatory)] [string] $Installer,
    [Parameter(Mandatory)] [string] $Desktop,
    [Parameter(Mandatory)] [string] $Result,
    [string] $Version = ''
)

$ErrorActionPreference = 'Continue'
$log = New-Object System.Collections.Generic.List[string]
function Note([string] $line) { $log.Add("$(Get-Date -Format HH:mm:ss) $line") }

function Finish([bool] $ok) {
    $out = [ordered]@{
        ok          = $ok
        version     = $Version
        log         = ($log -join "`n")
        finished_at = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    }
    $out | ConvertTo-Json | Set-Content -Path $Result -Encoding UTF8
    exit 0
}

# Let the engine finish answering the app ("updating…") before anything is closed.
Start-Sleep -Seconds 3

# Ask the running app to quit properly (this also stops its engine); then make sure.
if (Test-Path $Desktop) {
    Note 'closing I.R.I.S.'
    Start-Process $Desktop -ArgumentList '--quit' -WindowStyle Hidden | Out-Null
    for ($i = 0; $i -lt 40 -and (Get-Process iris-desktop -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Milliseconds 500 }
}
Get-Process IRIS, iris-desktop, iris-engine -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

Note "running the installer $Installer"
try {
    $p = Start-Process $Installer -ArgumentList '/S' -Wait -PassThru
    Note "the installer finished with exit code $($p.ExitCode)"
    if ($p.ExitCode -ne 0) { Finish $false }
} catch {
    Note "the installer could not run: $_"
    Finish $false
}

if (-not (Test-Path $Desktop)) { Note "the app was not found at $Desktop afterwards"; Finish $false }
Note 'starting I.R.I.S.'
Start-Process $Desktop | Out-Null

# Wait for the new engine, and for it to be the version we installed.
$deadline = (Get-Date).AddSeconds(120)
while ((Get-Date) -lt $deadline) {
    try {
        $h = Invoke-RestMethod -Uri 'http://127.0.0.1:7700/health' -TimeoutSec 4
        if ($h.status -eq 'ok' -and (-not $Version -or $h.version -eq $Version)) {
            Note "I.R.I.S. $($h.version) is up"
            Remove-Item $Installer -Force -ErrorAction SilentlyContinue
            Finish $true
        }
    } catch { }
    Start-Sleep -Seconds 3
}
Note "the new version did not answer within two minutes"
Finish $false
