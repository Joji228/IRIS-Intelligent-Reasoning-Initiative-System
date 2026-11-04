// End-to-end check of I.R.I.S. Link through the real relay, acting as a phone:
// pairs with a one-time code, makes API requests, receives live events, then
// signs in again with the token it got. Uses only WebCrypto, like the apps.
// Usage: node test/link-test.mjs '<pairing link from Settings → Remote access>'
const link = new URL(process.argv[2] ?? '');
const params = new URLSearchParams(link.hash.slice(1));
const code = params.get('pair');
const enginePub = params.get('k');
if (!code || !enginePub) throw new Error("usage: node test/link-test.mjs 'https://…/#pair=CODE&k=KEY'");
const relay = `${link.origin.replace(/^http/, 'ws')}/link/client`;

const subtle = globalThis.crypto.subtle;
const enc = new TextEncoder();
const dec = new TextDecoder();
const b64url = (bytes) => Buffer.from(bytes).toString('base64url');
const fromB64url = (s) => new Uint8Array(Buffer.from(s, 'base64url'));
const ecdh = { name: 'ECDH', namedCurve: 'P-256' };

/** A sealed session with the engine (the same protocol as link.rs). */
async function connect() {
  const ws = new WebSocket(relay);
  const inbox = [];
  const waiters = [];
  ws.onmessage = (m) => (waiters.length ? waiters.shift()(m.data) : inbox.push(m.data));
  const next = () => new Promise((resolve) => (inbox.length ? resolve(inbox.shift()) : waiters.push(resolve)));
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error(`can't reach ${relay}`));
  });

  const mine = await subtle.generateKey(ecdh, true, ['deriveBits']);
  const myPub = new Uint8Array(await subtle.exportKey('raw', mine.publicKey));
  ws.send(JSON.stringify({ t: 'hello', v: 1, epk: b64url(myPub) }));
  const welcome = JSON.parse(await next());
  if (welcome.t !== 'welcome') throw new Error(`expected welcome, got ${JSON.stringify(welcome)}`);
  const ephBytes = fromB64url(welcome.epk);

  const dh = async (raw) => {
    const pub = await subtle.importKey('raw', raw, ecdh, false, []);
    return new Uint8Array(await subtle.deriveBits({ name: 'ECDH', public: pub }, mine.privateKey, 256));
  };
  const ikm = new Uint8Array([...(await dh(fromB64url(enginePub))), ...(await dh(ephBytes))]);
  const hkdfKey = await subtle.importKey('raw', ikm, 'HKDF', false, ['deriveBits']);
  const okm = new Uint8Array(
    await subtle.deriveBits(
      { name: 'HKDF', hash: 'SHA-256', salt: enc.encode('iris-link-v1'), info: new Uint8Array([...myPub, ...ephBytes]) },
      hkdfKey,
      512,
    ),
  );
  const aes = (bytes) => subtle.importKey('raw', bytes, 'AES-GCM', false, ['encrypt', 'decrypt']);
  const sendKey = await aes(okm.slice(0, 32));
  const recvKey = await aes(okm.slice(32));
  const nonce = (n) => {
    const iv = new Uint8Array(12);
    new DataView(iv.buffer).setBigUint64(4, BigInt(n));
    return iv;
  };
  let sent = 0;
  let received = 0;

  const send = async (msg) => {
    sent += 1;
    const ct = await subtle.encrypt(
      { name: 'AES-GCM', iv: nonce(sent), additionalData: enc.encode('c2s') },
      sendKey,
      enc.encode(JSON.stringify(msg)),
    );
    ws.send(JSON.stringify({ s: sent, b: Buffer.from(ct).toString('base64') }));
  };
  const recv = async () => {
    const frame = JSON.parse(await next());
    if (frame.s <= received) throw new Error('replayed frame');
    const pt = await subtle.decrypt(
      { name: 'AES-GCM', iv: nonce(frame.s), additionalData: enc.encode('s2c') },
      recvKey,
      Buffer.from(frame.b, 'base64'),
    );
    received = frame.s;
    return JSON.parse(dec.decode(pt));
  };
  // Messages read while waiting for something else (events during a request).
  const backlog = [];
  /** Next message of type `t` (or an error), skipping others. */
  const expect = async (t) => {
    const i = backlog.findIndex((m) => m.t === t || m.t === 'error');
    if (i >= 0) return backlog.splice(i, 1)[0];
    for (;;) {
      const m = await recv();
      if (m.t === t || m.t === 'error') return m;
    }
  };
  let reqId = 0;
  const request = async (method, path, body) => {
    const id = ++reqId;
    await send({ t: 'req', id, method, path, body });
    for (;;) {
      const m = await recv();
      if (m.t === 'res' && m.id === id) return m;
      backlog.push(m);
    }
  };
  return { ws, send, recv, expect, request };
}

const check = (ok, what) => {
  if (!ok) throw new Error(`FAILED: ${what}`);
  console.log(`ok  ${what}`);
};

// 1. A wrong pairing code is refused, and requests need a sign-in.
let s = await connect();
check(true, 'handshake with the engine through the relay');
const early = await s.request('GET', '/v1/status');
check(early.status === 401, 'requests before signing in are refused');
await s.send({ t: 'pair', code: 'WRONG123', name: 'Link test' });
check((await s.expect('signed_in')).t === 'error', 'a wrong pairing code is refused');

// 2. The real code pairs this "phone" and returns its device token.
await s.send({ t: 'pair', code, name: 'Link test' });
const paired = await s.expect('signed_in');
check(paired.t === 'signed_in' && paired.token?.startsWith('iris_'), `paired as “${paired.device}”`);

// 3. API requests go through the engine's own router.
const status = await s.request('GET', '/v1/status');
check(status.status === 200, `GET /v1/status → ${status.status}`);
const agents = await s.request('GET', '/v1/agents');
const names = JSON.parse(agents.body).map((a) => a.name);
check(agents.status === 200 && names.length > 0, `GET /v1/agents → ${names.join(', ')}`);

// 4. Live events arrive: a far-future routine is added and removed again.
const agentId = JSON.parse(agents.body)[0].id;
const made = await s.request('POST', '/v1/routines', {
  agent_id: agentId,
  name: 'Link test',
  schedule: 'once 2099-01-01 00:00',
  kind: 'command',
  body: 'Write-Output ok',
});
check(made.status === 201, 'created a test routine');
const event = await s.expect('ev');
check(event.e?.type === 'routines_changed', `live event: ${event.e?.type}`);
const removed = await s.request('DELETE', `/v1/routines/${JSON.parse(made.body).id}`);
check(removed.status === 204, 'removed the test routine');
s.ws.close();

// 5. The code works only once; the token signs in again.
s = await connect();
await s.send({ t: 'pair', code, name: 'Link test' });
check((await s.expect('signed_in')).t === 'error', 'the pairing code works only once');
await s.send({ t: 'auth', token: paired.token, name: 'Link test' });
const again = await s.expect('signed_in');
check(again.t === 'signed_in' && !again.token, 'signed in again with the device token');

// 6. Clean up: the test device removes itself.
const devices = JSON.parse((await s.request('GET', '/v1/devices')).body);
const me = devices.find((d) => d.name === 'Link test');
const del = await s.request('DELETE', `/v1/devices/${me.id}`);
check(del.status === 204, 'removed the test device');
s.ws.close();

s = await connect();
await s.send({ t: 'auth', token: paired.token });
check((await s.expect('signed_in')).t === 'error', 'a removed device can no longer sign in');
s.ws.close();
console.log('I.R.I.S. Link works');
