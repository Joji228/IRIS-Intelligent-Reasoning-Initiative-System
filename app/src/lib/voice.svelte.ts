// Voice: the microphone records a clip that the engine turns into text, and
// replies are read aloud with the system's own voices (speechSynthesis; free,
// offline, no key).

import type { IrisApi } from './api';

class Voice {
  /** The message being read aloud, if any. */
  speakingId = $state<string | null>(null);
  recording = $state(false);
  transcribing = $state(false);
  /** The last message was dictated: read the reply aloud when it arrives. */
  autoSpeak = $state(false);
  /** Whether the engine can turn speech into text (null: not asked yet). */
  available = $state<boolean | null>(null);

  private recorder: MediaRecorder | null = null;
  private chunks: Blob[] = [];

  get canSpeak(): boolean {
    return typeof window !== 'undefined' && 'speechSynthesis' in window;
  }

  get canRecord(): boolean {
    return typeof navigator !== 'undefined' && !!navigator.mediaDevices?.getUserMedia && typeof MediaRecorder !== 'undefined';
  }

  /** Reads a reply aloud (or stops if it's the one being read). */
  speak(id: string, markdown: string) {
    if (!this.canSpeak) return;
    const synth = window.speechSynthesis;
    const again = this.speakingId === id;
    synth.cancel();
    this.speakingId = null;
    if (again) return;
    const text = speakable(markdown);
    if (!text) return;
    // Long replies are spoken in sentence-sized pieces: some engines stop
    // after ~200 words in one utterance.
    const pieces = text.match(/[^.!?;\n]+[.!?;]*\s*/g) ?? [text];
    const lang = /[Ͱ-Ͽ]/.test(text) ? 'el-GR' : navigator.language || 'en-US';
    const voice = synth.getVoices().find((v) => v.lang.toLowerCase().startsWith(lang.slice(0, 2).toLowerCase()));
    this.speakingId = id;
    pieces.forEach((piece, i) => {
      const u = new SpeechSynthesisUtterance(piece.trim());
      u.lang = lang;
      if (voice) u.voice = voice;
      if (i === pieces.length - 1) u.onend = u.onerror = () => this.speakingId === id && (this.speakingId = null);
      synth.speak(u);
    });
  }

  stopSpeaking() {
    if (this.canSpeak) window.speechSynthesis.cancel();
    this.speakingId = null;
  }

  /** Starts recording from the microphone. */
  async start(): Promise<void> {
    if (this.recording || !this.canRecord) return;
    this.stopSpeaking();
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const mime = ['audio/webm;codecs=opus', 'audio/webm', 'audio/ogg;codecs=opus', 'audio/mp4'].find((m) =>
      MediaRecorder.isTypeSupported(m),
    );
    this.recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
    this.chunks = [];
    this.recorder.ondataavailable = (e) => e.data.size && this.chunks.push(e.data);
    this.recorder.start();
    this.recording = true;
  }

  /** Stops recording and returns what was said (empty if nothing). */
  async finish(api: IrisApi): Promise<string> {
    const rec = this.recorder;
    if (!rec || !this.recording) return '';
    const stopped = new Promise<void>((resolve) => (rec.onstop = () => resolve()));
    rec.stop();
    rec.stream.getTracks().forEach((t) => t.stop());
    await stopped;
    this.recording = false;
    this.recorder = null;
    const blob = new Blob(this.chunks, { type: rec.mimeType || 'audio/webm' });
    this.chunks = [];
    if (blob.size < 1200) return ''; // a click, not speech
    this.transcribing = true;
    try {
      const { text } = await api.transcribe(await toBase64(blob), blob.type);
      return text;
    } finally {
      this.transcribing = false;
    }
  }

  cancel() {
    const rec = this.recorder;
    if (rec) {
      rec.onstop = null;
      if (rec.state !== 'inactive') rec.stop();
      rec.stream.getTracks().forEach((t) => t.stop());
    }
    this.recorder = null;
    this.chunks = [];
    this.recording = false;
  }
}

/** Markdown → plain text worth hearing: no code, URLs or markup. */
export function speakable(md: string): string {
  return md
    .replace(/```[\s\S]*?```/g, ' (code) ')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/https?:\/\/\S+/g, '')
    .replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+[.)])\s+/gm, '')
    .replace(/[*_~|]+/g, '')
    .replace(/\[\d+\]/g, '')
    .replace(/\s+/g, ' ')
    .trim();
}

function toBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result).split(',')[1] ?? '');
    r.onerror = () => reject(r.error);
    r.readAsDataURL(blob);
  });
}

export const voice = new Voice();
