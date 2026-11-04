//! Voice input: the app records a clip and sends it here; it's turned into
//! text by a provider that offers Whisper-style speech to text. DeepSeek
//! doesn't, so this uses the first configured provider that does (Groq or
//! OpenAI). Spoken replies need nothing here: the app reads them aloud with
//! the system's own voices.

use serde::Serialize;

use crate::api::AppState;

/// The largest clip accepted (the providers' own limit is 25 MB).
pub const MAX_CLIP_BYTES: usize = 24 * 1024 * 1024;

/// Providers known to transcribe, by a piece of their address, and the model
/// to use (fast and cheap first).
const TRANSCRIBERS: &[(&str, &str)] = &[("groq.com", "whisper-large-v3-turbo"), ("api.openai.com", "gpt-4o-mini-transcribe")];

#[derive(Debug, Serialize, PartialEq)]
pub struct VoiceStatus {
    /// Whether voice input works (a provider that transcribes has a key).
    pub available: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// The provider id and model used for speech to text, if any.
pub fn transcriber(st: &AppState) -> Option<(String, String, String)> {
    let providers = st.providers.read().unwrap();
    TRANSCRIBERS.iter().find_map(|(host, model)| {
        providers
            .all()
            .iter()
            .find(|p| p.base_url.contains(host) && p.key_source != crate::providers::KeySource::None)
            .map(|p| (p.id.clone(), p.name.clone(), (*model).to_owned()))
    })
}

pub fn status(st: &AppState) -> VoiceStatus {
    match transcriber(st) {
        Some((_, name, model)) => VoiceStatus { available: true, provider: Some(name), model: Some(model) },
        None => VoiceStatus { available: false, provider: None, model: None },
    }
}

/// Turns a recording into text.
pub async fn transcribe(st: &AppState, audio: Vec<u8>, mime: &str, language: Option<&str>) -> Result<String, String> {
    let (id, _, model) = transcriber(st)
        .ok_or("voice input needs a Groq or OpenAI provider with a key (Settings → Providers)")?;
    let client = st.providers.read().unwrap().get(&id).map(|p| p.client.clone()).ok_or("the voice provider is gone")?;
    let ext = match mime.split(';').next().unwrap_or_default().trim() {
        "audio/ogg" => "ogg",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        _ => "webm",
    };
    let mime = mime.split(';').next().unwrap_or("audio/webm").trim().to_owned();
    client
        .transcribe(&model, audio, &format!("voice.{ext}"), &mime, language)
        .await
        .map_err(|e| format!("couldn't turn the recording into text: {e}"))
}
