//! Files the user attaches in the chat box, stored under
//! `attachments/<thread>/<id>-<name>`. Images go to the model as images
//! (DeepSeek V4.1 Flash and most current models can see); text files, web
//! pages and PDFs go as their text.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use iris_core::{Attachment, new_id};

/// The largest file accepted.
pub const MAX_BYTES: usize = 20 * 1024 * 1024;
/// Text sent from a file in the latest messages, and in older ones.
const FULL_TEXT: usize = 30_000;
const SHORT_TEXT: usize = 2_000;

const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];
const TEXT_EXT: &[&str] = &[
    "txt", "md", "markdown", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "log", "html", "htm", "css",
    "js", "ts", "py", "rs", "ps1", "sh", "bat", "sql", "c", "cpp", "h", "java", "cs", "go", "php", "rb", "svelte",
];

fn ext(name: &str) -> String {
    Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

pub fn kind_of(name: &str) -> &'static str {
    let e = ext(name);
    if IMAGE_EXT.contains(&e.as_str()) {
        "image"
    } else if e == "pdf" {
        "pdf"
    } else if TEXT_EXT.contains(&e.as_str()) {
        "text"
    } else {
        "file"
    }
}

pub fn mime_of(name: &str) -> &'static str {
    match ext(name).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "csv" => "text/csv",
        e if TEXT_EXT.contains(&e) => "text/plain",
        _ => "application/octet-stream",
    }
}

/// A file name safe to put on disk.
fn safe_name(name: &str) -> String {
    let base = Path::new(name).file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let clean: String =
        base.chars().map(|c| if c.is_alphanumeric() || "._- ".contains(c) { c } else { '_' }).take(100).collect();
    if clean.trim().is_empty() { "file".into() } else { clean }
}

/// Ids (thread and attachment) are opaque strings we made ourselves (UUIDs).
/// Anything a client sends back has to look like that, so it can never be a
/// path (`..`, slashes, drive letters).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// An id that is safe to put in a path whatever it holds (anything odd becomes `_`).
fn clean_id(id: &str) -> String {
    let s: String = id.chars().take(64).map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if s.is_empty() { "_".into() } else { s }
}

fn dir(data_dir: &Path, thread_id: &str) -> PathBuf {
    data_dir.join("attachments").join(clean_id(thread_id))
}

pub fn path_of(data_dir: &Path, thread_id: &str, a: &Attachment) -> PathBuf {
    dir(data_dir, thread_id).join(format!("{}-{}", clean_id(&a.id), safe_name(&a.name)))
}

/// The folder holding a thread's files (for deleting them with the thread).
pub fn thread_dir(data_dir: &Path, thread_id: &str) -> Option<PathBuf> {
    valid_id(thread_id).then(|| dir(data_dir, thread_id))
}

/// Deletes the files of chats that no longer exist (or were just emptied).
pub fn remove_threads(data_dir: &Path, thread_ids: &[String]) {
    for id in thread_ids {
        if let Some(dir) = thread_dir(data_dir, id) {
            if dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&dir) {
                    tracing::warn!("couldn't delete the files of chat {id}: {e}");
                }
            }
        }
    }
}

