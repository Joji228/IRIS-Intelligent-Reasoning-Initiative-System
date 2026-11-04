# I.R.I.S. helper: runs elevated (the "IRIS Helper" scheduled task) so the
# engine, which never has admin rights, can update and remove software and
# install Windows updates. It only does the named actions below, reading
# requests the engine drops into the queue folder and writing results next
# to them. It lives in C:\ProgramData\IRIS\helper, writable by admins only.

$ErrorActionPreference = 'Continue'
$queue = 'C:\ProgramData\IRIS\helper\queue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

# Package ids look like "Microsoft.Edge" or "7zip.7zip"; nothing else gets through.
function Test-Id([string] $id) { return $id -match '^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$' }

$wingetArgs = @('--silent', '--accept-package-agreements', '--accept-source-agreements', '--disable-interactivity')

function Invoke-Action($req) {
    switch ($req.action) {
        'update_apps' {
            if ($req.all) {
                winget upgrade --all --include-unknown @wingetArgs 2>&1 | Out-String
            } else {
                foreach ($id in @($req.ids)) {
                    if (-not (Test-Id $id)) { "skipped invalid id: $id"; continue }
                    "== $id"
                    winget upgrade --id $id --exact @wingetArgs 2>&1 | Out-String
                }
            }
        }
        'uninstall_app' {
            if (-not (Test-Id $req.id)) { return "invalid id: $($req.id)" }
            winget uninstall --id $req.id --exact --silent --accept-source-agreements --disable-interactivity 2>&1 | Out-String
        }
        'install_app' {
            if (-not (Test-Id $req.id)) { return "invalid id: $($req.id)" }
            winget install --id $req.id --exact @wingetArgs 2>&1 | Out-String
        }
        'windows_updates_install' {
            $session = New-Object -ComObject Microsoft.Update.Session
            $found = $session.CreateUpdateSearcher().Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
            $updates = New-Object -ComObject Microsoft.Update.UpdateColl
            foreach ($u in $found.Updates) {
                if (-not $u.EulaAccepted) { $u.AcceptEula() }
                [void]$updates.Add($u)
            }
            if ($updates.Count -eq 0) { return 'No Windows updates to install.' }
            $titles = ($found.Updates | ForEach-Object { "- $($_.Title)" }) -join "`n"
            $downloader = $session.CreateUpdateDownloader(); $downloader.Updates = $updates; [void]$downloader.Download()
            $installer = $session.CreateUpdateInstaller(); $installer.Updates = $updates
            $result = $installer.Install()
            "Installed $($updates.Count) update(s), result code $($result.ResultCode) (2 = succeeded). Restart needed: $($result.RebootRequired)`n$titles"
        }
        default { "unknown action: $($req.action)" }
    }
}

Get-ChildItem $queue -Filter '*.req.json' -ErrorAction SilentlyContinue | ForEach-Object {
    $resPath = $_.FullName -replace '\.req\.json$', '.res.json'
    try {
        $req = Get-Content $_.FullName -Raw | ConvertFrom-Json
        $output = (Invoke-Action $req) -join "`n"
        @{ ok = $true; output = $output } | ConvertTo-Json -Compress | Set-Content $resPath -Encoding UTF8
    } catch {
        @{ ok = $false; output = "$_" } | ConvertTo-Json -Compress | Set-Content $resPath -Encoding UTF8
    }
    Remove-Item $_.FullName -Force
}
