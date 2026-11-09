// A voice call with an agent, in the desktop app and the web app (docs/DEV.md, "Voice calls").
// The same flow as the phone: the microphone is cut into turns, each turn becomes text on the
// server (Groq's Whisper; the key stays there), goes into the agent's chat as "[Voice call] …",
// and the streamed reply is read aloud sentence by sentence. Approvals are never given by voice.
//
// It talks to the app only through what start() is given and through onEvent(), so it doesn't
// import the app state (no import cycle).

import type { IrisApi } from '../api';
import type { Agent, IrisEvent } from '../types';
import {
  FRAME_SAMPLES, SAMPLE_RATE, EnergyDetector, ProgressGate, ReplySpeech, TurnDetector, callPhase, classifyProblem,
  encodeWav, frameLevel, hasForeignScript, isPhantomPhrase, isUsableTranscript, isWorthTranscribing, looksLikeEcho,
  speakable, toBase64, PROBLEM_LINES, type CallPhase,
} from './logic';
import { Speaker } from './speaker.svelte';
import { WAIT_MS, voicePrefs } from './settings.svelte';

/** Matches the engine's chat::VOICE_PREFIX. */
export const VOICE_PREFIX = '[Voice call]';

/** The agent's own voice leaks into the microphone a moment after it stops. */
const ECHO_TAIL_MS = 300;
/** While it talks, an interruption must also be clearly loud (the computer's speakers aren't always in the echo canceller's view). */
const INTERRUPT_LEVEL = 700;

interface Clip {
  pcm: Int16Array;
  echoRisk: boolean;
}

export interface CallDeps {
  api: () => IrisApi | null;
  /** Speakable phrase for a step ("Searching the web…"). */
  phrase: (activity: string) => string | undefined;
}

class Call {
  /** The agent on the call, or null when there is none. */
  agent = $state<Agent | null>(null);
  /** The call screen is shown (false: minimized to the bar). */
  open = $state(false);
  startedAt = $state(0);
  muted = $state(false);
  userSpeaking = $state(false);
  busyTurns = $state(0);
  working = $state(false);
  youSaid = $state('');
  trouble = $state<string | null>(null);
  /** Your microphone's level, 0..1 (the live meter). */
  level = $state(0);
  readonly speaker = new Speaker();

  get active() {
    return this.agent !== null;
  }

  get phase(): CallPhase {
    return callPhase(this.speaker.speaking, this.userSpeaking, this.busyTurns > 0 || this.working);
  }

  private deps: CallDeps | null = null;
  private threadId: string | null = null;
  private stream: MediaStream | null = null;
  private ctx: AudioContext | null = null;
  private node: ScriptProcessorNode | null = null;
  private turns: TurnDetector | null = null;
  private reply = new ReplySpeech();
  private progress = new ProgressGate();
  private clips: Clip[] = [];
  private draining = false;
  private sayQueue: string[] = [];
  private recentlySpoken: string[] = [];
  private toldAt = new Map<string, number>();
  private announced = new Set<string>();
  private carry = new Float32Array(0);
  private frame = new Int16Array(FRAME_SAMPLES);
  private filled = 0;
  private echoUntil = 0;
  private turnEchoRisk = false;
  private recentLevels: number[] = [];
  private shownLevel = 0;
  private tick = 0;