/// Deletes attachment folders whose chat is gone (left behind by older
/// versions, or by a delete that was interrupted). `exists` says whether a
/// chat id is still in the database. Returns how many folders were removed.
pub fn sweep_orphans(data_dir: &Path, exists: impl Fn(&str) -> bool) -> usize {
    let Ok(entries) = std::fs::read_dir(data_dir.join("attachments")) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.path().is_dir() && !exists(&name) && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Stores an uploaded file for a thread.
pub fn save(data_dir: &Path, thread_id: &str, name: &str, bytes: &[u8]) -> std::io::Result<Attachment> {
    let a = Attachment {
        id: new_id(),
        name: safe_name(name),
        mime: mime_of(name).into(),
        size: bytes.len() as i64,
        kind: kind_of(name).into(),
    };
    let path = path_of(data_dir, thread_id, &a);
    std::fs::create_dir_all(path.parent().expect("has parent"))?;
    std::fs::write(path, bytes)?;
    Ok(a)
}

/// An attachment as the client sent it back, checked against what's on disk.
pub fn verify(data_dir: &Path, thread_id: &str, a: &Attachment) -> Option<Attachment> {
    if !valid_id(thread_id) || !valid_id(&a.id) {
        return None;
    }
    let meta = std::fs::metadata(path_of(data_dir, thread_id, a)).ok()?;
    Some(Attachment {
        id: a.id.clone(),
        name: safe_name(&a.name),
        mime: mime_of(&a.name).into(),
        size: meta.len() as i64,
        kind: kind_of(&a.name).into(),
    })
}

/// An image file as a data URL for the model.
pub fn image_data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > MAX_BYTES {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    Some(format!("data:{};base64,{}", mime_of(name), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

// ---- PDFs ---------------------------------------------------------------------------
//
// The PDF parser (lopdf through pdf-extract) aborts the whole process on some
// hostile files (very deeply nested objects overflow the stack, which cannot
// be caught), and no fixed release exists yet. So PDFs are never parsed in the
// engine itself: a copy of the engine runs as a short-lived worker
// (`iris-engine --extract-pdf <file>`) under a time limit, and if it dies the
// engine only loses that one file's text.

/// How long the worker gets for one PDF.
const PDF_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
/// The most text read back from the worker.
const PDF_MAX_TEXT: u64 = 8 * 1024 * 1024;
/// Nesting like `[[[[…` this deep in raw bytes is never a real document.
const PDF_MAX_RUN: usize = 200;

/// Whether the raw bytes contain a long run of opening brackets (`[` or `<`),
/// the shape of the known stack-overflow files. Cheap, and only a first filter:
/// the worker process is the real protection.
fn looks_hostile(bytes: &[u8]) -> bool {
    let (mut arrays, mut dicts) = (0usize, 0usize);
    for &b in bytes {
        match b {
            b'[' => arrays += 1,
            b'<' => dicts += 1,
            b' ' | b'\t' | b'\r' | b'\n' | 0 => continue,
            _ => (arrays, dicts) = (0, 0),
        }
        if arrays > PDF_MAX_RUN || dicts > PDF_MAX_RUN * 2 {
            return true;
        }
    }
    false
}

/// The worker side: `iris-engine --extract-pdf <file>` prints the file's text.
/// (Called from `main` before anything else starts.)
pub fn pdf_worker(path: &str) -> i32 {
    use std::io::Write;
    let Ok(bytes) = std::fs::read(path) else { return 2 };
    match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) => {
            let mut out = std::io::stdout().lock();
            if out.write_all(text.as_bytes()).and_then(|()| out.flush()).is_err() {
                return 3;
            }
            0
        }
        Err(_) => 4,
    }
}

/// Runs `exe --extract-pdf <file>` with a time limit and returns its text, or
/// `None` if it failed, crashed or took too long.
fn pdf_text_in_worker(exe: &Path, file: &Path, limit: std::time::Duration) -> Option<String> {
    use std::io::Read;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--extract-pdf")
        .arg(file)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // no console window
    }
    let mut child = cmd.spawn().ok()?;
    let out = child.stdout.take()?;
    // Read while it runs, so a big result can't fill the pipe and stall it.
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = out.take(PDF_MAX_TEXT).read_to_end(&mut text);
        text
    });
    let deadline = std::time::Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let text = reader.join().ok()?;
    if !status?.success() {
        return None;
    }
    String::from_utf8(text).ok()
}

/// The text of a PDF, read safely (see above). A file that failed once is
/// marked and not tried again.
fn pdf_text(path: &Path, bytes: &[u8]) -> Option<String> {
    // (The marker's name must not start with the attachment's id, or the file
    // endpoint, which finds files by that prefix, could return it instead.)
    let marker = path.with_file_name(format!(".unreadable-{}", path.file_name()?.to_string_lossy()));
    if marker.exists() || looks_hostile(bytes) {
        return None;
    }
    let text = std::env::current_exe().ok().and_then(|exe| pdf_text_in_worker(&exe, path, PDF_TIMEOUT));
    if text.is_none() {
        let _ = std::fs::write(&marker, "this PDF could not be read safely");
    }
    text
}

