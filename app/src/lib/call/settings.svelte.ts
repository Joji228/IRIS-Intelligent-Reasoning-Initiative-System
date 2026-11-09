// Settings → Voice, kept in this browser or desktop app (like the phone keeps its own): what you
// speak, the voices, the speed and how calls behave.

export interface VoiceSettings {
  /** "" = English or Greek, picked per phrase; or "el" / "en". */
  language: '' | 'el' | 'en';
  /** Chosen speechSynthesis voice names; "" = the best one found. */
  voiceEn: string;
  voiceEl: string;
  speed: 'slow' | 'normal' | 'fast';
  /** Talking over the agent stops it speaking. */
  interrupt: boolean;
  /** The agent says what it is doing while it works. */
  sayProgress: boolean;
  /** How long a pause ends your turn. */
  wait: 'short' | 'normal' | 'long';
}

export const SPEED_RATE = { slow: 0.85, normal: 1, fast: 1.2 } as const;
export const WAIT_MS = { short: 500, normal: 800, long: 1200 } as const;

const DEFAULTS: VoiceSettings = { language: '', voiceEn: '', voiceEl: '', speed: 'normal', interrupt: true, sayProgress: true, wait: 'normal' };
const KEY = 'iris.voice';

function load(): VoiceSettings {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(KEY) ?? '{}') };
  } catch {
    return { ...DEFAULTS };
  }
}

class VoicePrefs {
  value = $state<VoiceSettings>(load());

  set<K extends keyof VoiceSettings>(key: K, v: VoiceSettings[K]) {
    this.value = { ...this.value, [key]: v };
    try {
      localStorage.setItem(KEY, JSON.stringify(this.value));
    } catch {
      /* private window: kept for this session only */
    }
  }
}

export const voicePrefs = new VoicePrefs();
