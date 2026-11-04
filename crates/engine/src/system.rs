//! "This computer" tools: the server's health, app updates (winget), Windows
//! Update, restart and shutdown, as typed functions instead of free-form
//! PowerShell. Changes that need admin rights go through the optional
//! "IRIS Helper" task (scripts/remote/iris-helper.ps1), an elevated helper
//! that only performs these named actions; without it, winget runs as the
//! user, which covers per-user apps.

use std::{path::PathBuf, time::Duration};

use iris_providers::ToolCall;
use serde_json::{Value, json};

use crate::{
    api::AutoMode,
    tools::{ToolOutcome, truncate},
};

const HELPER_TASK: &str = "IRIS Helper";
const HELPER_QUEUE: &str = r"C:\ProgramData\IRIS\helper\queue";
/// Updates can take a while.
const HELPER_TIMEOUT: Duration = Duration::from_secs(40 * 60);
const MAX_OUTPUT: usize = 8_000;

/// Whether this call waits for the user, given the agent's auto mode.
/// Uninstalling and power always ask; updates run by themselves in Full auto.
pub fn needs_approval(call: &ToolCall, mode: AutoMode) -> bool {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    match call.name.as_str() {
        "uninstall_app" | "install_app" | "power" => true,
        "update_apps" => mode != AutoMode::Full,
        "windows_updates" => args["install"].as_bool().unwrap_or(false) && mode != AutoMode::Full,
        _ => false,
    }
}

pub async fn run(call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let id = || args["id"].as_str().map(str::trim).filter(|s| valid_id(s)).map(str::to_owned);
    match call.name.as_str() {
        "system_status" => status().await,
        "app_updates" => {
            let out = powershell(
                "winget upgrade --include-unknown --accept-source-agreements --disable-interactivity",
                Duration::from_secs(120),
            )
            .await;
            ok("Checked for app updates", out)
        }
        "update_apps" => {
            let all = args["all"].as_bool().unwrap_or(false);
            let ids: Vec<String> = args["ids"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).filter(|s| valid_id(s)).map(str::to_owned).collect())
                .unwrap_or_default();
            if !all && ids.is_empty() {
                return err("Updating apps failed", "give `ids` (winget ids from app_updates) or all=true");
            }
            let summary = if all { "Updated all apps".to_owned() } else { format!("Updated {}", ids.join(", ")) };
            elevated_or_user(json!({ "action": "update_apps", "all": all, "ids": ids }), &summary, || {
                if all {
                    "winget upgrade --all --include-unknown --silent --accept-package-agreements --accept-source-agreements --disable-interactivity".to_owned()
                } else {
                    ids.iter()
                        .map(|i| format!("\"== {i}\"; winget upgrade --id {i} --exact --silent --accept-package-agreements --accept-source-agreements --disable-interactivity"))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            })
            .await
        }
        "uninstall_app" => match id() {
            Some(i) => {
                elevated_or_user(json!({ "action": "uninstall_app", "id": i }), &format!("Uninstalled {i}"), || {
                    format!("winget uninstall --id {i} --exact --silent --accept-source-agreements --disable-interactivity")
                })
                .await
            }
            None => err("Uninstalling failed", "give the app's winget `id` (see app_updates or search_apps)"),
        },
        "install_app" => match id() {
            Some(i) => {
                elevated_or_user(json!({ "action": "install_app", "id": i }), &format!("Installed {i}"), || {
                    format!("winget install --id {i} --exact --silent --accept-package-agreements --accept-source-agreements --disable-interactivity")
                })
                .await
            }
            None => err("Installing failed", "give the app's winget `id` (use search_apps)"),
        },
        "search_apps" => match args["query"].as_str().map(str::trim).filter(|q| !q.is_empty()) {
            Some(q) => {
                let q = q.replace(['"', '`', '$', ';'], "");
                let out = powershell(
                    &format!("winget search \"{q}\" --accept-source-agreements --disable-interactivity | Select-Object -First 25"),
                    Duration::from_secs(90),
                )
                .await;
                ok(format!("Searched apps for “{q}”"), out)
            }
            None => err("Searching apps failed", "`query` is required"),
        },
        "windows_updates" => {
            if args["install"].as_bool().unwrap_or(false) {
                if !helper_available().await {
                    return err(
                        "Windows updates need setup",
                        "installing Windows updates needs admin rights, which come from the IRIS Helper; it isn't \
                         set up on this computer yet. Tell the user it can be enabled during setup.",
                    );
                }
                return helper(json!({ "action": "windows_updates_install" }), "Installed Windows updates").await;
            }
            let out = powershell(
                "$s = (New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher(); \
                 $r = $s.Search(\"IsInstalled=0 and IsHidden=0 and Type='Software'\"); \
                 if ($r.Updates.Count -eq 0) { 'No Windows updates waiting.' } else { \
                 \"$($r.Updates.Count) Windows update(s) waiting:\"; $r.Updates | ForEach-Object { '- ' + $_.Title } }",
                Duration::from_secs(300),
            )
            .await;
            ok("Checked Windows Update", out)
        }
        "power" => {
            let delay = args["delay_minutes"].as_u64().unwrap_or(1).min(24 * 60) * 60;
            let (cmd, summary) = match args["action"].as_str().unwrap_or("") {
                "restart" => (format!("shutdown /r /t {delay} /c \"I.R.I.S.: restart requested\""), "Scheduled a restart"),
                "shutdown" => (format!("shutdown /s /t {delay} /c \"I.R.I.S.: shutdown requested\""), "Scheduled a shutdown"),
                "cancel" => ("shutdown /a".to_owned(), "Cancelled the pending restart/shutdown"),
                _ => return err("Power action failed", "`action` must be restart, shutdown or cancel"),
            };
            let out = powershell(&cmd, Duration::from_secs(30)).await;
            ok(summary, if out.trim().is_empty() || out.contains("Exit code: 0") { format!("{summary}.") } else { out })
        }
        other => err(format!("Unknown tool {other}"), "no such tool"),
    }
}