/// The text of a text, web page or PDF file (`full`: the long version).
pub fn text_of(path: &Path, kind: &str, full: bool) -> Option<String> {
    let limit = if full { FULL_TEXT } else { SHORT_TEXT };
    let bytes = std::fs::read(path).ok()?;
    let text = match kind {
        "pdf" => pdf_text(path, &bytes)?,
        _ => {
            let raw = String::from_utf8_lossy(&bytes).into_owned();
            let name = path.to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".html") || name.ends_with(".htm") {
                html2text::from_read(raw.as_bytes(), 120).unwrap_or(raw)
            } else {
                raw
            }
        }
    };
    Some(crate::tools::truncate(text.trim(), limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_pdf_shapes_are_stopped_before_parsing() {
        // The known crash is a PDF whose objects nest thousands of levels deep.
        let mut deep_arrays = b"%PDF-1.4\n1 0 obj\n".to_vec();
        deep_arrays.extend(std::iter::repeat_n(b'[', 10_000));
        let mut spaced = Vec::new();
        for _ in 0..1_000 {
            spaced.extend_from_slice(b"[ \n");
        }
        assert!(looks_hostile(&deep_arrays));
        assert!(looks_hostile(&spaced), "whitespace between the brackets doesn't hide them");
        // (Nesting broken up by names isn't caught here: the worker process is the real protection.)
        let ordinary = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Kids [3 0 R 4 0 R] >>\nendobj\n(text [with] brackets)";
        assert!(!looks_hostile(ordinary));
    }

    #[test]
    fn a_pdf_worker_that_fails_or_cant_start_gives_no_text() {
        let dir = std::env::temp_dir().join(format!("iris-pdf-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.pdf");
        std::fs::write(&file, b"%PDF-1.4 not really").unwrap();
        let limit = std::time::Duration::from_secs(5);
        assert_eq!(pdf_text_in_worker(&dir.join("no-such-program.exe"), &file, limit), None);
        // A program that doesn't understand the worker flag exits with an error: no text either.
        let any = if cfg!(windows) { Path::new("C:\\Windows\\System32\\whoami.exe") } else { Path::new("/bin/ls") };
        if any.exists() {
            assert_eq!(pdf_text_in_worker(any, &file, limit), None);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kinds_and_storage() {
        assert_eq!(kind_of("Photo.JPG"), "image");
        assert_eq!(kind_of("notes.md"), "text");
        assert_eq!(kind_of("report.pdf"), "pdf");
        assert_eq!(kind_of("setup.exe"), "file");
        assert_eq!(safe_name("..\\..\\evil:name?.txt"), "evil_name_.txt");

        let dir = std::env::temp_dir().join(format!("iris-att-{}", uuid::Uuid::new_v4().simple()));
        let a = save(&dir, "t1", "hello.txt", b"hello world").unwrap();
        assert_eq!(text_of(&path_of(&dir, "t1", &a), "text", true).as_deref(), Some("hello world"));
        assert!(verify(&dir, "t1", &a).is_some());
        assert!(verify(&dir, "other", &a).is_none());

        // A client can't point an attachment outside its folder: a dummy
        // secret one level up must stay unreachable through ids.
        std::fs::write(dir.join("local-token.txt"), "DUMMY_SECRET").unwrap();
        let evil = Attachment { id: "../../local".into(), name: "token.txt".into(), mime: String::new(), size: 0, kind: "text".into() };
        assert!(verify(&dir, "t1", &evil).is_none());
        assert_eq!(path_of(&dir, "t1", &evil).parent(), Some(dir.join("attachments").join("t1").as_path()));
        assert_eq!(text_of(&path_of(&dir, "t1", &evil), "text", true), None, "an odd id can't reach a file outside the folder");
        assert!(verify(&dir, "../..", &a).is_none());
        assert!(!valid_id("..") && !valid_id("a/b") && !valid_id("C:") && !valid_id(""));
        assert!(valid_id(&new_id()));
        assert_eq!(thread_dir(&dir, ".."), None);

        // Files go with their chat; folders of chats that no longer exist are swept.
        let b = save(&dir, "t2", "b.txt", b"b").unwrap();
        save(&dir, "gone", "c.txt", b"c").unwrap();
        remove_threads(&dir, &["t1".to_owned(), "..".to_owned()]);
        assert!(verify(&dir, "t1", &a).is_none(), "deleting the chat deletes its files");
        assert!(dir.join("local-token.txt").exists(), "a '..' id never reaches outside the attachments folder");
        assert_eq!(sweep_orphans(&dir, |id| id == "t2"), 1);
        assert!(verify(&dir, "t2", &b).is_some() && !dir.join("attachments").join("gone").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
