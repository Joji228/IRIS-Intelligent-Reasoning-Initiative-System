// I.R.I.S. Link: a tiny relay on Cloudflare's free tier between the server
// (the engine, which dials out) and your phone and browsers. It never reads
// what it carries: the apps and the engine seal every message end to end, so
// this only passes opaque text along, tagged with which client it's for.
//
//   engine  ── wss /link/server (X-Iris-Server-Key) ──┐
//                                                     ├── Hub (one Durable Object)
//   phone / browser ── wss /link/client ──────────────┘
//
// Frames the engine sees:   {"c": id, "d": data} | {"c": id, "open": true} | {"c": id, "close": true}
// Frames the engine sends:  {"c": id, "d": data} | {"c": id, "kick": true}
// Clients send and receive just their data, plus unsealed relay notices:
// {"relay":"offline"} when the server drops, {"relay":"online"} when it's back.
//
// GET /link/key gives the engine's public key (sent by the engine when it
// connects), so a browser signing in with a password knows which key to
// expect. It's as trustworthy as the web app itself, which this relay serves.

import { DurableObject } from 'cloudflare:workers';
import { securityHeaders } from './headers';

interface Env {
  HUB: DurableObjectNamespace<Hub>;
  /** Only the engine knows this (set with `wrangler secret put SERVER_KEY`). */
  SERVER_KEY: string;
  ASSETS: Fetcher;
}

/** Limits that keep a stranger from burning the free quota. */
const MAX_CLIENTS = 20;
/** A household (PC, phones, tablets, emulators) shares one address. */
const MAX_CLIENTS_PER_IP = 10;
/** Clients ping every 20 s; one quiet this long is gone (a killed app, a
 *  phone that slept) and makes room for a new connection. */
const STALE_MS = 45_000;
const MAX_FRAME = 32 * 1024 * 1024;
const MAX_MSGS_PER_MINUTE = 600;

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    if (url.pathname === '/health') return new Response('ok');
    if (url.pathname === '/link/key') return env.HUB.get(env.HUB.idFromName('main')).fetch(req);
    if (url.pathname === '/link/server' || url.pathname === '/link/client') {
      if (req.headers.get('Upgrade')?.toLowerCase() !== 'websocket') {
        return new Response('expected a WebSocket', { status: 426 });
      }
      if (url.pathname === '/link/server' && !safeEqual(req.headers.get('X-Iris-Server-Key') ?? '', env.SERVER_KEY ?? '')) {
        return new Response('forbidden', { status: 403 });
      }
      return env.HUB.get(env.HUB.idFromName('main')).fetch(req);
    }
    // Everything else: the web app (static files), with security headers.
    const res = await env.ASSETS.fetch(req);
    const headers = new Headers(res.headers);
    for (const [name, value] of Object.entries(securityHeaders(url.host, url.protocol === 'https:'))) headers.set(name, value);
    return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
  },
};

