//! `run_command`: PowerShell in the agent's workspace. The chat loop only gets
//! here after the user approved the exact command.

use std::{fs, path::Path, process::Stdio, time::Duration};

use super::{ToolOutcome, truncate};

const MAX_OUTPUT_CHARS: usize = 10_000;

pub async fn run(workspace: &Path, command: &str, timeout: Duration) -> ToolOutcome {
    let short: String = command.chars().take(80).collect();
    let summary = format!("Ran `{short}`{}", if command.chars().count() > 80 { "…" } else { "" });
    let _ = fs::create_dir_all(workspace);

    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("powershell.exe");
        // UTF-8 output so non-English text survives.
        let script = format!("[Console]::OutputEncoding = [Text.Encoding]::UTF8; {command}");
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script]);
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolOutcome::err(summary, format!("could not start the shell: {e}")),
    };
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(out)) => {
            let code = out.status.code().unwrap_or(-1);
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&stderr);
            }
            if text.trim().is_empty() {
                text = "(no output)".into();
            }
            let output = format!("Exit code: {code}\n\n{}", truncate(text.trim(), MAX_OUTPUT_CHARS));
            ToolOutcome { ok: out.status.success(), output, summary: format!("{summary} · exit {code}") }
        }
        Ok(Err(e)) => ToolOutcome::err(summary, format!("the command failed to run: {e}")),
        // Dropping the future kills the process (kill_on_drop).
        Err(_) => ToolOutcome::err(summary, format!("timed out after {} seconds and was stopped", timeout.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_in_workspace_and_times_out() {
        let ws = std::env::temp_dir().join(format!("iris-sh-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(ws.join("marker.txt"), "x").unwrap();

        let listing = if cfg!(windows) { "Get-ChildItem -Name" } else { "ls" };
        let r = run(&ws, listing, Duration::from_secs(60)).await;
        assert!(r.ok, "{}", r.output);
        assert!(r.output.contains("marker.txt"), "{}", r.output);

        let fail = run(&ws, "exit 3", Duration::from_secs(60)).await;
        assert!(!fail.ok);
        assert!(fail.output.contains("Exit code: 3"), "{}", fail.output);

        let slow = if cfg!(windows) { "Start-Sleep -Seconds 30" } else { "sleep 30" };
        let t = run(&ws, slow, Duration::from_secs(2)).await;
        assert!(!t.ok);
        assert!(t.output.contains("timed out"));
        let _ = std::fs::remove_dir_all(&ws);
    }
}
