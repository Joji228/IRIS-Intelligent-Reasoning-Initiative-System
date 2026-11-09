// Run with: node --test src/lib/call/logic.test.ts   (Node 22.18+ runs TypeScript as it is)
// The same cases as the phone app's SpeechTextTest.kt and CallLogicTest.kt.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  SpeechStream, ReplySpeech, TurnDetector, ProgressGate, languageOf, hasForeignScript, isPhantomPhrase,
  isUsableTranscript, looksLikeEcho, speechLevel, isWorthTranscribing, callPhase, classifyProblem, describeReset,
  encodeWav, toBase64, FRAME_SAMPLES,
} from './logic.ts';

function speak(text: string, size = 3): string[] {
  const s = new SpeechStream();
  const out: string[] = [];
  for (let i = 0; i < text.length; i += size) out.push(...s.push(text.slice(i, i + size)));
  out.push(...s.flush());
  return out;
}

test('splits on full stops, questions and exclamations', () => {
  assert.deepEqual(speak('It is sunny. Do you want a coat? Take one!'), ['It is sunny.', 'Do you want a coat?', 'Take one!']);
});

test('Greek question mark and ano teleia end sentences', () => {
  assert.deepEqual(speak('Τι ώρα είναι; Είναι πέντε.'), ['Τι ώρα είναι;', 'Είναι πέντε.']);
  assert.deepEqual(speak('Πρώτα το ένα· μετά το άλλο.'), ['Πρώτα το ένα·', 'μετά το άλλο.']);
});

test('decimals, URLs and abbreviations do not split', () => {
  assert.deepEqual(speak('It is 3.5 degrees. Cold.'), ['It is 3.5 degrees.', 'Cold.']);
  assert.deepEqual(speak('See https://example.com/a.b for details. Done.'), ['See for details.', 'Done.']);
  assert.deepEqual(speak('Use a tool, e.g. the browser. Then wait.'), ['Use a tool, e.g. the browser.', 'Then wait.']);
  assert.deepEqual(speak('1. First thing.\n2. Second thing.'), ['First thing.', 'Second thing.']);
});

test('the same sentences however the text is cut up', () => {
  const text = 'Γεια σου! The weather is 14.5 degrees, e.g. cloudy. Θες κάτι άλλο; Ναι\nΟκ.';
  const whole = speak(text, text.length);
  for (const size of [1, 2, 3, 5, 7, 11]) assert.deepEqual(speak(text, size), whole, `size ${size}`);
});

test('code is not read and nothing after [details] is spoken', () => {
  assert.deepEqual(speak('**Here** is the *result*.\n```js\nlet x = 1\n```\nHope that helps.'), ['Here is the result.', '(code)', 'Hope that helps.']);
  for (let size = 1; size < 10; size++) {
    assert.deepEqual(speak('Short answer.\n[details]\nlong report. with sentences.', size), ['Short answer.'], `size ${size}`);
  }
  assert.deepEqual(speak('Put [details] on its own line.'), ['Put [details] on its own line.']);
});

test('other scripts are never spoken', () => {
  assert.deepEqual(speak('Sure. Спасибо за просмотр. Done.'), ['Sure.', 'Done.']);
  assert.equal(hasForeignScript('Καλημέρα, how are you?'), false);
  assert.equal(hasForeignScript('ご視聴ありがとうございました'), true);
  assert.equal(hasForeignScript('42'), false);
});

test('language per sentence', () => {
  assert.deepEqual(speak('Γεια σου! The weather in London is cloudy. Θες να σε ενημερώσω; Just say the word.').map(languageOf), ['el', 'en', 'el', 'en']);
});

test('reply speech: rounds, new messages, silencing', () => {
  const r = new ReplySpeech();
  const said = [...r.onDelta('m1', 0, 'On it, give me a minute'), ...r.onDelta('m1', 1, 'Found it. All good.'), ...r.onEnd()];
  assert.deepEqual(said, ['On it, give me a minute', 'Found it.', 'All good.']);
  const s = new ReplySpeech();
  const out = [...s.onDelta('m1', 0, 'One. Tw')];
  s.silence();
  out.push(...s.onDelta('m1', 0, 'o. Three.'), ...s.onEnd(), ...s.onDelta('m2', 0, 'Fresh start.'), ...s.onEnd());
  assert.deepEqual(out, ['One.', 'Fresh start.']);
});

