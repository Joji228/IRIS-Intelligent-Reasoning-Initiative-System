# Voice calls with Iris: implementation plan (cloud route)

> Status: implemented, uncommitted (2026-11-09): phases 1–5 are built and unit-tested; the on-phone "Done when" checks are still to be confirmed. Planned 2026-11-08; the user chose the cloud route. The user can **call an agent from the Android app**, talk in **English or Greek**, and hear the answers. It works like a real phone call: hands-free, with interruptions, and it keeps going with the screen off.
>
> - **Hearing** uses **Groq's free Whisper** (no card; the key lives on the server).
> - **Speaking** uses the **phone's own voices**.
> - Nothing big is downloaded to the phone.
>
> Constraints: free; light; DeepSeek V4.1 Flash stays the only chat model (Groq is used **only** for speech to text); no home-screen widget.

## 1. How it works

```
 Phone                                                   Server (engine)                    Groq (free)
 ┌───────────────────────────────────────┐  one clip per  ┌──────────────────────────┐ clip ┌───────────┐
 │ mic ─► echo cancel ─► WebRTC VAD ─►   │  turn (WAV),   │ POST /v1/voice/transcribe│ ───► │ Whisper   │
 │        (Android)      (end of turn)   │  over I.R.I.S. │  (exists: voice.rs)      │ ◄─── │ large-v3  │
 │                                       │  Link, sealed  │                          │ text └───────────┘
 │ text ─► POST message "[Voice call] …" │ ─────────────► │ Iris + tools + approvals │
 │                                       │                │ DeepSeek Flash           │
 │ speaker ◄─ Android TextToSpeech ◄─    │ ◄───────────── │ streams the reply        │
 │            sentence splitter          │  message_delta │                          │
 └───────────────────────────────────────┘                └──────────────────────────┘
```

- A call is the agent's normal chat. Each spoken turn is:
  1. recorded on the phone;
  2. cut where the user stops talking;
  3. sent as **one small clip** through the encrypted link to the engine's existing `POST /v1/voice/transcribe`, which sends it to Groq;
  4. posted back as a normal message starting with `[Voice call]`.
- The reply streams as usual, and the phone reads it aloud sentence by sentence.
- Tools, approvals, the team and memory work exactly as when typing.
- **The Groq key never leaves the server.** The phone never talks to Groq directly.
- **Relay cost:** one message of about 160 KB (5 s of 16 kHz WAV) per spoken turn. That's nothing next to the relay's limits (600 messages/minute; 32 MB per message).

### Groq free tier (Whisper, Nov 2026)

- **Limits:** 20 requests/minute, 2,000 requests/day, 7,200 audio seconds/hour, 28,800 audio seconds/day.
- **Minimum billing:** each request counts as at least 10 s, so the audio caps give about 720 turns per hour and 2,880 per day. The 2,000 requests/day cap is reached first.
- **Model:** use **`whisper-large-v3`**, not the turbo version, for better Greek. Whisper large-v3 gets about 11% of words wrong on FLEURS Greek.
- **Language:** auto-detected per clip, with an optional fixed language in Settings.
- **Privacy:** voice clips are processed by Groq. Settings → Voice & calls says this plainly.

### What runs on the phone

| What | Size | RAM |
|---|---|---|
| WebRTC VAD (`com.github.gkonovalov.android-vad:webrtc`, MIT, from JitPack, pinned to `2.0.10`) | ~158 KB native | a few MB |
| Android `TextToSpeech` (Google or Samsung engine, Greek and English voices) | managed by Android | system service |
| Recording and WAV encoding | Android APIs, no library | ~1 MB per clip |

## 2. Before any code: what the user does

1. On the server's desktop app, open **Settings → AI providers → Groq** and paste the key there. Never paste it in a chat.
   - Leave the **default model and the fallback** on DeepSeek. Groq must not become a chat model.
2. Try the **mic button in the desktop app's message box** in Greek and English. It already uses Groq, so this checks Greek quality before any phone work starts.

## 3. Phases

Do them in order. Each phase ends with something that builds, passes its tests and can be tried. Follow the house style:
- Plain-language comments.
- The host is always called "the server".
- Nothing personal in the repo: it's public.

See [DEV.md](DEV.md) for the build, test and emulator workflow.

### Phase 1: engine (call mode and transcription tweaks)

