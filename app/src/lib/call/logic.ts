// Voice calls in the desktop and web app: the parts with no browser APIs in them, so they are
// tested with `node --test` (see logic.test.ts). The same rules as the phone app's
// CallLogic.kt and SpeechText.kt, ported one to one, so a call behaves the same everywhere.
//
// This file must not import anything at run time (only `import type`): the tests load it
// directly with Node.

/** One 20 ms frame of 16 kHz audio. */
export const FRAME_SAMPLES = 320;
export const FRAME_MS = 20;
export const SAMPLE_RATE = 16000;

// ---- what is said aloud -----------------------------------------------------------------

/** Markdown → plain text worth hearing: no code, URLs or markup (same as voice.svelte.ts). */
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

const GREEK = /[Ͱ-Ͽἀ-῿]/;

/** Greek if the sentence has any Greek letter (Greek replies often carry brand names), else English. */
export function languageOf(sentence: string): 'el' | 'en' {
  return GREEK.test(sentence) ? 'el' : 'en';
}

const isLetter = (c: string) => /\p{L}/u.test(c);
const isLatin = (c: string) => /[A-Za-zÀ-ɏ]/.test(c);
const isGreekChar = (c: string) => GREEK.test(c);

/** Text mostly in a script other than Latin or Greek (Cyrillic, Chinese…): never said or sent. */
export function hasForeignScript(text: string): boolean {
  const letters = [...text].filter(isLetter);
  if (letters.length === 0) return false;
  return letters.filter((c) => !isLatin(c) && !isGreekChar(c)).length * 2 > letters.length;
}

const DETAILS = '[details]';
const ENDINGS = '.!?;·…';
const CLOSERS = '"\')]»”’';
const MAX_PIECE = 300;
const ABBREVIATIONS = new Set([
  'dr', 'mr', 'mrs', 'ms', 'prof', 'sr', 'jr', 'st', 'vs', 'inc', 'ltd', 'co', 'fig', 'al', 'approx', 'e.g', 'i.e',
  'κ', 'κα', 'π.χ', 'δηλ', 'αρ', 'σελ', 'κ.α',
]);

function endsASentence(before: string, mark: string, single: boolean): boolean {
  if (mark !== '.' || !single) return true;
  const b = before.trim();
  if (b && b.length <= 3 && /^\d+$/.test(b)) return false;
  const word = /[\p{L}.]+$/u.exec(before)?.[0]?.toLowerCase();
  return !word || !ABBREVIATIONS.has(word);
}

function add(out: string[], raw: string) {
  const say = speakable(raw);
  if (!/[\p{L}\p{N}]/u.test(say)) return;
  if (hasForeignScript(say)) return;
  let rest = say;
  while (rest.length > MAX_PIECE) {
    let cut = rest.lastIndexOf(', ', MAX_PIECE);
    if (cut <= 40) cut = rest.lastIndexOf(' ', MAX_PIECE);
    if (cut <= 40) cut = MAX_PIECE;
    out.push(rest.slice(0, cut + 1).trim());
    rest = rest.slice(cut + 1).trim();
  }
  if (/[\p{L}\p{N}]/u.test(rest)) out.push(rest);
}

/**
 * Feeds on a reply's streamed text and hands back sentences as soon as they are complete,
 * cleaned up for speaking. Splits at . ! ? … ; · and line breaks followed by a space (so not
 * inside 3.5, example.com or e.g.), skips code blocks, and stops for good at a line that is
 * exactly [details].
 */
export class SpeechStream {
  private pending = '';
  private text = '';
  private atLineStart = true;
  private inFence = false;
  stopped = false;

  push(delta: string): string[] {
    if (this.stopped || !delta) return [];
    this.pending += delta;
    this.route(false);
    return this.sentences(false);
  }

  /** The end of a reply, or of one round of it: whatever is left, even without a full stop. */
  flush(): string[] {
    if (!this.stopped) this.route(true);
    const out = this.sentences(true);
    this.atLineStart = true;
    this.inFence = false;
    this.pending = '';
    return out;
  }