  /** Starts a call with [agent]. Throws with a message to show if the microphone can't be used. */
  async start(agent: Agent, deps: CallDeps) {
    if (this.active) {
      this.open = true;
      return;
    }
    const api = deps.api();
    if (!api) throw new Error('Not connected to the server');
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('This browser has no microphone access (it needs a secure https page).');
    this.deps = deps;
    // The chat thread the call talks in (it is the agent's one ongoing chat).
    this.threadId = (await api.chat(agent.id)).thread.id;
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
    });
    const ctx = new AudioContext();
    this.ctx = ctx;
    const source = ctx.createMediaStreamSource(this.stream);
    // ScriptProcessor rather than an AudioWorklet: a worklet needs a script URL the app's strict
    // content policy doesn't allow, and 1024-sample blocks are plenty for 20 ms frames.
    const node = ctx.createScriptProcessor(1024, 1, 1);
    node.onaudioprocess = (e) => this.onAudio(e.inputBuffer.getChannelData(0), ctx.sampleRate);
    source.connect(node);
    node.connect(ctx.destination); // outputs silence; Chrome only runs connected nodes
    this.node = node;
    const detector = new EnergyDetector();
    this.turns = new TurnDetector((f) => detector.isSpeech(f), WAIT_MS[voicePrefs.value.wait]);
    this.agent = agent;
    this.open = true;
    this.startedAt = Date.now();
    this.muted = false;
    this.youSaid = '';
    this.trouble = null;
    this.reply = new ReplySpeech();
    this.speaker.nowSaying = '';
    if (ctx.state === 'suspended') await ctx.resume();
  }

  hangUp() {
    if (!this.active) return;
    this.node?.disconnect();
    this.node = null;
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    void this.ctx?.close();
    this.ctx = null;
    this.speaker.stop();
    this.sayQueue = [];
    this.clips = [];
    this.agent = null;
    this.open = false;
    this.userSpeaking = false;
    this.working = false;
    this.busyTurns = 0;
    this.level = 0;
    this.announced.clear();
  }

  toggleMute() {
    this.muted = !this.muted;
    if (this.muted) {
      this.turns?.reset();
      this.userSpeaking = false;
    }
  }

  /** The Stop button (and the space bar): stops the agent's work and its voice. */
  async stopReply() {
    this.silenceNow();
    const tid = this.threadId;
    if (tid && this.working) await this.deps?.api()?.stop(tid).catch(() => {});
  }

  /** Stops the voice now; the rest of this reply stays unspoken. */
  silenceNow() {
    this.reply.silence();
    this.sayQueue = [];
    this.speaker.stop();
  }

  // ---- microphone → turns ---------------------------------------------------------------

  /** Audio from the browser at its own rate → 16 kHz 20 ms frames. */
  private onAudio(input: Float32Array, rate: number) {
    if (!this.turns) return;
    // Resample (linear) to 16 kHz.
    const step = rate / SAMPLE_RATE;
    const data = new Float32Array(this.carry.length + input.length);
    data.set(this.carry);
    data.set(input, this.carry.length);
    let pos = 0;
    for (; pos + 1 < data.length; pos += step) {
      const i = Math.floor(pos);
      const s = data[i] + (data[i + 1] - data[i]) * (pos - i);
      this.frame[this.filled++] = Math.max(-32768, Math.min(32767, Math.round(s * 32767)));
      if (this.filled === FRAME_SAMPLES) {
        this.onFrame(this.frame);
        this.filled = 0;
      }
    }
    this.carry = data.slice(Math.floor(pos));
  }

  private onFrame(frame: Int16Array) {
    const turns = this.turns!;
    const lvl = frameLevel(frame);
    this.recentLevels.push(lvl);
    if (this.recentLevels.length > 25) this.recentLevels.shift();
    // The live meter, smoothed, updated 25 times a second.
    this.shownLevel = Math.max(lvl, this.shownLevel * 0.85);
    if (++this.tick % 2 === 0) this.level = this.muted ? 0 : Math.min(1, Math.log10(1 + this.shownLevel / 60) / 2.4);
    if (this.muted) return;
    const s = voicePrefs.value;
    const now = performance.now();
    if (this.speaker.speaking) this.echoUntil = now + ECHO_TAIL_MS;
    const echoRisk = now < this.echoUntil;
    if (echoRisk && !s.interrupt && !turns.inTurn) return;
    turns.endSilenceMs = WAIT_MS[s.wait];
    turns.startSpeechMs = echoRisk ? 400 : 250;
    turns.tolerant = !echoRisk;
    const event = turns.feed(frame);
    if (!event) return;
    if (event.kind === 'started') {
      if (echoRisk) {
        // Not loud enough to be you talking over it: most likely its own voice from the speakers.
        const recent = this.recentLevels.reduce((a, b) => a + b, 0) / this.recentLevels.length;
        if (recent < INTERRUPT_LEVEL) {
          turns.reset();
          return;
        }
        this.silenceNow();
      }
      this.userSpeaking = true;
      this.turnEchoRisk = echoRisk;
    } else {
      this.userSpeaking = false;
      // A hum or a click isn't speech: Whisper would invent words ("Thank you.") for it.
      if (isWorthTranscribing(event.pcm, event.speechFrames)) {
        this.clips.push({ pcm: event.pcm, echoRisk: this.turnEchoRisk || this.speaker.speaking });
        if (this.clips.length > 8) this.clips.shift();
        void this.drainClips();
      }
    }
  }

  // ---- turn → text → message ------------------------------------------------------------

  private async drainClips() {
    if (this.draining) return;
    this.draining = true;
    try {
      while (this.clips.length && this.active) {
        const clip = this.clips.shift()!;
        this.busyTurns++;
        try {
          await this.handle(clip);
        } finally {
          this.busyTurns--;
        }
      }
    } finally {
      this.draining = false;
    }
  }

  private async handle(clip: Clip) {
    const data = toBase64(encodeWav(clip.pcm));
    for (let tries = 1; this.active; tries++) {
      const api = this.deps?.api();
      try {
        if (!api) throw Object.assign(new Error('Not connected to the server'), { status: 0 });
        const { text } = await api.transcribe(data, 'audio/wav', voicePrefs.value.language);
        this.trouble = null;
        const t = text.trim();
        if (!isUsableTranscript(t) || isPhantomPhrase(t) || hasForeignScript(t)) return;
        if (clip.echoRisk && looksLikeEcho(t, this.recentlySpoken)) return;
        this.youSaid = t;
        await this.post(t);
        return;
      } catch (e) {
        const status = (e as { status?: number }).status;
        const message = e instanceof Error ? e.message : String(e);
        const problem = classifyProblem(status, message);
        if (problem === 'limit') {
          this.tellOnce(PROBLEM_LINES.limit, 60_000);
          if (tries >= 4) return;
          await sleep(30_000);
        } else if (problem === 'offline') {
          this.trouble = 'Lost the connection to the server. Trying again…';
          this.tellOnce(PROBLEM_LINES.offline, 30_000);
          if (tries >= 12) return;
          await sleep(5_000);
        } else {
          this.tellOnce(message.length >= 3 && message.length <= 120 ? message : PROBLEM_LINES.other, 30_000);
          return;
        }
      }
    }
  }

  private async post(text: string) {
    for (let attempt = 0; attempt < 3 && this.active; attempt++) {
      try {
        const api = this.deps?.api();
        if (!api || !this.threadId) throw new Error('Not connected to the server');
        await api.postMessage(this.threadId, `${VOICE_PREFIX} ${text}`, []);
        return;
      } catch {
        if (attempt === 0) this.tellOnce(PROBLEM_LINES.offline, 30_000);
        await sleep(3_000);
      }
    }
  }

  // ---- the reply: events from the server -----------------------------------------------------

  /** The app hands every live event here while a call is on. */
  onEvent(e: IrisEvent) {
    const agent = this.agent;
    if (!agent) return;
    switch (e.type) {
      case 'run_started':
        if (e.agent_id === agent.id) {
          this.working = true;
          this.progress.reset();
        }
        break;
      case 'run_activity':
        if (e.agent_id === agent.id) {
          const p = this.progress.next(Date.now(), this.deps?.phrase(e.activity), e.activity, voicePrefs.value.sayProgress, this.speaker.speaking || this.sayQueue.length > 0);
          if (p) this.say(p);
        }
        break;
      case 'message_delta':
        // Only the reply's own words, never its thinking.
        if (e.thread_id === this.threadId && e.content) this.reply.onDelta(e.message_id, e.round, e.content).forEach((s) => this.say(s));
        break;
      case 'approval_requested':
        if (e.approval.agent_id === agent.id && !this.announced.has(e.approval.id)) {
          this.announced.add(e.approval.id);
          this.say('I need your OK on the screen.');
        }
        break;
      case 'run_finished':
        if (e.agent_id === agent.id) {
          this.working = false;
          this.reply.onEnd().forEach((s) => this.say(s));
          if (e.status === 'error') this.say("Something went wrong with that reply. It's in your inbox.");
          if (e.status === 'complete') void this.speakQuestion();
        }
        break;
    }
  }

  /** A question for you (ask_user) is in the tool step, not the text: say it too. */
  private async speakQuestion() {
    const api = this.deps?.api();
    if (!api || !this.threadId) return;
    try {
      const detail = await api.thread(this.threadId);
      const last = detail.messages.at(-1);
      const step = last?.role === 'assistant' ? [...(last.rounds?.at(-1)?.tools ?? [])].reverse().find((t) => t.name === 'ask_user' && t.status === 'done') : undefined;
      const q = step ? (JSON.parse(step.arguments || '{}').question as string | undefined) : undefined;
      if (q?.trim()) this.say(speakable(q));
    } catch {
      /* no question to read */
    }
  }

  private tellOnce(text: string, withinMs: number) {
    const now = Date.now();
    if (now - (this.toldAt.get(text) ?? 0) < withinMs) return;
    this.toldAt.set(text, now);
    this.say(text);
  }

  private say(text: string) {
    this.sayQueue.push(text);
    void this.speakQueued();
  }

  private speakingQueue = false;

  /** One sentence at a time, never over you while you talk. */
  private async speakQueued() {
    if (this.speakingQueue) return;
    this.speakingQueue = true;
    try {
      while (this.sayQueue.length && this.active) {
        while (this.userSpeaking && this.active) await sleep(50);
        const s = this.sayQueue.shift();
        if (!s) break;
        this.speaker.say(s, voicePrefs.value);
        this.recentlySpoken.push(s);
        if (this.recentlySpoken.length > 8) this.recentlySpoken.shift();
      }
    } finally {
      this.speakingQueue = false;
    }
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const call = new Call();
