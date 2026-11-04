//! `list_files`, `read_file`, `write_file`: confined to the agent's workspace.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use super::{ToolOutcome, truncate};

const MAX_READ_CHARS: usize = 60_000;
const MAX_WRITE_BYTES: usize = 2 * 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 300;

/// Resolves a workspace-relative path, refusing anything that could step
/// outside the workspace (absolute paths, drive letters, `..`).
pub fn resolve(workspace: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches("./");
    let path = Path::new(rel);
    let escapes = path.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_)));
    if path.is_absolute() || escapes {
        return Err(format!("`{rel}` is outside your workspace; use a path relative to it"));
    }
    let full = workspace.join(path);
    // A link or junction inside the workspace (made with a command) could
    // still point outside it: check where the path really goes.
    if !really_inside(workspace, &full) {
        return Err(format!("`{rel}` leads outside your workspace (through a link)"));
    }
    Ok(full)
}

/// Whether `path` (which may not exist yet) resolves to somewhere inside
/// `workspace`, following links on the part that exists.
fn really_inside(workspace: &Path, path: &Path) -> bool {
    let Ok(root) = fs::canonicalize(workspace) else { return true }; // no workspace yet: nothing to follow
    let mut existing = path;
    while fs::symlink_metadata(existing).is_err() {
        let Some(parent) = existing.parent() else { return false };
        existing = parent;
    }
    // A dangling link doesn't canonicalize: refuse it.
    fs::canonicalize(existing).is_ok_and(|real| real.starts_with(&root))
}

pub fn list(workspace: &Path, rel: &str) -> ToolOutcome {
    let label = if rel.trim().is_empty() || rel.trim() == "." { "the workspace".to_owned() } else { rel.to_owned() };
    let summary = format!("Listed {label}");
    let dir = match resolve(workspace, rel) {
        Ok(d) => d,
        Err(e) => return ToolOutcome::err(summary, e),
    };
    let _ = fs::create_dir_all(workspace);
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => return ToolOutcome::err(summary, format!("cannot list `{rel}`: {e}")),
    };
    let mut lines: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            match e.metadata() {
                Ok(m) if m.is_dir() => format!("{name}/"),
                Ok(m) => format!("{name}  ({} bytes)", m.len()),
                Err(_) => name,
            }
        })
        .collect();
    lines.sort();
    let total = lines.len();
    lines.truncate(MAX_LIST_ENTRIES);
    let mut output = if total == 0 { "(empty)".to_owned() } else { lines.join("\n") };
    if total > MAX_LIST_ENTRIES {
        output.push_str(&format!("\n… and {} more", total - MAX_LIST_ENTRIES));
    }
    ToolOutcome::ok(format!("{summary} · {total} items"), output)
}

pub fn read(workspace: &Path, rel: &str) -> ToolOutcome {
    let summary = format!("Read {rel}");
    let path = match resolve(workspace, rel) {
        Ok(p) => p,
        Err(e) => return ToolOutcome::err(summary, e),
    };
    match fs::read(&path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => ToolOutcome::ok(format!("{summary} · {} chars", text.chars().count()), truncate(&text, MAX_READ_CHARS)),
            Err(_) => ToolOutcome::err(summary, format!("`{rel}` is not a text file")),
        },
        Err(e) => ToolOutcome::err(summary, format!("cannot read `{rel}`: {e}")),
    }
}

pub fn write(workspace: &Path, rel: &str, content: &str) -> ToolOutcome {
    let summary = format!("Wrote {rel}");
    if content.len() > MAX_WRITE_BYTES {
        return ToolOutcome::err(summary, format!("content is too large (max {MAX_WRITE_BYTES} bytes)"));
    }
    let path = match resolve(workspace, rel) {
        Ok(p) if p != workspace => p,
        Ok(_) => return ToolOutcome::err(summary, "a file name is required"),
        Err(e) => return ToolOutcome::err(summary, e),
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return ToolOutcome::err(summary, format!("cannot create folder: {e}"));
        }
    }
    match fs::write(&path, content) {
        Ok(()) => ToolOutcome::ok(format!("{summary} · {} bytes", content.len()), format!("Saved `{rel}` ({} bytes).", content.len())),
        Err(e) => ToolOutcome::err(summary, format!("cannot write `{rel}`: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ws() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iris-ws-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn paths_stay_inside() {
        let ws = Path::new("C:/ws");
        assert!(resolve(ws, "notes/a.txt").is_ok());
        assert!(resolve(ws, "./a.txt").is_ok());
        assert!(resolve(ws, "../secret.txt").is_err());
        assert!(resolve(ws, "notes/../../x").is_err());
        assert!(resolve(ws, "/etc/passwd").is_err());
        assert!(resolve(ws, "C:\\Windows\\win.ini").is_err());
    }

    #[test]
    fn write_read_list() {
        let ws = temp_ws();
        assert!(write(&ws, "notes/todo.md", "# Todo\n- one").ok);
        let r = read(&ws, "notes/todo.md");
        assert!(r.ok);
        assert!(r.output.contains("- one"));
        let l = list(&ws, "");
        assert!(l.output.contains("notes/"), "{}", l.output);
        assert!(!read(&ws, "missing.txt").ok);
        assert!(!write(&ws, "../evil.txt", "x").ok);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn links_out_of_the_workspace_are_refused() {
        let ws = temp_ws();
        let outside = temp_ws();
        fs::write(outside.join("secret.txt"), "s3cret").unwrap();
        // A directory junction needs no admin rights on Windows (unlike symlinks).
        #[cfg(windows)]
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(ws.join("door"))
            .arg(&outside)
            .output()
            .is_ok_and(|o| o.status.success());
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&outside, ws.join("door")).is_ok();
        if made {
            assert!(!read(&ws, "door/secret.txt").ok);
            assert!(!write(&ws, "door/new.txt", "x").ok);
            assert!(!list(&ws, "door").ok);
            assert!(!outside.join("new.txt").exists());
        }
        assert!(write(&ws, "fresh/dir/ok.txt", "x").ok, "new folders inside still work");
        let _ = fs::remove_dir(ws.join("door")); // removes the link, not its target
        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&outside);
    }
}