  private route(final: boolean) {
    while (this.pending && !this.stopped) {
      if (this.atLineStart) {
        if (this.pending[0] === '\n') {
          this.text += '\n';
          this.pending = this.pending.slice(1);
          continue;
        }
        const trimmed = this.pending.replace(/^[ \t\r]+/, '');
        if (!trimmed) {
          if (final) this.pending = '';
          return;
        }
        const lineEnd = trimmed.indexOf('\n');
        const line = (lineEnd >= 0 ? trimmed.slice(0, lineEnd) : trimmed).trimEnd();
        const complete = lineEnd >= 0 || final;
        const used = this.pending.length - trimmed.length + (lineEnd >= 0 ? lineEnd + 1 : trimmed.length);
        if (this.inFence) {
          if (!complete) return;
          if (line.startsWith('```')) this.inFence = false;
          this.pending = this.pending.slice(used);
          continue;
        }
        if (line.startsWith('```')) {
          if (!complete) return;
          if (line.indexOf('```', 3) < 0) this.inFence = true;
          this.text += '(code)\n';
          this.pending = this.pending.slice(used);
          continue;
        }
        if (line.startsWith(DETAILS)) {
          if (!complete) return;
          if (!line.slice(DETAILS.length).trim()) {
            this.stopped = true;
            this.pending = '';
            return;
          }
        } else if (!complete && (DETAILS.startsWith(line) || '```'.startsWith(line))) {
          return;
        }
        this.atLineStart = false;
      }
      const nl = this.pending.indexOf('\n');
      if (nl >= 0) {
        this.text += this.pending.slice(0, nl + 1);
        this.pending = this.pending.slice(nl + 1);
        this.atLineStart = true;
      } else {
        this.text += this.pending;
        this.pending = '';
      }
    }
  }

  private sentences(final: boolean): string[] {
    const out: string[] = [];
    const t = this.text;
    let start = 0;
    let i = 0;
    while (i < t.length) {
      const c = t[i];
      if (c === '\n') {
        add(out, t.slice(start, i));
        start = i + 1;
        i++;
        continue;
      }
      if (!ENDINGS.includes(c)) {
        i++;
        continue;
      }
      let j = i;
      while (j + 1 < t.length && ENDINGS.includes(t[j + 1])) j++;
      let k = j + 1;
      while (k < t.length && CLOSERS.includes(t[k])) k++;
      if (k >= t.length) {
        // Can't tell yet what follows the mark.
        if (final) {
          add(out, t.slice(start, k));
          start = k;
        }
        break;
      }
      if (!/\s/.test(t[k]) || !endsASentence(t.slice(start, i), c, j === i)) {
        i = j + 1;
        continue;
      }
      add(out, t.slice(start, k));
      start = k;
      i = k;
    }
    if (final && start < t.length) {
      add(out, t.slice(start));
      start = t.length;
    }
    this.text = t.slice(start);
    return out;
  }
}

/**
 * The call's replies, as sentences to say. A new round flushes the last unfinished sentence
 * ("On it, give me a minute"); talking over a reply silences the rest of it.
 */
export class ReplySpeech {
  private messageId: string | null = null;
  private round = 0;
  private stream = new SpeechStream();
  private silenced = false;

  onDelta(messageId: string, round: number, content: string): string[] {
    if (!content) return [];
    const out: string[] = [];
    if (messageId !== this.messageId) {
      out.push(...this.finishQuietly());
      this.messageId = messageId;
      this.round = round;
      this.stream = new SpeechStream();
      this.silenced = false;
    } else if (round !== this.round) {
      out.push(...this.stream.flush());
      this.round = round;
    }
    out.push(...this.stream.push(content));
    return this.silenced ? [] : out;
  }

  onEnd(): string[] {
    return this.finishQuietly();
  }