test('phantom phrases, echo and tiny results', () => {
  for (const p of ['Thank you.', 'Thank you. Thank you.', 'Ευχαριστώ.', 'Bye.', 'You']) assert.equal(isPhantomPhrase(p), true, p);
  for (const p of ['Thank you for the summary, now check my mail', 'you know what', '', 'Bye the way']) assert.equal(isPhantomPhrase(p), false, p);
  assert.equal(looksLikeEcho('book the cheapest one', ['Do you want me to book the cheapest one?']), true);
  assert.equal(looksLikeEcho('Yes please do', ['Do you want me to book the cheapest one?']), false);
  assert.equal(isUsableTranscript('ok'), false);
  assert.equal(isUsableTranscript('Ναι'), true);
});

const tone = (frames: number, level: number) => Int16Array.from({ length: frames * FRAME_SAMPLES }, (_, i) => (i % 2 ? -level : level));
const concat = (...parts: Int16Array[]) => {
  const out = new Int16Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
};

test('only real speech is worth sending', () => {
  assert.ok(Math.abs(speechLevel(concat(tone(10, 100), tone(10, 2000)), 10) - 2000) < 1);
  assert.equal(isWorthTranscribing(tone(50, 80), 50), false); // a hum
  assert.equal(isWorthTranscribing(concat(tone(5, 3000), tone(40, 20)), 5), false); // a click
  assert.equal(isWorthTranscribing(concat(tone(10, 100), tone(30, 1500), tone(10, 100)), 30), true);
});

function runTurns(script: string, setup: (d: TurnDetector) => void = () => {}) {
  let i = 0;
  const d = new TurnDetector(() => script[i++] === 'S');
  setup(d);
  return [...script].map((_, n) => d.feed(Int16Array.from({ length: FRAME_SAMPLES }, () => n)));
}

test('turns start after 250 ms of speech and end after the wait', () => {
  assert.ok(runTurns('S'.repeat(12) + '.').every((e) => e === null));
  assert.equal(runTurns('S'.repeat(13))[12]?.kind, 'started');
  const ev = runTurns('.'.repeat(30) + 'S'.repeat(20) + '.'.repeat(40));
  const ended = ev.at(-1);
  assert.equal(ended?.kind, 'ended');
  if (ended?.kind === 'ended') {
    assert.equal(ended.pcm[0], 15); // 0.3 s of pre-roll
    assert.equal(ended.speechFrames, 20);
  }
  assert.ok(runTurns('S'.repeat(20) + '.'.repeat(45), (d) => (d.endSilenceMs = 1200)).every((e) => e?.kind !== 'ended'));
});

test('strict mode needs steady speech', () => {
  const strict = (d: TurnDetector) => {
    d.startSpeechMs = 400;
    d.tolerant = false;
  };
  assert.ok(runTurns('SSSS.'.repeat(6), strict).every((e) => e === null));
  assert.equal(runTurns('S'.repeat(20), strict)[19]?.kind, 'started');
});

test('phases, problems, progress, allowance', () => {
  assert.equal(callPhase(true, true, true), 'speaking');
  assert.equal(callPhase(false, true, true), 'you');
  assert.equal(callPhase(false, false, true), 'thinking');
  assert.equal(classifyProblem(429, ''), 'limit');
  assert.equal(classifyProblem(0, "Can't reach the I.R.I.S. engine"), 'offline');
  assert.equal(classifyProblem(502, 'couldn\'t turn the recording into text'), 'other');
  const g = new ProgressGate(8000);
  assert.equal(g.next(0, 'Searching the web…', 'web_search', true, false), 'Searching the web');
  assert.equal(g.next(3000, 'Reading a page…', 'web_fetch', true, false), null);
  assert.equal(g.next(9000, 'Reading a page…', 'web_fetch', true, false), 'Reading a page');
  assert.equal(g.next(20000, 'Thinking…', 'thinking', true, false), null);
  assert.equal(describeReset({ limit: 2000, remaining: 10, resets_at: 1000 + 5 * 3600 + 720 }, 1000), 'Fills up again in 5 h 12 min.');
  assert.equal(describeReset({ limit: 2000, remaining: 2000, resets_at: 9999 }, 1000), 'The full allowance is available.');
});

test('WAV header and base64', () => {
  const wav = encodeWav(new Int16Array([1, -2, 300]));
  const v = new DataView(wav.buffer);
  assert.equal(String.fromCharCode(...wav.subarray(0, 4)), 'RIFF');
  assert.equal(v.getUint32(24, true), 16000);
  assert.equal(v.getUint32(40, true), 6);
  assert.equal(wav.length, 50);
  assert.equal(toBase64(new Uint8Array([104, 105])), 'aGk=');
});