**Call mode (`crates/engine/src/chat.rs`):**
- Add `pub const VOICE_PREFIX: &str = "[Voice call]";` next to `RESEARCH_PREFIX`.
- Detect it the same way `research` is detected (the latest user message starts with it), and append `VOICE_INSTRUCTIONS` in `system_prompt()`. The text, roughly:
  > The user is talking to you on a voice call (their message starts with [Voice call]) and your reply is read aloud. Their words were transcribed automatically, so allow for small mistakes. Answer in the language they spoke (Greek or English). Keep what you say short and natural, 1–3 sentences unless they ask for more, with no markdown, lists, tables, emojis or URLs in the spoken part. Before a task that takes more than a few seconds, first say one short sentence (e.g. "On it, give me a minute."). If the answer needs code, a table, links or a long report, say a one-sentence summary, then put the rest after a line that is exactly `[details]`. The app shows that part in the chat and doesn't read it aloud. Approvals can't be given by voice: when you need one, say that the user should check the screen.
- Add tests: the prefix turns the instructions on, and a normal message doesn't.

**Transcription (`crates/engine/src/voice.rs`):**
- Change Groq's model from `whisper-large-v3-turbo` to **`whisper-large-v3`** (better Greek, same free limits).
- **Drop Whisper's known "silence" phrases.** When the whole transcript, trimmed and lowercased, is one of them, return empty text. English examples: "thank you.", "thanks for watching!", "you". Greek examples: "ευχαριστώ.", and subtitle credits such as "υπότιτλοι authorwave". Keep it a short list in a const, with a test.
- **Clear errors.** A 429 from the provider becomes "Groq's free limit is used up for now; try again in a minute". Network errors become "couldn't reach the speech service". Keep these errors short, because the phone speaks them.
- Optional: send Whisper a short `prompt` with the agent names (e.g. "Iris") so names are spelled right. This needs a `prompt: Option<&str>` argument on `transcribe` in `crates/providers/src/lib.rs`, plus its test.

**Both UIs:**
- `app/src/lib/components/MessageItem.svelte` and `android/.../ui/MessageItem.kt`: for user messages starting with `[Voice call]`, strip the prefix and show a small phone chip, like the research chip.
- In assistant messages, render a `[details]` line as a subtle divider.

**Done when:** `cargo test` and the app's type check pass. A message posted with the prefix gets a short, speakable reply. The desktop mic transcribes Greek with `whisper-large-v3`.

### Phase 2: Android, voice API and Settings → Voice & calls

**API (`data/IrisApi.kt`, `data/Models.kt`):**
- `suspend fun voice(): VoiceStatus` → `GET /v1/voice` (`available`, `provider`, `model`).
- `suspend fun transcribe(data: String, mime: String, language: String = ""): Transcript` → `POST /v1/voice/transcribe` with `{data, mime, language}`, where `data` is base64.
- These go through the link like every other call. Check that a ~300 KB request body passes through `LinkConnection` (photo uploads already do).

**Settings → Voice & calls page:**
- Add `Section("voice", "Voice", Lucide.Mic, "Speech, voices, calls")` to `SECTIONS`, after Notifications, plus a page in `SettingsPages.kt`. Add the `mic` and `phone` icons to `Icons.kt` (Lucide paths, same style).
- **Card "Speech to text":**
  - The status from `voice()`: "Groq · whisper-large-v3 · ready". If it's not available: "Add a Groq key on the server (Settings → AI providers on the desktop app)".
  - One line on privacy: "Your voice clips go through your server to Groq to be turned into text."
  - Free limits in plain words: "Free plan: up to 2,000 spoken turns a day."
  - **Language** chips: Auto (default) / Ελληνικά / English. These pass `language` = "" / "el" / "en".
- **Card "Voices":**
  - For English and for Greek, show whether a voice is installed. Use `TextToSpeech.isLanguageAvailable`, and check `voice.features` for `KEY_FEATURE_NOT_INSTALLED`. Prefer voices where `!isNetworkConnectionRequired`; they work on bad connections and start faster.
  - If one is missing, an **Install** button opens `TextToSpeech.Engine.ACTION_INSTALL_TTS_DATA`.
  - A voice picker per language, speed chips (Slower / Normal / Faster), and a **Test** button that speaks a sample in each language.
- **Card "Calls":**
  - "Interrupt by talking" (on).
  - "Say what I'm doing" (spoken progress, on).
  - "Wait before answering": Short 0.5 s / Normal 0.8 s / Long 1.2 s of silence ends your turn.
  - "Start on loudspeaker" (off).