  silence() {
    this.silenced = true;
  }

  private finishQuietly(): string[] {
    const rest = this.stream.flush();
    return this.silenced ? [] : rest;
  }
}

// ---- what is heard --------------------------------------------------------------------------

/** Lowercase letters and digits with single spaces. */
export function plainText(s: string): string {
  return s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, ' ').trim();
}

/** What Whisper "hears" in silence and noise (the server drops these too). */
const PHANTOM_PHRASES = [
  'you', 'thank you', 'thanks', 'thank you very much', 'thank you so much', 'thanks for watching',
  'thank you for watching', 'thank you so much for watching', 'thanks for listening', 'thank you for listening',
  'please subscribe', 'bye', 'bye bye', 'subtitles by the amara org community',
  'ευχαριστώ', 'ευχαριστώ πολύ', 'ευχαριστούμε', 'ευχαριστούμε πολύ', 'ευχαριστώ που παρακολουθήσατε',
  'ευχαριστούμε που παρακολουθήσατε', 'ευχαριστώ για την προσοχή σας', 'τα λέμε στο επόμενο βίντεο',
  'υπότιτλοι authorwave', 'υπότιτλοι amara org', 'υπότιτλοι από την κοινότητα του amara org',
].sort((a, b) => b.length - a.length);

/** Only such phrases, once or repeated; a real sentence that starts with one is kept. */
export function isPhantomPhrase(text: string): boolean {
  let rest = plainText(text);
  if (!rest) return false;
  while (rest) {
    const r = rest;
    const hit = PHANTOM_PHRASES.find((p) => r === p || r.startsWith(p + ' '));
    if (!hit) return false;
    rest = rest.slice(hit.length).trim();
  }
  return true;
}

/** Empty results and ones of one or two characters are noise. */
export function isUsableTranscript(text: string): boolean {
  return [...text].filter((c) => /[\p{L}\p{N}]/u.test(c)).length >= 3;
}

/** The agent's own voice coming back through the microphone (part of what was just said). */
export function looksLikeEcho(heard: string, recentlySpoken: string[]): boolean {
  const h = plainText(heard);
  if (h.length < 3) return false;
  return recentlySpoken.map(plainText).some((s) => s.includes(h) || (s.length >= 12 && h.includes(s)));
}

/** One frame's level (RMS of 32768). */
export function frameLevel(frame: Int16Array): number {
  let sum = 0;
  for (let i = 0; i < frame.length; i++) sum += frame[i] * frame[i];
  return frame.length ? Math.sqrt(sum / frame.length) : 0;
}

/** The average level of the turn's loudest [speechFrames] frames (RMS of 32768). */
export function speechLevel(pcm: Int16Array, speechFrames: number): number {
  const levels: number[] = [];
  for (let f = 0; f + FRAME_SAMPLES <= pcm.length; f += FRAME_SAMPLES) levels.push(frameLevel(pcm.subarray(f, f + FRAME_SAMPLES)));
  if (!levels.length) return 0;
  levels.sort((a, b) => b - a);
  const n = Math.min(Math.max(speechFrames, 1), levels.length);
  return levels.slice(0, n).reduce((a, b) => a + b, 0) / n;
}

/** About -41 dBFS. */
export const MIN_SPEECH_LEVEL = 300;

/** At least 0.3 s of speech, a fair share of the clip, and louder than a hum: else Whisper would invent words. */
export function isWorthTranscribing(pcm: Int16Array, speechFrames: number, minLevel = MIN_SPEECH_LEVEL): boolean {
  if (!pcm.length || speechFrames * FRAME_MS < 300) return false;
  if ((speechFrames * FRAME_SAMPLES) / pcm.length < 0.25) return false;
  return speechLevel(pcm, speechFrames) >= minLevel;
}