/** Constant-time comparison for the server key. */
function safeEqual(a: string, b: string): boolean {
  if (!a || !b || a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

type Attachment =
  | { role: 'server' }
  | { role: 'client'; id: string; window: number; count: number; seen?: number };

export class Hub extends DurableObject<Env> {
  async fetch(req: Request): Promise<Response> {
    const path = new URL(req.url).pathname;
    if (path === '/link/key') {
      const key = (await this.ctx.storage.get<string>('engine-key')) ?? null;
      return Response.json({ key, online: this.ctx.getWebSockets('server').length > 0 }, { headers: { 'cache-control': 'no-store' } });
    }
    const isServer = path === '/link/server';
    const [client, socket] = Object.values(new WebSocketPair());
    if (isServer) {
      // A new engine connection replaces the old one.
      for (const old of this.ctx.getWebSockets('server')) old.close(1012, 'replaced');
      this.ctx.acceptWebSocket(socket, ['server']);
      socket.serializeAttachment({ role: 'server' } satisfies Attachment);
      const key = req.headers.get('X-Iris-Public-Key') ?? '';
      if (/^[A-Za-z0-9_-]{80,100}$/.test(key)) await this.ctx.storage.put('engine-key', key);
      // Clients already here start over with the (new) engine connection.
      for (const c of this.ctx.getWebSockets('client')) c.send(JSON.stringify({ relay: 'online' }));
    } else {
      // One address can't take every slot (the engine also drops clients
      // that don't sign in within 20 seconds).
      const ip = `ip:${(req.headers.get('CF-Connecting-IP') ?? 'unknown').slice(0, 64)}`;
      // Full: first let go of connections that went quiet (clients that died
      // without closing would otherwise hold their slots for good).
      if (this.open(ip) >= MAX_CLIENTS_PER_IP) this.dropStale(ip);
      if (this.open(ip) >= MAX_CLIENTS_PER_IP) {
        return new Response('too many connections from your address', { status: 429 });
      }
      if (this.open('client') >= MAX_CLIENTS) this.dropStale('client');
      if (this.open('client') >= MAX_CLIENTS) {
        return new Response('too many connections', { status: 503 });
      }
      const id = crypto.randomUUID();
      this.ctx.acceptWebSocket(socket, ['client', `client:${id}`, ip]);
      const now = Date.now();
      socket.serializeAttachment({ role: 'client', id, window: now, count: 0, seen: now } satisfies Attachment);
      this.toServer({ c: id, open: true });
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    if (typeof message !== 'string' || message.length > MAX_FRAME) return;
    const a = ws.deserializeAttachment() as Attachment;
    if (a.role === 'client') {
      // Rate limit per client.
      const now = Date.now();
      if (now - a.window > 60_000) {
        a.window = now;
        a.count = 0;
      }
      if (++a.count > MAX_MSGS_PER_MINUTE) {
        ws.close(1008, 'slow down');
        return;
      }
      a.seen = now;
      ws.serializeAttachment(a);
      if (!this.toServer({ c: a.id, d: message })) ws.send(JSON.stringify({ relay: 'offline' }));
      return;
    }
    // From the engine: deliver to one client (or kick it).
    let frame: { c?: string; d?: string; kick?: boolean };
    try {
      frame = JSON.parse(message);
    } catch {
      return;
    }
    if (!frame.c) return;
    for (const c of this.ctx.getWebSockets(`client:${frame.c}`)) {
      if (frame.kick) c.close(1008, 'closed by the server');
      else if (typeof frame.d === 'string') c.send(frame.d);
    }
  }

  async webSocketClose(ws: WebSocket): Promise<void> {
    const a = ws.deserializeAttachment() as Attachment | null;
    if (a?.role === 'client') this.toServer({ c: a.id, close: true });
    if (a?.role === 'server' && !this.ctx.getWebSockets('server').some((s) => s !== ws)) {
      // Let clients know the server is away (they keep their connection),
      // unless this was an old connection a new one replaced.
      for (const c of this.ctx.getWebSockets('client')) c.send(JSON.stringify({ relay: 'offline' }));
    }
  }

  async webSocketError(ws: WebSocket): Promise<void> {
    await this.webSocketClose(ws);
  }

  /** Connections under `tag` that are still open (a closed one stays listed
   *  until its far end answers, which a dead client never does). */
  private open(tag: string): number {
    return this.ctx.getWebSockets(tag).filter((ws) => ws.readyState === WebSocket.READY_STATE_OPEN).length;
  }

  /** Closes the clients under `tag` that haven't sent anything for a while
   *  (connections from before `seen` was kept count as quiet). */
  private dropStale(tag: string): void {
    const now = Date.now();
    for (const ws of this.ctx.getWebSockets(tag)) {
      const a = ws.deserializeAttachment() as Attachment | null;
      if (a?.role !== 'client' || now - (a.seen ?? 0) < STALE_MS) continue;
      try {
        ws.close(1001, 'gone quiet');
      } catch {
        // already closing
      }
      this.toServer({ c: a.id, close: true });
    }
  }

  /** Sends a frame to the engine; false when it isn't connected. */
  private toServer(frame: object): boolean {
    const [server] = this.ctx.getWebSockets('server');
    if (!server) return false;
    server.send(JSON.stringify(frame));
    return true;
  }
}
