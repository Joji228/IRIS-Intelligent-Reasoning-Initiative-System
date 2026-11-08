// I.R.I.S. Link, the app's side: reaching the engine through the relay with
// every message sealed end to end (the protocol is described in
// crates/engine/src/link.rs). Used by the web app and the phone app; the
// desktop app talks to the engine directly.

import type { IrisEvent } from './types';

const subtle = globalThis.crypto?.subtle;
const enc = new TextEncoder();
const dec = new TextDecoder();
const ECDH = { name: 'ECDH', namedCurve: 'P-256' } as const;

function b64urlEncode(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
function b64Encode(bytes: Uint8Array): string {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}
function b64Decode(s: string): Uint8Array<ArrayBuffer> {
  const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/'));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
const concat = (a: Uint8Array, b: Uint8Array) => {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
};
const nonce = (n: number) => {
  const iv = new Uint8Array(12);
  new DataView(iv.buffer).setBigUint64(4, BigInt(n));
  return iv;
};

/** What this browser remembers about its pairing. */
export interface LinkCredentials {
  /** The relay's address (https://…). */
  relay: string;
  /** The engine's public key (base64url), pinned at pairing. */
  key: string;
  /** This device's token. */
  token: string;
  device: string;
}

export interface LinkResponse {
  status: number;
  body?: string;
  b64?: string;
  ct?: string;
}

export type LinkState = 'connecting' | 'online' | 'server-offline' | 'offline';

type SignIn =
  | { t: 'auth'; token: string }
  | { t: 'pair'; code: string; name: string }
  | { t: 'login'; user: string; password: string; name: string };

/** One sealed session (a fresh one after every reconnect). */
class Session {
  private sent = 0;
  private received = 0;
  private waiters = new Map<number, (r: LinkResponse) => void>();
  private nextId = 0;
  private pendingSignIn: ((m: any) => void) | null = null;
  onEvent: (e: IrisEvent) => void = () => {};
  onPong: () => void = () => {};

  private constructor(
    readonly ws: WebSocket,
    private sendKey: CryptoKey,
    private recvKey: CryptoKey,
  ) {}

  /** Opens the socket and does the handshake with the engine `key`. */
  static async open(relay: string, key: string, onRelay: (m: { relay: string }) => void): Promise<Session> {
    if (!subtle) throw new Error('This browser has no WebCrypto (an https page is needed).');
    const ws = new WebSocket(`${relay.replace(/^http/, 'ws').replace(/\/$/, '')}/link/client`);
    const first = new Promise<string>((resolve, reject) => {
      ws.onmessage = (m) => {
        const text = String(m.data);
        if (text.startsWith('{"relay"')) {
          const r = JSON.parse(text);
          onRelay(r);
          if (r.relay === 'offline') reject(new Error('server-offline'));
          return;
        }
        resolve(text);
      };
      ws.onerror = () => reject(new Error('offline'));
      ws.onclose = () => reject(new Error('offline'));
    });
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error('offline'));
    });

    const mine = await subtle.generateKey(ECDH, true, ['deriveBits']);
    const myPub = new Uint8Array(await subtle.exportKey('raw', mine.publicKey));
    ws.send(JSON.stringify({ t: 'hello', v: 1, epk: b64urlEncode(myPub) }));
    const timeout = new Promise<never>((_, reject) => setTimeout(() => reject(new Error('server-offline')), 10_000));
    const welcome = JSON.parse(await Promise.race([first, timeout]));
    if (welcome.t !== 'welcome') throw new Error('unexpected reply from the engine');
    const eph = b64Decode(welcome.epk);

    const dh = async (raw: Uint8Array<ArrayBuffer>) => {
      const pub = await subtle.importKey('raw', raw, ECDH, false, []);
      return new Uint8Array(await subtle.deriveBits({ name: 'ECDH', public: pub }, mine.privateKey, 256));
    };
    const ikm = concat(await dh(b64Decode(key)), await dh(eph));
    const hkdf = await subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits']);
    const okm = new Uint8Array(
      await subtle.deriveBits(
        { name: 'HKDF', hash: 'SHA-256', salt: enc.encode('iris-link-v1'), info: concat(myPub, eph) },
        hkdf,
        512,
      ),
    );
    const aes = (bytes: Uint8Array<ArrayBuffer>) => subtle.importKey('raw', bytes, 'AES-GCM', false, ['encrypt', 'decrypt']);
    const session = new Session(ws, await aes(okm.slice(0, 32)), await aes(okm.slice(32)));
    ws.onmessage = (m) => {
      const text = String(m.data);
      if (text.startsWith('{"relay"')) onRelay(JSON.parse(text));
      else void session.receive(text);
    };
    return session;
  }

  // Frames are sealed one at a time, so counters go out in order.
  private chain: Promise<void> = Promise.resolve();
  send(msg: object): Promise<void> {
    this.chain = this.chain.then(async () => {
      const n = ++this.sent;
      const ct = await subtle.encrypt(
        { name: 'AES-GCM', iv: nonce(n), additionalData: enc.encode('c2s') },
        this.sendKey,
        enc.encode(JSON.stringify(msg)),
      );
      if (this.ws.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify({ s: n, b: b64Encode(new Uint8Array(ct)) }));
    });
    return this.chain;
  }

  private opening: Promise<void> = Promise.resolve();
  private receive(frame: string) {
    // Decrypt in arrival order (counters must go up).
    this.opening = this.opening.then(async () => {
      let msg: any;
      try {
        const f = JSON.parse(frame);
        if (typeof f.s !== 'number' || f.s <= this.received) return;
        const pt = await subtle.decrypt(
          { name: 'AES-GCM', iv: nonce(f.s), additionalData: enc.encode('s2c') },
          this.recvKey,
          b64Decode(f.b),
        );
        this.received = f.s;
        msg = JSON.parse(dec.decode(pt));
      } catch {
        this.ws.close();
        return;
      }
      if (msg.t === 'res') {
        this.waiters.get(msg.id)?.(msg);
        this.waiters.delete(msg.id);
      } else if (msg.t === 'ev') {
        this.onEvent(msg.e);
      } else if (msg.t === 'pong') {
        this.onPong();
      } else if (msg.t === 'revoked') {
        // This device was removed on the server: hang up. The reconnect signs in again,
        // is refused, and sends the app back to its sign-in screen with the reason.
        this.ws.close();
      } else if (msg.t === 'signed_in' || msg.t === 'error') {
        this.pendingSignIn?.(msg);
        this.pendingSignIn = null;
      }
    });
  }

  signIn(msg: SignIn): Promise<{ t: 'signed_in'; device: string; token?: string } | { t: 'error'; error: string }> {
    return new Promise((resolve) => {
      this.pendingSignIn = resolve;
      void this.send(msg);
    });
  }

  request(method: string, path: string, body?: unknown): Promise<LinkResponse> {
    const id = ++this.nextId;
    return new Promise((resolve) => {
      this.waiters.set(id, resolve);
      void this.send({ t: 'req', id, method, path, body: body === undefined ? null : body });
    });
  }

  /** Fails everything still waiting (the connection dropped). */
  abort() {
    for (const w of this.waiters.values()) w({ status: 0, body: 'connection lost' });
    this.waiters.clear();
    this.pendingSignIn?.({ t: 'error', error: 'connection lost' });
  }
}

