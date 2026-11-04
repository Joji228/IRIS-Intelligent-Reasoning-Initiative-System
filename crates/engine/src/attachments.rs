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

fn dir(data_dir: &Path, thread_id: &str) -> PathBuf {
    data_dir.join("attachments").join(thread_id)
}

pub fn path_of(data_dir: &Path, thread_id: &str, a: &Attachment) -> PathBuf {
    dir(data_dir, thread_id).join(format!("{}-{}", a.id, safe_name(&a.name)))
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

/// The text of a text, web page or PDF file (`full`: the long version).
pub fn text_of(path: &Path, kind: &str, full: bool) -> Option<String> {
    let limit = if full { FULL_TEXT } else { SHORT_TEXT };
    let bytes = std::fs::read(path).ok()?;
    let text = match kind {
        "pdf" => pdf_extract::extract_text_from_mem(&bytes).ok()?,
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