- **Card "Try it":** hold to record (up to 15 s), release to send. It shows the text and how long the round trip took. This tests the whole path (phone → relay → server → Groq) before the call screen exists.
- Store settings in SharedPreferences (file `iris`, the existing pattern).

**Recording (`data/Recorder.kt`):**
- `AudioRecord(VOICE_COMMUNICATION, 16000 Hz, mono, PCM 16-bit)`.
- A function that wraps PCM in a WAV header (`audio/wav`). Groq accepts WAV. Opus would be ~10× smaller but needs more code; leave it for later.

**Done when:** on the phone, Settings → Voice & calls shows Groq ready, Try it transcribes Greek and English correctly, and the voice cards install and test both languages.

### Phase 3: speaking replies (`data/Speaker.kt`)

- Wrap `TextToSpeech`:
  - `setAudioAttributes(USAGE_VOICE_COMMUNICATION, CONTENT_TYPE_SPEECH)`, so the phone's echo canceller knows what is being played.
  - A queue of utterances with `UtteranceProgressListener`.
  - `stop()` for interruptions.
- **Sentence splitter for streamed text.** Feed it the `message_delta` content of the call's reply as it arrives. It releases a sentence when it sees `.`, `!`, `?`, the **Greek question mark `;`**, the ano teleia `·`, or a newline, followed by a space or the end. It never splits inside numbers like `3.5`, inside URLs, or in common abbreviations. At `run_finished` it flushes whatever is left.
- **What gets spoken:**
  - Port `speakable()` from `app/src/lib/voice.svelte.ts`: no code blocks, URLs or markdown.
  - **Stop at a `[details]` line.** Nothing after it is spoken.
  - Skip reasoning ("thinking") text.