/**
 * A light voice detector for the browser: a frame is speech when it is clearly louder than the
 * room. The room's level is learnt as the call goes (it follows quiet frames quickly and loud
 * ones slowly), so a fan or street noise raises the bar by itself. The browser's own echo
 * cancelling, noise suppression and gain control run before it.
 */
export class EnergyDetector {
  private floor = 120;

  isSpeech(frame: Int16Array): boolean {
    const level = frameLevel(frame);
    const speech = level > Math.max(this.floor * 3, 260);
    // Quiet frames pull the floor down fast; louder ones that aren't speech lift it slowly.
    if (!speech) this.floor = level < this.floor ? this.floor * 0.9 + level * 0.1 : this.floor * 0.995 + level * 0.005;
    this.floor = Math.min(Math.max(this.floor, 30), 3000);
    return speech;
  }
}

export type TurnEvent = { kind: 'started' } | { kind: 'ended'; pcm: Int16Array; speechFrames: number };

/**
 * Cuts the microphone stream into turns: starts after startSpeechMs of speech (a few quiet frames
 * inside it forgiven while tolerant), ends after endSilenceMs of quiet or at 30 s, and keeps
 * 0.3 s from before the speech began. Feed it 20 ms frames.
 */
export class TurnDetector {
  startSpeechMs = 250;
  tolerant = true;
  inTurn = false;
  endSilenceMs: number;
  private ring: Int16Array[] = [];
  private turn: Int16Array[] = [];
  private run = 0;
  private gap = 0;
  private sinceRun = 0;
  private silence = 0;
  private speechFrames = 0;
  private isSpeech: (frame: Int16Array) => boolean;
  private preRollMs: number;
  private maxTurnMs: number;

  constructor(isSpeech: (frame: Int16Array) => boolean, endSilenceMs = 800, preRollMs = 300, maxTurnMs = 30_000) {
    this.isSpeech = isSpeech;
    this.endSilenceMs = endSilenceMs;
    this.preRollMs = preRollMs;
    this.maxTurnMs = maxTurnMs;
  }

  reset() {
    this.ring = [];
    this.turn = [];
    this.run = this.gap = this.sinceRun = this.silence = this.speechFrames = 0;
    this.inTurn = false;
  }

  feed(frame: Int16Array): TurnEvent | null {
    const copy = frame.slice();
    const voice = this.isSpeech(frame);
    if (!this.inTurn) {
      const startFrames = Math.ceil(this.startSpeechMs / FRAME_MS);
      this.ring.push(copy);
      if (this.ring.length > 100) this.ring.shift();
      if (voice) {
        this.run++;
        this.gap = 0;
      } else {
        this.gap++;
        if (!this.tolerant || this.gap > 3) this.run = 0;
      }
      this.sinceRun = this.run > 0 ? this.sinceRun + 1 : 0;
      if (this.run < startFrames) return null;
      const keep = Math.min(this.sinceRun + Math.floor(this.preRollMs / FRAME_MS), this.ring.length);
      this.turn = this.ring.slice(this.ring.length - keep);
      this.ring = [];
      this.speechFrames = this.run;
      this.run = this.gap = this.sinceRun = this.silence = 0;
      this.inTurn = true;
      return { kind: 'started' };
    }
    this.turn.push(copy);
    if (voice) {
      this.silence = 0;
      this.speechFrames++;
    } else this.silence++;
    if (this.silence * FRAME_MS < this.endSilenceMs && this.turn.length * FRAME_MS < this.maxTurnMs) return null;
    // Keep 200 ms of the trailing quiet, not all of it.
    const extra = Math.max(this.silence - 10, 0);
    const kept = this.turn.slice(0, this.turn.length - extra);
    const pcm = new Int16Array(kept.length * FRAME_SAMPLES);
    kept.forEach((f, i) => pcm.set(f, i * FRAME_SAMPLES));
    const speechFrames = this.speechFrames;
    this.reset();
    return { kind: 'ended', pcm, speechFrames };
  }
}

// ---- the call around it -----------------------------------------------------------------------

