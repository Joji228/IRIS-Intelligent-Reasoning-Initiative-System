//! Voice input: the app records a clip and sends it here; it's turned into
//! text by a provider that offers Whisper-style speech to text. DeepSeek
//! doesn't, so this uses the first configured provider that does (Groq or
//! OpenAI). Spoken replies need nothing here: the app reads them aloud with
//! the system's own voices.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use iris_providers::{ProviderError, RateLimits, Segment, Transcription};
use serde::{Deserialize, Serialize};

use crate::api::AppState;

/// The largest clip accepted (the providers' own limit is 25 MB).
pub const MAX_CLIP_BYTES: usize = 24 * 1024 * 1024;

/// Providers known to transcribe, by a piece of their address, and the model
/// to use (fast and cheap first).
/// Groq uses the full large-v3 (not the turbo one): it is clearly better at Greek
/// and has the same free limits.
const TRANSCRIBERS: &[(&str, &str)] = &[("groq.com", "whisper-large-v3"), ("api.openai.com", "gpt-4o-mini-transcribe")];

/// What Whisper tends to "hear" in silence or noise: it invents a polite
/// closing line or subtitle credit. When a transcript is only one of these
/// (ignoring case and punctuation) it is dropped. Keep entries lowercase,
/// without punctuation.
const SILENCE_PHRASES: &[&str] = &[
    // English
    "you",
    "thank you",
    "thanks",
    "thank you very much",
    "thank you so much",
    "thanks for watching",
    "thank you for watching",
    "thank you so much for watching",
    "thanks for listening",
    "thank you for listening",
    "please subscribe",
    "bye",
    "bye bye",
    "subtitles by the amara org community",
    // Greek
    "ευχαριστώ",
    "ευχαριστώ πολύ",
    "ευχαριστούμε",
    "ευχαριστούμε πολύ",
    "ευχαριστώ που παρακολουθήσατε",
    "ευχαριστούμε που παρακολουθήσατε",
    "ευχαριστώ για την προσοχή σας",
    "τα λέμε στο επόμενο βίντεο",
    "υπότιτλοι authorwave",
    "υπότιτλοι amara org",
    "υπότιτλοι από την κοινότητα του amara org",
];

/// Lowercase, without punctuation, one space between words.
fn plain(text: &str) -> String {
    let spaced: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a transcript is only Whisper's silence phrases, once or repeated
/// ("Thank you. Thank you."). A real sentence that merely starts with one is kept.
fn is_silence_phrase(text: &str) -> bool {
    let mut rest = plain(text);
    if rest.is_empty() {
        return false;
    }
    while !rest.is_empty() {
        let found = SILENCE_PHRASES
            .iter()
            .filter(|p| rest.starts_with(**p) && (rest.len() == p.len() || rest[p.len()..].starts_with(' ')))
            .max_by_key(|p| p.len());
        match found {
            Some(p) => rest = rest[p.len()..].trim_start().to_owned(),
            None => return false,
        }
    }
    true
}

/// Whether Whisper named one of the two languages calls understand.
fn is_english_or_greek(language: &str) -> bool {
    let l = language.trim().to_lowercase();
    l == "en" || l == "el" || l.starts_with("english") || l.starts_with("greek") || l.starts_with("modern greek")
}

/// Whether Whisper itself doubts a stretch of the transcript: probably no speech there
/// (the classic invented "Thank you." on silence or noise), a stuck repetition, or very
/// unsure words.
fn is_made_up(s: &Segment) -> bool {
    s.no_speech_prob > 0.8
        || (s.no_speech_prob > 0.5 && s.avg_logprob < -0.7)
        || s.compression_ratio > 2.4
        || s.avg_logprob < -1.6
}

/// What was really said: the transcript without the stretches Whisper made up, and how sure
/// it is (the average log probability; closer to 0 is surer). `None` if nothing is left.
fn keep_heard(t: &Transcription) -> Option<(String, f64)> {
    if t.segments.is_empty() {
        // The provider reports no confidence (plain JSON): the text is all there is.
        return Some((t.text.clone(), 0.0)).filter(|(text, _)| !text.is_empty());
    }
    let kept: Vec<&Segment> = t.segments.iter().filter(|s| !is_made_up(s) && !s.text.is_empty()).collect();
    if kept.is_empty() {
        return None;
    }
    let text = kept.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    let confidence = kept.iter().map(|s| s.avg_logprob).sum::<f64>() / kept.len() as f64;
    Some((text, confidence))
}

/// The surest of several tries (each `Some((text, confidence))`).
fn surest(tries: Vec<Option<(String, f64)>>) -> Option<String> {
    tries.into_iter().flatten().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(text, _)| text)
}