- **Language per sentence:** Greek letters (`[Ͱ-Ͽἀ-῿]`) → the Greek voice, otherwise the English voice (the user's picks).
- Write JVM unit tests (`src/test`) for the splitter (Greek `;`, decimals, abbreviations, streaming in small pieces), the language choice and the `[details]` cut-off.

**Done when:** unit tests pass, and Settings → Voice & calls → Test reads a mixed Greek and English paragraph with the right voices.

### Phase 4: the call

**Starting a call:**
- A phone button in the chat header (every agent). Calling Iris is the main use.
- If `voice()` says speech isn't available, the button opens Settings → Voice & calls instead.
- The first time, ask for `RECORD_AUDIO`.

**`CallService` (foreground service, `foregroundServiceType="microphone"`):**
- Add the `RECORD_AUDIO` and `FOREGROUND_SERVICE_MICROPHONE` permissions to the manifest.
- Start it from the call screen while the app is in the foreground. Android 14 and later only allow starting a microphone service then.
- An ongoing notification: "Call with Iris · 03:12", with a **Hang up** action.
- It owns the audio loop and keeps the process alive with the screen off.

**Keep the link open:** `IrisViewModel.pause()` closes the link after a minute in the background. It must not do that while a call is active.

**Audio:**
- `AudioManager.mode = MODE_IN_COMMUNICATION` during the call, restored afterwards. Request audio focus `AUDIOFOCUS_GAIN_TRANSIENT` for voice communication.
- Record as in Phase 2, plus `AcousticEchoCanceler` and `NoiseSuppressor` on the record session when available.
- Routing uses `setCommunicationDevice()`:
  - Earpiece by default, with the loudspeaker toggle.
  - A Bluetooth headset when one is connected.
- While on the earpiece, hold a `PROXIMITY_SCREEN_OFF_WAKE_LOCK` so the screen turns off at the ear.

**Voice detection (`data/VoiceDetector.kt`):**
- Put it behind a small interface, so it can later be swapped for Silero (more accurate in noise) without touching the call code.
- WebRTC VAD on 20 ms frames (320 samples at 16 kHz), mode `VERY_AGGRESSIVE`.
- A turn starts after ~0.25 s of speech, and ends after the "Wait before answering" silence or at 30 s.
- Keep ~0.3 s of audio from before the speech started, so first words aren't cut off.

**Turn-taking (a small state machine):**
`Listening` → (speech, then silence) → `Transcribing` (WAV → `api.transcribe`) → `Sending` → `Working` → `Speaking` → back to `Listening`.

- Drop empty results and ones of one or two characters, and go back to listening.
- Post with `vm.send(agent, "[Voice call] " + text)`. If the agent is still busy, the engine's existing queue takes it.
- **Interrupting:**
  - While `Speaking`, speech that lasts 0.4 s or more stops the speaker and starts a new user turn. This uses a stricter rule against echo leftovers: needing more speech frames in a row.
  - While `Working`, a new turn is simply queued, as typing would be.
  - The on-screen **Stop** button calls `vm.stop()`.
  - Voice never stops the agent's work by itself.
- **Spoken progress:** when `run_activity` changes while `Working`, speak a short phrase from `Orbs.kt`'s `phrases()` ("Searching the web…"). Speak at most one every 8 s, never over a reply sentence, and only if "Say what I'm doing" is on.
- **Approvals and questions:**
  - On `approval_requested` for this agent, show the approval card over the call screen with Approve/Deny buttons, and say "I need your OK on the screen."
  - **Approvals are never given by voice.**
  - For `ask_user` questions, speak the question and show its buttons. Check how the app answers questions today; if the engine accepts a free-text answer, a spoken answer can be sent as one.
- **Errors (spoken briefly; the call goes on):**
  - Groq limit reached → "I've hit the free speech limit; give me a minute." Retry after 30 s and keep listening.
  - Link drops → "I lost the connection to the server." Keep the call screen up and retry.
  - A reply fails → say so; the problem goes to the inbox as usual.

**Call screen (Compose, the app's own look):**
- The agent's big orb, animated by state: listening, thinking, speaking.
- A live caption of the last thing said by each side.
- A timer, and Mute / Loudspeaker / Stop / Hang up buttons.
- The transcript of the call is simply the chat.

**Hang up:** stop recording and speaking, restore the audio mode, and stop the service.

**Done when:**
- A 5-minute call on the phone works in both languages, including with the screen off and with Bluetooth earbuds.
- Interrupting Iris works, and her own voice doesn't trigger it on loudspeaker.
- A tool task (e.g. "search the web for…") is spoken through.
- An approval appears on screen and needs a tap.
- A Groq error is spoken, and the call continues.

### Phase 5: polish and release

- README: a short "Call Iris" section: get a free Groq key, add it on the server, call from the phone, plus the privacy note.
- DEV.md: a voice section covering the path (phone → engine → Groq), the files, the call state machine, and testing on the emulator (its mic works; test echo cancellation on a real phone).
- Bump `versionCode`/`versionName` and release following the usual process.

## 4. Later (not part of this work)

- **Opus instead of WAV** (MediaCodec + `MediaMuxer` OGG, available from API 29): about 10× smaller clips, for slow mobile data.
- **Backup speech service:** Cloudflare Workers AI `whisper-large-v3-turbo`, free for about 3.5 hours of audio a day on the same Cloudflare account as the relay. The engine would use it when Groq hits its limit.
- **Offline option:** sherpa-onnx with Parakeet-TDT-0.6B-v3 int8 on the phone, as an optional download in Settings → Voice & calls. Measured 2026-11-08:
  - Size: 640 MB.
  - RAM: ~690 MB loaded, ~810 MB peak.
  - Speed: 4.8× real time on a 4-core desktop CPU.
  - APK: +~11 MB.
  - The files are pinned at `huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8`, revision `ed5d745623472f97cfc2e6c2916ee248798fc5a8`:
    - encoder `1afbe69c…c479`
    - decoder `0222a861…c0b5`
    - joiner `cbd02f36…fc98`
    - tokens `d5854467…c35d`
  - The VAD interface and the swappable transcription step make this a drop-in addition.
- **"Hey Iris" wake word**, and showing the call in the system call UI (Telecom self-managed `ConnectionService`).

## 5. Risks

| Risk | Mitigation |
|---|---|
| Groq changes or tightens its free plan | Clear spoken error; Cloudflare Whisper as a backup; the on-phone option later |
| Whisper invents text on silence or noise | The VAD only sends real speech; the engine drops known silence phrases |
| WebRTC VAD is too jumpy in noise or with echo | Voice-communication audio source + AEC, the stricter interrupt rule, an "Interrupt by talking" switch, and the interface allows swapping to Silero |
| Android kills the call in the background | A foreground microphone service; the link is kept open during calls |
| Agent replies too long for speech | `VOICE_INSTRUCTIONS` plus the `[details]` cut-off |
| A Groq provider accidentally becomes a chat model | Section 2: the default and fallback stay on DeepSeek; the voice code only uses Groq for transcription |