export type CallPhase = 'listening' | 'you' | 'thinking' | 'speaking';

export function callPhase(speaking: boolean, userSpeaking: boolean, busy: boolean): CallPhase {
  return speaking ? 'speaking' : userSpeaking ? 'you' : busy ? 'thinking' : 'listening';
}

export type CallProblem = 'limit' | 'offline' | 'other';

export const PROBLEM_LINES: Record<CallProblem, string> = {
  limit: "I've hit the free speech limit; give me a minute.",
  offline: 'I lost the connection to the server.',
  other: "Sorry, I couldn't make that out.",
};

/** 429 = Groq's free limit; no status (or "can't reach") = the connection; anything else. */
export function classifyProblem(status: number | undefined, message: string): CallProblem {
  if (status === 429) return 'limit';
  if (!status || /can't reach|cannot reach|network|offline|failed to fetch/i.test(message)) return 'offline';
  return 'other';
}

const QUIET_STEPS = new Set(['thinking', 'waiting', 'compacting', 'reviewing', 'ask_user']);

/**
 * When to say what the agent is doing: at most one phrase every gapMs, never over a sentence
 * being spoken, never the same twice in a row, never for plain thinking or waiting.
 */
export class ProgressGate {
  private lastAt = -Infinity;
  private lastPhrase = '';
  private gapMs: number;
  constructor(gapMs = 8000) {
    this.gapMs = gapMs;
  }

  next(now: number, phrase: string | undefined, activity: string | undefined, enabled: boolean, speakingNow: boolean): string | null {
    if (!enabled || speakingNow || !activity || !phrase) return null;
    if (QUIET_STEPS.has(activity)) return null;
    if (now - this.lastAt < this.gapMs) return null;
    const p = phrase.replace(/[….\s]+$/, '');
    if (!p || p === this.lastPhrase) return null;
    this.lastAt = now;
    this.lastPhrase = p;
    return p;
  }

  reset() {
    this.lastPhrase = '';
  }
}

/** Groq's allowance as the server reports it. */
export interface Allowance {
  limit: number;
  remaining: number;
  resets_at: number;
}

/** "Fills up again in 5 h 12 min." or that the full allowance is there. */
export function describeReset(l: Allowance, nowSec: number): string {
  const left = l.resets_at - nowSec;
  if (l.remaining >= l.limit || left <= 0) return 'The full allowance is available.';
  const minutes = Math.ceil(left / 60);
  let text: string;
  if (minutes < 2) text = 'about a minute';
  else if (minutes < 60) text = `${minutes} min`;
  else if (minutes < 24 * 60) text = `${Math.floor(minutes / 60)} h` + (minutes % 60 ? ` ${minutes % 60} min` : '');
  else text = `${Math.floor(minutes / 1440)} d ${Math.floor(minutes / 60) % 24} h`;
  return `Fills up again in ${text}.`;
}

/** 16-bit mono PCM → a WAV file (what Groq takes as it is). */
export function encodeWav(pcm: Int16Array, sampleRate = SAMPLE_RATE): Uint8Array {
  const data = pcm.length * 2;
  const buf = new ArrayBuffer(44 + data);
  const v = new DataView(buf);
  const str = (o: number, s: string) => {
    for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i));
  };
  str(0, 'RIFF');
  v.setUint32(4, 36 + data, true);
  str(8, 'WAVE');
  str(12, 'fmt ');
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true);
  v.setUint16(22, 1, true);
  v.setUint32(24, sampleRate, true);
  v.setUint32(28, sampleRate * 2, true);
  v.setUint16(32, 2, true);
  v.setUint16(34, 16, true);
  str(36, 'data');
  v.setUint32(40, data, true);
  for (let i = 0; i < pcm.length; i++) v.setInt16(44 + i * 2, pcm[i], true);
  return new Uint8Array(buf);
}

/** Bytes → base64 without blowing the call stack on big clips. */
export function toBase64(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}