/// Package ids like "Microsoft.Edge" or "7zip.7zip"; nothing that could be
/// read as a second command.
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}

fn ok(summary: impl Into<String>, output: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: true, output: output.into(), summary: summary.into() }
}

fn err(summary: impl Into<String>, e: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: false, output: format!("Error: {}", e.into()), summary: summary.into() }
}

async fn status() -> ToolOutcome {
    let out = powershell(
        r#"$os = Get-CimInstance Win32_OperatingSystem; $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
"$($os.Caption) $($os.Version), up $([int]((Get-Date) - $os.LastBootUpTime).TotalHours) h"
"CPU: $($cpu.Name.Trim()), load $($cpu.LoadPercentage)%"
"RAM: $([math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory)/1MB,1)) of $([math]::Round($os.TotalVisibleMemorySize/1MB,1)) GB used"
Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" | ForEach-Object { "Disk $($_.DeviceID) $([math]::Round($_.FreeSpace/1GB,1)) GB free of $([math]::Round($_.Size/1GB,1)) GB" }
$reboot = (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') -or (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending')
"Restart pending: $reboot"
"Top memory: " + ((Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 5 | ForEach-Object { "$($_.Name) $([int]($_.WorkingSet64/1MB)) MB" }) -join ', ')"#,
        Duration::from_secs(60),
    )
    .await;
    ok("Checked this computer", out)
}

/// Runs PowerShell and returns its output (with the exit code if it failed).
async fn powershell(command: &str, timeout: Duration) -> String {
    let dir = std::env::temp_dir();
    let r = crate::tools::run_powershell(&dir, command, timeout).await;
    truncate(&r.output, MAX_OUTPUT)
}

async fn helper_available() -> bool {
    tokio::process::Command::new("schtasks")
        .args(["/query", "/tn", HELPER_TASK])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

/// With the helper: elevated. Without it: as the user (per-user apps work;
/// machine-wide ones may fail for lack of admin rights).
async fn elevated_or_user(request: Value, summary: &str, user_command: impl FnOnce() -> String) -> ToolOutcome {
    if helper_available().await {
        return helper(request, summary).await;
    }
    let out = powershell(&user_command(), Duration::from_secs(30 * 60)).await;
    ok(
        summary,
        format!(
            "{out}\n\n(Ran without admin rights: apps installed for all users may need the IRIS Helper, which isn't set up.)"
        ),
    )
}

/// Hands a request to the elevated helper and waits for its answer.
async fn helper(request: Value, summary: &str) -> ToolOutcome {
    let id = iris_core::new_id();
    let queue = PathBuf::from(HELPER_QUEUE);
    let req = queue.join(format!("{id}.req.json"));
    let res = queue.join(format!("{id}.res.json"));
    if let Err(e) = std::fs::write(&req, request.to_string()) {
        return err(summary, format!("couldn't reach the helper: {e}"));
    }
    let started = tokio::process::Command::new("schtasks").args(["/run", "/tn", HELPER_TASK]).output().await;
    if !started.is_ok_and(|o| o.status.success()) {
        let _ = std::fs::remove_file(&req);
        return err(summary, "couldn't start the IRIS Helper");
    }
    let deadline = std::time::Instant::now() + HELPER_TIMEOUT;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Ok(text) = std::fs::read_to_string(&res) {
            let _ = std::fs::remove_file(&res);
            let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);
            let output = truncate(v["output"].as_str().unwrap_or(&text), MAX_OUTPUT);
            return if v["ok"].as_bool().unwrap_or(false) { ok(summary, output) } else { err(summary, output) };
        }
    }
    err(summary, "the helper didn't answer in time")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_approvals() {
        assert!(valid_id("Microsoft.Edge"));
        assert!(valid_id("7zip.7zip"));
        assert!(!valid_id("x; Remove-Item C:\\"));
        assert!(!valid_id("-all"));

        let call = |name: &str, args: &str| ToolCall { id: "1".into(), name: name.into(), arguments: args.into() };
        assert!(!needs_approval(&call("system_status", "{}"), AutoMode::Off));
        assert!(!needs_approval(&call("windows_updates", "{}"), AutoMode::Off));
        assert!(needs_approval(&call("windows_updates", r#"{"install":true}"#), AutoMode::Careful));
        assert!(!needs_approval(&call("update_apps", r#"{"all":true}"#), AutoMode::Full));
        assert!(needs_approval(&call("power", r#"{"action":"restart"}"#), AutoMode::Full), "power always asks");
        assert!(needs_approval(&call("uninstall_app", r#"{"id":"x"}"#), AutoMode::Full), "uninstalling always asks");
    }
}