/** Opens a session and signs in once; used by the sign-in screen. */
export async function signInOnce(relay: string, key: string, msg: SignIn): Promise<LinkCredentials> {
  let serverAway = false;
  let session: Session;
  try {
    session = await Session.open(relay, key, (r) => (serverAway ||= r.relay === 'offline'));
  } catch (e) {
    throw new Error(
      (e as Error).message === 'server-offline' || serverAway
        ? "Your server isn't connected right now. Check that it's on and Remote access is turned on."
        : (e as Error).message === 'offline'
          ? "Can't reach the relay. Check your internet connection."
          : (e as Error).message,
    );
  }
  try {
    const res = await session.signIn(msg);
    if (res.t === 'error') throw new Error(res.error);
    const token = res.token ?? (msg.t === 'auth' ? msg.token : '');
    return { relay, key, token, device: res.device };
  } finally {
    session.ws.close();
  }
}

/**
 * A lasting connection: reconnects by itself (new handshake, sign-in with the
 * token) and tells the app when it's back so it can refetch.
 */
export class LinkConnection {
  private session: Session | null = null;
  private ready: Promise<Session> | null = null;
  private closed = false;
  private delay = 1000;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private pingTimer: ReturnType<typeof setInterval> | undefined;
  private lastPong = Date.now();
  state: LinkState = 'connecting';

  onEvent: (e: IrisEvent) => void = () => {};
  onOpen: () => void = () => {};
  onState: (s: LinkState) => void = () => {};
  /** The token was refused (device removed): sign in again. */
  onSignedOut: (reason: string) => void = () => {};

  constructor(readonly creds: LinkCredentials) {}

  private stateWaiters: (() => void)[] = [];

  private setState(s: LinkState) {
    if (this.state === s) return;
    this.state = s;
    this.onState(s);
    if (s === 'online') for (const w of this.stateWaiters.splice(0)) w();
  }

  /** Resolves once the link is (back) online, or after `ms`. */
  private waitOnline(ms: number): Promise<void> {
    if (this.state === 'online') return Promise.resolve();
    return new Promise((resolve) => {
      this.stateWaiters.push(resolve);
      setTimeout(resolve, ms);
    });
  }

  /** Connects (or waits for the connection in progress). */
  start(): Promise<Session> {
    if (this.ready) return this.ready;
    this.ready = this.connect();
    this.ready.catch(() => {});
    return this.ready;
  }