/// A short error the phone can read aloud, for a failed call to the provider.
fn friendly_error(e: &ProviderError, provider: &str) -> String {
    match e {
        ProviderError::Api { status: 429, .. } => {
            format!("{provider}'s free limit is used up for now; try again in a minute")
        }
        ProviderError::Api { status: 401 | 403, .. } => format!("{provider} refused the key; check it on the server"),
        ProviderError::Api { status: 413, .. } => "that recording is too long".to_owned(),
        ProviderError::Http(_) => "couldn't reach the speech service".to_owned(),
        _ => "couldn't turn the recording into text".to_owned(),
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct VoiceStatus {
    /// Whether voice input works (a provider that transcribes has a key).
    pub available: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// How much of Groq's free daily allowance of spoken turns is left, as of the last turn.
    /// `None` until the first spoken turn has been sent (the provider only tells with its answers).
    pub limits: Option<Allowance>,
}

/// The free plan's request allowance (spoken turns per day), as Groq last reported it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Allowance {
    pub limit: u64,
    pub remaining: u64,
    /// When it is full again (Unix seconds).
    pub resets_at: i64,
}

impl Allowance {
    fn from_headers(l: RateLimits, now: i64) -> Self {
        Self { limit: l.limit, remaining: l.remaining.min(l.limit), resets_at: now + l.reset_secs.ceil() as i64 }
    }

    /// Once the reset time has passed the allowance is full again, whatever it was.
    fn as_of(mut self, now: i64) -> Self {
        if now >= self.resets_at {
            self.remaining = self.limit;
        }
        self
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Last allowance seen, in memory and in `voice-limits.json` (so it survives a restart).
static ALLOWANCE: Mutex<Option<Allowance>> = Mutex::new(None);

fn allowance_file(dir: &Path) -> PathBuf {
    dir.join("voice-limits.json")
}

fn remember_allowance(dir: &Path, l: RateLimits) {
    let a = Allowance::from_headers(l, now_secs());
    *ALLOWANCE.lock().unwrap() = Some(a);
    if let Ok(text) = serde_json::to_string(&a) {
        let _ = std::fs::write(allowance_file(dir), text);
    }
}

fn current_allowance(dir: &Path) -> Option<Allowance> {
    let mut last = ALLOWANCE.lock().unwrap();
    if last.is_none() {
        *last = std::fs::read_to_string(allowance_file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok());
    }
    last.map(|a| a.as_of(now_secs()))
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
        Some((_, name, model)) => {
            // Only Groq's Whisper reports a daily allowance in a way that means "spoken turns per day".
            let limits = model.starts_with("whisper").then(|| current_allowance(&st.data_dir)).flatten();
            VoiceStatus { available: true, provider: Some(name), model: Some(model), limits }
        }
        None => VoiceStatus { available: false, provider: None, model: None, limits: None },
    }
}

/// Why a recording couldn't be turned into text: the HTTP status to answer
/// with (429 when the free limit is used up, so the phone can tell it apart)
/// and a short message that is fine to read aloud.
#[derive(Debug, PartialEq)]
pub struct VoiceError {
    pub status: u16,
    pub message: String,
}

impl VoiceError {
    fn new(status: u16, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

/// The HTTP status to answer with for a failed call to the provider.
fn error_status(e: &ProviderError) -> u16 {
    match e {
        ProviderError::Api { status: 429, .. } => 429,
        ProviderError::Api { status: 413, .. } => 413,
        _ => 502,
    }
}

/// Turns a recording into text.
pub async fn transcribe(st: &AppState, audio: Vec<u8>, mime: &str, language: Option<&str>) -> Result<String, VoiceError> {
    let (id, provider, model) = transcriber(st).ok_or_else(|| {
        VoiceError::new(400, "voice input needs a Groq or OpenAI provider with a key (Settings → AI providers)")
    })?;
    let client = st
        .providers
        .read()
        .unwrap()
        .get(&id)
        .map(|p| p.client.clone())
        .ok_or_else(|| VoiceError::new(400, "the voice provider is gone"))?;
    let ext = match mime.split(';').next().unwrap_or_default().trim() {
        "audio/ogg" => "ogg",
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        _ => "webm",
    };
    let mime = mime.split(';').next().unwrap_or("audio/webm").trim().to_owned();
    // The agents' names, so a spoken "Iris" isn't written as "Iris" in five spellings.
    let names = st
        .store
        .list_agents()
        .map(|agents| agents.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let hint = (!names.is_empty()).then(|| names.chars().take(200).collect::<String>());
    let file = format!("voice.{ext}");
    let (client_r, model_r, audio_r, file_r, mime_r, hint_r) = (&client, &model, &audio, &file, &mime, &hint);
    let dir = st.data_dir.clone();
    let attempt = |lang: Option<String>| {
        let dir = dir.clone();
        async move {
            let heard = client_r.transcribe_detailed(model_r, audio_r.clone(), file_r, mime_r, lang.as_deref(), hint_r.as_deref()).await;
            // Keep what the provider says is left of the free plan (every try counts, retries included).
            if let Ok(t) = &heard {
                if let (Some(l), true) = (t.limits, model_r.starts_with("whisper")) {
                    remember_allowance(&dir, l);
                }
            }
            heard
        }
    };

    // A fixed language is used as given; otherwise Whisper detects it.
    let first = attempt(language.map(str::to_owned)).await.map_err(|e| {
        tracing::warn!("speech to text failed: {e}");
        VoiceError::new(error_status(&e), friendly_error(&e, &provider))
    })?;
    let mut heard = keep_heard(&first).map(|(text, _)| text);
    // Calls understand English and Greek only. If Whisper detected another language, that is
    // almost always noise or an accent it misjudged: ask again for each of the two and keep the surer.
    if language.is_none() && first.language.as_deref().is_some_and(|l| !is_english_or_greek(l)) {
        tracing::info!("speech detected as {:?}; trying English and Greek", first.language);
        let english = attempt(Some("en".into())).await.ok().and_then(|t| keep_heard(&t));
        let greek = attempt(Some("el".into())).await.ok().and_then(|t| keep_heard(&t));
        heard = surest(vec![english, greek]);
    }
    let text = heard.unwrap_or_default();
    // Silence or noise: say nothing rather than let a made-up phrase (or the agents' own names, which
    // were only a spelling hint) become a message.
    let hint_only = hint.as_deref().is_some_and(|h| !plain(h).is_empty() && plain(&text) == plain(h));
    Ok(if text.is_empty() || is_silence_phrase(&text) || hint_only { String::new() } else { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_phrases_are_dropped() {
        for s in [
            "Thank you.", "  thanks for watching!  ", "Thank you for watching.", "You", "Ευχαριστώ.", "Υπότιτλοι AUTHORWAVE", "Thank you!!",
            "Thanks.", "Thank you very much.", "Bye.", "Ευχαριστώ πολύ.", "Ευχαριστούμε που παρακολουθήσατε",
        ] {
            assert!(is_silence_phrase(s), "{s}");
        }
    }

    #[test]
    fn repeated_silence_phrases_are_dropped_too() {
        assert!(is_silence_phrase("Thank you. Thank you. Thank you."));
        assert!(is_silence_phrase("Thank you. Bye."));
        assert!(is_silence_phrase("Ευχαριστώ. Ευχαριστώ."));
    }

    #[test]
    fn real_sentences_are_kept() {
        for s in [
            "Thank you for the summary, now check my mail", "Ευχαριστώ, τι ώρα είναι;", "you know what", "", "Call Iris",
            "Thanks, and one more thing", "Bye the way, what's the weather",
        ] {
            assert!(!is_silence_phrase(s), "{s}");
        }
    }

    fn seg(text: &str, no_speech: f64, logprob: f64, compression: f64) -> Segment {
        Segment { text: text.into(), no_speech_prob: no_speech, avg_logprob: logprob, compression_ratio: compression }
    }

    fn heard(segments: Vec<Segment>) -> Transcription {
        let text = segments.iter().map(|s| s.text.clone()).collect::<Vec<_>>().join(" ");
        Transcription { text, segments, ..Default::default() }
    }

    #[test]
    fn a_sentence_whisper_is_sure_about_is_kept() {
        let t = heard(vec![seg("What's the weather like?", 0.02, -0.2, 1.1)]);
        assert_eq!(keep_heard(&t).map(|(text, _)| text).as_deref(), Some("What's the weather like?"));
    }

    #[test]
    fn a_thank_you_on_silence_is_dropped_by_whispers_own_doubt() {
        // It "heard" speech in near silence: very likely no speech at all.
        assert_eq!(keep_heard(&heard(vec![seg("Thank you.", 0.92, -0.4, 1.0)])), None);
        // Probably no speech and unsure words.
        assert_eq!(keep_heard(&heard(vec![seg("Thank you.", 0.6, -0.9, 1.0)])), None);
    }

    #[test]
    fn a_stuck_loop_or_wild_guess_is_dropped() {
        assert_eq!(keep_heard(&heard(vec![seg("la la la la la la", 0.1, -0.3, 3.0)])), None);
        assert_eq!(keep_heard(&heard(vec![seg("kjsdh sdf", 0.1, -1.9, 1.0)])), None);
    }

    #[test]
    fn only_the_made_up_part_of_a_transcript_is_dropped() {
        let t = heard(vec![seg("Open my mail.", 0.03, -0.25, 1.0), seg("Thank you.", 0.9, -0.5, 1.0)]);
        assert_eq!(keep_heard(&t).map(|(text, _)| text).as_deref(), Some("Open my mail."));
    }

    #[test]
    fn a_provider_without_confidence_numbers_is_not_filtered() {
        let t = Transcription { text: "hello".into(), ..Default::default() };
        assert_eq!(keep_heard(&t).map(|(text, _)| text).as_deref(), Some("hello"));
        assert_eq!(keep_heard(&Transcription::default()), None);
    }

    #[test]
    fn the_allowance_comes_from_the_headers() {
        let l = RateLimits { limit: 2000, remaining: 1843, reset_secs: 3600.4 };
        let a = Allowance::from_headers(l, 1_000);
        assert_eq!((a.limit, a.remaining, a.resets_at), (2000, 1843, 1_000 + 3601));
    }

    #[test]
    fn the_allowance_is_full_again_after_its_reset_time() {
        let a = Allowance { limit: 2000, remaining: 12, resets_at: 5_000 };
        assert_eq!(a.as_of(4_999).remaining, 12);
        assert_eq!(a.as_of(5_000).remaining, 2000);
        assert_eq!(a.as_of(9_999).remaining, 2000);
    }

    #[test]
    fn the_allowance_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("iris-voice-limits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        remember_allowance(&dir, RateLimits { limit: 2000, remaining: 5, reset_secs: 600.0 });
        // A fresh start reads it back from the file.
        *ALLOWANCE.lock().unwrap() = None;
        let a = current_allowance(&dir).unwrap();
        assert_eq!((a.limit, a.remaining), (2000, 5));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_english_and_greek_count() {
        for l in ["english", "English", "en", "greek", "el", "modern greek (1453-)"] {
            assert!(is_english_or_greek(l), "{l}");
        }
        for l in ["welsh", "russian", "cy", "german", "dutch", ""] {
            assert!(!is_english_or_greek(l), "{l}");
        }
    }

    #[test]
    fn the_surer_of_english_and_greek_wins() {
        let en = Some(("Open my mail".to_string(), -0.2));
        let el = Some(("Άνοιξε το mail".to_string(), -0.6));
        assert_eq!(surest(vec![en.clone(), el.clone()]).as_deref(), Some("Open my mail"));
        assert_eq!(surest(vec![None, el]).as_deref(), Some("Άνοιξε το mail"));
        assert_eq!(surest(vec![None, None]), None);
    }

    #[test]
    fn the_free_limit_error_is_short_and_speakable() {
        let e = ProviderError::Api { status: 429, message: "rate limit reached for model whisper-large-v3".into() };
        assert_eq!(friendly_error(&e, "Groq"), "Groq's free limit is used up for now; try again in a minute");
        let e = ProviderError::Api { status: 500, message: "boom".into() };
        assert_eq!(friendly_error(&e, "Groq"), "couldn't turn the recording into text");
    }

    #[test]
    fn a_used_up_free_limit_answers_429() {
        let limit = ProviderError::Api { status: 429, message: String::new() };
        assert_eq!(error_status(&limit), 429);
        let other = ProviderError::Api { status: 500, message: String::new() };
        assert_eq!(error_status(&other), 502);
    }

    #[test]
    fn groq_uses_the_full_large_v3_model() {
        assert!(TRANSCRIBERS.contains(&("groq.com", "whisper-large-v3")));
    }
}
