// The computer's or phone browser's own voices (speechSynthesis): free, no download. A queue of
// sentences, each in its own language, that can be cut off at once.

import { languageOf } from './logic';
import { SPEED_RATE, type VoiceSettings } from './settings.svelte';

export interface VoiceChoice {
  name: string;
  label: string;
  /** Works offline and starts faster. */
  local: boolean;
}

export const canSpeak = () => typeof window !== 'undefined' && 'speechSynthesis' in window;

/** The voices for "en" or "el", best first (on this device, then by name). Empty until the browser has loaded them. */
export function voicesFor(lang: 'en' | 'el'): VoiceChoice[] {
  if (!canSpeak()) return [];
  return speechSynthesis
    .getVoices()
    .filter((v) => v.lang.toLowerCase().startsWith(lang))
    .sort((a, b) => Number(b.localService) - Number(a.localService) || a.name.localeCompare(b.name))
    .map((v) => ({ name: v.name, label: `${v.name}${v.localService ? '' : ' · online'}`, local: v.localService }));
}

function pickVoice(lang: 'en' | 'el', wanted: string): SpeechSynthesisVoice | undefined {
  const all = speechSynthesis.getVoices().filter((v) => v.lang.toLowerCase().startsWith(lang));
  return all.find((v) => v.name === wanted) ?? all.find((v) => v.localService) ?? all[0];
}

export class Speaker {
  /** True while a sentence is being said or waits in the queue. */
  speaking = $state(false);
  /** The sentence being said now (the call's caption); stays on the last one. */
  nowSaying = $state('');
  private pending = 0;
  private generation = 0;

  say(text: string, settings: VoiceSettings) {
    if (!canSpeak() || !text.trim()) return;
    const lang = languageOf(text);
    const u = new SpeechSynthesisUtterance(text);
    u.lang = lang === 'el' ? 'el-GR' : 'en-US';
    const voice = pickVoice(lang, lang === 'el' ? settings.voiceEl : settings.voiceEn);
    if (voice) u.voice = voice;
    u.rate = SPEED_RATE[settings.speed];
    const gen = this.generation;
    this.pending++;
    this.speaking = true;
    u.onstart = () => gen === this.generation && (this.nowSaying = text);
    u.onend = u.onerror = () => {
      if (gen !== this.generation) return;
      this.pending = Math.max(0, this.pending - 1);
      if (this.pending === 0) this.speaking = false;
    };
    speechSynthesis.speak(u);
  }

  /** Stops at once and forgets the queue. */
  stop() {
    this.generation++;
    this.pending = 0;
    this.speaking = false;
    if (canSpeak()) speechSynthesis.cancel();
  }
}