  private async connect(): Promise<Session> {
    this.setState('connecting');
    let session: Session;
    try {
      session = await Session.open(this.creds.relay, this.creds.key, (r) => {
        if (r.relay === 'offline') {
          this.setState('server-offline');
        } else if (r.relay === 'online') {
          // The server reconnected to the relay: its old sessions are gone.
          this.restart(200);
        }
      });
    } catch (e) {
      this.setState((e as Error).message === 'server-offline' ? 'server-offline' : 'offline');
      this.restart();
      throw e;
    }
    const res = await session.signIn({ t: 'auth', token: this.creds.token });
    if (res.t === 'error') {
      session.ws.close();
      if (res.error === 'connection lost') {
        this.restart();
        throw new Error(res.error);
      }
      this.closed = true;
      this.onSignedOut(res.error);
      throw new Error(res.error);
    }
    session.onEvent = (e) => this.onEvent(e);
    session.onPong = () => (this.lastPong = Date.now());
    session.ws.onclose = () => {
      session.abort();
      if (this.session === session) this.restart();
    };
    this.session = session;
    this.delay = 1000;
    this.lastPong = Date.now();
    clearInterval(this.pingTimer);
    // Keep-alive, and a way to notice a session the engine has forgotten.
    this.pingTimer = setInterval(() => {
      if (Date.now() - this.lastPong > 50_000) session.ws.close();
      else void session.send({ t: 'ping' });
    }, 20_000);
    this.setState('online');
    this.onOpen();
    return session;
  }

  private restart(delay = this.delay) {
    if (this.closed) return;
    this.session?.ws.close();
    this.session = null;
    this.ready = null;
    clearInterval(this.pingTimer);
    clearTimeout(this.timer);
    if (this.state === 'online') this.setState('connecting');
    this.timer = setTimeout(() => void this.start(), delay);
    this.delay = Math.min(this.delay * 2, 15_000);
  }

  /** Reconnects now (e.g. the phone came back to the app). */
  nudge() {
    if (this.closed) return;
    if (!this.session || this.session.ws.readyState !== WebSocket.OPEN) {
      this.delay = 1000;
      this.restart(0);
    }
  }

  /** A request; while the link is reconnecting it waits (up to 20 s) for it. */
  async request(method: string, path: string, body?: unknown): Promise<LinkResponse> {
    const deadline = Date.now() + 20_000;
    for (;;) {
      if (this.closed) return { status: 0, body: 'signed out' };
      const session = this.state === 'online' ? this.session : null;
      if (session && session.ws.readyState === WebSocket.OPEN) {
        const res = await session.request(method, path, body);
        // This device was revoked: reconnecting runs the sign-in again, which
        // fails and sends the app back to its sign-in page.
        if (res.status === 401) this.restart(0);
        // Lost mid-request: a read is retried once the link is back (a write
        // may already have happened, so it isn't).
        if (res.status !== 0 || method !== 'GET' || Date.now() > deadline) return res;
      } else if (Date.now() > deadline) {
        return { status: 0, body: "Can't reach the I.R.I.S. engine" };
      }
      if (!this.ready) void this.start();
      await this.waitOnline(Math.max(0, deadline - Date.now()));
    }
  }

  stop() {
    this.closed = true;
    clearTimeout(this.timer);
    clearInterval(this.pingTimer);
    this.session?.ws.close();
  }
}

// ---- remembering the pairing -------------------------------------------------

const KEY = 'iris.link';

export function loadCredentials(): LinkCredentials | null {
  try {
    const saved = localStorage.getItem(KEY);
    return saved ? (JSON.parse(saved) as LinkCredentials) : null;
  } catch {
    return null;
  }
}

export function saveCredentials(c: LinkCredentials) {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    /* storage unavailable: signed in for this visit only */
  }
}

export function forgetCredentials() {
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* nothing to forget */
  }
}

/** The engine's key as the relay reports it (for password sign-in). */
export async function relayEngineKey(relay: string): Promise<string | null> {
  try {
    const r = await fetch(`${relay.replace(/\/$/, '')}/link/key`);
    if (!r.ok) return null;
    return ((await r.json()) as { key?: string }).key ?? null;
  } catch {
    return null;
  }
}

/** A friendly default name for this device. */
export function guessDeviceName(): string {
  const ua = navigator.userAgent;
  const phone = /Android|iPhone|iPad/i.test(ua);
  const os = /Android/i.test(ua) ? 'Android' : /iPhone|iPad/i.test(ua) ? 'iPhone' : /Windows/i.test(ua) ? 'Windows' : /Mac/i.test(ua) ? 'Mac' : /Linux/i.test(ua) ? 'Linux' : '';
  const browser = /Edg\//.test(ua) ? 'Edge' : /SamsungBrowser/.test(ua) ? 'Samsung Internet' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome' : /Safari\//.test(ua) ? 'Safari' : 'Browser';
  return phone ? `${os} phone (${browser})` : `${browser} on ${os || 'a computer'}`;
}
