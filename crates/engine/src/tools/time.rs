//! `get_time`.

use super::ToolOutcome;

/// e.g. "Sunday, 01 November 2026, 07:30 (UTC+02:00)".
pub fn now_line() -> String {
    let now = chrono::Local::now();
    format!("{} (UTC{})", now.format("%A, %d %B %Y, %H:%M"), now.format("%:z"))
}

pub fn now() -> ToolOutcome {
    ToolOutcome::ok("Checked the date and time", now_line())
}
