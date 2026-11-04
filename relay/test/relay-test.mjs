// End-to-end check of the deployed relay: a fake engine (with the server key
// from IRIS_SERVER_KEY) and a fake phone exchange messages through it, and a
// wrong key is refused. Usage: node test/relay-test.mjs https://iris.<you>.workers.dev
const base = (process.argv[2] ?? '').replace(/^http/, 'ws').replace(/\/$/, '');
const key = process.env.IRIS_SERVER_KEY ?? '';
if (!base || !key) throw new Error('usage: IRIS_SERVER_KEY=… node test/relay-test.mjs <relay url>');

const open = (url, headers) =>
  new Promise((resolve, reject) => {
    const ws = new WebSocket(url, { headers });
    ws.onopen = () => resolve(ws);
    ws.onerror = (e) => reject(new Error(`${url}: ${e.message ?? 'refused'}`));
  });
const next = (ws) => new Promise((resolve) => ws.addEventListener('message', (m) => resolve(m.data), { once: true }));

// 1. A wrong key must be refused.
try {
  await open(`${base}/link/server`, { 'X-Iris-Server-Key': 'wrong' });
  throw new Error('a wrong server key was accepted');
} catch (e) {
  if (e.message.includes('accepted')) throw e;
  console.log('ok  wrong key refused');
}

// 2. Engine and client talk through the relay.
const server = await open(`${base}/link/server`, { 'X-Iris-Server-Key': key });
console.log('ok  engine connected');
const hello = next(server);
const client = await open(`${base}/link/client`);
const opened = JSON.parse(await hello);
if (!opened.open || !opened.c) throw new Error(`expected an open frame, got ${JSON.stringify(opened)}`);
console.log('ok  engine told about the new client');

const toServer = next(server);
client.send('sealed-hello');
const got = JSON.parse(await toServer);
if (got.c !== opened.c || got.d !== 'sealed-hello') throw new Error(`bad frame ${JSON.stringify(got)}`);
console.log('ok  client → engine');

const toClient = next(client);
server.send(JSON.stringify({ c: opened.c, d: 'sealed-reply' }));
if ((await toClient) !== 'sealed-reply') throw new Error('reply not delivered');
console.log('ok  engine → client');

const closed = next(server);
client.close();
const bye = JSON.parse(await closed);
if (!bye.close) throw new Error('no close frame');
console.log('ok  engine told the client left');
server.close();
console.log('relay works');
