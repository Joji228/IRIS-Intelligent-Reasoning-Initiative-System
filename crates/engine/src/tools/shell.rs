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

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolOutcome::err(summary, format!("could not start the shell: {e}")),
    };
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return ToolOutcome::err(summary, "could not read the command's output");
    };
    // Both pipes are drained as they fill, keeping only the first part: a
    // command that prints without end can't use up the server's memory.
    let work = async {
        let ((out, out_cut), (err, err_cut), status) = tokio::join!(read_capped(stdout), read_capped(stderr), child.wait());
        (out, out_cut, err, err_cut, status)
    };
    let finished = tokio::time::timeout(timeout, work).await;
    match finished {
        Ok((out, out_cut, err, err_cut, Ok(status))) => {
            let code = status.code().unwrap_or(-1);
            let mut text = String::from_utf8_lossy(&out).into_owned();
            let stderr = String::from_utf8_lossy(&err);
            if !stderr.trim().is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&stderr);
            }
            if text.trim().is_empty() {
                text = "(no output)".into();
            }
            let mut shown = truncate(text.trim(), MAX_OUTPUT_CHARS);
            if out_cut || err_cut {
                shown.push_str("\n[the command printed more than the limit; the rest was dropped]");
            }
            let output = format!("Exit code: {code}\n\n{shown}");
            ToolOutcome { ok: status.success(), output, summary: format!("{summary} · exit {code}") }
        }
        Ok((.., Err(e))) => ToolOutcome::err(summary, format!("the command failed to run: {e}")),
        Err(_) => {
            let _ = child.start_kill();
            ToolOutcome::err(summary, format!("timed out after {} seconds and was stopped", timeout.as_secs()))
        }
    }
}

/// The most output kept per stream (bytes); the rest is read and thrown away.
const MAX_CAPTURE: usize = 1024 * 1024;

/// Reads a pipe to its end, keeping at most [`MAX_CAPTURE`] bytes. The flag
/// says whether anything was dropped.
async fn read_capped(mut pipe: impl tokio::io::AsyncRead + Unpin) -> (Vec<u8>, bool) {
    use tokio::io::AsyncReadExt;
    let mut kept = Vec::new();
    let mut cut = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = MAX_CAPTURE - kept.len();
                if n > room {
                    cut = true;
                }
                kept.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
    }
    (kept, cut)
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

        // A flood of output is read in bounded memory and cut, not buffered whole.
        // (3 MB in a few thousand big lines: quick even on a slow computer.)
        let flood = if cfg!(windows) { "1..3000 | ForEach-Object { 'x' * 1000 }" } else { "head -c 3000000 /dev/zero | tr '\\0' 'x'" };
        let big = run(&ws, flood, Duration::from_secs(120)).await;
        assert!(big.ok, "{}", big.output);
        assert!(big.output.contains("more than the limit"), "{}", &big.output[big.output.len().saturating_sub(300)..]);
        assert!(big.output.len() < 20_000, "the result stays small");
        let _ = std::fs::remove_dir_all(&ws);
    }
}
