#!/usr/bin/env node
// Tiny command-line client for the I.R.I.S. engine (dev/testing tool).
//
//   node scripts/iris.mjs health
//   node scripts/iris.mjs agents
//   node scripts/iris.mjs threads [--agent NAME]
//   node scripts/iris.mjs chat [--agent NAME] [--new] ["message"]   (no message: interactive)
//
// Engine URL: IRIS_URL, from the shell or scripts/.env.local (default http://localhost:7700).
// Token: IRIS_TOKEN, or the file .iris-token in the repo root (git-ignored).

import { readFileSync } from 'node:fs';
import { createInterface } from 'node:readline/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const BASE = (process.env.IRIS_URL ?? localSetting('IRIS_URL') ?? 'http://localhost:7700').replace(/\/$/, '');
const dim = (s) => `\x1b[2m${s}\x1b[0m`;
const bold = (s) => `\x1b[1m${s}\x1b[0m`;

function localSetting(key) {
  try {
    const line = readFileSync(join(ROOT, 'scripts', '.env.local'), 'utf8')
      .split(/\r?\n/)
      .find((l) => l.startsWith(`${key}=`));
    return line?.slice(key.length + 1).trim() || undefined;
  } catch {
    return undefined;
  }
}

function token() {
  if (process.env.IRIS_TOKEN) return process.env.IRIS_TOKEN.trim();
  try {
    return readFileSync(join(ROOT, '.iris-token'), 'utf8').trim();
  } catch {
    console.error('No token: set IRIS_TOKEN or put it in .iris-token in the repo root.');
    process.exit(1);
  }
}

async function api(method, path, body) {
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers: { authorization: `Bearer ${token()}`, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status}: ${data?.error ?? text}`);
  return data;
}

function arg(name) {
  const i = process.argv.indexOf(name);
  return i > 0 ? process.argv[i + 1] : undefined;
}

async function pickAgent() {
  const agents = await api('GET', '/v1/agents');
  const want = arg('--agent');
  const agent = want ? agents.find((a) => a.name.toLowerCase() === want.toLowerCase()) : agents[0];
  if (!agent) throw new Error(want ? `no agent named ${want}` : 'no agents');
  return agent;
}

function connectEvents() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${BASE.replace(/^http/, 'ws')}/v1/events?token=${token()}`);
    ws.addEventListener('message', (e) => {
      if (JSON.parse(e.data).type === 'hello') resolve(ws);
    });
    ws.addEventListener('error', () => reject(new Error('could not open the events WebSocket')));
  });
}

// Send one message and stream the reply to stdout; resolves when it finishes.
async function ask(ws, thread, content) {
  let replyId;
  let thinking = false;
  const early = [];
  const done = new Promise((resolve) => {
    const onEvent = (e) => {
      const ev = JSON.parse(e.data);
      if (!replyId) return early.push(ev);
      handle(ev);
    };
    const handle = (ev) => {
      if (ev.type === 'message_delta' && ev.message_id === replyId) {
        if (ev.reasoning) {
          if (!thinking) process.stdout.write(dim('thinking: '));
          thinking = true;
          process.stdout.write(dim(ev.reasoning));
        }
        if (ev.content) {
          if (thinking) process.stdout.write('\n\n');
          thinking = false;
          process.stdout.write(ev.content);
        }
      } else if (ev.type === 'message_completed' && ev.message.id === replyId) {
        ws.removeEventListener('message', onEvent);
        const m = ev.message;
        const usage = m.prompt_tokens != null ? `, ${m.prompt_tokens}+${m.completion_tokens} tokens` : '';
        process.stdout.write('\n' + dim(`[${m.status}${m.error ? ': ' + m.error : ''} · ${m.model ?? '?'}${usage}]`) + '\n');
        resolve();
      }
    };
    ws.addEventListener('message', onEvent);
    ask.flush = () => early.splice(0).forEach(handle);
  });
  const posted = await api('POST', `/v1/threads/${thread.id}/messages`, { content });
  replyId = posted.assistant_message.id;
  ask.flush();
  await done;
}

async function chat() {
  const agent = await pickAgent();
  // Continue the most recent thread unless --new is given.
  let thread = process.argv.includes('--new')
    ? undefined
    : (await api('GET', `/v1/agents/${agent.id}/threads`))[0];
  thread ??= await api('POST', `/v1/agents/${agent.id}/threads`, {});
  console.log(dim(`${agent.avatar} ${agent.name} · ${agent.model} · thread ${thread.title || '(new)'}`));

  const ws = await connectEvents();
  const flags = new Set(['--agent', arg('--agent'), '--new']);
  const message = process.argv.slice(3).filter((a) => !flags.has(a)).join(' ').trim();
  try {
    if (message) {
      await ask(ws, thread, message);
      return;
    }
    const rl = createInterface({ input: process.stdin, output: process.stdout });
    for (;;) {
      const line = (await rl.question(bold('\nyou> '))).trim();
      if (!line || line === '/quit') break;
      process.stdout.write(bold(`${agent.name.toLowerCase()}> `));
      await ask(ws, thread, line);
    }
    rl.close();
  } finally {
    ws.close();
  }
}

const commands = {
  health: async () => console.log(await (await fetch(`${BASE}/health`)).json()),
  agents: async () => {
    for (const a of await api('GET', '/v1/agents')) console.log(`${a.avatar} ${a.name}  ${dim(`${a.provider}/${a.model}  ${a.id}`)}`);
  },
  threads: async () => {
    const agent = await pickAgent();
    for (const t of await api('GET', `/v1/agents/${agent.id}/threads`)) console.log(`${t.title || '(untitled)'}  ${dim(t.id)}`);
  },
  chat,
};

const cmd = commands[process.argv[2] ?? 'chat'];
if (!cmd) {
  console.error(`unknown command; use one of: ${Object.keys(commands).join(', ')}`);
  process.exit(2);
}
cmd().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
